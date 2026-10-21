//! Actual authenticated HTTP and production WireClient; no S3 or mount claim.
use super::*;
use s3drive_authority::{ContentGuard, ManifestRoot, Node, Payload, WireClient, payload_reference};
use s3drive_core::manifest::{Entry, Page, Reference, objects::RootDescriptor};

fn id(n: u64) -> String {
    format!("{n:032x}")
}
fn binary(s: &str) -> [u8; 16] {
    std::array::from_fn(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
}
pub(super) fn reservation(v: &Volume, n: &Node, op: u64) -> Request {
    let page = Page::new(
        binary(&v.id),
        binary(&n.id),
        0,
        0,
        vec![Entry::Hole {
            length: 9 * 1024 * 1024,
        }],
    )
    .unwrap();
    let root = RootDescriptor::new(
        binary(&v.id),
        binary(&n.id),
        page.length(),
        Reference::new(binary(&id(4000)), binary(&id(op)), 1 << 56, page.digest()).unwrap(),
    )
    .unwrap();
    change(
        v,
        op,
        Mutation::ReserveManifest {
            node: n.id.clone(),
            expected_revision: n.revision,
            store: id(4000),
            size: page.encode().len() as u64,
            sha256: page.digest().iter().map(|b| format!("{b:02x}")).collect(),
            manifest: Box::new(ManifestRoot::new(root).unwrap()),
            guard: ContentGuard {
                removal_claim: None,
                expected: payload_reference(&n.payload),
                detached: false,
                removal: None,
            },
        },
    )
}
async fn publish(c: &reqwest::Client, url: &str, n: &Node, op: u64) -> Node {
    let v = info(c, url).await;
    let r = applied(ask(c, url, &reservation(&v, n, op)).await);
    let Some(s3drive_authority::Artifact::Reservation(r)) = r.artifact else {
        panic!("reservation")
    };
    let req = change(
        &info(c, url).await,
        op + 1,
        Mutation::Publish {
            reservation: r.id,
            upload_operation: "d".repeat(64),
            etag: "\"history-root\"".into(),
            version: None,
        },
    );
    applied(ask(c, url, &req).await).node
}
fn lookup(p: &Payload) -> Request {
    Request::ManifestPayload {
        volume: p.reservation.volume.clone(),
        node: p.reservation.node.clone(),
        reservation: p.reservation.id.clone(),
        reference: payload_reference(&Some(p.clone())).unwrap(),
    }
}
fn wire(url: &str) -> WireClient {
    WireClient::loopback(url, Token::new(SECRET.into()).unwrap()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manifest_history_authenticates_original_reference_after_restart_without_rebasing() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let authority = Authority::open_with_policy(path.clone(), "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(authority.client()).await;
    let c = http();
    let Reply::Features(features) = wire(&url).request(&Request::Features {}).await.unwrap() else {
        panic!("features")
    };
    assert!(features.iter().any(|f| f == "manifest-history-v1"));
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
    let empty = applied(
        ask(
            &c,
            &url,
            &change(
                &info(&c, &url).await,
                4001,
                Mutation::CreateEntry {
                    parent: ROOT.into(),
                    parent_revision: 0,
                    store: Some(id(4000)),
                    name: "history café 日本語 🦀".into(),
                    node_kind: Kind::EmptyFile,
                },
            ),
        )
        .await,
    )
    .node;
    let old = publish(&c, &url, &empty, 4002).await;
    let p = old.payload.as_ref().unwrap().clone();
    let req = lookup(&p);
    assert_eq!(
        wire(&url).request(&req).await.unwrap(),
        Reply::ManifestPayload(p.clone())
    );
    let latest = publish(&c, &url, &old, 4004).await;
    assert_ne!(
        payload_reference(&latest.payload),
        payload_reference(&old.payload)
    );
    let volume = info(&c, &url).await;
    assert_eq!(
        wire(&url).request(&req).await.unwrap(),
        Reply::ManifestPayload(p.clone())
    );
    assert_eq!(
        info(&c, &url).await,
        volume,
        "history lookup mutated sequence"
    );
    // History has no authority to silently adopt latest as a dirty edit's base.
    assert_eq!(
        ask(&c, &url, &reservation(&volume, &old, 4006)).await,
        Reply::Error(Error::Conflict)
    );
    let renamed = applied(
        ask(
            &c,
            &url,
            &change(
                &info(&c, &url).await,
                4007,
                Mutation::Move {
                    node: latest.id,
                    parent: ROOT.into(),
                    name: "history renamed 🦀".into(),
                },
            ),
        )
        .await,
    )
    .node;
    applied(
        ask(
            &c,
            &url,
            &change(
                &info(&c, &url).await,
                4008,
                Mutation::Remove { node: renamed.id },
            ),
        )
        .await,
    );
    let volume = info(&c, &url).await;
    assert_eq!(
        wire(&url).request(&req).await.unwrap(),
        Reply::ManifestPayload(p.clone())
    );
    for bearer in [None, Some("0".repeat(64))] {
        let call = c.post(&url).json(&req);
        let call = if let Some(b) = bearer {
            call.bearer_auth(b)
        } else {
            call
        };
        assert_eq!(
            call.send().await.unwrap().status(),
            reqwest::StatusCode::UNAUTHORIZED
        );
    }
    let mut extra = serde_json::to_value(&req).unwrap();
    extra["extra"] = true.into();
    assert_eq!(
        c.post(&url)
            .bearer_auth(SECRET)
            .json(&extra)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    for case in 0..4 {
        let mut bad = req.clone();
        let Request::ManifestPayload {
            volume,
            node,
            reservation,
            reference,
        } = &mut bad
        else {
            unreachable!()
        };
        let expected = match case {
            0 => {
                *volume = id(9000);
                Error::WrongVolume
            }
            1 => {
                *node = id(9000);
                Error::NotFound
            }
            2 => {
                *reservation = id(9000);
                Error::NotFound
            }
            _ => {
                *reference = "0".repeat(64);
                Error::Conflict
            }
        };
        assert_eq!(
            wire(&url).request(&bad).await.unwrap(),
            Reply::Error(expected)
        );
    }
    assert_eq!(info(&c, &url).await, volume);
    close(stop, task, authority).await;
    let authority = Authority::open_with_policy(path, "portable-collision-v1".into())
        .await
        .unwrap();
    let (url, stop, task) = start(authority.client()).await;
    assert_eq!(
        wire(&url).request(&req).await.unwrap(),
        Reply::ManifestPayload(p)
    );
    assert_eq!(info(&c, &url).await, volume);
    close(stop, task, authority).await;
}
