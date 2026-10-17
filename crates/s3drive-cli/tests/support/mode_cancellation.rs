//! Actual SDK/HTTP and killed SQLite owners; no live provider credentials.
use super::pending_owner::{arm, close, prepare, request, stage};
use super::*;
use s3drive_authority::{NodeGuard, ROOT, RemovalMode};
const FIRST: &[u8] = b"binary first\0\xff";
const NEXT: &[u8] = b"binary successor\0\xfe";
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_after_lost_committed_transition_survives_client_death_and_successor_publication() {
    pending_owner::run_children(
        &[
            "mode_cancel_stage",
            "mode_cancel_cancel",
            "mode_cancel_recover",
            "mode_cancel_finish",
        ],
        &[1, 1, 3, 3],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_before_unsent_transition_survives_client_death_and_late_rejection() {
    pending_owner::run_children(
        &[
            "mode_cancel_before_stage",
            "mode_cancel_before_cancel",
            "mode_cancel_before_recover",
            "mode_cancel_before_finish",
        ],
        &[1, 1, 3, 3],
    )
    .await;
}
fn guard(n: &Node) -> NodeGuard {
    NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    }
}
fn parent() -> NodeGuard {
    NodeGuard {
        id: ROOT.into(),
        revision: 0,
    }
}
fn saved(root: &Path, name: &str) -> s3drive_state::namespace_client::Input {
    serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap()
}
pub(super) async fn run_mode(
    mode: &str,
    config: &s3drive_core::Config,
    root: &Path,
    w: &WireClient,
    volume: &str,
) {
    let before = mode.starts_with("mode_cancel_before_");
    let normalized = mode.replace("mode_cancel_before_", "mode_cancel_");
    let mode = normalized.as_str();
    if mode == "mode_cancel_stage" {
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
        for (n, name) in [(10, "cancel café 日本語 🦀"), (11, "empty e\u{301} 🦀")] {
            change(
                w,
                n,
                Mutation::CreateEntry {
                    parent: ROOT.into(),
                    parent_revision: 0,
                    store: Some(id(200)),
                    name: name.into(),
                    node_kind: Kind::EmptyFile,
                },
            )
            .await;
        }
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
        "mode-cancel-client",
        WireClient::loopback(&endpoint, Token::new(SECRET.into()).unwrap()).unwrap(),
        volume.into(),
    )
    .await;
    Box::pin(run_step(mode, before, &p, w, root, volume, &loss)).await;
    p.storage().close().await.unwrap();
    stop.send(()).unwrap();
    server.await.unwrap();
}
async fn run_step(
    mode: &str,
    before: bool,
    p: &Publisher,
    w: &WireClient,
    root: &Path,
    volume: &str,
    loss: &AtomicU8,
) {
    if mode == "mode_cancel_stage" {
        p.bind_edit_file(&id(10)).await.unwrap();
        arm(p, w, 10, 8000, true).await;
        stage(p, w, 10, 5000, FIRST).await;
        prepare(p, w, 10, 5000, 6000, true).await;
        p.flush_edit(&id(5000)).await.unwrap();
        let file = p
            .storage()
            .logical()
            .file(volume.into(), id(10))
            .await
            .unwrap();
        let owned = p.owned_removal(&id(10)).await.unwrap().unwrap();
        let input = p
            .namespace_change_input(
                request(
                    w,
                    8001,
                    Mutation::TransitionRemoval {
                        node: guard(&file.base),
                        parent: parent(),
                        fence: owned.fence,
                        expected: RemovalMode::Deferred,
                        mode: RemovalMode::Pending,
                    },
                )
                .await,
                &[file.base],
            )
            .unwrap();
        std::fs::write(
            root.join("transition.json"),
            serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
        if before {
            p.storage().namespace().begin(input.clone()).await.unwrap();
        } else {
            loss.store(7, Ordering::SeqCst);
            assert!(p.change_namespace(input.clone()).await.is_err());
            assert_eq!(loss.load(Ordering::SeqCst), 0);
        }
        assert!(
            p.transition_cancellation(&input.request.operation)
                .await
                .unwrap()
                .is_none()
        );
    } else if mode == "mode_cancel_cancel" {
        let file = p
            .storage()
            .logical()
            .file(volume.into(), id(10))
            .await
            .unwrap();
        let owned = p.owned_removal(&id(10)).await.unwrap().unwrap();
        assert_eq!(
            owned.mode,
            RemovalMode::Deferred,
            "lost transition was not acknowledged"
        );
        let cancel = p
            .namespace_change_input(
                request(
                    w,
                    8002,
                    Mutation::CancelRemoval {
                        node: id(10),
                        fence: owned.fence,
                    },
                )
                .await,
                &[file.base],
            )
            .unwrap();
        std::fs::write(
            root.join("cancel.json"),
            serde_json::to_vec(&cancel).unwrap(),
        )
        .unwrap();
        loss.store(5, Ordering::SeqCst);
        assert!(p.change_namespace(cancel).await.is_err());
        assert_eq!(loss.load(Ordering::SeqCst), 0);
        assert!(
            p.transition_cancellation(&id(8001))
                .await
                .unwrap()
                .is_none()
        );
        assert!(p.owned_removal(&id(10)).await.unwrap().is_some());
    } else if mode == "mode_cancel_recover" {
        let unknown = saved(root, "transition.json");
        let cancel = saved(root, "cancel.json");
        p.change_namespace(cancel.clone()).await.unwrap();
        let proof = p
            .transition_cancellation(&unknown.request.operation)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(proof.transition.input, unknown);
        assert!(proof.transition.outcome.is_none());
        assert_eq!(proof.cancellation.input, cancel);
        assert!(
            p.storage()
                .namespace()
                .pending(None, 100)
                .await
                .unwrap()
                .iter()
                .any(|r| r.input == unknown && r.outcome.is_none())
        );
        loss.store(7, Ordering::SeqCst);
        let error = p.change_namespace(unknown.clone()).await.unwrap_err();
        assert!(matches!(
            error.downcast_ref::<s3drive_managed::Failure>(),
            Some(s3drive_managed::Failure::CancelledTransition)
        ));
        assert_eq!(
            loss.load(Ordering::SeqCst),
            7,
            "closed intent must not reach the proxy"
        );
        loss.store(0, Ordering::SeqCst);
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), FIRST);
        let fresh = arm(p, w, 10, 8003, true).await;
        stage(p, w, 10, 5010, NEXT).await;
        prepare(p, w, 10, 5010, 6010, true).await;
        p.flush_edit(&id(5010)).await.unwrap();
        let latest = p
            .storage()
            .logical()
            .file(volume.into(), id(10))
            .await
            .unwrap();
        let reply = w
            .request(&Request::Apply {
                request: unknown.request.clone(),
            })
            .await
            .unwrap();
        let outcome = match reply {
            Reply::Applied(r) if !before => Ok(r),
            Reply::Error(s3drive_authority::Error::Conflict) if before => {
                Err(s3drive_authority::Error::Conflict)
            }
            r => panic!("late exact result: {r:?}"),
        };
        p.storage()
            .namespace()
            .acknowledge(unknown.request.operation, outcome)
            .await
            .unwrap();
        assert_eq!(
            p.storage()
                .logical()
                .file(volume.into(), id(10))
                .await
                .unwrap()
                .base,
            latest.base
        );
        assert_eq!(
            p.owned_removal(&id(10)).await.unwrap().unwrap().fence,
            fresh
        );
        close(p, w, 10, 8004, false).await;
        p.bind_edit_file(&id(11)).await.unwrap();
        arm(p, w, 11, 8011, true).await;
        stage(p, w, 11, 5011, b"").await;
        prepare(p, w, 11, 5011, 6011, true).await;
        p.flush_edit(&id(5011)).await.unwrap();
        close(p, w, 11, 8012, false).await;
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), NEXT);
        assert!(p.read(&id(11)).await.unwrap().bytes().is_empty());
    } else {
        assert_eq!(mode, "mode_cancel_finish");
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), NEXT);
        assert!(p.read(&id(11)).await.unwrap().bytes().is_empty());
        assert!(p.owned_removal(&id(10)).await.unwrap().is_none());
        for edit in [5000, 5010, 5011] {
            p.flush_edit(&id(edit)).await.unwrap();
        }
        return;
    }
    checkpoint("FENCED_CHECKPOINT");
    std::future::pending::<()>().await;
}
