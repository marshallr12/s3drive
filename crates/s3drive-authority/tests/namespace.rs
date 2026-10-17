use s3drive_authority::{
    Apply, Authority, Error, Kind, Mutation, ROOT, Reply, Request, Token, Volume, serve_loopback,
};
use std::time::Duration;
use tokio::{io::AsyncReadExt, sync::oneshot, task::JoinHandle};

const SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
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
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), authority.shutdown())
        .await
        .unwrap()
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
