//! Real CLI/TCP/TLS restart identity; no S3 operations or startup installation.
use s3drive_authority::{Apply, Mutation, Reply, Request, Token, WireClient};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
};

#[path = "support/fixture.rs"]
pub mod fixture;
#[path = "support/managed.rs"]
#[allow(dead_code)]
mod managed;
#[path = "support/writes.rs"]
#[allow(dead_code)]
mod writes;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn managed_native_child() {
    managed::child().await;
}

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
    async fn stop(mut self) -> String {
        drop(self.child.stdin.take());
        let out = tokio::time::timeout(Duration::from_secs(20), self.child.wait_with_output())
            .await
            .unwrap()
            .unwrap();
        assert!(out.status.success(), "coordinator shutdown failed");
        String::from_utf8(out.stderr).unwrap()
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

// These tests run separately and early in CI as well as in the workspace gate.
// They use actual CLI processes and the production TLS listener, but no S3 or
// machine startup registration. Passing is not a Windows reliability fix.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tls_cli_fresh_processes_and_reused_clients_preserve_exact_receipts() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let secret = token(root);
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let ca = s3drive_state::create_private_document(
        &root.join("tls"),
        "cert.pem",
        cert.pem().as_bytes(),
    )
    .unwrap();
    let key = s3drive_state::create_private_document(
        ca.parent().unwrap(),
        "key.pem",
        signing_key.serialize_pem().as_bytes(),
    )
    .unwrap();
    let server = Server::start(command(
        &root.join("database"),
        &secret,
        None,
        Some((&ca, &key)),
        "127.0.0.1:0",
    ))
    .await;
    // No credential resolution or provider connection is needed for bootstrap.
    let s3 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = root.join("sources.toml");
    std::fs::write(&config, format!("version=1\nmode='read-only'\n[[sources]]\nalias='docs'\nbucket='test-bucket'\nregion='us-east-1'\nprefix='資料/'\nprofile='unconfigured-fixture-profile'\nendpoint='http://{}'\nallow_http_loopback=true\npath_style=true\n", s3.local_addr().unwrap())).unwrap();
    let endpoint = server.endpoint.clone();
    let volume = server.volume.clone();
    let work = root.to_owned();
    let mut exercise = tokio::spawn(async move {
        let cli = |verb: &str| {
            let mut c = Command::new(env!("CARGO_BIN_EXE_s3drive"));
            c.args(["managed", verb])
                .arg("--token-file")
                .arg(&secret)
                .arg("--ca-file")
                .arg(&ca)
                .env(
                    "AWS_SHARED_CREDENTIALS_FILE",
                    work.join("absent-credentials"),
                )
                .env("AWS_CONFIG_FILE", work.join("absent-config"))
                .env("AWS_EC2_METADATA_DISABLED", "true")
                .kill_on_drop(true);
            c
        };
        async fn checked(c: &mut Command, stage: &str, index: usize) -> serde_json::Value {
            let out = output(c).await;
            assert!(
                out.status.success(),
                "stage={stage} index={index}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            serde_json::from_slice(&out.stdout).unwrap()
        }
        let w = wire(&endpoint, Some(cert.pem().as_bytes()));
        let mut plans = Vec::new();
        let mut receipts = Vec::new();
        for i in 0..16 {
            let plan = checked(
                cli("store-plan")
                    .arg(&config)
                    .arg("docs")
                    .args([
                        "--authority",
                        &endpoint,
                        "--volume",
                        &volume,
                        "--root",
                        "managed/.s3drive/",
                    ])
                    .arg("--directory")
                    .arg(work.join(format!("plan-{i}"))),
                "plan",
                i,
            )
            .await;
            let result = checked(
                cli("store-apply")
                    .arg(&config)
                    .arg("docs")
                    .arg(plan["plan"].as_str().unwrap())
                    .arg("--confirm")
                    .arg(plan["plan_sha256"].as_str().unwrap()),
                "apply",
                i,
            )
            .await;
            assert_eq!(result["status"], "registered");
            let receipt = w
                .request(&Request::Operation {
                    volume: volume.clone(),
                    operation: plan["store_id"].as_str().unwrap().into(),
                })
                .await
                .unwrap();
            assert!(matches!(receipt, Reply::Operation(Some(Ok(_)))));
            assert_eq!(info(&w).await.sequence, i as u64 + 1);
            receipts.push(receipt);
            plans.push(plan);
        }
        // Repeated fresh caller processes exercise several consecutive RPCs
        // on each reqwest client. Exact receipt and sequence must not change.
        for round in 0..4 {
            for (i, plan) in plans.iter().enumerate() {
                let result = checked(
                    cli("store-apply")
                        .arg(&config)
                        .arg("docs")
                        .arg(plan["plan"].as_str().unwrap())
                        .arg("--confirm")
                        .arg(plan["plan_sha256"].as_str().unwrap()),
                    "observe-exact-plan",
                    round * 16 + i,
                )
                .await;
                assert_eq!(result["status"], "registered");
                assert_eq!(
                    w.request(&Request::Operation {
                        volume: volume.clone(),
                        operation: plan["store_id"].as_str().unwrap().into(),
                    })
                    .await
                    .unwrap(),
                    receipts[i]
                );
            }
        }
        let mut callers = tokio::task::JoinSet::new();
        for _ in 0..4 {
            let w = w.clone();
            let volume = volume.clone();
            callers.spawn(async move {
                for i in 0..32 {
                    let v = info(&w).await;
                    assert_eq!(v.id, volume, "concurrent Info {i}");
                    assert_eq!(v.sequence, 16);
                }
            });
        }
        while let Some(result) = callers.join_next().await {
            result.unwrap();
        }
        assert_eq!(info(&w).await.sequence, 16);
    });
    let result = tokio::time::timeout(Duration::from_secs(180), &mut exercise).await;
    if result.is_err() {
        exercise.abort();
        let _ = exercise.await;
    }
    let log = server.stop().await;
    assert!(
        matches!(result, Ok(Ok(()))),
        "TLS/CLI churn failed: {result:?}; coordinator diagnostics:\n{log}"
    );
    // Every caller above independently received and validated its full reply.
    // Closing TLS after that may fail in AsyncWrite::shutdown if the caller
    // process has already exited. Allow only that explicitly typed phase;
    // generic I/O, missing headers, request/body failures and timeouts still fail.
    for event in log.lines().filter(|line| line.starts_with("authority TLS")) {
        assert!(
            event.starts_with("authority TLS connection failure: HttpShutdown "),
            "unclassified exchange failure:\n{log}"
        );
        for field in ["read_bytes", "written_bytes", "flushes"] {
            let count = event
                .split_whitespace()
                .find_map(|part| part.strip_prefix(&format!("{field}=")))
                .and_then(|n| n.parse::<u64>().ok())
                .expect("typed positive shutdown counter");
            assert!(count > 0, "shutdown before exchange completion:\n{log}");
        }
        assert!(
            event.ends_with("dispatched=true"),
            "shutdown before dispatch:\n{log}"
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(30), s3.accept())
            .await
            .is_err(),
        "bootstrap contacted S3"
    );
    eprintln!(
        "PASS: TLS/CLI 16 registrations, 64 exact-plan observations and 128 concurrent Info requests, original receipts/sequence intact"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tls_cli_diagnostics_distinguish_idle_partial_headers_and_partial_body_privately() {
    use std::sync::Arc;
    use tokio_rustls::{
        TlsConnector,
        rustls::{ClientConfig, RootCertStore, pki_types::ServerName},
    };
    let tmp = tempfile::tempdir().unwrap();
    let secret = token(tmp.path());
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let ca = s3drive_state::create_private_document(
        &tmp.path().join("tls"),
        "cert.pem",
        cert.pem().as_bytes(),
    )
    .unwrap();
    let key = s3drive_state::create_private_document(
        ca.parent().unwrap(),
        "key.pem",
        signing_key.serialize_pem().as_bytes(),
    )
    .unwrap();
    let server = Server::start(command(
        &tmp.path().join("database"),
        &secret,
        None,
        Some((&ca, &key)),
        "127.0.0.1:0",
    ))
    .await;
    let mut roots = RootCertStore::empty();
    roots.add(cert.der().clone()).unwrap();
    let config = ClientConfig::builder_with_provider(Arc::new(
        tokio_rustls::rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let endpoint = url::Url::parse(&server.endpoint).unwrap();
    let partial_header = b"POST /v1/namespace HTTP/1.1\r\nX-Private: never-log-this";
    let partial_body = format!(
        "POST /v1/namespace HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: 4096\r\n\r\n{{\"private\":\"never-log-this\"",
        "a".repeat(64)
    );
    let mut peers = tokio::task::JoinSet::new();
    let mut sockets = Vec::new();
    for payload in [&b""[..], &partial_header[..], partial_body.as_bytes()] {
        let tcp = tokio::net::TcpStream::connect(("127.0.0.1", endpoint.port().unwrap()))
            .await
            .unwrap();
        sockets.push((
            payload.len(),
            s3drive_authority::LoopbackSocket {
                ipv6: false,
                client_port: tcp.local_addr().unwrap().port(),
                server_port: tcp.peer_addr().unwrap().port(),
            },
        ));
        let mut tls = connector
            .connect(ServerName::try_from("127.0.0.1".to_owned()).unwrap(), tcp)
            .await
            .unwrap();
        tls.write_all(payload).await.unwrap();
        tls.flush().await.unwrap();
        peers.spawn(async move {
            let mut response = Vec::new();
            let _ = tokio::time::timeout(Duration::from_secs(12), tls.read_to_end(&mut response))
                .await
                .expect("incomplete connection deadline");
            assert!(
                response.is_empty(),
                "incomplete request produced application response"
            );
        });
    }
    let w = wire(&server.endpoint, Some(cert.pem().as_bytes()));
    assert_eq!(info(&w).await.sequence, 0);
    let mut peer_failures = Vec::new();
    while let Some(result) = peers.join_next().await {
        if let Err(error) = result {
            peer_failures.push(error);
        }
    }
    assert_eq!(info(&w).await.sequence, 0);
    let log = server.stop().await;
    assert!(
        peer_failures.is_empty(),
        "{peer_failures:?}; coordinator diagnostics:\n{log}"
    );
    let events: Vec<_> = log
        .lines()
        .filter(|l| l.starts_with("authority TLS connection failure:"))
        .collect();
    assert_eq!(events.len(), 3, "{log}");
    for (count, dispatched, kind) in [
        (0, false, "HttpTimeout"),
        (partial_header.len(), false, "HttpTimeout"),
        (partial_body.len(), true, "Lifetime"),
    ] {
        let socket = sockets
            .iter()
            .find(|(length, _)| *length == count)
            .unwrap()
            .1;
        assert_eq!(
            events
                .iter()
                .filter(|l| l.contains(&format!("failure: {kind} "))
                    && l.split_whitespace()
                        .any(|s| s == format!("read_bytes={count}"))
                    && l.contains(&format!("loopback=Some({socket:?})"))
                    && l.ends_with(&format!("dispatched={dispatched}")))
                .count(),
            1,
            "{log}"
        );
    }
    for private in [
        "never-log-this",
        endpoint.as_str(),
        &"a".repeat(64),
        &cert.pem(),
        &signing_key.serialize_pem(),
    ] {
        assert!(!log.contains(private), "diagnostic disclosed private input");
    }
    eprintln!(
        "PASS: TLS failed-connection byte counts distinguish idle/partial header/partial body without logging contents"
    );
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
    // Unauthenticated garbage must not disclose its contents or flood logs,
    // and hitting the diagnostic budget must not stop authenticated service.
    let address = url::Url::parse(&server.endpoint).unwrap();
    for _ in 0..24 {
        let mut peer = tokio::net::TcpStream::connect(("127.0.0.1", address.port().unwrap()))
            .await
            .unwrap();
        peer.write_all(b"private-handshake-marker").await.unwrap();
        let mut alert = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(3), peer.read_to_end(&mut alert))
            .await
            .expect("malformed handshake did not close");
    }
    assert_eq!(info(&w).await.sequence, 2);
    let original_endpoint = server.endpoint.clone();
    let log = server.stop().await;
    let events: Vec<_> = log
        .lines()
        .filter(|line| line.starts_with("authority TLS"))
        .collect();
    assert_eq!(events.len(), 17, "connection diagnostic budget");
    assert!(
        events[..16]
            .iter()
            .all(|line| *line == "authority TLS connection failure: HandshakeRejected")
    );
    assert_eq!(
        events[16],
        "authority TLS: further connection diagnostics suppressed"
    );
    for private in [
        "private-handshake-marker",
        &original_endpoint,
        &"a".repeat(64),
        &cert.pem(),
        &signing_key.serialize_pem(),
    ] {
        assert!(
            !log.contains(private),
            "server diagnostic disclosed private input"
        );
    }
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

// Test fixture only. A separate connection leaves disposable free pages while
// the production owner lock stays held; all SQLite handles close before hashing.
fn pad_snapshot_console_fixture(root: &Path, bytes: u32) {
    // Only the existing disposable ordinary-account fixture after removal.
    // Keep production directory ownership while a separate SQLite connection
    // grows free pages. Never raw-open/hash the database during SQLite use.
    assert!(root.join("snapshot-fixture.marker").is_file());
    let pin: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("authority-identity.json")).unwrap())
            .unwrap();
    let volume = pin["volume"].as_str().unwrap();
    let directory = root.join("authority-state");
    let owner =
        s3drive_state::State::open_existing_authority(&directory, volume, "portable-collision-v1")
            .unwrap();
    let before = owner.namespace_volume().unwrap();
    let padding = rusqlite::Connection::open_with_flags(
        directory.join("metadata.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .unwrap();
    padding.busy_timeout(Duration::from_secs(5)).unwrap();
    let auto_vacuum: i32 = padding
        .pragma_query_value(None, "auto_vacuum", |row| row.get(0))
        .unwrap();
    assert_eq!(auto_vacuum, 0);
    assert!((8 * 1024 * 1024..=128 * 1024 * 1024).contains(&bytes));
    padding
        .execute_batch("BEGIN IMMEDIATE; CREATE TABLE snapshot_console_fixture_padding(data BLOB);")
        .unwrap();
    padding
        .execute(
            "INSERT INTO snapshot_console_fixture_padding VALUES(zeroblob(?1))",
            [bytes],
        )
        .unwrap();
    padding
        .execute_batch("COMMIT; DROP TABLE snapshot_console_fixture_padding;")
        .unwrap();
    let free: i64 = padding
        .pragma_query_value(None, "freelist_count", |row| row.get(0))
        .unwrap();
    assert!(free > 32, "console fixture needs observable copy work");
    let integrity: String = padding
        .pragma_query_value(None, "integrity_check", |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    padding.close().unwrap();
    assert_eq!(owner.namespace_volume().unwrap(), before);
    drop(owner); // Last SQLite close checkpoints WAL before external hashing.
    let observed = s3drive_state::State::inspect_existing_authority(
        &directory,
        volume,
        "portable-collision-v1",
    )
    .unwrap();
    assert_eq!(observed, before);
}

#[test]
fn stopped_console_fixture_padding_preserves_authority_identity_and_exact_receipts() {
    use s3drive_state::{State, namespace};
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let directory = root.join("authority-state");
    let mut state = State::open_managed(&directory).unwrap();
    let volume = state
        .namespace_initialize_with_policy("portable-collision-v1")
        .unwrap();
    let operation = "1".repeat(32);
    let receipt = state
        .namespace_apply(&namespace::Apply {
            volume: volume.id.clone(),
            operation: operation.clone(),
            expected_sequence: 0,
            mutation: namespace::Mutation::Create {
                parent: namespace::ROOT.into(),
                name: "console fixture 資料 🦀".into(),
                node_kind: namespace::Kind::Directory,
            },
        })
        .unwrap();
    let before = state.namespace_volume().unwrap();
    drop(state);
    std::fs::write(
        root.join("snapshot-fixture.marker"),
        "owned temporary portable fixture",
    )
    .unwrap();
    std::fs::write(
        root.join("authority-identity.json"),
        serde_json::to_vec(&serde_json::json!({"volume":volume.id})).unwrap(),
    )
    .unwrap();
    fn operation_bytes(directory: &Path) -> (Vec<u8>, Vec<u8>) {
        let connection = rusqlite::Connection::open_with_flags(
            directory.join("metadata.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .unwrap();
        let bytes = connection
            .query_row(
                "SELECT request,outcome FROM namespace_operations",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        connection.close().unwrap();
        bytes
    }
    let original_bytes = operation_bytes(&directory);
    pad_snapshot_console_fixture(root, 8 * 1024 * 1024);
    assert_eq!(operation_bytes(&directory), original_bytes);
    assert!(
        std::fs::metadata(directory.join("metadata.sqlite3"))
            .unwrap()
            .len()
            >= 8 * 1024 * 1024
    );
    let state =
        State::open_existing_authority(&directory, &volume.id, "portable-collision-v1").unwrap();
    assert_eq!(state.namespace_volume().unwrap(), before);
    assert_eq!(
        state
            .namespace_operation(&volume.id, &operation)
            .unwrap()
            .unwrap()
            .unwrap(),
        receipt
    );
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
        if root.join("managed-fixture.json").exists() {
            managed::windows_session_initialize(&root);
        }
        return;
    }
    #[cfg(windows)]
    if mode == "SnapshotPad" {
        pad_snapshot_console_fixture(&root, 128 * 1024 * 1024);
        eprintln!(
            "console fixture: stopped original authority,128MiB disposable free pages, unchanged namespace identity/sequence"
        );
        return;
    }
    if root.join("managed-fixture.json").exists() {
        managed::windows_session_worker(&root, &mode).await;
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
    let mut last_failure = None;
    let v = tokio::time::timeout(deadline, async {
        loop {
            match w.request(&Request::Info {}).await {
                Ok(Reply::Info(v)) => break v,
                Ok(_) => panic!("coordinator readiness returned an unexpected reply"),
                Err(error) => last_failure = Some(error),
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!("authenticated pinned TLS coordinator not available: {last_failure:?}")
    });
    assert_eq!(v.id, volume);
    eprintln!("coordinator fixture {mode}: authenticated original volume");
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
        eprintln!("coordinator fixture {mode}: original operation verified");
        assert_eq!(
            w.request(&Request::Apply { request }).await.unwrap(),
            original
        );
        eprintln!("coordinator fixture {mode}: exact replay verified");
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

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "paired native Windows startup: disposable elevated runner and WinFsp"]
async fn supervised_windows_managed_drive_and_coordinator_recovery() {
    use std::sync::atomic::Ordering;
    let mut f = fixture::Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    for which in ["first", "second"] {
        std::fs::create_dir(tmp.path().join(which)).unwrap();
    }
    writes::setup(&f, &tmp.path().join("first")).await;
    writes::setup_source(
        &f,
        &tmp.path().join("second"),
        "other-bucket",
        "docs2",
        "fixture-alt",
    )
    .await;
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    std::fs::write(tmp.path().join("cert.pem"), cert.pem()).unwrap();
    std::fs::write(tmp.path().join("key.pem"), signing_key.serialize_pem()).unwrap();
    std::fs::copy(
        std::env::current_exe().unwrap(),
        tmp.path().join("authority-worker.exe"),
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("managed-fixture.json"),
        serde_json::to_vec(&serde_json::json!({"endpoint":f.endpoint})).unwrap(),
    )
    .unwrap();
    let root_file = tmp.path().join("control-root.txt");
    let hidden = f.hide_committed.clone();
    let head = f.deny_head.clone();
    let get = f.deny_get.clone();
    let requests = f.requests.clone();
    let control = tokio::spawn(async move {
        let mut previous = String::new();
        loop {
            if let Ok(root) = std::fs::read_to_string(&root_file) {
                let root = PathBuf::from(root);
                if let Ok(bytes) = std::fs::read(root.join("control-request.json"))
                    && let Ok(request) = serde_json::from_slice::<serde_json::Value>(&bytes)
                    && let Some(id) = request["id"].as_str()
                    && id != previous
                {
                    match request["action"].as_str().unwrap() {
                        "hide" => hidden.store(true, Ordering::SeqCst),
                        "restore" => {
                            head.store(false, Ordering::SeqCst);
                            get.store(false, Ordering::SeqCst);
                        }
                        "count" => {}
                        _ => panic!("unknown owned fixture control"),
                    }
                    let puts = requests
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|r| r.starts_with("PUT ") && r.contains("/writable/.s3drive/"))
                        .count();
                    std::fs::write(
                        root.join("control-reply.json"),
                        serde_json::to_vec(&serde_json::json!({"id":id,"puts":puts})).unwrap(),
                    )
                    .unwrap();
                    previous = id.into();
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    let out = tokio::time::timeout(
        Duration::from_secs(900),
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
            .arg("-ManagedFixture")
            .arg(tmp.path())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    control.abort();
    let _ = control.await;
    eprintln!("{}", String::from_utf8_lossy(&out.stdout));
    assert!(
        out.status.success(),
        "Windows paired startup: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("PASS: paired Windows TLS coordinator and managed drive")
    );
    assert_eq!(
        f.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.starts_with("PUT ") && r.contains("/writable/.s3drive/"))
            .count(),
        4,
        "seed, recovered payload and rival must not repeat PUT"
    );
    assert_eq!(
        f.objects
            .lock()
            .unwrap()
            .values()
            .filter(|(body, _)| body.as_slice() == b"paired Windows binary\0\xff")
            .count(),
        1,
        "actual immutable binary object missing"
    );
    assert!(!f.server.is_finished());
    f.server.abort();
    let _ = (&mut f.server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn paired_windows_fixture_sdk_tls_recovery_and_conflict_preparation() {
    use std::sync::atomic::Ordering;
    let f = fixture::Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    for which in ["first", "second"] {
        std::fs::create_dir(root.join(which)).unwrap();
    }
    writes::setup(&f, &root.join("first")).await;
    writes::setup_source(
        &f,
        &root.join("second"),
        "other-bucket",
        "docs2",
        "fixture-alt",
    )
    .await;
    let profile = root.join("ordinary-profile");
    std::fs::create_dir(&profile).unwrap();
    std::fs::write(root.join("profile-path.txt"), profile.to_str().unwrap()).unwrap();
    std::fs::write(
        root.join("managed-fixture.json"),
        serde_json::to_vec(&serde_json::json!({"endpoint":f.endpoint})).unwrap(),
    )
    .unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_s3drive"), root.join("s3drive.exe")).unwrap();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    std::fs::write(root.join("cert.pem"), cert.pem()).unwrap();
    std::fs::write(root.join("key.pem"), signing_key.serialize_pem()).unwrap();
    async fn run(root: &Path, mode: &str) {
        let profile = std::fs::read_to_string(root.join("profile-path.txt")).unwrap();
        let out = tokio::time::timeout(
            Duration::from_secs(100),
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "windows_authority_fixture_worker",
                    "--nocapture",
                ])
                .env("S3DRIVE_AUTHORITY_FIXTURE_ROOT", root)
                .env("S3DRIVE_AUTHORITY_FIXTURE_MODE", mode)
                .env(
                    "S3DRIVE_NATIVE_MANAGED_EXECUTABLE",
                    root.join("s3drive.exe"),
                )
                .env(
                    "AWS_SHARED_CREDENTIALS_FILE",
                    Path::new(&profile).join(".aws/credentials"),
                )
                .env("AWS_CONFIG_FILE", Path::new(&profile).join(".aws/config"))
                .env("AWS_EC2_METADATA_DISABLED", "true")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            out.status.success(),
            "session worker {mode}: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    run(root, "Init").await;
    let db = root.join("authority-state");
    let secret = root.join("authority-inputs/authority.token");
    let cert = root.join("authority-inputs/cert.pem");
    let key = root.join("authority-inputs/key.pem");
    let server = Server::start(command(
        &db,
        &secret,
        None,
        Some((&cert, &key)),
        "127.0.0.1:0",
    ))
    .await;
    let volume = server.volume.clone();
    let endpoint = server.endpoint.clone();
    std::fs::write(
        root.join("authority-identity.json"),
        serde_json::to_vec(&serde_json::json!({"endpoint":endpoint,"volume":volume})).unwrap(),
    )
    .unwrap();
    run(root, "Seed").await;
    run(root, "Verify").await;
    // Use an explicit child for edits too: SDK credential discovery is process
    // global, so no mutation of this test process's environment is permitted.
    let first = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "paired_windows_fixture_engine_child",
            "--nocapture",
        ])
        .env("S3DRIVE_WINDOWS_ENGINE_ROOT", root)
        .env(
            "AWS_SHARED_CREDENTIALS_FILE",
            profile.join(".aws/credentials"),
        )
        .env("AWS_CONFIG_FILE", profile.join(".aws/config"))
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut first = Server {
        child: first,
        endpoint: endpoint.clone(),
        volume: volume.clone(),
    };
    let mut lines = BufReader::new(first.child.stdout.take().unwrap()).lines();
    async fn until(
        lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
        expected: &str,
    ) {
        tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                if line == expected {
                    return;
                }
            }
            panic!("engine worker exited before {expected}")
        })
        .await
        .unwrap();
    }
    until(&mut lines, "ENGINE_READY").await;
    f.hide_committed.store(true, Ordering::SeqCst);
    first
        .child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"dirty\n")
        .await
        .unwrap();
    until(&mut lines, "ENGINE_UNKNOWN").await;
    run(root, "Snapshot").await;
    server.kill().await;
    let address = url::Url::parse(&endpoint).unwrap();
    let listen = format!("127.0.0.1:{}", address.port().unwrap());
    let server = Server::start(command(
        &db,
        &secret,
        Some(&volume),
        Some((&cert, &key)),
        &listen,
    ))
    .await;
    writes::restore_observation(&f);
    first
        .child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"recover\n")
        .await
        .unwrap();
    until(&mut lines, "ENGINE_RECOVERED").await;
    run(root, "CheckRecovered").await;
    first
        .child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"edit\n")
        .await
        .unwrap();
    until(&mut lines, "ENGINE_DIRTY").await;
    run(root, "Rival").await;
    first
        .child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"conflict\n")
        .await
        .unwrap();
    until(&mut lines, "ENGINE_CONFLICT").await;
    first.stop().await;
    run(root, "CheckConflict").await;
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "paired fixture engine subprocess only"]
async fn paired_windows_fixture_engine_child() {
    use std::io::{BufRead, Write};
    use std::sync::Arc;
    let root = PathBuf::from(std::env::var_os("S3DRIVE_WINDOWS_ENGINE_ROOT").unwrap());
    let c =
        s3drive_core::Config::parse(&std::fs::read_to_string(root.join("managed.toml")).unwrap())
            .unwrap();
    let fs = s3drive_fs::ReadOnlyFs::new(&c).await.unwrap();
    let dir = fs.lookup(s3drive_fs::ROOT, "資料 café 🦀").await.unwrap();
    let node = fs.lookup(dir.id, "日本語 %2F + café.bin").await.unwrap();
    let file = fs.open_write(node.id).await.unwrap();
    println!("ENGINE_READY");
    std::io::stdout().flush().unwrap();
    for line in std::io::stdin().lock().lines() {
        match line.unwrap().as_str() {
            "dirty" => {
                file.truncate(0).await.unwrap();
                file.write(
                    0,
                    Arc::from(&b"paired Windows binary\0\xff"[..]),
                    false,
                    false,
                )
                .await
                .unwrap();
                assert!(file.flush().await.is_err());
                println!("ENGINE_UNKNOWN");
            }
            "recover" => {
                file.flush().await.unwrap();
                println!("ENGINE_RECOVERED");
            }
            "edit" => {
                file.write(0, Arc::from(&b"dirty conflict"[..]), false, false)
                    .await
                    .unwrap();
                println!("ENGINE_DIRTY");
            }
            "conflict" => {
                assert!(file.flush().await.is_err());
                assert_eq!(&file.read(0, 14).await.unwrap(), b"dirty conflict");
                println!("ENGINE_CONFLICT");
            }
            _ => panic!("unknown engine fixture command"),
        }
        std::io::stdout().flush().unwrap();
    }
    drop(file);
    fs.cache_control().unwrap().close().await.unwrap();
}
