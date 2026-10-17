//! Real S3 SDK + actual authority HTTP + private SQLite. All cloud traffic is
//! confined to the owned loopback fixture with dummy credentials.
#[path = "support/fixture.rs"]
pub mod fixture;
#[path = "support/writes.rs"]
#[allow(dead_code)]
mod writes;
use s3drive_authority::{
    Apply, Authority, Kind, Mutation, Node, Reply, Request, Token, WireClient, serve_loopback,
};
use s3drive_managed::Publisher;
use s3drive_state::{cache, journal, worker::Storage};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
    sync::oneshot,
};
const SECRET: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
fn id(n: u64) -> String {
    format!("{n:032x}")
}
fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn puts(f: &fixture::Fixture) -> usize {
    f.requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.starts_with("PUT ") && r.contains("/writable/.s3drive/"))
        .count()
}
fn checkpoint(name: &str) {
    println!("{name}");
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
}
fn signal(root: &Path, name: &str) {
    std::fs::write(root.join(name), b"ready").unwrap();
}
async fn wait(root: &Path, name: &str) {
    tokio::time::timeout(Duration::from_secs(15), async {
        while !root.join(name).exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn managed_sdk_authority_races_and_process_death_recovery() {
    let f = fixture::Fixture::start().await;
    let temp = tempfile::tempdir().unwrap();
    let setup = writes::setup(&f, temp.path()).await;
    let mut before_restart = 0;
    let mut corrupted: Option<(String, Vec<u8>)> = None;
    for mode in ["initial", "resume", "finish"] {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "managed_child", "--nocapture"])
            .env("S3DRIVE_MANAGED_MODE", mode)
            .env("S3DRIVE_MANAGED_CONFIG", &setup.config)
            .env("S3DRIVE_MANAGED_ROOT", temp.path())
            .env("AWS_SHARED_CREDENTIALS_FILE", &setup.credentials)
            .env("AWS_CONFIG_FILE", &setup.aws_config)
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut errors = child.stderr.take().unwrap();
        let stderr = tokio::spawn(async move {
            let mut s = String::new();
            errors.read_to_string(&mut s).await.unwrap();
            s
        });
        let protocol = tokio::time::timeout(Duration::from_secs(120), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                match line.as_str() {
                    "LOST_REPLY" => {
                        f.lose_put_reply.store(true, Ordering::SeqCst);
                        signal(temp.path(), "lose");
                    }
                    "HIDE" => {
                        f.hide_committed.store(true, Ordering::SeqCst);
                        signal(temp.path(), "hide");
                    }
                    "CORRUPT" => {
                        let key = format!(
                            "/test-bucket/desktop/{}",
                            std::fs::read_to_string(temp.path().join("corrupt-key")).unwrap()
                        );
                        let mut objects = f.objects.lock().unwrap();
                        let bytes = &mut objects.get_mut(&key).unwrap().0;
                        corrupted = Some((key, bytes.clone()));
                        bytes[0] ^= 0xff;
                        drop(objects);
                        signal(temp.path(), "corrupt");
                    }
                    "RESTORE" => {
                        let (key, bytes) = corrupted.take().unwrap();
                        f.objects.lock().unwrap().get_mut(&key).unwrap().0 = bytes;
                        signal(temp.path(), "restored");
                    }
                    "CRASH_STAGE" | "CRASH_PUBLISH" => return true,
                    "RECONCILED" => assert_eq!(puts(&f), before_restart, "recovery reissued a PUT"),
                    _ => {}
                }
            }
            false
        })
        .await;
        let crash = protocol.as_ref().copied().unwrap_or(false);
        if crash || protocol.is_err() {
            child.kill().await.unwrap();
        }
        let status = tokio::time::timeout(Duration::from_secs(15), child.wait())
            .await
            .unwrap()
            .unwrap();
        let errors = stderr.await.unwrap();
        assert!(
            protocol.is_ok()
                && if mode == "finish" {
                    status.success() && !crash
                } else {
                    crash
                },
            "mode {mode}: {status}: {errors}"
        );
        before_restart = puts(&f);
        writes::restore_observation(&f);
    }
    assert_eq!(
        puts(&f),
        5,
        "each immutable generation must have exactly one PUT"
    );
    let objects = f.objects.lock().unwrap();
    let blobs = objects
        .iter()
        .filter(|(key, _)| key.contains("/writable/.s3drive/"))
        .collect::<Vec<_>>();
    assert_eq!(
        blobs.len(),
        5,
        "no losing, old or unlinked payload may be reclaimed"
    );
    assert!(blobs.iter().any(|(_, (bytes, _))| bytes.is_empty()));
    assert!(!f.server.is_finished());
}
async fn ask(w: &WireClient, r: Request) -> Reply {
    w.request(&r).await.unwrap()
}
async fn info(w: &WireClient) -> s3drive_authority::Volume {
    let Reply::Info(v) = ask(w, Request::Info {}).await else {
        panic!("info")
    };
    v
}
async fn node(w: &WireClient, volume: &str, n: u64) -> Node {
    let Reply::Node(n) = ask(
        w,
        Request::Node {
            volume: volume.into(),
            node: id(n),
        },
    )
    .await
    else {
        panic!("node")
    };
    n
}
async fn change(w: &WireClient, op: u64, m: Mutation) -> s3drive_authority::Receipt {
    let v = info(w).await;
    let Reply::Applied(r) = ask(
        w,
        Request::Apply {
            request: Apply {
                volume: v.id,
                operation: id(op),
                expected_sequence: v.sequence,
                mutation: m,
            },
        },
    )
    .await
    else {
        panic!("apply")
    };
    r
}
async fn stage(p: &Publisher, w: &WireClient, n: u64, op: u64, bytes: &[u8]) {
    let v = info(w).await;
    let n = node(w, &v.id, n).await;
    let request = Apply {
        volume: v.id,
        operation: id(op),
        expected_sequence: v.sequence,
        mutation: Mutation::Reserve {
            node: n.id,
            expected_revision: n.revision,
            store: p.store().id.clone(),
            size: bytes.len() as u64,
            sha256: sha(bytes),
        },
    };
    p.stage(request, id(op + 1000), Arc::from(bytes))
        .await
        .unwrap();
}
async fn publisher(
    config: &s3drive_core::Config,
    root: &Path,
    name: &str,
    wire: WireClient,
    volume: String,
) -> Publisher {
    let source = &config.sources[0];
    let writes = config.writes.as_ref().unwrap();
    let cfg = &writes.sources[0];
    let policy = s3drive_s3::uploads::WritePolicy::from_toml(
        source,
        &std::fs::read_to_string(&cfg.policy).unwrap(),
        &std::fs::read_to_string(&cfg.plan).unwrap(),
        &std::fs::read_to_string(&cfg.report).unwrap(),
    )
    .unwrap();
    let reader = s3drive_s3::S3Reader::from_source(source).await.unwrap();
    let storage = Storage::open(
        &root.join(name),
        cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap();
    let executor = s3drive_s3::uploads::Executor::open(reader, storage, policy)
        .await
        .unwrap();
    Publisher::open(executor, wire, volume, id(200))
        .await
        .unwrap()
}
#[derive(Clone)]
struct Proxy {
    upstream: WireClient,
    loss: Arc<AtomicU8>,
}
async fn proxy(
    axum::extract::State(p): axum::extract::State<Proxy>,
    axum::Json(request): axum::Json<Request>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let r = p.upstream.request(&request).await.unwrap();
    let kind = match request {
        Request::Apply {
            request:
                Apply {
                    mutation:
                        Mutation::Reserve { .. }
                        | Mutation::ReserveContent { .. }
                        | Mutation::ReserveDetachedContent { .. },
                    ..
                },
        } => 1,
        Request::Apply {
            request:
                Apply {
                    mutation: Mutation::Publish { .. },
                    ..
                },
        } => 2,
        Request::Apply {
            request:
                Apply {
                    mutation: Mutation::RemoveEntry { .. },
                    ..
                },
        } => 3,
        _ => 0,
    };
    if kind != 0
        && p.loss
            .compare_exchange(kind, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    axum::Json(r).into_response()
}
async fn serve(
    client: s3drive_authority::Client,
) -> (WireClient, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/namespace", listener.local_addr().unwrap());
    let (stop, receive) = oneshot::channel();
    let task = tokio::spawn(async move {
        serve_loopback(
            listener,
            client,
            Token::new(SECRET.into()).unwrap(),
            async {
                let _ = receive.await;
            },
        )
        .await
        .unwrap();
    });
    (
        WireClient::loopback(&endpoint, Token::new(SECRET.into()).unwrap()).unwrap(),
        stop,
        task,
    )
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn managed_child() {
    let Ok(mode) = std::env::var("S3DRIVE_MANAGED_MODE") else {
        return;
    };
    let root = PathBuf::from(std::env::var("S3DRIVE_MANAGED_ROOT").unwrap());
    let config = s3drive_core::Config::parse(
        &std::fs::read_to_string(std::env::var("S3DRIVE_MANAGED_CONFIG").unwrap()).unwrap(),
    )
    .unwrap();
    let authority = if mode.starts_with("detached_") {
        Authority::open_with_policy(root.join("authority"), "portable-collision-v1".into())
            .await
            .unwrap()
    } else {
        Authority::open(root.join("authority")).await.unwrap()
    };
    let (wire, stop, server) = serve(authority.client()).await;
    let volume = info(&wire).await.id;
    if mode.starts_with("detached_") {
        detached_mode(&mode, &config, &root, &wire, &volume).await;
        stop.send(()).unwrap();
        server.await.unwrap();
        authority.shutdown().await.unwrap();
        return;
    }
    if mode.starts_with("edits_") {
        logical_edits_mode(&mode, &config, &root, &wire, &volume).await;
        stop.send(()).unwrap();
        server.await.unwrap();
        authority.shutdown().await.unwrap();
        return;
    }
    if mode == "initial" {
        let reader = s3drive_s3::S3Reader::from_source(&config.sources[0])
            .await
            .unwrap();
        change(
            &wire,
            200,
            Mutation::RegisterStore {
                route: reader.storage_fingerprint(),
                root: "writable/.s3drive/".into(),
            },
        )
        .await;
        change(
            &wire,
            10,
            Mutation::Create {
                parent: s3drive_authority::ROOT.into(),
                name: "café 日本語 🦀.bin".into(),
                node_kind: Kind::EmptyFile,
            },
        )
        .await;
        let a = publisher(&config, &root, "client-a", wire.clone(), volume.clone()).await;
        let b = publisher(&config, &root, "client-b", wire.clone(), volume.clone()).await;
        let one = (0..65000).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let two = b"other machine\0\xff";
        stage(&a, &wire, 10, 1000, &one).await;
        a.reserve(&id(1000)).await.unwrap();
        stage(&b, &wire, 10, 1001, two).await;
        b.reserve(&id(1001)).await.unwrap();
        let first = id(1000);
        let second = id(1001);
        let (x, y) = tokio::join!(a.flush(&first), b.flush(&second));
        assert!(x.is_ok() ^ y.is_ok(), "exactly one writer must publish");
        let old = a.read(&id(10)).await.unwrap();
        let mut pins = Vec::new();
        for _ in 0..3 {
            pins.push(a.read(&id(10)).await.unwrap());
        }
        let busy = a
            .read(&id(10))
            .await
            .err()
            .expect("fifth snapshot must be refused");
        assert!(matches!(
            busy.downcast_ref::<s3drive_managed::Failure>(),
            Some(s3drive_managed::Failure::Busy)
        ));
        pins.pop();
        assert!(a.read(&id(10)).await.is_ok());
        drop(pins);
        assert!(old.bytes() == one || old.bytes() == two);
        for (p, n) in [(&a, 1000), (&b, 1001)] {
            let r = p.storage().managed().get(id(n)).await.unwrap();
            let g = r.generation.unwrap();
            assert!(matches!(
                p.storage()
                    .uploads()
                    .for_generation(g)
                    .await
                    .unwrap()
                    .unwrap()
                    .guard,
                s3drive_state::remote::Guard::Missing
            ));
            if r.receipt.is_none() {
                // Even a fresh GLOBAL guard cannot publish a reservation made
                // against the old file revision. This is an adversarial new
                // metadata operation, not an automatic retry by Publisher.
                let v = info(&wire).await;
                let request = Apply {
                    volume: v.id,
                    operation: id(n + 6000),
                    expected_sequence: v.sequence,
                    mutation: r.publication.as_ref().unwrap().mutation.clone(),
                };
                assert_eq!(
                    ask(&wire, Request::Apply { request }).await,
                    Reply::Error(s3drive_authority::Error::Conflict)
                );
                assert!(
                    p.storage()
                        .managed()
                        .pending(None, 100)
                        .await
                        .unwrap()
                        .iter()
                        .any(|x| x.input.id() == id(n))
                );
            }
        }
        change(
            &wire,
            30,
            Mutation::Create {
                parent: s3drive_authority::ROOT.into(),
                name: "empty 🦀".into(),
                node_kind: Kind::EmptyFile,
            },
        )
        .await;
        stage(&a, &wire, 30, 1030, b"").await;
        checkpoint("LOST_REPLY");
        wait(&root, "lose").await;
        a.flush(&id(1030)).await.unwrap();
        assert!(a.read(&id(30)).await.unwrap().bytes().is_empty());
        stage(&a, &wire, 10, 1040, b"uncertain S3 commit\0\xff").await;
        a.reserve(&id(1040)).await.unwrap();
        checkpoint("HIDE");
        wait(&root, "hide").await;
        assert!(a.flush(&id(1040)).await.is_err());
        assert!(
            a.storage()
                .managed()
                .get(id(1040))
                .await
                .unwrap()
                .publication
                .is_none()
        );
        assert!(old.bytes() == one || old.bytes() == two);
        checkpoint("CRASH_STAGE");
        std::future::pending::<()>().await;
    } else if mode == "resume" {
        let a = publisher(&config, &root, "client-a", wire.clone(), volume.clone()).await;
        a.flush(&id(1040)).await.unwrap();
        assert_eq!(
            a.read(&id(10)).await.unwrap().bytes(),
            b"uncertain S3 commit\0\xff"
        );
        checkpoint("RECONCILED");
        change(
            &wire,
            40,
            Mutation::Create {
                parent: s3drive_authority::ROOT.into(),
                name: "metadata lost 日本語".into(),
                node_kind: Kind::EmptyFile,
            },
        )
        .await;
        let loss = Arc::new(AtomicU8::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/namespace", listener.local_addr().unwrap());
        let app = axum::Router::new()
            .route("/v1/namespace", axum::routing::post(proxy))
            .with_state(Proxy {
                upstream: wire.clone(),
                loss: loss.clone(),
            });
        let proxy_task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        // Replace only the transport; executor and local owner remain the same.
        a.storage().close().await.unwrap();
        drop(a);
        let proxy_wire =
            WireClient::loopback(&endpoint, Token::new(SECRET.into()).unwrap()).unwrap();
        let a = publisher(&config, &root, "client-a", proxy_wire, volume.clone()).await;
        stage(&a, &wire, 40, 1041, b"committed metadata reply lost").await;
        let before = info(&wire).await.sequence;
        loss.store(1, Ordering::SeqCst);
        assert!(a.reserve(&id(1041)).await.is_err());
        assert!(
            a.storage()
                .managed()
                .get(id(1041))
                .await
                .unwrap()
                .reservation
                .is_none()
        );
        a.reserve(&id(1041)).await.unwrap();
        assert_eq!(info(&wire).await.sequence, before + 1);
        loss.store(2, Ordering::SeqCst);
        assert!(a.flush(&id(1041)).await.is_err());
        let r = a.storage().managed().get(id(1041)).await.unwrap();
        assert!(r.publication.is_some() && r.receipt.is_none());
        assert_eq!(
            a.read(&id(40)).await.unwrap().bytes(),
            b"committed metadata reply lost"
        );
        checkpoint("CRASH_PUBLISH");
        std::future::pending::<()>().await;
        proxy_task.abort();
    } else {
        let a = publisher(&config, &root, "client-a", wire.clone(), volume.clone()).await;
        let r = a.flush(&id(1041)).await.unwrap();
        a.flush(&id(1041)).await.unwrap();
        checkpoint("RECONCILED");
        let old = a.read(&id(40)).await.unwrap();
        assert_eq!(old.bytes(), b"committed metadata reply lost");
        std::fs::write(
            root.join("corrupt-key"),
            &old.metadata().payload.as_ref().unwrap().reservation.key,
        )
        .unwrap();
        checkpoint("CORRUPT");
        wait(&root, "corrupt").await;
        assert!(
            a.read(&id(40)).await.is_err(),
            "a matching ETag cannot excuse corrupted bytes"
        );
        assert_eq!(old.bytes(), b"committed metadata reply lost");
        checkpoint("RESTORE");
        wait(&root, "restored").await;
        assert_eq!(a.read(&id(40)).await.unwrap().bytes(), old.bytes());
        let moved = change(
            &wire,
            3041,
            Mutation::Move {
                node: id(40),
                parent: s3drive_authority::ROOT.into(),
                name: "renamed café 🦀".into(),
            },
        )
        .await;
        assert_eq!(moved.node.payload, r.metadata().node.payload);
        assert!(moved.artifact.is_none());
        assert_eq!(a.read(&id(40)).await.unwrap().bytes(), old.bytes());
        change(&wire, 3042, Mutation::Remove { node: id(40) }).await;
        assert!(a.read(&id(40)).await.is_err());
        assert_eq!(old.bytes(), b"committed metadata reply lost");
        assert!(
            !a.storage()
                .managed()
                .pending(None, 100)
                .await
                .unwrap()
                .iter()
                .any(|r| [id(1040), id(1041)].contains(&r.input.id().to_string()))
        );
        a.storage().close().await.unwrap();
        drop(a);
    }
    stop.send(()).unwrap();
    server.await.unwrap();
    authority.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn managed_logical_edits_recover_settlement_and_retain_conflicting_newer_heads() {
    let f = fixture::Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let setup = writes::setup(&f, tmp.path()).await;
    for mode in ["edits_stage", "edits_resume"] {
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
        let protocol = tokio::time::timeout(Duration::from_secs(100), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                if line == "CRASH_LOGICAL" {
                    return true;
                }
                if line == "LOGICAL_ACK_RECOVERED" {
                    assert_eq!(puts(&f), 1, "logical settlement recovery reissued a PUT");
                    signal(tmp.path(), "settlement-observed");
                }
            }
            false
        })
        .await;
        if mode == "edits_stage" || protocol.is_err() {
            child.kill().await.unwrap();
        }
        let status = tokio::time::timeout(Duration::from_secs(15), child.wait())
            .await
            .unwrap()
            .unwrap();
        let errors = error_task.await.unwrap();
        assert!(
            protocol.is_ok()
                && if mode == "edits_stage" {
                    protocol.unwrap()
                } else {
                    status.success()
                },
            "{mode}: {status}: {errors}"
        );
    }
    assert_eq!(
        puts(&f),
        3,
        "two successful revisions and one retained losing blob"
    );
    assert_eq!(
        f.objects
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.contains("/writable/.s3drive/"))
            .count(),
        3
    );
    assert!(!f.server.is_finished());
}
async fn reserve_edit(p: &Publisher, w: &WireClient, edit: u64, operation: u64) {
    let local = p.storage().logical();
    let record = local.get(id(edit)).await.unwrap();
    let file = local
        .file(record.input.volume.clone(), record.input.node.clone())
        .await
        .unwrap();
    let v = info(w).await;
    p.stage_edit(
        &id(edit),
        Apply {
            volume: v.id,
            operation: id(operation),
            expected_sequence: v.sequence,
            mutation: Mutation::Reserve {
                node: file.binding.node.id,
                expected_revision: file.base.revision,
                store: p.store().id.clone(),
                size: record.input.size,
                sha256: record.input.sha256,
            },
        },
        id(operation + 1000),
    )
    .await
    .unwrap();
}
async fn logical_edits_mode(
    mode: &str,
    config: &s3drive_core::Config,
    root: &Path,
    w: &WireClient,
    volume: &str,
) {
    if mode == "edits_stage" {
        let r = s3drive_s3::S3Reader::from_source(&config.sources[0])
            .await
            .unwrap();
        change(
            w,
            200,
            Mutation::RegisterStore {
                route: r.storage_fingerprint(),
                root: "writable/.s3drive/".into(),
            },
        )
        .await;
        change(
            w,
            10,
            Mutation::Create {
                parent: s3drive_authority::ROOT.into(),
                name: "編集 café 日本語 🦀.bin".into(),
                node_kind: Kind::EmptyFile,
            },
        )
        .await;
    }
    let p = publisher(config, root, "logical-client", w.clone(), volume.into()).await;
    let local = p.storage().logical();
    let first = b"original local edit\0\xff";
    let second = b"newer local generation stays dirty";
    if mode == "edits_stage" {
        let file = p.bind_edit_file(&id(10)).await.unwrap();
        let warmup = p
            .stage_local(
                s3drive_state::logical::Input::from_bytes(
                    id(900),
                    &file.binding,
                    None,
                    b"coalesced predecessor",
                ),
                Arc::from(b"coalesced predecessor".as_slice()),
            )
            .await
            .unwrap();
        let a = p
            .stage_local(
                s3drive_state::logical::Input::from_bytes(
                    id(901),
                    &file.binding,
                    warmup.generation,
                    first,
                ),
                Arc::from(first.as_slice()),
            )
            .await
            .unwrap();
        reserve_edit(&p, w, 901, 1901).await;
        p.stage_local(
            s3drive_state::logical::Input::from_bytes(id(902), &file.binding, a.generation, second),
            Arc::from(second.as_slice()),
        )
        .await
        .unwrap();
        // Stop after S3 + authority + physical logical-publication receipt,
        // before this local file's settlement. The newer local head exists.
        p.flush(&id(1901)).await.unwrap();
        assert!(local.get(id(901)).await.unwrap().receipt.is_none());
        checkpoint("CRASH_LOGICAL");
        std::future::pending::<()>().await;
    } else {
        let a = local.get(id(901)).await.unwrap();
        let b = local.get(id(902)).await.unwrap();
        assert!(
            local
                .file(volume.into(), id(10))
                .await
                .unwrap()
                .settled
                .is_none()
        );
        p.flush_edit(&id(901)).await.unwrap();
        checkpoint("LOGICAL_ACK_RECOVERED");
        wait(root, "settlement-observed").await;
        let file = local.file(volume.into(), id(10)).await.unwrap();
        assert_eq!(file.head, b.generation);
        assert_eq!(file.settled, a.generation);
        assert_eq!(local.pending(None, 10).await.unwrap().len(), 1);
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), first);
        reserve_edit(&p, w, 902, 1902).await;
        p.flush_edit(&id(902)).await.unwrap();
        assert!(local.pending(None, 10).await.unwrap().is_empty());
        let reopened = p.bind_edit_file(&id(10)).await.unwrap();
        assert_eq!(reopened.head, b.generation);
        assert_eq!(reopened.binding, file.binding);
        let old = p
            .storage()
            .staging()
            .snapshot(a.generation.unwrap())
            .await
            .unwrap();
        assert_eq!(old.read(0, first.len()).await.unwrap(), first);
        p.stage_local(a.input.clone(), Arc::from(first.as_slice()))
            .await
            .unwrap();
        assert_eq!(
            local.file(volume.into(), id(10)).await.unwrap().head,
            b.generation
        );
        let bytes = b"losing local bytes are retained";
        let c = p
            .stage_local(
                s3drive_state::logical::Input::from_bytes(
                    id(903),
                    &file.binding,
                    b.generation,
                    bytes,
                ),
                Arc::from(bytes.as_slice()),
            )
            .await
            .unwrap();
        reserve_edit(&p, w, 903, 1903).await;
        p.reserve(&id(1903)).await.unwrap();
        change(
            w,
            9000,
            Mutation::Move {
                node: id(10),
                parent: s3drive_authority::ROOT.into(),
                name: "外部 rename".into(),
            },
        )
        .await;
        assert!(p.flush_edit(&id(903)).await.is_err());
        assert!(p.bind_edit_file(&id(10)).await.is_err());
        let mut changed = local
            .get(id(903))
            .await
            .unwrap()
            .publication
            .unwrap()
            .reserve;
        changed.expected_sequence = info(w).await.sequence;
        if let Mutation::Reserve {
            expected_revision, ..
        } = &mut changed.mutation
        {
            *expected_revision = node(w, volume, 10).await.revision;
        }
        assert!(p.stage_edit(&id(903), changed, id(2903)).await.is_err());
        let empty = p
            .stage_local(
                s3drive_state::logical::Input::from_bytes(
                    id(904),
                    &file.binding,
                    c.generation,
                    b"",
                ),
                Arc::from([]),
            )
            .await
            .unwrap();
        assert!(
            p.storage()
                .staging()
                .snapshot(empty.generation.unwrap())
                .await
                .unwrap()
                .read(0, 1)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(local.pending(None, 10).await.unwrap().len(), 2);
        assert!(local.get(id(903)).await.unwrap().receipt.is_none());
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), second);
        assert_eq!(
            p.storage()
                .staging()
                .snapshot(c.generation.unwrap())
                .await
                .unwrap()
                .read(0, bytes.len())
                .await
                .unwrap(),
            bytes
        );
        p.storage().close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unlinked_dirty_sdk_generations_recover_without_resurrection_or_repeated_puts() {
    let f = fixture::Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let setup = writes::setup(&f, tmp.path()).await;
    for mode in ["detached_stage", "detached_resume", "detached_finish"] {
        if mode == "detached_resume" {
            f.lose_put_reply.store(true, Ordering::SeqCst);
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
        let error_task = tokio::spawn(async move {
            let mut s = String::new();
            errors.read_to_string(&mut s).await.unwrap();
            s
        });
        let protocol = tokio::time::timeout(Duration::from_secs(120), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                match line.as_str() {
                    "CRASH_DETACHED_NAMESPACE" => {
                        assert_eq!(puts(&f), 0);
                        return true;
                    }
                    "CRASH_DETACHED_UPLOAD" => {
                        assert_eq!(puts(&f), 1);
                        return true;
                    }
                    "DETACHED_ACK_RECOVERED" => {
                        assert_eq!(puts(&f), 1, "recovery reissued a PUT");
                        signal(tmp.path(), "detached-observed");
                    }
                    _ => (),
                }
            }
            false
        })
        .await;
        let crash = protocol.as_ref().copied().unwrap_or(false);
        if crash || protocol.is_err() {
            child.kill().await.unwrap();
        }
        let status = tokio::time::timeout(Duration::from_secs(15), child.wait())
            .await
            .unwrap()
            .unwrap();
        let errors = error_task.await.unwrap();
        assert!(
            protocol.is_ok()
                && if mode == "detached_finish" {
                    status.success() && !crash
                } else {
                    crash
                },
            "{mode}: {status}: {errors}"
        );
        writes::restore_observation(&f);
    }
    assert_eq!(puts(&f), 2, "one immutable PUT per detached generation");
    let objects = f.objects.lock().unwrap();
    let blobs = objects
        .iter()
        .filter(|(key, _)| key.contains("/writable/.s3drive/"))
        .collect::<Vec<_>>();
    assert_eq!(
        blobs.len(),
        2,
        "both unlinked generations must remain retained"
    );
    assert!(
        blobs
            .iter()
            .any(|(_, (b, _))| b == b"unlinked binary\0\xff")
    );
    assert!(
        blobs
            .iter()
            .any(|(_, (b, _))| b == b"newer retained unlinked data")
    );
}
async fn detached_prepare(p: &Publisher, w: &WireClient, edit: u64, op: u64) {
    let local = p.storage().logical();
    let r = local.get(id(edit)).await.unwrap();
    let file = local
        .file(r.input.volume.clone(), r.input.node.clone())
        .await
        .unwrap();
    assert!(file.base.deleted);
    p.stage_edit(
        &id(edit),
        Apply {
            volume: r.input.volume,
            operation: id(op),
            expected_sequence: info(w).await.sequence,
            mutation: Mutation::ReserveDetachedContent {
                node: file.binding.node.id,
                expected_revision: file.base.revision,
                store: p.store().id.clone(),
                size: r.input.size,
                sha256: r.input.sha256,
                expected_payload: s3drive_authority::payload_reference(&file.base.payload),
            },
        },
        id(op + 1000),
    )
    .await
    .unwrap();
}
async fn detached_mode(
    mode: &str,
    config: &s3drive_core::Config,
    root: &Path,
    w: &WireClient,
    volume: &str,
) {
    if mode == "detached_stage" {
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
                parent: s3drive_authority::ROOT.into(),
                parent_revision: 0,
                store: Some(id(200)),
                name: "removed 日本語 🦀.bin".into(),
                node_kind: Kind::EmptyFile,
            },
        )
        .await;
        // The real authority commits unlink; the forwarding service then drops
        // that successful reply. No local observation may be acknowledged yet.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/namespace", listener.local_addr().unwrap());
        let app = axum::Router::new()
            .route("/v1/namespace", axum::routing::post(proxy))
            .with_state(Proxy {
                upstream: w.clone(),
                loss: Arc::new(AtomicU8::new(3)),
            });
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let wire = WireClient::loopback(&endpoint, Token::new(SECRET.into()).unwrap()).unwrap();
        let p = publisher(config, root, "detached-client", wire, volume.into()).await;
        let file = p.bind_edit_file(&id(10)).await.unwrap();
        p.stage_local(
            s3drive_state::logical::Input::from_bytes(
                id(900),
                &file.binding,
                None,
                b"unlinked binary\0\xff",
            ),
            Arc::from(&b"unlinked binary\0\xff"[..]),
        )
        .await
        .unwrap();
        let request = Apply {
            volume: volume.into(),
            operation: id(4000),
            expected_sequence: info(w).await.sequence,
            mutation: Mutation::RemoveEntry {
                node: s3drive_authority::NodeGuard {
                    id: id(10),
                    revision: file.base.revision,
                },
                parent: s3drive_authority::NodeGuard {
                    id: s3drive_authority::ROOT.into(),
                    revision: 0,
                },
            },
        };
        let input = p
            .namespace_change_input(request, std::slice::from_ref(&file.base))
            .unwrap();
        assert!(p.change_namespace(input).await.is_err());
        assert_eq!(
            p.storage()
                .namespace()
                .pending(None, 100)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            !p.storage()
                .logical()
                .file(volume.into(), id(10))
                .await
                .unwrap()
                .base
                .deleted
        );
        assert!(node(w, volume, 10).await.deleted);
        assert!(p.read(&id(10)).await.is_err());
        assert!(!server.is_finished());
        checkpoint("CRASH_DETACHED_NAMESPACE");
        std::future::pending::<()>().await;
    }
    let p = publisher(config, root, "detached-client", w.clone(), volume.into()).await;
    let local = p.storage().logical();
    if mode == "detached_resume" {
        let input = p
            .storage()
            .namespace()
            .pending(None, 100)
            .await
            .unwrap()
            .pop()
            .unwrap()
            .input;
        let sequence = info(w).await.sequence;
        p.change_namespace(input).await.unwrap();
        assert_eq!(
            info(w).await.sequence,
            sequence,
            "namespace replay repeated unlink"
        );
        let file = local.file(volume.into(), id(10)).await.unwrap();
        assert!(file.base.deleted);
        assert!(file.settled.is_none());
        change(
            w,
            11,
            Mutation::CreateEntry {
                parent: s3drive_authority::ROOT.into(),
                parent_revision: 0,
                store: Some(id(200)),
                name: file.base.name.clone(),
                node_kind: Kind::EmptyFile,
            },
        )
        .await;
        detached_prepare(&p, w, 900, 5000).await;
        assert!(
            p.flush_edit(&id(900)).await.is_err(),
            "fault did not retain uncertain upload"
        );
        let old = local.get(id(900)).await.unwrap();
        assert!(old.receipt.is_none());
        p.stage_local(
            s3drive_state::logical::Input::from_bytes(
                id(901),
                &file.binding,
                old.generation,
                b"newer retained unlinked data",
            ),
            Arc::from(&b"newer retained unlinked data"[..]),
        )
        .await
        .unwrap();
        checkpoint("CRASH_DETACHED_UPLOAD");
        std::future::pending::<()>().await;
    }
    p.flush_edit(&id(900)).await.unwrap();
    let file = local.file(volume.into(), id(10)).await.unwrap();
    assert!(file.base.deleted);
    assert_ne!(file.head, file.settled);
    assert!(p.read(&id(10)).await.is_err());
    checkpoint("DETACHED_ACK_RECOVERED");
    wait(root, "detached-observed").await;
    detached_prepare(&p, w, 901, 5001).await;
    p.flush_edit(&id(901)).await.unwrap();
    let final_file = local.file(volume.into(), id(10)).await.unwrap();
    assert!(final_file.base.deleted);
    assert_eq!(final_file.head, final_file.settled);
    let live = node(w, volume, 11).await;
    assert!(!live.deleted);
    assert!(live.payload.is_none());
    let sequence = info(w).await.sequence;
    let original = p
        .namespace_change_input(
            Apply {
                volume: volume.into(),
                operation: id(4000),
                expected_sequence: 2,
                mutation: Mutation::RemoveEntry {
                    node: s3drive_authority::NodeGuard {
                        id: id(10),
                        revision: 2,
                    },
                    parent: s3drive_authority::NodeGuard {
                        id: s3drive_authority::ROOT.into(),
                        revision: 0,
                    },
                },
            },
            std::slice::from_ref(&final_file.binding.node),
        )
        .unwrap();
    p.change_namespace(original).await.unwrap();
    assert_eq!(info(w).await.sequence, sequence);
    assert_eq!(
        local.file(volume.into(), id(10)).await.unwrap().base,
        final_file.base
    );
    // A retained semantic rejection remains an error on cached replay too.
    change(
        w,
        12,
        Mutation::CreateEntry {
            parent: s3drive_authority::ROOT.into(),
            parent_revision: 0,
            store: Some(id(200)),
            name: "rejected unlink".into(),
            node_kind: Kind::EmptyFile,
        },
    )
    .await;
    let stale = node(w, volume, 12).await;
    let mutation = Mutation::RemoveEntry {
        node: s3drive_authority::NodeGuard {
            id: stale.id.clone(),
            revision: stale.revision,
        },
        parent: s3drive_authority::NodeGuard {
            id: s3drive_authority::ROOT.into(),
            revision: 0,
        },
    };
    let input = p
        .namespace_change_input(
            Apply {
                volume: volume.into(),
                operation: id(7000),
                expected_sequence: info(w).await.sequence,
                mutation: mutation.clone(),
            },
            std::slice::from_ref(&stale),
        )
        .unwrap();
    change(w, 7001, mutation).await;
    let sequence = info(w).await.sequence;
    for _ in 0..2 {
        let error = p.change_namespace(input.clone()).await.unwrap_err();
        assert!(matches!(
            error.downcast_ref::<s3drive_managed::Failure>(),
            Some(s3drive_managed::Failure::Authority(
                s3drive_authority::Error::Conflict
            ))
        ));
    }
    assert_eq!(info(w).await.sequence, sequence);
    p.storage().close().await.unwrap();
}
