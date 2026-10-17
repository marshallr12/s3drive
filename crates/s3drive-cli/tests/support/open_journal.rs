//! Durable open facts through the actual shared worker, SDK and HTTP authority.
use super::pending_owner::{prepare, request, stage};
use super::*;
use s3drive_authority::{NodeGuard, ROOT, RemovalMode};
use s3drive_state::namespace_client::opens::{Callback, OpenInput};
const FIRST: &[u8] = b"open snapshot binary\0\xff";
const NEXT: &[u8] = b"post-cleanup binary\0\xfe";
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn open_journal_killed_owners_recover_admission_cleanup_close_and_exact_namespace_proofs() {
    pending_owner::run_children_retaining(
        &[
            "open_journal_start",
            "open_journal_promote",
            "open_journal_write",
            "open_journal_commit",
            "open_journal_finish",
        ],
        &[0, 2, 3, 3, 3],
        &[FIRST, NEXT, b""],
    )
    .await;
}
fn guard(n: &Node) -> NodeGuard {
    NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    }
}
async fn frozen_open(p: &Publisher, w: &WireClient, n: u64) -> OpenInput {
    let owner = p.client_identity().await.unwrap();
    let admission = p
        .namespace_change_input(
            request(
                w,
                n,
                Mutation::CreateDeferredEntry {
                    parent: ROOT.into(),
                    parent_revision: 0,
                    store: Some(id(200)),
                    name: format!("open café 日本語 🦀 {n}"),
                    node_kind: Kind::EmptyFile,
                    owner,
                },
            )
            .await,
            &[],
        )
        .unwrap();
    OpenInput {
        id: id(1000 + n),
        session: id(2000),
        volume: admission.request.volume.clone(),
        node: id(n),
        store: admission.store.clone(),
        source: admission.source,
        fingerprint: admission.fingerprint.clone(),
        delete_on_close: true,
        admission: Some(Box::new(admission)),
    }
}
async fn mutation(
    p: &Publisher,
    w: &WireClient,
    n: u64,
    op: u64,
    commit: bool,
) -> s3drive_state::namespace_client::Input {
    let before = p
        .storage()
        .logical()
        .file(info(w).await.id, id(n))
        .await
        .unwrap()
        .base;
    let owned = p.owned_removal(&id(n)).await.unwrap().unwrap();
    let m = if commit {
        Mutation::CommitRemoval {
            node: guard(&before),
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            fence: owned.fence,
        }
    } else {
        Mutation::TransitionRemoval {
            node: guard(&before),
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            fence: owned.fence,
            expected: owned.mode,
            mode: RemovalMode::Pending,
        }
    };
    p.namespace_change_input(request(w, op, m).await, std::slice::from_ref(&before))
        .unwrap()
}
pub(super) async fn run_mode(
    mode: &str,
    config: &s3drive_core::Config,
    root: &Path,
    w: &WireClient,
    volume: &str,
) {
    if mode == "open_journal_start" {
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
        "open-journal-client",
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
    _root: &Path,
    volume: &str,
    loss: &AtomicU8,
) {
    let opens = p.storage().opens();
    if mode == "open_journal_start" {
        for n in [10, 11] {
            let input = frozen_open(p, w, n).await;
            opens.prepare(input.clone()).await.unwrap();
            assert!(
                opens
                    .activate(input.id.clone(), id(3000 + n))
                    .await
                    .is_err()
            );
            loss.store(8, Ordering::SeqCst);
            assert!(p.change_namespace(*input.admission.unwrap()).await.is_err());
            assert_eq!(loss.load(Ordering::SeqCst), 0);
        }
    } else if mode == "open_journal_promote" {
        for (n, edit, bytes) in [(10, 5000, FIRST), (11, 5001, b"".as_slice())] {
            let r = opens.get(id(1000 + n)).await.unwrap();
            assert!(!r.active());
            p.change_namespace(*r.input.admission.unwrap())
                .await
                .unwrap();
            opens.activate(id(1000 + n), id(3000 + n)).await.unwrap();
            p.bind_edit_file(&id(n)).await.unwrap();
            stage(p, w, n, edit, bytes).await;
            prepare(p, w, n, edit, edit + 1000, true).await;
            p.flush_edit(&id(edit)).await.unwrap();
        }
        let original = opens.get(id(1010)).await.unwrap();
        let mut reader = original.input;
        reader.id = id(1012);
        reader.delete_on_close = false;
        reader.admission = None;
        opens.prepare(reader).await.unwrap();
        opens.activate(id(1012), id(3012)).await.unwrap();
        let retained = p.read(&id(10)).await.unwrap();
        assert_eq!(retained.bytes(), FIRST);
        opens
            .callback(id(1010), id(3100), Callback::SetDelete { delete: true })
            .await
            .unwrap();
        let i = mutation(p, w, 10, 8100, false).await;
        opens
            .link(id(1010), id(3200), id(3100), i.clone())
            .await
            .unwrap();
        loss.store(7, Ordering::SeqCst);
        assert!(p.change_namespace(i).await.is_err());
        assert_eq!(loss.load(Ordering::SeqCst), 0);
        assert!(
            opens
                .acknowledge(id(1010), id(3400), id(8100))
                .await
                .is_err()
        );
        opens
            .callback(id(1010), id(3300), Callback::Cleanup { flags: 1 })
            .await
            .unwrap();
        opens
            .callback(id(1010), id(3301), Callback::Close)
            .await
            .unwrap();
        assert_eq!(retained.bytes(), FIRST);
    } else if mode == "open_journal_write" {
        let original = opens.get(id(1010)).await.unwrap();
        assert!(original.cleanup().is_some() && original.close().is_some());
        let i = original.pending()[0].clone();
        p.change_namespace(i).await.unwrap();
        opens
            .acknowledge(id(1010), id(3400), id(8100))
            .await
            .unwrap();
        assert_eq!(opens.abandoned(id(2001)).await.unwrap().len(), 2);
        let mut new = opens.get(id(1012)).await.unwrap().input;
        new.id = id(1013);
        assert!(opens.prepare(new).await.is_err());
        stage(p, w, 10, 5002, NEXT).await;
        prepare(p, w, 10, 5002, 6002, true).await;
        loss.store(2, Ordering::SeqCst);
        assert!(p.flush_edit(&id(5002)).await.is_err());
        assert_eq!(loss.load(Ordering::SeqCst), 0);
        opens
            .callback(id(1012), id(3312), Callback::Cleanup { flags: 1 })
            .await
            .unwrap();
        opens
            .callback(id(1012), id(3313), Callback::Close)
            .await
            .unwrap();
        opens
            .callback(id(1011), id(3311), Callback::Cleanup { flags: 1 })
            .await
            .unwrap();
        let i = mutation(p, w, 11, 8111, false).await;
        opens
            .link(id(1011), id(3211), id(3311), i.clone())
            .await
            .unwrap();
        p.change_namespace(i).await.unwrap();
        opens
            .acknowledge(id(1011), id(3411), id(8111))
            .await
            .unwrap();
        opens
            .callback(id(1011), id(3511), Callback::Close)
            .await
            .unwrap();
        let i = mutation(p, w, 11, 8211, true).await;
        opens
            .link(id(1011), id(3611), id(3511), i.clone())
            .await
            .unwrap();
        p.change_namespace(i).await.unwrap();
        opens
            .acknowledge(id(1011), id(3711), id(8211))
            .await
            .unwrap();
    } else if mode == "open_journal_commit" {
        p.flush_edit(&id(5002)).await.unwrap();
        let file = p
            .storage()
            .logical()
            .file(volume.into(), id(10))
            .await
            .unwrap();
        assert_eq!(file.head, file.settled);
        assert_eq!(
            file.base.payload.as_ref().unwrap().reservation.sha256,
            Sha256::digest(NEXT)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let i = mutation(p, w, 10, 8200, true).await;
        opens
            .link(id(1012), id(3600), id(3313), i.clone())
            .await
            .unwrap();
        loss.store(6, Ordering::SeqCst);
        assert!(p.change_namespace(i).await.is_err());
        assert_eq!(loss.load(Ordering::SeqCst), 0);
        assert!(
            opens
                .acknowledge(id(1012), id(3700), id(8200))
                .await
                .is_err()
        );
    } else {
        assert_eq!(mode, "open_journal_finish");
        let r = opens.get(id(1012)).await.unwrap();
        assert!(r.close().is_some());
        assert_eq!(r.pending().len(), 1);
        p.change_namespace(r.pending()[0].clone()).await.unwrap();
        opens
            .acknowledge(id(1012), id(3700), id(8200))
            .await
            .unwrap();
        assert!(opens.abandoned(id(2001)).await.unwrap().is_empty());
        let records = opens.records().await.unwrap();
        assert_eq!(records.len(), 3);
        for r in records {
            assert!(
                r.active()
                    && r.cleanup().is_some()
                    && r.close().is_some()
                    && r.pending().is_empty()
            );
        }
        for n in [10, 11] {
            assert!(p.owned_removal(&id(n)).await.unwrap().is_none());
            assert!(p.read(&id(n)).await.is_err());
            let file = p
                .storage()
                .logical()
                .file(volume.into(), id(n))
                .await
                .unwrap();
            assert!(file.base.deleted);
        }
        for edit in [5000, 5001, 5002] {
            p.flush_edit(&id(edit)).await.unwrap();
        }
        return;
    }
    checkpoint("FENCED_CHECKPOINT");
    std::future::pending::<()>().await;
}
