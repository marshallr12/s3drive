//! Real SDK, authority HTTP and killed owners across identity changes.
use super::pending_owner::{arm, prepare, request, stage};
use super::*;
use s3drive_authority::{NodeGuard, ROOT, RemovalMode};
const FIRST: &[u8] = b"original binary\0\xff";
const NEXT: &[u8] = b"moved binary\0\xfe";
const LATE: &[u8] = b"dirty during lost move\0\xfb";
const FINAL: &[u8] = b"replacement binary\0\xfd";
const RIVAL: &[u8] = b"retained target binary\0\xfc";
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deferred_owner_moves_recover_killed_clients_replacement_and_late_receipts_without_put_replay()
 {
    pending_owner::run_children_retaining(
        &[
            "owner_move_start",
            "owner_move_recover",
            "owner_move_publish",
            "owner_move_finish",
        ],
        &[4, 5, 6, 6],
        &[FIRST, NEXT, LATE, FINAL, RIVAL, b""],
    )
    .await;
}
fn guard(n: &Node) -> NodeGuard {
    NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    }
}
fn saved(root: &Path, label: &str) -> s3drive_state::namespace_client::Input {
    serde_json::from_slice(&std::fs::read(root.join(format!("{label}.json"))).unwrap()).unwrap()
}
async fn movement(
    p: &Publisher,
    w: &WireClient,
    n: u64,
    op: u64,
    parent: u64,
    name: &str,
    target: Option<u64>,
) -> s3drive_state::namespace_client::Input {
    let volume = info(w).await.id;
    let before = node(w, &volume, n).await;
    let source = node(
        w,
        &volume,
        u64::from_str_radix(before.parent.as_ref().unwrap(), 16).unwrap(),
    )
    .await;
    let parent = node(w, &volume, parent).await;
    let old = p.owned_removal(&id(n)).await.unwrap().unwrap();
    let mut entries = vec![before.clone()];
    let destination = if let Some(t) = target {
        let t = node(w, &volume, t).await;
        if t.id != before.id {
            entries.push(t.clone());
        }
        Some(guard(&t))
    } else {
        None
    };
    p.namespace_change_input(
        request(
            w,
            op,
            Mutation::MoveDeferredEntry {
                node: guard(&before),
                source_parent: guard(&source),
                parent: guard(&parent),
                name: name.into(),
                destination,
                fence: Box::new(old.fence),
                previous: old.event.into(),
            },
        )
        .await,
        &entries,
    )
    .unwrap()
}
async fn save_lost(
    p: &Publisher,
    root: &Path,
    label: &str,
    i: s3drive_state::namespace_client::Input,
    loss: &AtomicU8,
) {
    std::fs::write(
        root.join(format!("{label}.json")),
        serde_json::to_vec(&i).unwrap(),
    )
    .unwrap();
    loss.store(9, Ordering::SeqCst);
    assert!(p.change_namespace(i).await.is_err());
    assert_eq!(loss.load(Ordering::SeqCst), 0);
}
async fn close_node(
    p: &Publisher,
    w: &WireClient,
    n: u64,
    op: u64,
    commit: bool,
) -> s3drive_state::namespace_client::Input {
    let old = p.owned_removal(&id(n)).await.unwrap().unwrap();
    let volume = info(w).await.id;
    let before = if commit {
        old.receipt.node
    } else {
        node(w, &volume, n).await
    };
    let parent = node(
        w,
        &volume,
        u64::from_str_radix(before.parent.as_ref().unwrap(), 16).unwrap(),
    )
    .await;
    let m = if commit {
        Mutation::CommitRemoval {
            node: guard(&before),
            parent: guard(&parent),
            fence: old.fence,
        }
    } else {
        Mutation::CancelRemoval {
            node: id(n),
            fence: old.fence,
        }
    };
    let i = p
        .namespace_change_input(request(w, op, m).await, std::slice::from_ref(&before))
        .unwrap();
    p.change_namespace(i.clone()).await.unwrap();
    i
}
async fn promote(p: &Publisher, w: &WireClient, n: u64, op: u64) {
    let old = p.owned_removal(&id(n)).await.unwrap().unwrap();
    let before = node(w, &info(w).await.id, n).await;
    let parent = node(
        w,
        &info(w).await.id,
        u64::from_str_radix(before.parent.as_ref().unwrap(), 16).unwrap(),
    )
    .await;
    let i = p
        .namespace_change_input(
            request(
                w,
                op,
                Mutation::TransitionRemoval {
                    node: guard(&before),
                    parent: guard(&parent),
                    fence: old.fence,
                    expected: old.mode,
                    mode: RemovalMode::Pending,
                },
            )
            .await,
            std::slice::from_ref(&before),
        )
        .unwrap();
    p.change_namespace(i).await.unwrap();
}
pub(super) async fn run_mode(
    mode: &str,
    config: &s3drive_core::Config,
    root: &Path,
    w: &WireClient,
    volume: &str,
) {
    if mode == "owner_move_start" {
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
        "owner-move-client",
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
    if mode == "owner_move_start" {
        for (n, kind) in [
            (10, Kind::EmptyFile),
            (11, Kind::EmptyFile),
            (12, Kind::EmptyFile),
            (20, Kind::Directory),
            (21, Kind::Directory),
        ] {
            change(
                w,
                n,
                Mutation::CreateEntry {
                    parent: ROOT.into(),
                    parent_revision: 0,
                    store: (kind == Kind::EmptyFile).then(|| id(200)),
                    name: format!("source café 日本語 🦀 {n}"),
                    node_kind: kind,
                },
            )
            .await;
            if kind == Kind::EmptyFile {
                p.bind_edit_file(&id(n)).await.unwrap();
            }
        }
        let dir = node(w, volume, 21).await;
        change(
            w,
            22,
            Mutation::CreateEntry {
                parent: id(21),
                parent_revision: dir.revision,
                store: Some(id(200)),
                name: "child 🦀".into(),
                node_kind: Kind::EmptyFile,
            },
        )
        .await;
        for (n, edit, bytes) in [
            (11, 5011, RIVAL),
            (10, 5000, FIRST),
            (12, 5012, b"".as_slice()),
        ] {
            stage(p, w, n, edit, bytes).await;
            prepare(p, w, n, edit, edit + 1000, false).await;
            p.flush_edit(&id(edit)).await.unwrap();
        }
        arm(p, w, 10, 8000, true).await;
        arm(p, w, 12, 8002, true).await;
        let dir = node(w, volume, 21).await;
        let owner = p.client_identity().await.unwrap();
        let i = p
            .namespace_change_input(
                request(
                    w,
                    8021,
                    Mutation::PrepareRemoval {
                        node: guard(&dir),
                        parent: NodeGuard {
                            id: ROOT.into(),
                            revision: 0,
                        },
                        owner,
                    },
                )
                .await,
                std::slice::from_ref(&dir),
            )
            .unwrap();
        p.change_namespace(i).await.unwrap();
        stage(p, w, 10, 5001, NEXT).await;
        let dirty = movement(p, w, 10, 8100, 20, "moved binary café 🦀", None).await;
        assert!(p.change_namespace(dirty).await.is_err());
        prepare(p, w, 10, 5001, 6001, true).await;
        p.flush_edit(&id(5001)).await.unwrap();
        let i = movement(p, w, 10, 8100, 20, "moved binary café 🦀", None).await;
        save_lost(p, root, "first-move", i, loss).await;
        let old = p.owned_removal(&id(10)).await.unwrap().unwrap();
        assert_eq!(old.event, id(8000));
        let cancel = p
            .namespace_change_input(
                request(
                    w,
                    8999,
                    Mutation::CancelRemoval {
                        node: id(10),
                        fence: old.fence,
                    },
                )
                .await,
                std::slice::from_ref(&old.receipt.node),
            )
            .unwrap();
        assert!(p.change_namespace(cancel).await.is_err());
        let file = p
            .storage()
            .logical()
            .file(volume.into(), id(10))
            .await
            .unwrap();
        let edit =
            s3drive_state::logical::Input::from_bytes(id(5009), &file.binding, file.head, LATE);
        p.stage_local(edit, Arc::from(LATE)).await.unwrap();
        let edit = p.storage().logical().get(id(5009)).await.unwrap();
        let reserve = request(
            w,
            6009,
            Mutation::ReservePendingContent {
                node: id(10),
                expected_revision: file.base.revision,
                store: id(200),
                size: edit.input.size,
                sha256: edit.input.sha256,
                expected_payload: s3drive_authority::payload_reference(&file.base.payload),
                fence: p.owned_removal(&id(10)).await.unwrap().unwrap().fence,
            },
        )
        .await;
        assert!(
            p.stage_edit(&id(5009), reserve, id(7009)).await.is_err(),
            "unknown move blocks publication and preserves local dirty bytes"
        );
        let i = movement(p, w, 21, 8121, 20, "directory renamed 資料", None).await;
        save_lost(p, root, "directory-move", i, loss).await;
    } else if mode == "owner_move_recover" {
        for (label, n) in [("first-move", 10), ("directory-move", 21)] {
            let i = saved(root, label);
            p.change_namespace(i.clone()).await.unwrap();
            let old = p.owned_removal(&id(n)).await.unwrap().unwrap();
            assert_eq!(old.event, i.request.operation);
            assert_eq!(old.receipt.node.parent.as_deref(), Some(id(20).as_str()));
            assert_ne!(old.input.request.operation, old.event);
        }
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), NEXT);
        assert_eq!(
            node(w, volume, 22).await.parent.as_deref(),
            Some(id(21).as_str())
        );
        let file = p
            .storage()
            .logical()
            .file(volume.into(), id(10))
            .await
            .unwrap();
        assert_ne!(file.head, file.settled);
        prepare(p, w, 10, 5009, 6009, true).await;
        p.flush_edit(&id(5009)).await.unwrap();
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), LATE);
        let retained = p.read(&id(11)).await.unwrap();
        assert_eq!(retained.bytes(), RIVAL);
        let target = node(w, volume, 11).await;
        let i = movement(p, w, 10, 8200, 0, &target.name, Some(11)).await;
        save_lost(p, root, "replacement", i, loss).await;
        assert_eq!(retained.bytes(), RIVAL);
        let i = movement(p, w, 12, 8212, 20, "empty moved 🦀", None).await;
        p.change_namespace(i).await.unwrap();
        promote(p, w, 12, 8312).await;
        let i = close_node(p, w, 12, 8412, true).await;
        std::fs::write(
            root.join("empty-commit.json"),
            serde_json::to_vec(&i).unwrap(),
        )
        .unwrap();
    } else if mode == "owner_move_publish" {
        p.change_namespace(saved(root, "replacement"))
            .await
            .unwrap();
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), LATE);
        assert!(p.read(&id(11)).await.is_err());
        let before = p.owned_removal(&id(10)).await.unwrap().unwrap();
        p.change_namespace(saved(root, "first-move")).await.unwrap();
        assert_eq!(p.owned_removal(&id(10)).await.unwrap().unwrap(), before);
        stage(p, w, 10, 5002, FINAL).await;
        prepare(p, w, 10, 5002, 6002, true).await;
        loss.store(2, Ordering::SeqCst);
        assert!(p.flush_edit(&id(5002)).await.is_err());
        assert_eq!(loss.load(Ordering::SeqCst), 0);
    } else {
        assert_eq!(mode, "owner_move_finish");
        p.flush_edit(&id(5002)).await.unwrap();
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), FINAL);
        close_node(p, w, 10, 8500, false).await;
        arm(p, w, 10, 8501, true).await;
        let before = p.owned_removal(&id(10)).await.unwrap().unwrap();
        for label in ["first-move", "replacement"] {
            p.change_namespace(saved(root, label)).await.unwrap();
        }
        assert_eq!(p.owned_removal(&id(10)).await.unwrap().unwrap(), before);
        close_node(p, w, 10, 8502, false).await;
        close_node(p, w, 21, 8521, false).await;
        assert_eq!(
            node(w, volume, 21).await.parent.as_deref(),
            Some(id(20).as_str())
        );
        assert_eq!(
            node(w, volume, 22).await.parent.as_deref(),
            Some(id(21).as_str())
        );
        let empty = p
            .storage()
            .logical()
            .file(volume.into(), id(12))
            .await
            .unwrap();
        assert!(empty.base.deleted);
        assert_eq!(empty.base.payload.as_ref().unwrap().reservation.size, 0);
        let parent = node(w, volume, 20).await;
        let fresh = change(
            w,
            30,
            Mutation::CreateEntry {
                parent: id(20),
                parent_revision: parent.revision,
                store: Some(id(200)),
                name: "empty moved 🦀".into(),
                node_kind: Kind::EmptyFile,
            },
        )
        .await;
        let i = saved(root, "empty-commit");
        p.change_namespace(i).await.unwrap();
        assert_eq!(node(w, volume, 30).await, fresh.node);
        for edit in [5000, 5001, 5002, 5009, 5011, 5012] {
            p.flush_edit(&id(edit)).await.unwrap();
        }
        return;
    }
    checkpoint("FENCED_CHECKPOINT");
    std::future::pending::<()>().await;
}
