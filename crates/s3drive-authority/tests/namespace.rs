use s3drive_authority::{
    Apply, Authority, Error, Kind, Mutation, ROOT, Reply, Request, Token, Volume, serve_loopback,
};
use std::time::Duration;
use tokio::{io::AsyncReadExt, sync::oneshot, task::JoinHandle};

const SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
#[path = "namespace/deferred.rs"]
mod deferred;
#[path = "namespace/empty.rs"]
mod empty;
#[path = "namespace/event.rs"]
mod event;
#[path = "namespace/history.rs"]
mod history;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owned_target_replacements_race_and_recover_original_two_node_results_over_http() {
    use s3drive_authority::{NodeGuard, RemovalClaim, RemovalMode};
    for (index, mode) in [RemovalMode::Deferred, RemovalMode::Pending]
        .into_iter()
        .enumerate()
    {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("authority");
        let authority = Authority::open_with_policy(path.clone(), "portable-collision-v1".into())
            .await
            .unwrap();
        let (url, stop, task) = start(authority.client()).await;
        let a = http();
        let b = http();
        let Reply::Features(features) = ask(&a, &url, &Request::Features {}).await else {
            panic!()
        };
        assert!(features.iter().any(|f| f == "owned-target-replace-v1"));
        let mut nodes = Vec::new();
        for n in 0..3 {
            let r = change(
                &info(&a, &url).await,
                70000 + n,
                Mutation::CreateEntry {
                    parent: ROOT.into(),
                    parent_revision: 0,
                    store: None,
                    name: format!("directory café 日本語 🦀 {n}"),
                    node_kind: Kind::Directory,
                },
            );
            nodes.push(applied(ask(&a, &url, &r).await).node);
        }
        let guard = |n: &s3drive_authority::Node| NodeGuard {
            id: n.id.clone(),
            revision: n.revision,
        };
        let parent = NodeGuard {
            id: ROOT.into(),
            revision: 0,
        };
        let owner = format!("{:032x}", 9000);
        let arm = change(
            &info(&a, &url).await,
            71000,
            if mode == RemovalMode::Deferred {
                Mutation::PrepareRemoval {
                    node: guard(&nodes[2]),
                    parent: parent.clone(),
                    owner,
                }
            } else {
                Mutation::ArmRemoval {
                    node: guard(&nodes[2]),
                    parent: parent.clone(),
                    owner,
                }
            },
        );
        let p = applied(ask(&a, &url, &arm).await);
        let Some(s3drive_authority::Artifact::Removal(fence)) = p.artifact else {
            panic!()
        };
        let v = info(&a, &url).await;
        let movement = |n: usize| {
            change(
                &v,
                72000 + n as u64,
                Mutation::MoveEntry {
                    node: guard(&nodes[n]),
                    source_parent: parent.clone(),
                    parent: parent.clone(),
                    name: p.node.name.clone(),
                    destination: Some(guard(&p.node)),
                    destination_removal: Some(Box::new(RemovalClaim {
                        fence: fence.clone(),
                        previous: format!("{:032x}", 71000),
                        mode,
                    })),
                },
            )
        };
        let left = movement(0);
        let right = movement(1);
        let (x, y) = tokio::join!(ask(&a, &url, &left), ask(&b, &url, &right));
        let (winner, original, loser) = match (x, y) {
            (Reply::Applied(r), Reply::Error(Error::Conflict)) => (r, left, right),
            (Reply::Error(Error::Conflict), Reply::Applied(r)) => (r, right, left),
            other => panic!("one owned-target winner: {other:?}"),
        };
        let response = a
            .post(&url)
            .bearer_auth(SECRET)
            .json(&original)
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        drop(response);
        close(stop, task, authority).await;
        let authority = Authority::open_with_policy(path, "portable-collision-v1".into())
            .await
            .unwrap();
        let (url, stop, task) = start(authority.client()).await;
        let sequence = info(&a, &url).await.sequence;
        assert_eq!(
            ask(&a, &url, &original).await,
            Reply::Applied(winner.clone())
        );
        assert_eq!(ask(&b, &url, &loser).await, Reply::Error(Error::Conflict));
        assert_eq!(info(&a, &url).await.sequence, sequence);
        assert_eq!(
            ask(
                &a,
                &url,
                &Request::PendingRemoval {
                    volume: v.id.clone(),
                    node: p.node.id.clone()
                }
            )
            .await,
            Reply::PendingRemoval(None)
        );
        let old_close = change(
            &info(&a, &url).await,
            73000 + index as u64,
            Mutation::CancelRemoval {
                node: p.node.id.clone(),
                fence,
            },
        );
        assert_eq!(
            ask(&a, &url, &old_close).await,
            Reply::Error(Error::Conflict)
        );
        let listed = listing(&a, &url, &v.id, ROOT).await;
        assert!(listed.children.contains(&winner.node));
        assert_eq!(listed.children.len(), 2);
        close(stop, task, authority).await;
    }
}
fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}
async fn ask(c: &reqwest::Client, url: &str, r: &Request) -> Reply {
    c.post(url)
        .bearer_auth(SECRET)
        .json(r)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}
async fn start(client: s3drive_authority::Client) -> (String, oneshot::Sender<()>, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1/namespace", listener.local_addr().unwrap());
    let (stop, receive) = oneshot::channel();
    let server = tokio::spawn(async move {
        serve_loopback(
            listener,
            client,
            Token::new(SECRET.into()).unwrap(),
            async {
                let _ = receive.await;
            },
        )
        .await
        .unwrap();
    });
    (url, stop, server)
}
async fn info(c: &reqwest::Client, url: &str) -> Volume {
    let Reply::Info(v) = ask(c, url, &Request::Info {}).await else {
        panic!("info")
    };
    v
}
fn change(v: &Volume, id: u64, mutation: Mutation) -> Request {
    Request::Apply {
        request: Apply {
            volume: v.id.clone(),
            expected_sequence: v.sequence,
            operation: format!("{id:032x}"),
            mutation,
        },
    }
}
fn create(parent: &str, name: &str, kind: Kind) -> Mutation {
    Mutation::Create {
        parent: parent.into(),
        name: name.into(),
        node_kind: kind,
    }
}
fn applied(reply: Reply) -> s3drive_authority::Receipt {
    let Reply::Applied(r) = reply else {
        panic!("expected applied, got {reply:?}")
    };
    r
}
async fn listing(
    c: &reqwest::Client,
    url: &str,
    volume: &str,
    parent: &str,
) -> s3drive_authority::Listing {
    let Reply::List(r) = ask(
        c,
        url,
        &Request::List {
            volume: volume.into(),
            parent: parent.into(),
        },
    )
    .await
    else {
        panic!("list")
    };
    r
}
async fn close(stop: oneshot::Sender<()>, task: JoinHandle<()>, authority: Authority) {
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("authority HTTP server did not drain within thirty seconds")
        .unwrap();
    // This joins the owner after SQLite closes/checkpoints and releases its
    // private directory. A loaded Windows runner exceeded the old five-second
    // wrapper; do not cancel that durable shutdown or skip the reopen checks.
    tokio::time::timeout(Duration::from_secs(30), authority.shutdown())
        .await
        .expect("authority SQLite owner did not close within thirty seconds")
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn independent_clients_race_atomic_rename_and_recover_committed_ids() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("authority");
    let authority = Authority::open(state.clone()).await.unwrap();
    // A competing process/owner may not open this private authority directory.
    assert!(Authority::open(state.clone()).await.is_err());
    let (url, stop, task) = start(authority.client()).await;
    let a = http();
    let b = http();
    let mut v = info(&a, &url).await;
    assert_eq!(v.name_policy, "exact-utf8-v1");
    let first = change(&v, 1, create(ROOT, "café 日本語 🦀", Kind::Directory));
    let second = change(&v, 2, create(ROOT, "café 日本語 🦀", Kind::Directory));
    let (r1, r2) = tokio::join!(ask(&a, &url, &first), ask(&b, &url, &second));
    let (winner, original, loser) = match (r1, r2) {
        (Reply::Applied(r), Reply::Error(Error::Conflict)) => (r, first, second),
        (Reply::Error(Error::Conflict), Reply::Applied(r)) => (r, second, first),
        r => panic!("one winner required: {r:?}"),
    };
    v.sequence = winner.sequence;
    assert_eq!(ask(&a, &url, &loser).await, Reply::Error(Error::Conflict));
    let dest = applied(
        ask(
            &a,
            &url,
            &change(&v, 3, create(ROOT, "destination", Kind::Directory)),
        )
        .await,
    );
    v.sequence = dest.sequence;
    let child = applied(
        ask(
            &b,
            &url,
            &change(
                &v,
                4,
                create(&winner.node.id, "empty 🦀.txt", Kind::EmptyFile),
            ),
        )
        .await,
    );
    v.sequence = child.sequence;
    let moved = applied(
        ask(
            &a,
            &url,
            &change(
                &v,
                5,
                Mutation::Move {
                    node: winner.node.id.clone(),
                    parent: dest.node.id.clone(),
                    name: "renamed 🌍".into(),
                },
            ),
        )
        .await,
    );
    v.sequence = moved.sequence;
    assert_eq!(listing(&b, &url, &v.id, ROOT).await.children.len(), 1);
    assert_eq!(
        listing(&b, &url, &v.id, &dest.node.id).await.children[0].id,
        winner.node.id
    );
    assert_eq!(
        listing(&a, &url, &v.id, &winner.node.id).await.children[0],
        child.node
    );
    let cycle = change(
        &v,
        6,
        Mutation::Move {
            node: dest.node.id.clone(),
            parent: winner.node.id.clone(),
            name: "cycle".into(),
        },
    );
    assert_eq!(ask(&b, &url, &cycle).await, Reply::Error(Error::Cycle));
    let nonempty = change(
        &v,
        7,
        Mutation::Remove {
            node: winner.node.id.clone(),
        },
    );
    assert_eq!(
        ask(&a, &url, &nonempty).await,
        Reply::Error(Error::NotEmpty)
    );
    let remove = applied(
        ask(
            &a,
            &url,
            &change(
                &v,
                8,
                Mutation::Remove {
                    node: child.node.id.clone(),
                },
            ),
        )
        .await,
    );
    assert!(remove.node.deleted);
    v.sequence = remove.sequence;
    assert!(
        listing(&b, &url, &v.id, &winner.node.id)
            .await
            .children
            .is_empty()
    );
    let rebound = change(&v, 4, create(ROOT, "malicious reuse", Kind::EmptyFile));
    assert_eq!(
        ask(&a, &url, &rebound).await,
        Reply::Error(Error::OperationMismatch)
    );
    // Original reply is immutable; it does not move the renamed node back.
    assert_eq!(
        ask(&a, &url, &original).await,
        Reply::Applied(winner.clone())
    );
    let before = info(&a, &url).await;
    close(stop, task, authority).await;
    let authority = Authority::open(state).await.unwrap();
    let (url, stop, task) = start(authority.client()).await;
    assert_eq!(info(&b, &url).await, before);
    assert_eq!(ask(&a, &url, &original).await, Reply::Applied(winner));
    assert_eq!(ask(&a, &url, &cycle).await, Reply::Error(Error::Cycle));
    assert_eq!(
        listing(&b, &url, &v.id, &dest.node.id).await.children[0],
        moved.node
    );
    close(stop, task, authority).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lost_http_reply_is_looked_up_without_reapplying_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("authority");
    let authority = Authority::open(state.clone()).await.unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let c = http();
    let v = info(&c, &url).await;
    let original = change(&v, 101, create(ROOT, "lost reply.txt", Kind::EmptyFile));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!("http://{}/", listener.local_addr().unwrap());
    let upstream_url = url.clone();
    let upstream_request = original.clone();
    // A bounded disposable proxy waits for request bytes, commits upstream and
    // deliberately closes the downstream socket without sending the receipt.
    let proxy = tokio::spawn(async move {
        let (mut downstream, _) = listener.accept().await.unwrap();
        let mut byte = [0u8; 1];
        downstream.read_exact(&mut byte).await.unwrap();
        let result = ask(&http(), &upstream_url, &upstream_request).await;
        drop(downstream);
        result
    });
    assert!(c.post(proxy_url).json(&original).send().await.is_err());
    let committed = tokio::time::timeout(Duration::from_secs(5), proxy)
        .await
        .unwrap()
        .unwrap();
    let receipt = applied(committed);
    assert_eq!(info(&c, &url).await.sequence, 1);
    close(stop, task, authority).await;
    let authority = Authority::open(state).await.unwrap();
    let (url, stop, task) = start(authority.client()).await;
    assert_eq!(
        ask(
            &c,
            &url,
            &Request::Operation {
                volume: v.id.clone(),
                operation: format!("{:032x}", 101)
            }
        )
        .await,
        Reply::Operation(Some(Ok(receipt.clone())))
    );
    assert_eq!(ask(&c, &url, &original).await, Reply::Applied(receipt));
    assert_eq!(info(&c, &url).await.sequence, 1);
    assert_eq!(listing(&c, &url, &v.id, ROOT).await.children.len(), 1);
    close(stop, task, authority).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_directory_moves_cannot_create_a_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let authority = Authority::open(dir.path().join("authority")).await.unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let a = http();
    let b = http();
    let mut v = info(&a, &url).await;
    let x = applied(
        ask(
            &a,
            &url,
            &change(&v, 11, create(ROOT, "A", Kind::Directory)),
        )
        .await,
    );
    v.sequence = x.sequence;
    let y = applied(
        ask(
            &a,
            &url,
            &change(&v, 12, create(ROOT, "B", Kind::Directory)),
        )
        .await,
    );
    v.sequence = y.sequence;
    let xy = change(
        &v,
        13,
        Mutation::Move {
            node: x.node.id.clone(),
            parent: y.node.id.clone(),
            name: "A".into(),
        },
    );
    let yx = change(
        &v,
        14,
        Mutation::Move {
            node: y.node.id.clone(),
            parent: x.node.id.clone(),
            name: "B".into(),
        },
    );
    let (r1, r2) = tokio::join!(ask(&a, &url, &xy), ask(&b, &url, &yx));
    assert!(matches!(
        (&r1, &r2),
        (Reply::Applied(_), Reply::Error(Error::Conflict))
            | (Reply::Error(Error::Conflict), Reply::Applied(_))
    ));
    v = info(&a, &url).await;
    let (id, node, parent, name) = if matches!(r1, Reply::Applied(_)) {
        (15, y.node.id.clone(), x.node.id.clone(), "B")
    } else {
        (16, x.node.id.clone(), y.node.id.clone(), "A")
    };
    assert_eq!(
        ask(
            &a,
            &url,
            &change(
                &v,
                id,
                Mutation::Move {
                    node,
                    parent,
                    name: name.into()
                }
            )
        )
        .await,
        Reply::Error(Error::Cycle)
    );
    close(stop, task, authority).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authentication_strict_requests_wrong_volume_and_unicode_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let authority = Authority::open(dir.path().join("authority")).await.unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let c = http();
    assert_eq!(
        c.post(&url)
            .json(&Request::Info {})
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        c.post(&url)
            .bearer_auth("wrong")
            .json(&Request::Info {})
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let invalid = c
        .post(&url)
        .bearer_auth(SECRET)
        .header("content-type", "application/json")
        .body(r#"{"action":"info","secret":"must not echo"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), 400);
    assert!(!invalid.text().await.unwrap().contains("must not echo"));
    assert_eq!(
        c.post(&url)
            .bearer_auth(SECRET)
            .header("content-type", "application/json")
            .body("x".repeat(4097))
            .send()
            .await
            .unwrap()
            .status(),
        413
    );
    assert_eq!(
        c.post(&url)
            .bearer_auth(SECRET)
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        415
    );
    let mut v = info(&c, &url).await;
    let original = v.clone();
    v.id = "ffffffffffffffffffffffffffffffff".into();
    assert_eq!(
        ask(
            &c,
            &url,
            &change(&v, 20, create(ROOT, "wrong", Kind::EmptyFile))
        )
        .await,
        Reply::Error(Error::WrongVolume)
    );
    v = original;
    for (i, name) in [
        "café.txt",
        "cafe\u{301}.txt",
        "日本語.txt",
        "🦀🌍.txt",
        "Case",
        "case",
    ]
    .iter()
    .enumerate()
    {
        let r = applied(
            ask(
                &c,
                &url,
                &change(&v, 30 + i as u64, create(ROOT, name, Kind::EmptyFile)),
            )
            .await,
        );
        assert_eq!(&r.node.name, name);
        v.sequence = r.sequence;
    }
    // Prototype exact-byte names deliberately do not claim Windows equivalence.
    assert_eq!(listing(&c, &url, &v.id, ROOT).await.children.len(), 6);
    assert_eq!(
        ask(
            &c,
            &url,
            &change(&v, 40, create(ROOT, "café.txt", Kind::EmptyFile))
        )
        .await,
        Reply::Error(Error::AlreadyExists)
    );
    assert_eq!(
        ask(
            &c,
            &url,
            &change(&v, 41, Mutation::Remove { node: ROOT.into() })
        )
        .await,
        Reply::Error(Error::Root)
    );
    assert_eq!(
        ask(
            &c,
            &url,
            &change(&v, 42, create(ROOT, "../escape", Kind::EmptyFile))
        )
        .await,
        Reply::Error(Error::Invalid)
    );
    assert_eq!(info(&c, &url).await.sequence, 6);
    close(stop, task, authority).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn guarded_replacements_race_and_replay_after_authority_restart() {
    use s3drive_authority::{Artifact, NodeGuard};
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let authority = Authority::open_with_policy(path.clone(), "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let a = http();
    let b = http();
    let Reply::Features(features) = ask(&a, &url, &Request::Features {}).await else {
        panic!()
    };
    assert!(features.iter().any(|s| s == "namespace-entry-cas-v1"));
    let mut v = info(&a, &url).await;
    let store = applied(
        ask(
            &a,
            &url,
            &change(
                &v,
                200,
                Mutation::RegisterStore {
                    route: "a".repeat(64),
                    root: "internal/".into(),
                },
            ),
        )
        .await,
    );
    let Some(Artifact::Store(store)) = store.artifact else {
        panic!()
    };
    let mut nodes = Vec::new();
    for (op, name) in [
        (1, "temporary A 🦀"),
        (2, "temporary B 日本語"),
        (3, "café"),
    ] {
        v = info(&a, &url).await;
        nodes.push(
            applied(
                ask(
                    &a,
                    &url,
                    &change(
                        &v,
                        op,
                        Mutation::CreateEntry {
                            parent: ROOT.into(),
                            parent_revision: 0,
                            store: Some(store.id.clone()),
                            name: name.into(),
                            node_kind: Kind::EmptyFile,
                        },
                    ),
                )
                .await,
            )
            .node,
        );
    }
    v = info(&a, &url).await;
    let guard = |n: &s3drive_authority::Node| NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    };
    let root = NodeGuard {
        id: ROOT.into(),
        revision: 0,
    };
    let first = change(
        &v,
        10,
        Mutation::MoveEntry {
            destination_removal: None,
            node: guard(&nodes[0]),
            source_parent: root.clone(),
            parent: root.clone(),
            name: "cafe\u{301}".into(),
            destination: Some(guard(&nodes[2])),
        },
    );
    let second = change(
        &v,
        11,
        Mutation::MoveEntry {
            destination_removal: None,
            node: guard(&nodes[1]),
            source_parent: root.clone(),
            parent: root.clone(),
            name: "CAFÉ".into(),
            destination: Some(guard(&nodes[2])),
        },
    );
    let (ra, rb) = tokio::join!(ask(&a, &url, &first), ask(&b, &url, &second));
    let (winner, original, loser) = match (ra, rb) {
        (Reply::Applied(r), Reply::Error(Error::Conflict)) => (r, first, second),
        (Reply::Error(Error::Conflict), Reply::Applied(r)) => (r, second, first),
        other => panic!("expected one guarded replacement winner: {other:?}"),
    };
    let Some(Artifact::Namespace { displaced: Some(t) }) = &winner.artifact else {
        panic!()
    };
    assert_eq!(t.id, nodes[2].id);
    assert!(t.deleted);
    assert_eq!(listing(&a, &url, &v.id, ROOT).await.children.len(), 2);
    close(stop, task, authority).await;
    let authority = Authority::open_with_policy(path, "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let sequence = info(&a, &url).await.sequence;
    assert_eq!(
        ask(&a, &url, &original).await,
        Reply::Applied(winner.clone())
    );
    assert_eq!(ask(&a, &url, &loser).await, Reply::Error(Error::Conflict));
    let current = info(&a, &url).await;
    assert_eq!(sequence, current.sequence);
    let removed = applied(
        ask(
            &a,
            &url,
            &change(
                &current,
                12,
                Mutation::RemoveEntry {
                    node: guard(&winner.node),
                    parent: root,
                },
            ),
        )
        .await,
    );
    assert!(removed.node.deleted);
    assert_eq!(listing(&a, &url, &v.id, ROOT).await.children.len(), 1);
    assert_eq!(ask(&a, &url, &original).await, Reply::Applied(winner));
    let Reply::Node(still_deleted) = ask(
        &a,
        &url,
        &Request::Node {
            volume: v.id,
            node: removed.node.id,
        },
    )
    .await
    else {
        panic!()
    };
    assert!(still_deleted.deleted);
    close(stop, task, authority).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_disposition_races_fences_and_exact_recovery_after_http_reply_loss() {
    use s3drive_authority::{Artifact, NodeGuard, RemovalFence};
    let id = |n: u64| format!("{n:032x}");
    let guard = |n: &s3drive_authority::Node| NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    };
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let authority = Authority::open_with_policy(path.clone(), "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let a = http();
    let b = http();
    let Reply::Features(features) = ask(&a, &url, &Request::Features {}).await else {
        panic!()
    };
    assert!(features.iter().any(|s| s == "pending-removal-v1"));
    let mut v = info(&a, &url).await;
    let store = applied(
        ask(
            &a,
            &url,
            &change(
                &v,
                4000,
                Mutation::RegisterStore {
                    route: "a".repeat(64),
                    root: "internal/".into(),
                },
            ),
        )
        .await,
    );
    v.sequence = store.sequence;
    let file = applied(
        ask(
            &a,
            &url,
            &change(
                &v,
                4001,
                Mutation::CreateEntry {
                    parent: ROOT.into(),
                    parent_revision: 0,
                    store: Some(id(4000)),
                    name: "deferred 日本語 🦀".into(),
                    node_kind: Kind::EmptyFile,
                },
            ),
        )
        .await,
    )
    .node;
    v = info(&a, &url).await;
    let arm_a = change(
        &v,
        4002,
        Mutation::ArmRemoval {
            node: guard(&file),
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            owner: id(9000),
        },
    );
    let arm_b = change(
        &v,
        4003,
        Mutation::ArmRemoval {
            node: guard(&file),
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            owner: id(9001),
        },
    );
    let (left, right) = tokio::join!(ask(&a, &url, &arm_a), ask(&b, &url, &arm_b));
    let (armed, winning_request) = match (left, right) {
        (Reply::Applied(r), Reply::Error(Error::Conflict)) => (r, arm_a),
        (Reply::Error(Error::Conflict), Reply::Applied(r)) => (r, arm_b),
        replies => panic!("one reservation winner required: {replies:?}"),
    };
    let Some(Artifact::Removal(fence)) = armed.artifact.clone() else {
        panic!()
    };
    assert_eq!(
        ask(
            &b,
            &url,
            &Request::Node {
                volume: v.id.clone(),
                node: file.id.clone()
            }
        )
        .await,
        Reply::Error(Error::DeletePending)
    );
    let Reply::PendingRemoval(Some(pending)) = ask(
        &b,
        &url,
        &Request::PendingRemoval {
            volume: v.id.clone(),
            node: file.id.clone(),
        },
    )
    .await
    else {
        panic!()
    };
    assert_eq!(pending.node, armed.node);
    assert_eq!(pending.fence, fence);
    assert_eq!(
        listing(&a, &url, &v.id, ROOT).await.children,
        vec![armed.node.clone()]
    );
    v = info(&a, &url).await;
    let reserve = |op, fence: Option<RemovalFence>| {
        change(
            &v,
            op,
            match fence {
                Some(fence) => Mutation::ReservePendingContent {
                    node: file.id.clone(),
                    expected_revision: armed.node.revision,
                    store: id(4000),
                    size: 1,
                    sha256: "b".repeat(64),
                    expected_payload: None,
                    fence,
                },
                None => Mutation::ReserveContent {
                    node: file.id.clone(),
                    expected_revision: armed.node.revision,
                    store: id(4000),
                    size: 1,
                    sha256: "b".repeat(64),
                    expected_payload: None,
                },
            },
        )
    };
    assert_eq!(
        ask(&b, &url, &reserve(4004, None)).await,
        Reply::Error(Error::DeletePending)
    );
    assert_eq!(
        ask(
            &b,
            &url,
            &reserve(
                4005,
                Some(RemovalFence {
                    owner: id(99),
                    ..fence.clone()
                })
            )
        )
        .await,
        Reply::Error(Error::Conflict)
    );
    let reserved = applied(ask(&a, &url, &reserve(4006, Some(fence.clone()))).await);
    let Some(Artifact::Reservation(reservation)) = reserved.artifact else {
        panic!()
    };
    assert_eq!(
        reservation.guard.as_ref().unwrap().removal.as_ref(),
        Some(&fence)
    );
    // Metadata-only authority fixture: no S3 upload or physical receipt is claimed.
    v = info(&a, &url).await;
    let cancel = change(
        &v,
        4007,
        Mutation::CancelRemoval {
            node: file.id.clone(),
            fence: fence.clone(),
        },
    );
    let response = a
        .post(&url)
        .bearer_auth(SECRET)
        .json(&cancel)
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    drop(response); // discard the acknowledgment body
    close(stop, task, authority).await;
    let authority = Authority::open_with_policy(path.clone(), "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let cancelled = applied(ask(&b, &url, &cancel).await);
    assert!(!cancelled.node.deleted);
    assert_eq!(cancelled.node.payload, file.payload);
    let sequence = info(&b, &url).await.sequence;
    assert_eq!(
        ask(&b, &url, &cancel).await,
        Reply::Applied(cancelled.clone())
    );
    assert_eq!(info(&b, &url).await.sequence, sequence);
    v = info(&a, &url).await;
    let next = change(
        &v,
        4008,
        Mutation::ArmRemoval {
            node: guard(&cancelled.node),
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            owner: id(9002),
        },
    );
    let next = applied(ask(&b, &url, &next).await);
    let Some(Artifact::Removal(next_fence)) = next.artifact else {
        panic!()
    };
    assert_ne!(fence, next_fence);
    assert_eq!(ask(&a, &url, &winning_request).await, Reply::Applied(armed));
    assert_eq!(ask(&a, &url, &cancel).await, Reply::Applied(cancelled));
    v = info(&a, &url).await;
    let stale_publish = change(
        &v,
        4009,
        Mutation::Publish {
            reservation: reservation.id,
            upload_operation: "d".repeat(64),
            etag: "\"metadata-only\"".into(),
            version: None,
        },
    );
    assert_eq!(
        ask(&a, &url, &stale_publish).await,
        Reply::Error(Error::Conflict)
    );
    let commit = change(
        &v,
        4010,
        Mutation::CommitRemoval {
            node: guard(&next.node),
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            fence: next_fence,
        },
    );
    let response = b
        .post(&url)
        .bearer_auth(SECRET)
        .json(&commit)
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    drop(response);
    close(stop, task, authority).await;
    let authority = Authority::open_with_policy(path, "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let removed = applied(ask(&a, &url, &commit).await);
    assert!(removed.node.deleted);
    assert_eq!(
        ask(
            &a,
            &url,
            &Request::Operation {
                volume: v.id.clone(),
                operation: id(4010)
            }
        )
        .await,
        Reply::Operation(Some(Ok(removed.clone())))
    );
    assert!(listing(&a, &url, &v.id, ROOT).await.children.is_empty());
    v = info(&a, &url).await;
    let reused = applied(
        ask(
            &a,
            &url,
            &change(
                &v,
                4011,
                Mutation::CreateEntry {
                    parent: ROOT.into(),
                    parent_revision: 0,
                    store: Some(id(4000)),
                    name: file.name,
                    node_kind: Kind::EmptyFile,
                },
            ),
        )
        .await,
    );
    assert_ne!(reused.node.id, removed.node.id);
    let sequence = info(&a, &url).await.sequence;
    assert_eq!(ask(&b, &url, &commit).await, Reply::Applied(removed));
    assert_eq!(
        listing(&a, &url, &v.id, ROOT).await.children,
        vec![reused.node]
    );
    assert_eq!(info(&a, &url).await.sequence, sequence);
    close(stop, task, authority).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deferred_mode_clients_race_and_recover_exact_dropped_transition_after_restart() {
    use s3drive_authority::{Artifact, NodeGuard, RemovalMode};
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let authority = Authority::open_with_policy(path.clone(), "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let a = http();
    let b = http();
    let Reply::Features(features) = ask(&a, &url, &Request::Features {}).await else {
        panic!()
    };
    assert!(features.iter().any(|s| s == "deferred-removal-v1"));
    let v = info(&a, &url).await;
    let dir = applied(
        ask(
            &a,
            &url,
            &change(&v, 5000, create(ROOT, "deferred café 🦀", Kind::Directory)),
        )
        .await,
    )
    .node;
    let v = info(&a, &url).await;
    let parent = NodeGuard {
        id: ROOT.into(),
        revision: 0,
    };
    let guard = |n: &s3drive_authority::Node| NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    };
    let prepared = applied(
        ask(
            &a,
            &url,
            &change(
                &v,
                5001,
                Mutation::PrepareRemoval {
                    node: guard(&dir),
                    parent: parent.clone(),
                    owner: format!("{:032x}", 9000),
                },
            ),
        )
        .await,
    );
    let Some(Artifact::Removal(fence)) = prepared.artifact.clone() else {
        panic!()
    };
    assert_eq!(
        ask(
            &b,
            &url,
            &Request::Node {
                volume: v.id.clone(),
                node: dir.id.clone()
            }
        )
        .await,
        Reply::Node(prepared.node.clone())
    );
    let v = info(&a, &url).await;
    let promote = |op| {
        change(
            &v,
            op,
            Mutation::TransitionRemoval {
                node: guard(&prepared.node),
                parent: parent.clone(),
                fence: fence.clone(),
                expected: RemovalMode::Deferred,
                mode: RemovalMode::Pending,
            },
        )
    };
    let ra = promote(5002);
    let rb = promote(5003);
    let (left, right) = tokio::join!(ask(&a, &url, &ra), ask(&b, &url, &rb));
    let (promoted, winner) = match (left, right) {
        (Reply::Applied(r), Reply::Error(Error::Conflict)) => (r, ra),
        (Reply::Error(Error::Conflict), Reply::Applied(r)) => (r, rb),
        other => panic!("one exact mode winner required: {other:?}"),
    };
    assert_eq!(
        ask(
            &b,
            &url,
            &Request::Node {
                volume: v.id.clone(),
                node: dir.id.clone()
            }
        )
        .await,
        Reply::Error(Error::DeletePending)
    );
    let v = info(&a, &url).await;
    let defer = change(
        &v,
        5004,
        Mutation::TransitionRemoval {
            node: guard(&promoted.node),
            parent,
            fence: fence.clone(),
            expected: RemovalMode::Pending,
            mode: RemovalMode::Deferred,
        },
    );
    let response = a
        .post(&url)
        .bearer_auth(SECRET)
        .json(&defer)
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    drop(response);
    close(stop, task, authority).await;
    let authority = Authority::open_with_policy(path, "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let sequence = info(&a, &url).await.sequence;
    let deferred = applied(ask(&b, &url, &defer).await);
    assert_eq!(ask(&a, &url, &winner).await, Reply::Applied(promoted));
    assert_eq!(info(&a, &url).await.sequence, sequence);
    let Reply::PendingRemoval(Some(observed)) = ask(
        &a,
        &url,
        &Request::PendingRemoval {
            volume: v.id.clone(),
            node: dir.id.clone(),
        },
    )
    .await
    else {
        panic!()
    };
    assert_eq!(observed.fence, fence);
    assert_eq!(observed.mode, RemovalMode::Deferred);
    assert_eq!(observed.node, deferred.node);
    let v = info(&a, &url).await;
    let cancel = change(
        &v,
        5005,
        Mutation::CancelRemoval {
            node: dir.id.clone(),
            fence,
        },
    );
    let cancelled = applied(ask(&b, &url, &cancel).await);
    assert_eq!(ask(&a, &url, &defer).await, Reply::Applied(deferred));
    assert_eq!(
        ask(
            &a,
            &url,
            &Request::Node {
                volume: v.id,
                node: dir.id
            }
        )
        .await,
        Reply::Node(cancelled.node)
    );
    close(stop, task, authority).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn atomic_deferred_creators_race_and_recover_lost_exact_receipt_after_restart() {
    use s3drive_authority::{Artifact, RemovalMode};
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let authority = Authority::open_with_policy(path.clone(), "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let a = http();
    let b = http();
    let Reply::Features(features) = ask(&a, &url, &Request::Features {}).await else {
        panic!()
    };
    assert!(features.iter().any(|s| s == "deferred-create-v1"));
    let v = info(&a, &url).await;
    let first = change(
        &v,
        6000,
        Mutation::CreateDeferredEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            store: None,
            name: "directory café 🦀".into(),
            node_kind: Kind::Directory,
            owner: format!("{:032x}", 9000),
        },
    );
    let second = change(
        &v,
        6001,
        Mutation::CreateDeferredEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            store: None,
            name: "directory cafe\u{301} 🦀".into(),
            node_kind: Kind::Directory,
            owner: format!("{:032x}", 9001),
        },
    );
    let (left, right) = tokio::join!(ask(&a, &url, &first), ask(&b, &url, &second));
    let (winner, original) = match (left, right) {
        (Reply::Applied(r), Reply::Error(Error::AlreadyExists)) => (r, first),
        (Reply::Error(Error::AlreadyExists), Reply::Applied(r)) => (r, second),
        other => panic!("one atomic create winner: {other:?}"),
    };
    let Some(Artifact::Removal(fence)) = winner.artifact.clone() else {
        panic!()
    };
    let Reply::PendingRemoval(Some(p)) = ask(
        &b,
        &url,
        &Request::PendingRemoval {
            volume: v.id.clone(),
            node: winner.node.id.clone(),
        },
    )
    .await
    else {
        panic!()
    };
    assert_eq!(p.node, winner.node);
    assert_eq!(p.fence, fence);
    assert_eq!(p.mode, RemovalMode::Deferred);
    assert_eq!(
        listing(&a, &url, &v.id, ROOT).await.children,
        vec![winner.node.clone()]
    );
    let response = a
        .post(&url)
        .bearer_auth(SECRET)
        .json(&original)
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    drop(response);
    close(stop, task, authority).await;
    let authority = Authority::open_with_policy(path, "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let before = info(&b, &url).await.sequence;
    assert_eq!(
        ask(&b, &url, &original).await,
        Reply::Applied(winner.clone())
    );
    assert_eq!(info(&b, &url).await.sequence, before);
    let cancel = change(
        &info(&a, &url).await,
        6002,
        Mutation::CancelRemoval {
            node: winner.node.id.clone(),
            fence,
        },
    );
    let restored = applied(ask(&a, &url, &cancel).await);
    assert!(!restored.node.deleted);
    assert_eq!(ask(&b, &url, &original).await, Reply::Applied(winner));
    assert_eq!(
        ask(
            &a,
            &url,
            &Request::PendingRemoval {
                volume: v.id,
                node: restored.node.id
            }
        )
        .await,
        Reply::PendingRemoval(None)
    );
    close(stop, task, authority).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deferred_owner_moves_race_on_unicode_destination_and_recover_exact_reply_after_restart() {
    use s3drive_authority::{Artifact, NodeGuard, Receipt, RemovalFence};
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let authority = Authority::open_with_policy(path.clone(), "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let a = http();
    let b = http();
    let Reply::Features(features) = ask(&a, &url, &Request::Features {}).await else {
        panic!()
    };
    assert!(features.iter().any(|s| s == "deferred-owner-move-v1"));
    let mut inputs = vec![];
    let mut nodes = vec![];
    for n in [7000, 7001] {
        let v = info(&a, &url).await;
        let r = change(
            &v,
            n,
            Mutation::CreateDeferredEntry {
                parent: ROOT.into(),
                parent_revision: 0,
                store: None,
                name: format!("source {n}"),
                node_kind: Kind::Directory,
                owner: format!("{:032x}", 9000),
            },
        );
        let result = applied(ask(&a, &url, &r).await);
        inputs.push(r);
        nodes.push(result.node);
    }
    let move_request = |v: &Volume, op, n: usize, name: &str| {
        let node = &nodes[n];
        change(
            v,
            op,
            Mutation::MoveDeferredEntry {
                destination_removal: None,
                node: NodeGuard {
                    id: node.id.clone(),
                    revision: node.revision,
                },
                source_parent: NodeGuard {
                    id: ROOT.into(),
                    revision: 0,
                },
                parent: NodeGuard {
                    id: ROOT.into(),
                    revision: 0,
                },
                name: name.into(),
                destination: None,
                fence: Box::new(RemovalFence {
                    intent: node.id.clone(),
                    owner: format!("{:032x}", 9000),
                }),
                previous: node.id.clone().into(),
            },
        )
    };
    let v = info(&a, &url).await;
    let first = move_request(&v, 7010, 0, "destination café 🦀");
    let second = move_request(&v, 7011, 1, "destination cafe\u{301} 🦀");
    let (left, right) = tokio::join!(ask(&a, &url, &first), ask(&b, &url, &second));
    let (winner, original): (Receipt, Request) = match (left, right) {
        (Reply::Applied(r), Reply::Error(Error::AlreadyExists)) => (r, first),
        (Reply::Error(Error::AlreadyExists), Reply::Applied(r)) => (r, second),
        other => panic!("one owner-move winner: {other:?}"),
    };
    let Some(Artifact::RemovalMove {
        fence,
        displaced: None,
    }) = winner.artifact.clone()
    else {
        panic!()
    };
    let response = a
        .post(&url)
        .bearer_auth(SECRET)
        .json(&original)
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    drop(response);
    close(stop, task, authority).await;
    let authority = Authority::open_with_policy(path, "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let before = info(&b, &url).await.sequence;
    assert_eq!(
        ask(&b, &url, &original).await,
        Reply::Applied(winner.clone())
    );
    assert_eq!(info(&b, &url).await.sequence, before);
    for r in &inputs {
        assert!(matches!(ask(&a, &url, r).await, Reply::Applied(_)));
    }
    let Reply::PendingRemoval(Some(current)) = ask(
        &b,
        &url,
        &Request::PendingRemoval {
            volume: v.id.clone(),
            node: winner.node.id.clone(),
        },
    )
    .await
    else {
        panic!()
    };
    assert_eq!(current.node, winner.node);
    assert_eq!(current.fence, fence);
    let cancel = change(
        &info(&a, &url).await,
        7020,
        Mutation::CancelRemoval {
            node: winner.node.id.clone(),
            fence: fence.clone(),
        },
    );
    let kept = applied(ask(&a, &url, &cancel).await);
    let successor = change(
        &info(&a, &url).await,
        7021,
        Mutation::PrepareRemoval {
            node: NodeGuard {
                id: kept.node.id.clone(),
                revision: kept.node.revision,
            },
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            owner: fence.owner,
        },
    );
    let next = applied(ask(&a, &url, &successor).await);
    assert_eq!(ask(&a, &url, &original).await, Reply::Applied(winner));
    let Reply::PendingRemoval(Some(current)) = ask(
        &b,
        &url,
        &Request::PendingRemoval {
            volume: v.id,
            node: kept.node.id,
        },
    )
    .await
    else {
        panic!()
    };
    assert_eq!(current.node, next.node);
    assert_eq!(current.fence.intent, format!("{:032x}", 7021));
    close(stop, task, authority).await;
}
