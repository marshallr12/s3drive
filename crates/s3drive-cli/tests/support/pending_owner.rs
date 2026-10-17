//! One-shot SDK PUTs, actual authority HTTP and killed private SQLite owners.
use super::*;
use s3drive_authority::{NodeGuard, ROOT, RemovalFence, payload_reference};
const BYTES: &[u8] = b"pending owner binary bytes\0\xff";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fenced_owner_writes_recover_each_lost_reply_then_cancel_or_commit_exact_payloads() {
    run_children(
        &[
            "fenced_stage",
            "fenced_upload",
            "fenced_metadata",
            "fenced_cancel",
            "fenced_finish",
        ],
        &[0, 1, 1, 2, 2],
    )
    .await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fenced_confirmed_conflicts_require_explicit_retention_before_closing_and_preserve_all_bytes()
 {
    run_children(
        &[
            "fenced_conflict_detect",
            "fenced_conflict_stage",
            "fenced_conflict_finish",
        ],
        &[1, 2, 3],
    )
    .await;
}
async fn run_children(modes: &[&str], expected_puts: &[usize]) {
    let f = fixture::Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let setup = writes::setup(&f, tmp.path()).await;
    let deletes = || {
        f.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.starts_with("DELETE "))
            .count()
    };
    let baseline = deletes();
    for (index, mode) in modes.iter().enumerate() {
        if *mode == "fenced_upload" {
            f.hide_committed.store(true, Ordering::SeqCst);
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "managed_child", "--nocapture"])
            .env("S3DRIVE_MANAGED_MODE", mode)
            .env("S3DRIVE_MANAGED_CONFIG", &setup.config)
            .env("S3DRIVE_MANAGED_ROOT", tmp.path())
            .env("AWS_SHARED_CREDENTIALS_FILE", &setup.credentials)
            .env("AWS_CONFIG_FILE", &setup.aws_config)
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut errors = child.stderr.take().unwrap();
        let errors = tokio::spawn(async move {
            let mut s = String::new();
            errors.read_to_string(&mut s).await.unwrap();
            s
        });
        let protocol = tokio::time::timeout(Duration::from_secs(120), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                if line == "INJECT_PHYSICAL_REJECTION" {
                    f.race_create.store(true, Ordering::SeqCst);
                    rusqlite::Connection::open(tmp.path().join("fenced-client/metadata.sqlite3")).unwrap()
                        .execute_batch("CREATE TRIGGER fail_retention BEFORE INSERT ON managed_rejections BEGIN SELECT RAISE(IGNORE); END").unwrap();
                    signal(tmp.path(), "inject-physical");
                }
                if line == "FENCED_CHECKPOINT" {
                    assert_eq!(puts(&f), expected_puts[index]);
                    return true;
                }
            }
            false
        })
        .await;
        let last = index + 1 == modes.len();
        if !last || protocol.is_err() {
            child.kill().await.unwrap();
        }
        let status = tokio::time::timeout(Duration::from_secs(15), child.wait())
            .await
            .unwrap()
            .unwrap();
        let errors = errors.await.unwrap();
        assert!(
            protocol.is_ok()
                && if last {
                    status.success()
                } else {
                    protocol.unwrap() && !status.success()
                },
            "{mode}: {errors}"
        );
        if *mode == "fenced_conflict_detect" {
            rusqlite::Connection::open(tmp.path().join("fenced-client/metadata.sqlite3"))
                .unwrap()
                .execute_batch("DROP TRIGGER fail_retention")
                .unwrap();
        }
        f.deny_head.store(false, Ordering::SeqCst);
        f.deny_get.store(false, Ordering::SeqCst);
    }
    assert_eq!(puts(&f), *expected_puts.last().unwrap());
    assert_eq!(deletes(), baseline, "no managed S3 DELETE");
    let count = f
        .objects
        .lock()
        .unwrap()
        .keys()
        .filter(|key| key.contains("/writable/.s3drive/"))
        .count();
    assert_eq!(
        count,
        if modes[0] == "fenced_stage" { 2 } else { 3 },
        "conflicting/orphan objects stay retained"
    );
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
async fn request(w: &WireClient, op: u64, mutation: Mutation) -> Apply {
    let v = info(w).await;
    Apply {
        volume: v.id,
        operation: id(op),
        expected_sequence: v.sequence,
        mutation,
    }
}
async fn disposition(
    p: &Publisher,
    w: &WireClient,
    op: u64,
    before: &Node,
    mutation: Mutation,
) -> s3drive_state::namespace_client::Record {
    let input = p
        .namespace_change_input(request(w, op, mutation).await, std::slice::from_ref(before))
        .unwrap();
    p.change_namespace(input).await.unwrap()
}
async fn arm(p: &Publisher, w: &WireClient, n: u64, op: u64) -> RemovalFence {
    let file = p
        .storage()
        .logical()
        .file(info(w).await.id, id(n))
        .await
        .unwrap();
    let owner = p.client_identity().await.unwrap();
    disposition(
        p,
        w,
        op,
        &file.base,
        Mutation::ArmRemoval {
            node: guard(&file.base),
            parent: parent(),
            owner,
        },
    )
    .await;
    p.owned_removal(&id(n)).await.unwrap().unwrap().fence
}
async fn close(p: &Publisher, w: &WireClient, n: u64, op: u64, commit: bool) {
    let file = p
        .storage()
        .logical()
        .file(info(w).await.id, id(n))
        .await
        .unwrap();
    let fence = p.owned_removal(&id(n)).await.unwrap().unwrap().fence;
    let mutation = if commit {
        Mutation::CommitRemoval {
            node: guard(&file.base),
            parent: parent(),
            fence,
        }
    } else {
        Mutation::CancelRemoval { node: id(n), fence }
    };
    disposition(p, w, op, &file.base, mutation).await;
    assert!(p.owned_removal(&id(n)).await.unwrap().is_none());
}
async fn stage(p: &Publisher, w: &WireClient, n: u64, edit: u64, bytes: &[u8]) {
    let file = p
        .storage()
        .logical()
        .file(info(w).await.id, id(n))
        .await
        .unwrap();
    p.stage_local(
        s3drive_state::logical::Input::from_bytes(id(edit), &file.binding, file.head, bytes),
        Arc::from(bytes),
    )
    .await
    .unwrap();
}
async fn prepare(p: &Publisher, w: &WireClient, n: u64, edit: u64, op: u64, pending: bool) {
    let local = p.storage().logical();
    let r = local.get(id(edit)).await.unwrap();
    let file = local.file(r.input.volume, r.input.node).await.unwrap();
    let expected_payload = payload_reference(&file.base.payload);
    let mutation = if pending {
        Mutation::ReservePendingContent {
            node: id(n),
            expected_revision: file.base.revision,
            store: id(200),
            size: r.input.size,
            sha256: r.input.sha256,
            expected_payload,
            fence: p.owned_removal(&id(n)).await.unwrap().unwrap().fence,
        }
    } else {
        Mutation::ReserveContent {
            node: id(n),
            expected_revision: file.base.revision,
            store: id(200),
            size: r.input.size,
            sha256: r.input.sha256,
            expected_payload,
        }
    };
    p.stage_edit(&id(edit), request(w, op, mutation).await, id(op + 1000))
        .await
        .unwrap();
}
async fn cannot_close_unknown(p: &Publisher, w: &WireClient, n: u64, op: u64, edit: u64) {
    assert!(p.retain_edit_conflict(&id(edit)).await.is_err());
    cannot_close(p, w, n, op).await;
}
async fn cannot_close(p: &Publisher, w: &WireClient, n: u64, op: u64) {
    let file = p
        .storage()
        .logical()
        .file(info(w).await.id, id(n))
        .await
        .unwrap();
    let fence = p.owned_removal(&id(n)).await.unwrap().unwrap().fence;
    let input = p
        .namespace_change_input(
            request(w, op, Mutation::CancelRemoval { node: id(n), fence }).await,
            std::slice::from_ref(&file.base),
        )
        .unwrap();
    assert!(p.change_namespace(input).await.is_err());
    assert!(
        p.storage()
            .namespace()
            .pending(None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        ask(
            w,
            Request::Operation {
                volume: info(w).await.id,
                operation: id(op)
            }
        )
        .await,
        Reply::Operation(None)
    );
}
pub(super) async fn run_mode(
    mode: &str,
    config: &s3drive_core::Config,
    root: &Path,
    w: &WireClient,
    volume: &str,
) {
    if mode == "fenced_stage" || mode == "fenced_conflict_detect" {
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
        for (n, name) in [
            (10, "fenced café 日本語 🦀"),
            (11, "empty 資料 e\u{301} 🦀"),
        ] {
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
        "fenced-client",
        WireClient::loopback(&endpoint, Token::new(SECRET.into()).unwrap()).unwrap(),
        volume.into(),
    )
    .await;
    let owner = p.client_identity().await.unwrap();
    if mode == "fenced_stage" || mode == "fenced_conflict_detect" {
        std::fs::write(root.join("fenced-owner"), &owner).unwrap();
    } else {
        assert_eq!(
            std::fs::read_to_string(root.join("fenced-owner")).unwrap(),
            owner
        );
    }
    if mode.starts_with("fenced_conflict_") {
        Box::pin(conflict_mode(mode, &p, w, volume, root, &loss)).await;
    } else {
        Box::pin(success_mode(mode, &p, w, volume, &loss)).await;
    }
    p.storage().close().await.unwrap();
    stop.send(()).unwrap();
    server.await.unwrap();
}
async fn success_mode(mode: &str, p: &Publisher, w: &WireClient, volume: &str, loss: &AtomicU8) {
    if mode == "fenced_stage" {
        p.bind_edit_file(&id(10)).await.unwrap();
        arm(p, w, 10, 8000).await;
        stage(p, w, 10, 5000, BYTES).await;
        prepare(p, w, 10, 5000, 6000, true).await;
        loss.store(1, Ordering::SeqCst);
        assert!(p.flush_edit(&id(5000)).await.is_err());
        assert_eq!(loss.load(Ordering::SeqCst), 0);
        assert!(
            p.storage()
                .managed()
                .get(id(6000))
                .await
                .unwrap()
                .reservation
                .is_none()
        );
    } else if mode == "fenced_upload" {
        assert!(p.flush_edit(&id(5000)).await.is_err());
        let record = p.storage().managed().get(id(6000)).await.unwrap();
        assert!(record.reservation.is_some() && record.rejection.is_none());
        cannot_close_unknown(p, w, 10, 8900, 5000).await;
    } else if mode == "fenced_metadata" {
        loss.store(2, Ordering::SeqCst);
        assert!(p.flush_edit(&id(5000)).await.is_err());
        assert_eq!(loss.load(Ordering::SeqCst), 0);
        let record = p.storage().managed().get(id(6000)).await.unwrap();
        assert!(
            record.publication.is_some() && record.receipt.is_none() && record.rejection.is_none()
        );
        cannot_close_unknown(p, w, 10, 8901, 5000).await;
    } else if mode == "fenced_cancel" {
        p.flush_edit(&id(5000)).await.unwrap();
        let sequence = info(w).await.sequence;
        p.flush_edit(&id(5000)).await.unwrap();
        assert_eq!(info(w).await.sequence, sequence);
        assert!(
            p.read(&id(10)).await.is_err(),
            "new opens stay denied while pending"
        );
        close(p, w, 10, 8001, false).await;
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), BYTES);
        p.bind_edit_file(&id(11)).await.unwrap();
        arm(p, w, 11, 8011).await;
        stage(p, w, 11, 5011, b"").await;
        prepare(p, w, 11, 5011, 6011, true).await;
        p.flush_edit(&id(5011)).await.unwrap();
        close(p, w, 11, 8012, true).await;
    } else {
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), BYTES);
        let empty = p
            .storage()
            .logical()
            .file(volume.into(), id(11))
            .await
            .unwrap();
        assert!(empty.base.deleted);
        assert_eq!(empty.base.payload.as_ref().unwrap().reservation.size, 0);
        assert_eq!(empty.head, empty.settled);
        assert!(
            p.storage()
                .staging()
                .snapshot(empty.head.unwrap())
                .await
                .unwrap()
                .read(0, 1)
                .await
                .unwrap()
                .is_empty()
        );
        p.flush_edit(&id(5000)).await.unwrap();
        p.flush_edit(&id(5011)).await.unwrap();
        assert!(p.read(&id(11)).await.is_err());
        return;
    }
    checkpoint("FENCED_CHECKPOINT");
    std::future::pending::<()>().await;
}
async fn conflict_mode(
    mode: &str,
    p: &Publisher,
    w: &WireClient,
    volume: &str,
    root: &Path,
    loss: &AtomicU8,
) {
    if mode == "fenced_conflict_detect" {
        for n in [10, 11] {
            p.bind_edit_file(&id(n)).await.unwrap();
            arm(p, w, n, 8000 + n).await;
            stage(p, w, n, 5000 + n, BYTES).await;
            prepare(p, w, n, 5000 + n, 6000 + n, true).await;
        }
        checkpoint("INJECT_PHYSICAL_REJECTION");
        wait(root, "inject-physical").await;
        assert!(p.flush_edit(&id(5010)).await.is_err());
        let record = p.storage().managed().get(id(6010)).await.unwrap();
        assert!(
            record.rejection.is_none(),
            "injected ignored INSERT must retain an unknown local result"
        );
        let physical = p
            .storage()
            .uploads()
            .for_generation(record.generation.unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(physical.phase, s3drive_state::remote::Phase::Rejected);
        cannot_close_unknown(p, w, 10, 8899, 5010).await;
        checkpoint("FENCED_CHECKPOINT");
        std::future::pending::<()>().await;
    } else if mode == "fenced_conflict_stage" {
        assert!(p.flush_edit(&id(5010)).await.is_err());
        assert!(matches!(
            p.storage().managed().get(id(6010)).await.unwrap().rejection,
            Some(s3drive_state::managed::Rejection::Physical { status: 412, .. })
        ));
        cannot_close(p, w, 10, 8900).await; // confirmed is still not explicitly retained
    } else {
        for edit in [5010, 5011] {
            assert!(
                p.storage()
                    .logical()
                    .retained_conflict(id(edit))
                    .await
                    .unwrap()
                    .is_some()
            );
            assert!(p.flush_edit(&id(edit)).await.is_err());
        }
        close(p, w, 10, 9000, false).await;
        stage(p, w, 10, 5100, BYTES).await;
        prepare(p, w, 10, 5100, 6100, false).await;
        p.flush_edit(&id(5100)).await.unwrap();
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), BYTES);
        close(p, w, 11, 9001, true).await;
        let file = p
            .storage()
            .logical()
            .file(volume.into(), id(11))
            .await
            .unwrap();
        assert!(file.base.deleted && file.base.payload.is_none() && file.settled.is_none());
        assert_eq!(
            p.storage()
                .staging()
                .snapshot(file.head.unwrap())
                .await
                .unwrap()
                .read(0, 100)
                .await
                .unwrap(),
            BYTES
        );
        assert!(
            p.storage()
                .logical()
                .retained_conflict(id(5010))
                .await
                .unwrap()
                .is_some()
        );
        return;
    }
    // Explicit retention is separate from detecting the definitive rejection.
    p.retain_edit_conflict(&id(5010)).await.unwrap();
    // Simulate an inconsistent/future sequence observation at the HTTP service.
    // The real authority permanently rejects the exact frozen publication after
    // the SDK verifies its immutable blob. The orphan must stay retained.
    loss.store(7, Ordering::SeqCst);
    assert!(p.flush_edit(&id(5011)).await.is_err());
    assert_eq!(loss.load(Ordering::SeqCst), 0);
    let record = p.storage().managed().get(id(6011)).await.unwrap();
    let Some(s3drive_state::managed::Rejection::Authority {
        request,
        error: s3drive_authority::Error::Conflict,
    }) = record.rejection
    else {
        panic!()
    };
    assert!(matches!(request.mutation, Mutation::Publish { .. }));
    p.retain_edit_conflict(&id(5011)).await.unwrap();
    checkpoint("FENCED_CHECKPOINT");
    std::future::pending::<()>().await;
}
