//! Real CLI/TCP/TLS restart identity; no S3 operations or startup installation.
use s3drive_authority::{Apply, Mutation, Reply, Request, Token, WireClient};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
};

struct Server {
    child: Child,
    endpoint: String,
    volume: String,
}
fn command(
    directory: &Path,
    token: &Path,
    expected: Option<&str>,
    tls: Option<(&Path, &Path)>,
    listen: &str,
) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_s3drive"));
    c.arg("authority-serve")
        .arg("--directory")
        .arg(directory)
        .arg("--token-file")
        .arg(token)
        .args(["--name-policy", "portable-collision-v1", "--listen", listen])
        .kill_on_drop(true);
    if let Some(v) = expected {
        c.args(["--expect-volume", v]);
    }
    if let Some((cert, key)) = tls {
        c.arg("--tls-cert").arg(cert).arg("--tls-key").arg(key);
    }
    c
}
impl Server {
    async fn start(mut c: Command) -> Self {
        let mut child = c
            .arg("--stop-on-stdin-close")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let line = tokio::time::timeout(Duration::from_secs(20), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .expect("coordinator readiness");
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        Self {
            child,
            endpoint: v["endpoint"].as_str().unwrap().into(),
            volume: v["volume"].as_str().unwrap().into(),
        }
    }
    async fn stop(mut self) {
        drop(self.child.stdin.take());
        assert!(
            tokio::time::timeout(Duration::from_secs(20), self.child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
    }
    async fn kill(mut self) {
        self.child.kill().await.unwrap();
        self.child.wait().await.unwrap();
    }
}
async fn output(c: &mut Command) -> std::process::Output {
    tokio::time::timeout(Duration::from_secs(20), c.output())
        .await
        .unwrap()
        .unwrap()
}
fn token(root: &Path) -> PathBuf {
    s3drive_state::create_private_secret(&root.join("secret"), "a".repeat(64).as_bytes()).unwrap()
}
fn wire(endpoint: &str, roots: Option<&[u8]>) -> WireClient {
    let token = Token::new("a".repeat(64)).unwrap();
    if endpoint.starts_with("https:") {
        WireClient::https(endpoint, token, roots).unwrap()
    } else {
        WireClient::loopback(endpoint, token).unwrap()
    }
}
async fn info(w: &WireClient) -> s3drive_authority::Volume {
    let Reply::Info(v) = w.request(&Request::Info {}).await.unwrap() else {
        panic!("volume")
    };
    v
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pinned_cli_http_tls_restart_recovers_original_volume_and_unread_operation() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let db = root.join("database");
    let secret = token(root);
    let server = Server::start(command(&db, &secret, None, None, "127.0.0.1:0")).await;
    let volume = server.volume.clone();
    let endpoint = server.endpoint.clone();
    let w = wire(&endpoint, None);
    let registered = Apply {
        volume: volume.clone(),
        operation: "1".repeat(32),
        expected_sequence: 0,
        mutation: Mutation::RegisterStore {
            route: "2".repeat(64),
            root: "managed/.s3drive/".into(),
        },
    };
    assert!(matches!(
        w.request(&Request::Apply {
            request: registered.clone()
        })
        .await
        .unwrap(),
        Reply::Applied(_)
    ));
    server.stop().await;
    // Offline check must work even when its configured port is occupied.
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen = occupied.local_addr().unwrap().to_string();
    let out = output(
        Command::new(env!("CARGO_BIN_EXE_s3drive"))
            .arg("authority-check")
            .arg("--directory")
            .arg(&db)
            .arg("--token-file")
            .arg(&secret)
            .args([
                "--expect-volume",
                &volume,
                "--name-policy",
                "portable-collision-v1",
                "--listen",
                &listen,
            ]),
    )
    .await;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["running"], false);
    assert_eq!(v["volume"]["sequence"], 1);
    let server = Server::start(command(&db, &secret, Some(&volume), None, "127.0.0.1:0")).await;
    assert_eq!(server.volume, volume);
    let endpoint = url::Url::parse(&server.endpoint).unwrap();
    let request = Apply {
        volume: volume.clone(),
        operation: "3".repeat(32),
        expected_sequence: 1,
        mutation: Mutation::CreateEntry {
            parent: s3drive_authority::ROOT.into(),
            parent_revision: 0,
            store: None,
            name: "資料 café 🦀".into(),
            node_kind: s3drive_authority::Kind::Directory,
        },
    };
    let body = serde_json::to_vec(&Request::Apply {
        request: request.clone(),
    })
    .unwrap();
    let mut unread = tokio::net::TcpStream::connect(("127.0.0.1", endpoint.port().unwrap()))
        .await
        .unwrap();
    let headers = format!(
        "POST /v1/namespace HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        "a".repeat(64),
        body.len()
    );
    unread.write_all(headers.as_bytes()).await.unwrap();
    unread.write_all(&body).await.unwrap();
    let w = wire(&server.endpoint, None);
    let recorded = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let r = w
                .request(&Request::Operation {
                    volume: volume.clone(),
                    operation: request.operation.clone(),
                })
                .await
                .unwrap();
            if matches!(r, Reply::Operation(Some(Ok(_)))) {
                break r;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    drop(unread);
    server.kill().await;
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let cert_file = s3drive_state::create_private_document(
        &root.join("tls"),
        "cert.pem",
        cert.pem().as_bytes(),
    )
    .unwrap();
    let key_file = s3drive_state::create_private_document(
        cert_file.parent().unwrap(),
        "key.pem",
        signing_key.serialize_pem().as_bytes(),
    )
    .unwrap();
    let server = Server::start(command(
        &db,
        &secret,
        Some(&volume),
        Some((&cert_file, &key_file)),
        "127.0.0.1:0",
    ))
    .await;
    let w = wire(&server.endpoint, Some(cert.pem().as_bytes()));
    assert_eq!(info(&w).await.id, volume);
    assert_eq!(
        w.request(&Request::Operation {
            volume: volume.clone(),
            operation: request.operation.clone()
        })
        .await
        .unwrap(),
        recorded
    );
    assert!(matches!(
        w.request(&Request::Apply { request }).await.unwrap(),
        Reply::Applied(_)
    ));
    assert_eq!(info(&w).await.sequence, 2);
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pinned_cli_mismatch_and_missing_metadata_refuse_before_listening_or_readiness() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let db = root.join("database");
    let secret = token(root);
    let server = Server::start(command(&db, &secret, None, None, "127.0.0.1:0")).await;
    let volume = server.volume.clone();
    server.stop().await;
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen = occupied.local_addr().unwrap().to_string();
    for wrong in ["0".repeat(32), "f".repeat(32)] {
        let out = output(&mut command(&db, &secret, Some(&wrong), None, &listen)).await;
        assert!(!out.status.success() && out.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&out.stderr).contains("Address already in use"));
    }
    let absent = root.join("missing");
    let out = output(&mut command(&absent, &secret, Some(&volume), None, &listen)).await;
    assert!(!out.status.success() && out.stdout.is_empty() && !absent.exists());
    let before = std::fs::read(db.join("metadata.sqlite3")).unwrap();
    let out = output(
        Command::new(env!("CARGO_BIN_EXE_s3drive"))
            .arg("authority-check")
            .arg("--directory")
            .arg(&db)
            .arg("--token-file")
            .arg(&secret)
            .args([
                "--expect-volume",
                &volume,
                "--name-policy",
                "exact-utf8-v1",
                "--listen",
                &listen,
            ]),
    )
    .await;
    assert!(!out.status.success() && out.stdout.is_empty());
    assert_eq!(std::fs::read(db.join("metadata.sqlite3")).unwrap(), before);
    let server = Server::start(command(&db, &secret, Some(&volume), None, "127.0.0.1:0")).await;
    assert_eq!(server.volume, volume);
    assert_eq!(info(&wire(&server.endpoint, None)).await.sequence, 0);
    server.stop().await;
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "coordinator supervisor acceptance: ordinary Linux user, tmux and gio"]
async fn supervised_linux_coordinator_http_and_tls_with_private_ca() {
    let tmp = tempfile::tempdir().unwrap();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let cert_file = s3drive_state::create_private_document(
        &tmp.path().join("tls"),
        "cert.pem",
        cert.pem().as_bytes(),
    )
    .unwrap();
    let key_file = s3drive_state::create_private_document(
        cert_file.parent().unwrap(),
        "key.pem",
        signing_key.serialize_pem().as_bytes(),
    )
    .unwrap();
    for tls in [false, true] {
        let mut c = Command::new("python3");
        c.arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/test-linux-authority.py"))
            .arg(env!("CARGO_BIN_EXE_s3drive"))
            .kill_on_drop(true);
        if tls {
            c.arg(&cert_file).arg(&key_file);
        }
        let out = tokio::time::timeout(Duration::from_secs(180), c.output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let log = String::from_utf8_lossy(&out.stdout);
        assert!(log.contains("PASS: no-systemd pinned coordinator"));
        assert!(log.contains(if tls {
            "HTTPS/private CA"
        } else {
            "loopback HTTP"
        }));
        eprintln!("{log}");
    }
}

// A subprocess test worker lets the disposable ordinary Windows account use
// the production Rust TLS client. No machine certificate-store changes or
// certificate-verification callbacks are involved.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "fixture subprocess only: requires an owned root and explicit mode"]
async fn windows_authority_fixture_worker() {
    let root = PathBuf::from(std::env::var_os("S3DRIVE_AUTHORITY_FIXTURE_ROOT").unwrap());
    let mode = std::env::var("S3DRIVE_AUTHORITY_FIXTURE_MODE").unwrap();
    if mode == "Init" {
        let private = root.join("authority-inputs");
        s3drive_state::create_private_secret(&private, "a".repeat(64).as_bytes()).unwrap();
        for name in ["cert.pem", "key.pem"] {
            s3drive_state::create_private_document(
                &private,
                name,
                &std::fs::read(root.join(name)).unwrap(),
            )
            .unwrap();
        }
        return;
    }
    assert!(matches!(mode.as_str(), "Seed" | "Verify" | "Recover"));
    let identity: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("authority-identity.json")).unwrap())
            .unwrap();
    let volume = identity["volume"].as_str().unwrap();
    let cert = std::fs::read(root.join("authority-inputs/cert.pem")).unwrap();
    let w = wire(identity["endpoint"].as_str().unwrap(), Some(&cert));
    let deadline = Duration::from_secs(if mode == "Recover" { 100 } else { 20 });
    let v = tokio::time::timeout(deadline, async {
        loop {
            if let Ok(Reply::Info(v)) = w.request(&Request::Info {}).await {
                break v;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    })
    .await
    .expect("authenticated pinned TLS coordinator not available");
    assert_eq!(v.id, volume);
    let request = Apply {
        volume: volume.into(),
        operation: "c".repeat(32),
        expected_sequence: 0,
        mutation: Mutation::CreateEntry {
            parent: s3drive_authority::ROOT.into(),
            parent_revision: 0,
            store: None,
            name: "資料 café 🦀".into(),
            node_kind: s3drive_authority::Kind::Directory,
        },
    };
    let retained = root.join("authority-receipt.json");
    if mode == "Seed" {
        let reply = w.request(&Request::Apply { request }).await.unwrap();
        assert!(matches!(reply, Reply::Applied(_)), "{reply:?}");
        std::fs::write(retained, serde_json::to_vec(&reply).unwrap()).unwrap();
    } else {
        let original: Reply = serde_json::from_slice(&std::fs::read(retained).unwrap()).unwrap();
        let Reply::Applied(receipt) = &original else {
            panic!("original receipt missing")
        };
        assert_eq!(
            w.request(&Request::Operation {
                volume: volume.into(),
                operation: request.operation.clone(),
            })
            .await
            .unwrap(),
            Reply::Operation(Some(Ok(receipt.clone())))
        );
        assert_eq!(
            w.request(&Request::Apply { request }).await.unwrap(),
            original
        );
    }
    assert_eq!(
        info(&w).await.sequence,
        1,
        "replay repeated namespace effect"
    );
    let Reply::List(list) = w
        .request(&Request::List {
            volume: volume.into(),
            parent: s3drive_authority::ROOT.into(),
        })
        .await
        .unwrap()
    else {
        panic!("Unicode namespace listing")
    };
    assert_eq!(list.children.len(), 1);
    assert_eq!(list.children[0].name, "資料 café 🦀");
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native coordinator supervision: disposable elevated Windows runner"]
async fn supervised_windows_coordinator_tls_independent_logons_and_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    // These are ephemeral test-only inputs, copied into account-private files
    // by the ordinary account before Rust can use them.
    std::fs::write(tmp.path().join("cert.pem"), cert.pem()).unwrap();
    std::fs::write(tmp.path().join("key.pem"), signing_key.serialize_pem()).unwrap();
    std::fs::copy(
        std::env::current_exe().unwrap(),
        tmp.path().join("authority-worker.exe"),
    )
    .unwrap();
    let out = tokio::time::timeout(
        Duration::from_secs(540),
        Command::new("powershell.exe")
            .env_remove("PSModulePath")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/test-windows-session.ps1"),
            )
            .arg("-Executable")
            .arg(env!("CARGO_BIN_EXE_s3drive"))
            .arg("-AuthorityOnly")
            .arg("-AuthorityFixture")
            .arg(tmp.path())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    eprintln!("{}", String::from_utf8_lossy(&out.stdout));
    assert!(
        out.status.success(),
        "Windows coordinator supervision: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("PASS: non-admin TLS coordinator"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disposable_account_worker_uses_private_tls_inputs_and_exact_restart_receipts() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    std::fs::write(root.join("cert.pem"), cert.pem()).unwrap();
    std::fs::write(root.join("key.pem"), signing_key.serialize_pem()).unwrap();
    async fn run_worker(root: &Path, mode: &str) {
        let out = tokio::time::timeout(
            Duration::from_secs(30),
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "windows_authority_fixture_worker",
                    "--nocapture",
                ])
                .env("S3DRIVE_AUTHORITY_FIXTURE_ROOT", root)
                .env("S3DRIVE_AUTHORITY_FIXTURE_MODE", mode)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            out.status.success(),
            "worker {mode}: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    run_worker(root, "Init").await;
    let private = root.join("authority-inputs");
    let secret = private.join("authority.token");
    let cert = private.join("cert.pem");
    let key = private.join("key.pem");
    let db = root.join("authority-state");
    let first = Server::start(command(
        &db,
        &secret,
        None,
        Some((&cert, &key)),
        "127.0.0.1:0",
    ))
    .await;
    let write_identity = |server: &Server| {
        std::fs::write(
            root.join("authority-identity.json"),
            serde_json::to_vec(
                &serde_json::json!({"endpoint":server.endpoint,"volume":server.volume}),
            )
            .unwrap(),
        )
        .unwrap();
    };
    write_identity(&first);
    run_worker(root, "Seed").await;
    run_worker(root, "Verify").await;
    let volume = first.volume.clone();
    first.kill().await;
    let second = Server::start(command(
        &db,
        &secret,
        Some(&volume),
        Some((&cert, &key)),
        "127.0.0.1:0",
    ))
    .await;
    write_identity(&second);
    run_worker(root, "Recover").await;
    second.stop().await;
}
