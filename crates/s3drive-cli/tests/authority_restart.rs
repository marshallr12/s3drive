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
