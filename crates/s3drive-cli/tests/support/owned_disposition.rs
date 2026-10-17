//! Actual HTTP/SQLite/SDK recovery; every endpoint and credential is a fixture.
use super::*;
use s3drive_authority::{NodeGuard, ROOT};
const DIRTY: &[u8] = b"dirty survives commit and restart\0\xff";
const PUBLISHED: &[u8] = b"one verified upload after cancelling disposition";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owned_dispositions_recover_lost_replies_and_killed_clients_with_exact_identity() {
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
    let setup_deletes = deletes();
    for mode in ["owned_arm", "owned_cancel", "owned_commit", "owned_finish"] {
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
        let error_task = tokio::spawn(async move {
            let mut s = String::new();
            errors.read_to_string(&mut s).await.unwrap();
            s
        });
        let protocol = tokio::time::timeout(Duration::from_secs(120), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                if line == "CRASH_OWNED_DISPOSITION" {
                    assert_eq!(puts(&f), usize::from(mode == "owned_commit"));
                    return true;
                }
            }
            false
        })
        .await;
        if mode != "owned_finish" || protocol.is_err() {
            child.kill().await.unwrap();
        }
        let status = tokio::time::timeout(Duration::from_secs(15), child.wait())
            .await
            .unwrap()
            .unwrap();
        let errors = error_task.await.unwrap();
        assert!(
            protocol.is_ok()
                && if mode == "owned_finish" {
                    status.success()
                } else {
                    protocol.unwrap() && !status.success()
                },
            "{mode}: {errors}"
        );
    }
    assert_eq!(puts(&f), 1, "no upload during arm/cancel/commit or replay");
    assert_eq!(
        deletes(),
        setup_deletes,
        "the managed workflow must never delete S3 objects"
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
async fn input(
    p: &Publisher,
    w: &WireClient,
    op: u64,
    before: &Node,
    mutation: Mutation,
) -> s3drive_state::namespace_client::Input {
    p.namespace_change_input(
        Apply {
            volume: info(w).await.id,
            operation: id(op),
            expected_sequence: info(w).await.sequence,
            mutation,
        },
        std::slice::from_ref(before),
    )
    .unwrap()
}
pub(super) async fn run_mode(
    mode: &str,
    config: &s3drive_core::Config,
    root: &Path,
    w: &WireClient,
    volume: &str,
) {
    if mode == "owned_arm" {
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
        change(
            w,
            10,
            Mutation::CreateEntry {
                parent: ROOT.into(),
                parent_revision: 0,
                store: Some(id(200)),
                name: "owned 資料 café 🦀".into(),
                node_kind: Kind::EmptyFile,
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
    let wire = WireClient::loopback(&endpoint, Token::new(SECRET.into()).unwrap()).unwrap();
    let p = publisher(config, root, "owned-client", wire, volume.into()).await;
    let owner = p.client_identity().await.unwrap();
    if mode == "owned_arm" {
        std::fs::write(root.join("owner"), &owner).unwrap();
    } else {
        assert_eq!(std::fs::read_to_string(root.join("owner")).unwrap(), owner);
    }
    let logical = p.storage().logical();
    if mode != "owned_arm" {
        let pending = p.storage().namespace().pending(None, 100).await.unwrap();
        assert_eq!(pending.len(), 1);
        let input = pending[0].input.clone();
        let before = info(w).await.sequence;
        let receipt = p.change_namespace(input.clone()).await.unwrap();
        assert_eq!(
            info(w).await.sequence,
            before,
            "exact replay must not repeat authority mutation"
        );
        assert_eq!(p.change_namespace(input).await.unwrap(), receipt);
    }
    if mode == "owned_finish" {
        assert!(p.owned_removal(&id(10)).await.unwrap().is_none());
        let file = logical.file(volume.into(), id(10)).await.unwrap();
        assert!(file.base.deleted);
        assert_ne!(file.head, file.settled);
        assert_eq!(
            p.storage()
                .staging()
                .snapshot(file.head.unwrap())
                .await
                .unwrap()
                .read(0, 100)
                .await
                .unwrap(),
            DIRTY
        );
        assert!(p.read(&id(10)).await.is_err());
        assert!(
            p.storage()
                .namespace()
                .pending(None, 100)
                .await
                .unwrap()
                .is_empty()
        );
        p.storage().close().await.unwrap();
        stop.send(()).unwrap();
        server.await.unwrap();
        return;
    }
    if mode == "owned_commit" {
        assert!(p.owned_removal(&id(10)).await.unwrap().is_none());
        let file = logical.file(volume.into(), id(10)).await.unwrap();
        p.stage_edit(
            &id(5000),
            Apply {
                volume: volume.into(),
                operation: id(6000),
                expected_sequence: info(w).await.sequence,
                mutation: Mutation::ReserveContent {
                    node: id(10),
                    expected_revision: file.base.revision,
                    store: id(200),
                    size: PUBLISHED.len() as u64,
                    sha256: sha(PUBLISHED),
                    expected_payload: s3drive_authority::payload_reference(&file.base.payload),
                },
            },
            id(7000),
        )
        .await
        .unwrap();
        p.flush_edit(&id(5000)).await.unwrap();
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), PUBLISHED);
        let file = logical.file(volume.into(), id(10)).await.unwrap();
        p.stage_local(
            s3drive_state::logical::Input::from_bytes(id(5001), &file.binding, file.head, DIRTY),
            Arc::from(DIRTY),
        )
        .await
        .unwrap();
        let i = input(
            &p,
            w,
            8002,
            &file.base,
            Mutation::ArmRemoval {
                node: guard(&file.base),
                parent: parent(),
                owner: owner.clone(),
            },
        )
        .await;
        p.change_namespace(i).await.unwrap();
    }
    let file = if mode == "owned_arm" {
        let file = p.bind_edit_file(&id(10)).await.unwrap();
        p.stage_local(
            s3drive_state::logical::Input::from_bytes(
                id(5000),
                &file.binding,
                file.head,
                PUBLISHED,
            ),
            Arc::from(PUBLISHED),
        )
        .await
        .unwrap();
        file
    } else {
        logical.file(volume.into(), id(10)).await.unwrap()
    };
    let mutation = match mode {
        "owned_arm" => Mutation::ArmRemoval {
            node: guard(&file.base),
            parent: parent(),
            owner,
        },
        "owned_cancel" => Mutation::CancelRemoval {
            node: id(10),
            fence: p.owned_removal(&id(10)).await.unwrap().unwrap().fence,
        },
        _ => Mutation::CommitRemoval {
            node: guard(&file.base),
            parent: parent(),
            fence: p.owned_removal(&id(10)).await.unwrap().unwrap().fence,
        },
    };
    let (op, kind) = match mode {
        "owned_arm" => (8000, 4),
        "owned_cancel" => (8001, 5),
        _ => (8003, 6),
    };
    let i = input(&p, w, op, &file.base, mutation).await;
    loss.store(kind, Ordering::SeqCst);
    assert!(p.change_namespace(i).await.is_err());
    assert_eq!(
        loss.load(Ordering::SeqCst),
        0,
        "proxy must drop a committed reply"
    );
    assert_eq!(
        logical.file(volume.into(), id(10)).await.unwrap().base,
        file.base,
        "lost reply must not change local observation"
    );
    assert_eq!(
        p.storage()
            .namespace()
            .pending(None, 100)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(!server.is_finished());
    checkpoint("CRASH_OWNED_DISPOSITION");
    std::future::pending::<()>().await;
}
