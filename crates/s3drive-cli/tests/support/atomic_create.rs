//! Exact atomic origin recovery through actual SDK/HTTP and killed local owners.
use super::pending_owner::{arm, close, prepare, request, stage};
use super::*;
use s3drive_authority::{NodeGuard, ROOT, RemovalMode};
const FIRST: &[u8] = b"created binary\0\xff";
const NEXT: &[u8] = b"successor binary\0\xfe";
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn atomic_deferred_creation_recovers_killed_clients_and_publishes_exact_owned_generations() {
    pending_owner::run_children(
        &[
            "atomic_create_start",
            "atomic_create_publish",
            "atomic_create_cancel",
            "atomic_create_finish",
        ],
        &[0, 1, 3, 3],
    )
    .await;
}
async fn create_input(
    p: &Publisher,
    w: &WireClient,
    n: u64,
    kind: Kind,
) -> s3drive_state::namespace_client::Input {
    let owner = p.client_identity().await.unwrap();
    p.namespace_change_input(
        request(
            w,
            n,
            Mutation::CreateDeferredEntry {
                parent: ROOT.into(),
                parent_revision: 0,
                store: (kind == Kind::EmptyFile).then(|| id(200)),
                name: format!("created café 日本語 🦀 {n}"),
                node_kind: kind,
                owner,
            },
        )
        .await,
        &[],
    )
    .unwrap()
}
fn saved(root: &Path, n: u64) -> s3drive_state::namespace_client::Input {
    serde_json::from_slice(&std::fs::read(root.join(format!("create-{n}.json"))).unwrap()).unwrap()
}
pub(super) async fn run_mode(
    mode: &str,
    config: &s3drive_core::Config,
    root: &Path,
    w: &WireClient,
    volume: &str,
) {
    if mode == "atomic_create_start" {
        let reader = s3drive_s3::S3Reader::from_source(&config.sources[0])
            .await
            .unwrap();
        change(
            w,
            200,
            Mutation::RegisterStore {
                route: reader.storage_fingerprint(),
                root: "writable/.s3drive/".into(),
            },
        )
        .await;
    }
    let loss = Arc::new(AtomicU8::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/namespace", listener.local_addr().unwrap());
    let app = axum::Router::new()
        .route("/v1/namespace", axum::routing::post(proxy))
        .with_state(Proxy {
            upstream: w.clone(),
            loss: loss.clone(),
        });
    let (stop, receive) = oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = receive.await;
            })
            .await
            .unwrap();
    });
    let p = publisher(
        config,
        root,
        "atomic-create-client",
        WireClient::loopback(&endpoint, Token::new(SECRET.into()).unwrap()).unwrap(),
        volume.into(),
    )
    .await;
    Box::pin(step(mode, &p, w, root, volume, &loss)).await;
    p.storage().close().await.unwrap();
    stop.send(()).unwrap();
    server.await.unwrap();
}
async fn step(
    mode: &str,
    p: &Publisher,
    w: &WireClient,
    root: &Path,
    volume: &str,
    loss: &AtomicU8,
) {
    if mode == "atomic_create_start" {
        for (n, kind) in [(10, Kind::EmptyFile), (12, Kind::Directory)] {
            let i = create_input(p, w, n, kind).await;
            std::fs::write(
                root.join(format!("create-{n}.json")),
                serde_json::to_vec(&i).unwrap(),
            )
            .unwrap();
            loss.store(8, Ordering::SeqCst);
            assert!(p.change_namespace(i).await.is_err());
            assert_eq!(loss.load(Ordering::SeqCst), 0);
            assert!(p.owned_removal(&id(n)).await.unwrap().is_none());
        }
        p.bind_edit_file(&id(10)).await.unwrap();
        stage(p, w, 10, 5000, FIRST).await;
        let file = p
            .storage()
            .logical()
            .file(volume.into(), id(10))
            .await
            .unwrap();
        let Reply::PendingRemoval(Some(pending)) = w
            .request(&Request::PendingRemoval {
                volume: volume.into(),
                node: id(10),
            })
            .await
            .unwrap()
        else {
            panic!()
        };
        let cancel = p
            .namespace_change_input(
                request(
                    w,
                    8000,
                    Mutation::CancelRemoval {
                        node: id(10),
                        fence: pending.fence.clone(),
                    },
                )
                .await,
                std::slice::from_ref(&file.base),
            )
            .unwrap();
        assert!(p.change_namespace(cancel).await.is_err());
        let edit = p.storage().logical().get(id(5000)).await.unwrap();
        let reserve = request(
            w,
            6000,
            Mutation::ReservePendingContent {
                node: id(10),
                expected_revision: file.base.revision,
                store: id(200),
                size: edit.input.size,
                sha256: edit.input.sha256,
                expected_payload: None,
                fence: pending.fence,
            },
        )
        .await;
        assert!(
            p.stage_edit(&id(5000), reserve, id(7000)).await.is_err(),
            "fresh binding cannot bypass unknown created origin"
        );
        let dir = node(w, volume, 12).await;
        let child = p
            .namespace_change_input(
                request(
                    w,
                    13,
                    Mutation::CreateEntry {
                        parent: id(12),
                        parent_revision: dir.revision,
                        store: Some(id(200)),
                        name: "child".into(),
                        node_kind: Kind::EmptyFile,
                    },
                )
                .await,
                &[],
            )
            .unwrap();
        assert!(p.change_namespace(child).await.is_err());
        assert_eq!(
            p.storage()
                .namespace()
                .pending(None, 100)
                .await
                .unwrap()
                .len(),
            2
        );
    } else if mode == "atomic_create_publish" {
        for n in [10, 12] {
            let i = saved(root, n);
            let r = p.change_namespace(i.clone()).await.unwrap();
            let owned = p.owned_removal(&id(n)).await.unwrap().unwrap();
            assert_eq!(owned.input, i);
            assert_eq!(owned.fence.intent, id(n));
            assert_eq!(owned.mode, RemovalMode::Deferred);
            assert_eq!(owned.receipt, r.outcome.unwrap().unwrap());
        }
        prepare(p, w, 10, 5000, 6000, true).await;
        loss.store(2, Ordering::SeqCst);
        assert!(p.flush_edit(&id(5000)).await.is_err());
        assert_eq!(loss.load(Ordering::SeqCst), 0);
        let r = p.storage().managed().get(id(6000)).await.unwrap();
        assert!(r.publication.is_some() && r.receipt.is_none());
    } else if mode == "atomic_create_cancel" {
        p.flush_edit(&id(5000)).await.unwrap();
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), FIRST);
        let dir = node(w, volume, 12).await;
        let child = p
            .namespace_change_input(
                request(
                    w,
                    13,
                    Mutation::CreateEntry {
                        parent: id(12),
                        parent_revision: dir.revision,
                        store: Some(id(200)),
                        name: "child".into(),
                        node_kind: Kind::EmptyFile,
                    },
                )
                .await,
                &[],
            )
            .unwrap();
        p.change_namespace(child).await.unwrap();
        let dir = node(w, volume, 12).await;
        let fence = p.owned_removal(&id(12)).await.unwrap().unwrap().fence;
        let cancel = p
            .namespace_change_input(
                request(
                    w,
                    8012,
                    Mutation::CancelRemoval {
                        node: id(12),
                        fence,
                    },
                )
                .await,
                &[dir],
            )
            .unwrap();
        p.change_namespace(cancel).await.unwrap();
        assert!(p.owned_removal(&id(12)).await.unwrap().is_none());
        assert_eq!(
            node(w, volume, 13).await.parent.as_deref(),
            Some(id(12).as_str())
        );
        p.change_namespace(create_input(p, w, 11, Kind::EmptyFile).await)
            .await
            .unwrap();
        p.bind_edit_file(&id(11)).await.unwrap();
        stage(p, w, 11, 5011, b"").await;
        prepare(p, w, 11, 5011, 6011, true).await;
        p.flush_edit(&id(5011)).await.unwrap();
        let file = p
            .storage()
            .logical()
            .file(volume.into(), id(11))
            .await
            .unwrap();
        let owned = p.owned_removal(&id(11)).await.unwrap().unwrap();
        let promote = p
            .namespace_change_input(
                request(
                    w,
                    8021,
                    Mutation::TransitionRemoval {
                        node: NodeGuard {
                            id: id(11),
                            revision: file.base.revision,
                        },
                        parent: NodeGuard {
                            id: ROOT.into(),
                            revision: 0,
                        },
                        fence: owned.fence,
                        expected: RemovalMode::Deferred,
                        mode: RemovalMode::Pending,
                    },
                )
                .await,
                &[file.base],
            )
            .unwrap();
        p.change_namespace(promote).await.unwrap();
        close(p, w, 11, 8022, true).await;
        close(p, w, 10, 8010, false).await;
        let fresh = arm(p, w, 10, 8030, true).await;
        stage(p, w, 10, 5010, NEXT).await;
        prepare(p, w, 10, 5010, 6010, true).await;
        p.flush_edit(&id(5010)).await.unwrap();
        let before = p
            .storage()
            .logical()
            .file(volume.into(), id(10))
            .await
            .unwrap();
        p.change_namespace(saved(root, 10)).await.unwrap();
        assert_eq!(
            p.owned_removal(&id(10)).await.unwrap().unwrap().fence,
            fresh
        );
        assert_eq!(
            p.storage()
                .logical()
                .file(volume.into(), id(10))
                .await
                .unwrap()
                .base,
            before.base
        );
        close(p, w, 10, 8031, false).await;
    } else {
        assert_eq!(mode, "atomic_create_finish");
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), NEXT);
        let empty = p
            .storage()
            .logical()
            .file(volume.into(), id(11))
            .await
            .unwrap();
        assert!(empty.base.deleted);
        assert_eq!(empty.base.payload.as_ref().unwrap().reservation.size, 0);
        assert_eq!(empty.head, empty.settled);
        assert!(p.read(&id(11)).await.is_err());
        assert_eq!(
            node(w, volume, 13).await.parent.as_deref(),
            Some(id(12).as_str())
        );
        for edit in [5000, 5010, 5011] {
            p.flush_edit(&id(edit)).await.unwrap();
        }
        return;
    }
    checkpoint("FENCED_CHECKPOINT");
    std::future::pending::<()>().await;
}
