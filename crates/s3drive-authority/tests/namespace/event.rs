//! Real authority/WireClient HTTP only; no S3 bytes or native handle claim.
use super::*;
use s3drive_authority::{
    Artifact, ContentGuard, NodeGuard, RemovalEpoch, RemovalMode, RemovalPublicationMode,
    WireClient, payload_reference,
};
fn id(n: u64) -> String {
    format!("{n:032x}")
}
fn wire(url: &str) -> WireClient {
    WireClient::loopback(url, Token::new(SECRET.into()).unwrap()).unwrap()
}
async fn epoch(w: &WireClient, v: &Volume, n: &str) -> RemovalEpoch {
    let Reply::RemovalEpoch(Some(e)) = w
        .request(&Request::RemovalEpoch {
            volume: v.id.clone(),
            node: n.into(),
        })
        .await
        .unwrap()
    else {
        panic!("owned epoch")
    };
    e
}
fn reserve(v: &Volume, e: &RemovalEpoch, op: u64) -> Request {
    let mut q = super::history::reservation(v, &e.node, op);
    let Request::Apply { request } = &mut q else {
        panic!()
    };
    let Mutation::ReserveManifest { guard, .. } = &mut request.mutation else {
        panic!()
    };
    *guard = ContentGuard {
        expected: payload_reference(&e.node.payload),
        detached: e.claim.mode.detached(),
        removal: Some(e.claim.fence.clone()),
        removal_claim: Some(Box::new(e.claim.clone())),
    };
    q
}
fn publish(v: &Volume, reservation: u64, op: u64) -> Request {
    change(
        v,
        op,
        Mutation::Publish {
            reservation: id(reservation),
            upload_operation: "d".repeat(64),
            etag: "\"event-root\"".into(),
            version: None,
        },
    )
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn event_manifest_http_clients_race_and_replay_lost_receipt_after_committed_removal_epoch() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("authority");
    let a = Authority::open_with_policy(path.clone(), "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(a.client()).await;
    let c = http();
    let left = wire(&url);
    let right = wire(&url);
    let Reply::Features(f) = left.request(&Request::Features {}).await.unwrap() else {
        panic!()
    };
    assert!(f.iter().any(|s| s == "event-fenced-manifest-v1"));
    let v = info(&c, &url).await;
    applied(
        ask(
            &c,
            &url,
            &change(
                &v,
                4000,
                Mutation::RegisterStore {
                    route: "a".repeat(64),
                    root: "managed/".into(),
                },
            ),
        )
        .await,
    );
    let v = info(&c, &url).await;
    let n = applied(
        ask(
            &c,
            &url,
            &change(
                &v,
                4001,
                Mutation::CreateEntry {
                    parent: ROOT.into(),
                    parent_revision: 0,
                    store: Some(id(4000)),
                    name: "café 日本語 🦀 event".into(),
                    node_kind: Kind::EmptyFile,
                },
            ),
        )
        .await,
    )
    .node;
    let v = info(&c, &url).await;
    let arm = change(
        &v,
        4002,
        Mutation::PrepareRemoval {
            node: NodeGuard {
                id: n.id.clone(),
                revision: n.revision,
            },
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            owner: id(9000),
        },
    );
    applied(left.request(&arm).await.unwrap());
    let v = info(&c, &url).await;
    let e = epoch(&left, &v, &n.id).await;
    let q1 = reserve(&v, &e, 4101);
    let q2 = reserve(&v, &e, 4102);
    let (r1, r2) = tokio::join!(left.request(&q1), right.request(&q2));
    assert!(matches!(r1.unwrap(), Reply::Applied(_)));
    assert!(matches!(r2.unwrap(), Reply::Applied(_)));
    let v = info(&c, &url).await;
    let p1 = publish(&v, 4101, 4201);
    let p2 = publish(&v, 4102, 4202);
    let (x, y) = tokio::join!(left.request(&p1), right.request(&p2));
    let (winner, request) = match (x.unwrap(), y.unwrap()) {
        (Reply::Applied(r), Reply::Error(Error::Conflict)) => (r, p1),
        (Reply::Error(Error::Conflict), Reply::Applied(r)) => (r, p2),
        other => panic!("one exact winner: {other:?}"),
    };
    let response = c
        .post(&url)
        .bearer_auth(SECRET)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    drop(response);
    let v = info(&c, &url).await;
    let current = epoch(&left, &v, &n.id).await;
    let moved = change(
        &v,
        4301,
        Mutation::TransitionRemoval {
            node: NodeGuard {
                id: n.id.clone(),
                revision: current.node.revision,
            },
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            fence: current.claim.fence.clone(),
            expected: RemovalMode::Deferred,
            mode: RemovalMode::Pending,
        },
    );
    applied(right.request(&moved).await.unwrap());
    let v = info(&c, &url).await;
    let pending = epoch(&left, &v, &n.id).await;
    let close_request = change(
        &v,
        4302,
        Mutation::CommitRemoval {
            node: NodeGuard {
                id: n.id.clone(),
                revision: pending.node.revision,
            },
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            fence: pending.claim.fence.clone(),
        },
    );
    applied(right.request(&close_request).await.unwrap());
    let v = info(&c, &url).await;
    let closed = epoch(&left, &v, &n.id).await;
    assert_eq!(closed.claim.mode, RemovalPublicationMode::Committed);
    assert!(closed.node.deleted);
    let q3 = reserve(&v, &closed, 4401);
    let r = applied(left.request(&q3).await.unwrap());
    assert!(matches!(r.artifact, Some(Artifact::Reservation(_))));
    let v = info(&c, &url).await;
    let p3 = publish(&v, 4401, 4402);
    let detached = applied(right.request(&p3).await.unwrap());
    assert!(detached.node.deleted);
    close(stop, task, a).await;
    let a = Authority::open_with_policy(path, "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(a.client()).await;
    let w = wire(&url);
    assert_eq!(w.request(&request).await.unwrap(), Reply::Applied(winner));
    assert_eq!(
        w.request(&p3).await.unwrap(),
        Reply::Applied(detached.clone())
    );
    let v = info(&c, &url).await;
    let latest = epoch(&w, &v, &n.id).await;
    assert_eq!(latest.claim, closed.claim);
    assert_eq!(latest.node, detached.node);
    let Reply::List(list) = w
        .request(&Request::List {
            volume: v.id,
            parent: ROOT.into(),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(list.children.iter().all(|child| child.id != n.id));
    close(stop, task, a).await;
}
