use super::*;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn atomic_empty_http_clients_race_unicode_names_and_recover_discarded_reply_after_restart() {
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
        assert!(features.iter().any(|f| f == "atomic-empty-manifest-v1"));
        let v = info(&a, &url).await;
        applied(
            ask(
                &a,
                &url,
                &change(
                    &v,
                    3,
                    Mutation::RegisterStore {
                        route: "a".repeat(64),
                        root: "managed/".into(),
                    },
                ),
            )
            .await,
        );
        let v = info(&a, &url).await;
        let make = |op, name: &str| {
            change(
                &v,
                op,
                Mutation::CreateEmptyManifest {
                    parent: ROOT.into(),
                    parent_revision: 0,
                    name: name.into(),
                    store: format!("{:032x}", 3),
                    upload_operation: "a".repeat(64),
                    etag: "\"root-proof\"".into(),
                    version: None,
                },
            )
        };
        let left = make(4, "report café 日本語 🦀");
        let right = make(5, "REPORT café 日本語 🦀");
        let (x, y) = tokio::join!(ask(&a, &url, &left), ask(&b, &url, &right));
        let (winner, original) = match (x, y) {
            (Reply::Applied(r), Reply::Error(Error::AlreadyExists)) => (r, left),
            (Reply::Error(Error::AlreadyExists), Reply::Applied(r)) => (r, right),
            other => panic!("one atomic name winner: {other:?}"),
        };
        assert_eq!(winner.node.kind, Kind::File);
        assert_eq!(
            winner
                .node
                .payload
                .as_ref()
                .unwrap()
                .reservation
                .manifest
                .as_ref()
                .unwrap()
                .root()
                .unwrap()
                .length(),
            0
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
        assert_eq!(applied(ask(&b, &url, &original).await), winner);
        let p = winner.node.payload.as_ref().unwrap();
        let history = Request::ManifestPayload {
            volume: v.id.clone(),
            node: winner.node.id.clone(),
            reservation: p.reservation.id.clone(),
            reference: s3drive_authority::payload_reference(&winner.node.payload).unwrap(),
        };
        assert_eq!(
            ask(&a, &url, &history).await,
            Reply::ManifestPayload(p.clone())
        );
        close(stop, task, authority).await;
        let authority = Authority::open_with_policy(path, "portable-collision-v1".into())
            .await
            .unwrap();
        let (url, stop, task) = start(authority.client()).await;
        assert_eq!(applied(ask(&a, &url, &original).await), winner);
        assert_eq!(
            ask(&b, &url, &history).await,
            Reply::ManifestPayload(p.clone())
        );
        close(stop, task, authority).await;
    })
    .await
    .expect("bounded atomic empty HTTP acceptance");
}
