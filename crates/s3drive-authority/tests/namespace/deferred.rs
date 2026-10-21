//! Authenticated authority HTTP and production wire client, not provider writes.
use super::*;
use s3drive_authority::{Artifact, RemovalMode, WireClient};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deferred_canonical_http_cross_store_unicode_contenders_replay_original_owner_after_restart()
 {
    tokio::time::timeout(Duration::from_secs(30), async {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("authority café 🦀");
        let authority = Authority::open_with_policy(path.clone(), "portable-collision-v1".into())
            .await
            .unwrap();
        let (url, stop, task) = start(authority.client()).await;
        let a = http();
        let b = http();
        let Reply::Features(features) = ask(&a, &url, &Request::Features {}).await else {
            panic!()
        };
        assert!(
            features
                .iter()
                .any(|f| f == "atomic-deferred-empty-manifest-v1")
        );
        for n in [3, 4] {
            let v = info(&a, &url).await;
            applied(
                ask(
                    &a,
                    &url,
                    &change(
                        &v,
                        n,
                        Mutation::RegisterStore {
                            route: format!("{n:x}").repeat(64),
                            root: format!("managed{n}/"),
                        },
                    ),
                )
                .await,
            );
        }
        let v = info(&a, &url).await;
        let make = |op, store, name: &str| {
            change(
                &v,
                op,
                Mutation::CreateDeferredEmptyManifest {
                    parent: ROOT.into(),
                    parent_revision: 0,
                    name: name.into(),
                    store: format!("{store:032x}"),
                    upload_operation: "a".repeat(64),
                    etag: "\"root-proof\"".into(),
                    version: None,
                    owner: format!("{:032x}", 1000 + op),
                },
            )
        };
        let left = make(5, 3, "report café 日本語 🦀");
        let right = make(6, 4, "REPORT cafe\u{301} 日本語 🦀");
        let json = serde_json::to_value(&left).unwrap();
        assert_eq!(
            json["request"]["mutation"]["kind"],
            "create-deferred-empty-manifest"
        );
        assert!(json["request"]["mutation"]["owner"].as_str().is_some());
        let (x, y) = tokio::join!(ask(&a, &url, &left), ask(&b, &url, &right));
        let (winner, original) = match (x, y) {
            (Reply::Applied(r), Reply::Error(Error::AlreadyExists)) => (r, left),
            (Reply::Error(Error::AlreadyExists), Reply::Applied(r)) => (r, right),
            other => panic!("one canonical ownership winner: {other:?}"),
        };
        let Some(Artifact::PublishedRemoval { payload, fence }) = &winner.artifact else {
            panic!("combined receipt")
        };
        let json = serde_json::to_value(&winner).unwrap();
        assert_eq!(json["artifact"]["type"], "published-removal");
        assert!(json["artifact"]["value"]["payload"].is_object());
        assert!(json["artifact"]["value"]["fence"].is_object());
        let payload = payload.clone();
        let fence = fence.clone();
        assert_eq!(winner.node.payload.as_ref(), Some(&payload));
        assert_eq!(payload.reservation.size, 64);
        let Request::Apply { request } = &original else {
            panic!()
        };
        let Mutation::CreateDeferredEmptyManifest { owner, store, .. } = &request.mutation else {
            panic!()
        };
        assert_eq!(&fence.owner, owner);
        assert_eq!(fence.intent, request.operation);
        assert_eq!(&payload.reservation.store.id, store);
        let wire = WireClient::loopback(&url, Token::new(SECRET.into()).unwrap()).unwrap();
        let pending = Request::PendingRemoval {
            volume: v.id.clone(),
            node: winner.node.id.clone(),
        };
        let Reply::PendingRemoval(Some(p)) = wire.request(&pending).await.unwrap() else {
            panic!()
        };
        assert_eq!(p.node, winner.node);
        assert_eq!(p.fence, fence);
        assert_eq!(p.mode, RemovalMode::Deferred);
        let history = Request::ManifestPayload {
            volume: v.id.clone(),
            node: winner.node.id.clone(),
            reservation: payload.reservation.id.clone(),
            reference: s3drive_authority::payload_reference(&winner.node.payload).unwrap(),
        };
        assert_eq!(
            wire.request(&history).await.unwrap(),
            Reply::ManifestPayload(payload.clone())
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
        drop(wire);
        close(stop, task, authority).await;
        let authority = Authority::open_with_policy(path, "portable-collision-v1".into())
            .await
            .unwrap();
        let (url, stop, task) = start(authority.client()).await;
        let wire = WireClient::loopback(&url, Token::new(SECRET.into()).unwrap()).unwrap();
        let before = info(&a, &url).await.sequence;
        assert_eq!(
            wire.request(&original).await.unwrap(),
            Reply::Applied(winner.clone())
        );
        assert_eq!(info(&a, &url).await.sequence, before);
        let cancel = change(
            &info(&a, &url).await,
            7,
            Mutation::CancelRemoval {
                node: winner.node.id.clone(),
                fence,
            },
        );
        applied(wire.request(&cancel).await.unwrap());
        assert_eq!(
            wire.request(&original).await.unwrap(),
            Reply::Applied(winner)
        );
        assert_eq!(
            wire.request(&pending).await.unwrap(),
            Reply::PendingRemoval(None)
        );
        assert_eq!(
            wire.request(&history).await.unwrap(),
            Reply::ManifestPayload(payload)
        );
        close(stop, task, authority).await;
    })
    .await
    .expect("bounded deferred canonical HTTP acceptance");
}
