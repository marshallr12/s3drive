//! Actual CLI authority, SDK fixture and native adapters; no live cloud writes.
#[path = "managed/lifecycle.rs"]
mod lifecycle;
use super::{fixture::Fixture, writes};
use s3drive_authority::{Apply, Kind, Mutation, Reply, Request, Token, WireClient};
use s3drive_managed::Publisher;
use s3drive_state::{cache, journal, worker::Storage};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
};
const DIR: &str = "資料 café 🦀";
const FILE: &str = "日本語 %2F + café.bin";
const MOVED: &str = "renamed 日本語.bin";
fn id(n: u64) -> String {
    format!("{n:032x}")
}
fn data(revision: u8) -> Vec<u8> {
    (0..70_123)
        .map(|n| ((n + revision as usize) % 251) as u8)
        .collect()
}
async fn info(w: &WireClient) -> s3drive_authority::Volume {
    let Reply::Info(v) = w.request(&Request::Info {}).await.unwrap() else {
        panic!("volume")
    };
    v
}
async fn change(w: &WireClient, op: u64, mutation: Mutation) {
    let v = info(w).await;
    let r = w
        .request(&Request::Apply {
            request: Apply {
                volume: v.id,
                operation: id(op),
                expected_sequence: v.sequence,
                mutation,
            },
        })
        .await
        .unwrap();
    assert!(matches!(r, Reply::Applied(_)), "{r:?}");
}
#[derive(Clone)]
struct NamespaceFault {
    upstream: WireClient,
    cut: Arc<AtomicBool>,
    kind: u8,
    committed: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
async fn namespace_proxy(
    axum::extract::State(p): axum::extract::State<NamespaceFault>,
    axum::Json(request): axum::Json<Request>,
) -> axum::Json<Reply> {
    let kind = match &request {
        Request::Apply {
            request:
                Apply {
                    mutation: Mutation::CreateEntry { .. } | Mutation::CreateDeferredEntry { .. },
                    ..
                },
        } => 1,
        Request::Apply {
            request:
                Apply {
                    mutation: Mutation::MoveEntry { .. } | Mutation::MoveDeferredEntry { .. },
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
        Request::Apply {
            request:
                Apply {
                    mutation: Mutation::ArmRemoval { .. } | Mutation::TransitionRemoval { .. },
                    ..
                },
        } => 4,
        Request::Node { node, .. } if node != s3drive_authority::ROOT => 5,
        _ => 0,
    };
    let reply = p.upstream.request(&request).await.unwrap();
    if kind == p.kind
        && matches!(reply, Reply::Applied(_) | Reply::Node(_))
        && p.cut.swap(false, Ordering::SeqCst)
    {
        p.committed.notify_one();
        p.release.notified().await;
    }
    axum::Json(reply)
}
struct NamespaceProxy {
    state: NamespaceFault,
    endpoint: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for NamespaceProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl NamespaceProxy {
    async fn start(h: &Harness) -> Self {
        Self::start_kind(h, 1).await
    }
    async fn start_kind(h: &Harness, kind: u8) -> Self {
        let state = NamespaceFault {
            upstream: wire(&cfg(&h.config)),
            cut: Arc::new(AtomicBool::new(true)),
            kind,
            committed: Arc::new(tokio::sync::Notify::new()),
            release: Arc::new(tokio::sync::Notify::new()),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/namespace", listener.local_addr().unwrap());
        let app = axum::Router::new()
            .route("/v1/namespace", axum::routing::post(namespace_proxy))
            .with_state(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            state,
            endpoint,
            task,
        }
    }
    fn route(&self, h: &Harness) {
        let c = cfg(&h.config);
        let old = &c.managed.unwrap().authority;
        let text = std::fs::read_to_string(&h.config)
            .unwrap()
            .replace(old, &self.endpoint);
        // This owned fault-injection hop is numeric loopback HTTP; its upstream
        // keeps the original HTTPS trust. Never attach a TLS CA to HTTP.
        let text = text
            .lines()
            .filter(|line| !line.trim().starts_with("authority_ca_file"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&h.config, text).unwrap();
    }
    async fn committed(&self) {
        tokio::time::timeout(Duration::from_secs(30), self.state.committed.notified())
            .await
            .unwrap();
    }
}
struct Service {
    child: Child,
    endpoint: String,
    volume: String,
    ca_file: Option<PathBuf>,
}
impl Service {
    async fn start(root: &Path) -> Self {
        Self::start_policy(root, "exact-utf8-v1").await
    }
    async fn start_policy(root: &Path, policy: &str) -> Self {
        Self::start_options(root, policy, None).await
    }
    async fn start_tls(root: &Path, policy: &str) -> Self {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let cert_file =
            s3drive_state::create_private_secret(&root.join("tls-cert"), b"fixture").unwrap();
        let key_file =
            s3drive_state::create_private_secret(&root.join("tls-key"), b"fixture").unwrap();
        std::fs::write(&cert_file, cert.pem()).unwrap();
        std::fs::write(&key_file, signing_key.serialize_pem()).unwrap();
        Self::start_options(root, policy, Some((cert_file, key_file))).await
    }
    async fn start_options(root: &Path, policy: &str, tls: Option<(PathBuf, PathBuf)>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_s3drive"));
        command
            .arg("authority-serve")
            .args(["--name-policy", policy])
            .arg("--directory")
            .arg(root.join("authority"))
            .arg("--token-file")
            .arg(root.join("secrets/authority.token"))
            .arg("--stop-on-stdin-close")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        if let Some((cert, key)) = &tls {
            command
                .arg("--tls-cert")
                .arg(cert)
                .arg("--tls-key")
                .arg(key);
        }
        let mut child = command.spawn().unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let line = tokio::time::timeout(Duration::from_secs(20), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .expect("authority readiness");
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        Self {
            child,
            endpoint: v["endpoint"].as_str().unwrap().into(),
            volume: v["volume"].as_str().unwrap().into(),
            ca_file: tls.map(|(cert, _)| cert),
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
}
struct Harness {
    root: PathBuf,
    config: PathBuf,
    credentials: PathBuf,
    mount: PathBuf,
}
impl Harness {
    async fn use_tls(self, old: Service, f: &Fixture) -> (Self, Service) {
        let policy = info(&wire(&cfg(&self.config))).await.name_policy;
        let volume = old.volume.clone();
        old.stop().await;
        let server = Service::start_tls(&self.root, &policy).await;
        assert_eq!(
            server.volume, volume,
            "TLS restart changed the volume identity"
        );
        self.configure(&server, f);
        (self, server)
    }
    async fn new(root: &Path, f: &Fixture) -> (Self, Service) {
        Self::new_policy(root, f, false).await
    }
    async fn new_policy(root: &Path, f: &Fixture, portable: bool) -> (Self, Service) {
        Self::new_options(root, f, portable, false).await
    }
    async fn new_options(root: &Path, f: &Fixture, portable: bool, tls: bool) -> (Self, Service) {
        for name in ["first", "second"] {
            std::fs::create_dir(root.join(name)).unwrap();
        }
        writes::setup(f, &root.join("first")).await;
        writes::setup_source(
            f,
            &root.join("second"),
            "other-bucket",
            "docs2",
            "fixture-alt",
        )
        .await;
        let h = Self {
            root: root.into(),
            config: root.join("managed.toml"),
            credentials: root.join("combined-credentials"),
            mount: root.join("managed-mount"),
        };
        std::fs::write(&h.credentials, "[fixture]\naws_access_key_id=TESTPROFILE\naws_secret_access_key=TESTPROFILESECRET\n[fixture-alt]\naws_access_key_id=TESTPROFILE\naws_secret_access_key=TESTPROFILESECRET\n").unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_s3drive"))
            .arg("authority-token")
            .arg("--directory")
            .arg(root.join("secrets"))
            .output()
            .await
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.len() < 150);
        // A second invocation must fail rather than rotate an in-use token.
        assert!(
            !Command::new(env!("CARGO_BIN_EXE_s3drive"))
                .arg("authority-token")
                .arg("--directory")
                .arg(root.join("secrets"))
                .output()
                .await
                .unwrap()
                .status
                .success()
        );
        let policy = if portable {
            "portable-collision-v1"
        } else {
            "exact-utf8-v1"
        };
        let server = if tls {
            Service::start_tls(root, policy).await
        } else {
            Service::start_policy(root, policy).await
        };
        h.configure(&server, f);
        h.child("seed").await;
        h.configure_bootstrap(false, false).await;
        (h, server)
    }
    async fn configure_bootstrap(&self, writable: bool, lifecycle: bool) {
        if writable {
            self.configure_writable();
        }
        let c = cfg(&self.config);
        let managed = c.managed.as_ref().unwrap();
        let name = if lifecycle {
            "bootstrap-posix"
        } else if writable {
            "bootstrap-write"
        } else {
            "bootstrap-read"
        };
        let input = self.root.join(format!("{name}-input.toml"));
        let mut value: toml::Value =
            toml::from_str(&std::fs::read_to_string(&self.config).unwrap()).unwrap();
        if writable {
            // Input refers to the actual direct fixture journal. The generated
            // managed journal must be separate, even before either exists.
            let direct = cfg(&self.root.join("first/drive.toml"));
            value["writes"]["directory"] =
                direct.writes.unwrap().directory.to_str().unwrap().into();
        }
        std::fs::write(&input, toml::to_string(&value).unwrap()).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_s3drive"));
        command
            .args(["managed", "configure"])
            .arg(&input)
            .arg("--token-file")
            .arg(&managed.token_file)
            .arg("--directory")
            .arg(self.root.join(name));
        if let Some(ca) = &managed.authority_ca_file {
            command.arg("--ca-file").arg(ca);
        }
        for (n, alias) in [(200, &c.sources[0].alias), (201, &c.sources[1].alias)] {
            command
                .arg("--plan")
                .arg(self.root.join(format!("bootstrap-{n}/store-plan.toml")))
                .arg("--map")
                .arg(format!("{}={alias}", id(n)));
        }
        if writable {
            command
                .args(["--mode", "read-write", "--default-store", &id(200)])
                .arg("--state-directory")
                .arg(self.root.join("managed-write-state"));
        }
        if lifecycle {
            command.args(["--windows-disposition", "posix-v1"]);
        }
        let out = command.output().await.unwrap();
        assert!(
            out.status.success(),
            "bootstrap configure: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let out: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        std::fs::copy(out["config"].as_str().unwrap(), &self.config).unwrap();
        assert_eq!(
            cfg(&self.config).version,
            if lifecycle {
                7
            } else if writable {
                6
            } else {
                3
            }
        );
    }
    fn configure(&self, s: &Service, f: &Fixture) {
        let mut text = format!(
            "version=3\nmode='read-only'\n[managed]\nauthority={:?}\nvolume={:?}\ntoken_file={:?}\n",
            s.endpoint,
            s.volume,
            self.root.join("secrets/authority.token").to_str().unwrap()
        );
        if let Some(ca) = &s.ca_file {
            text += &format!("authority_ca_file={:?}\n", ca.to_str().unwrap());
        }
        for (n, alias) in [(200, "viewer-first"), (201, "viewer-second")] {
            text += &format!("[[managed.stores]]\nid={:?}\nsource={alias:?}\n", id(n));
        }
        for (alias, bucket, profile) in [
            ("viewer-first", "test-bucket", "fixture"),
            ("viewer-second", "other-bucket", "fixture-alt"),
        ] {
            text += &format!(
                "[[sources]]\nalias={alias:?}\nbucket={bucket:?}\nprofile={profile:?}\nprefix='desktop/'\nregion='us-east-1'\nendpoint={:?}\npath_style=true\nallow_http_loopback=true\n",
                f.endpoint
            );
        }
        std::fs::write(&self.config, text).unwrap();
    }
    fn edit_config(&self, edit: impl FnOnce(&mut toml::Value)) {
        let mut value: toml::Value =
            toml::from_str(&std::fs::read_to_string(&self.config).unwrap()).unwrap();
        edit(&mut value);
        let text = toml::to_string(&value).unwrap();
        s3drive_core::Config::parse(&text).unwrap();
        std::fs::write(&self.config, text).unwrap();
    }
    fn configure_writable(&self) {
        let mut writes = format!(
            "[writes]\ndirectory={:?}\n",
            self.root.join("managed-write-state").to_str().unwrap()
        );
        for (which, alias) in [("first", "docs"), ("second", "docs2")] {
            writes += &format!(
                "[[writes.sources]]\nalias={alias:?}\npolicy={:?}\nplan={:?}\nreport={:?}\n",
                self.root.join(which).join("policy.toml").to_str().unwrap(),
                self.root.join(which).join("plan.toml").to_str().unwrap(),
                self.root.join(which).join("report.toml").to_str().unwrap()
            );
        }
        let value: toml::Value = toml::from_str(&writes).unwrap();
        self.edit_config(|c| {
            c["version"] = 4.into();
            c["mode"] = "read-write".into();
            c["managed"]
                .as_table_mut()
                .unwrap()
                .insert("default_store".into(), id(200).into());
            c["managed"]
                .as_table_mut()
                .unwrap()
                .remove("windows-disposition");
            for (i, alias) in ["docs", "docs2"].iter().enumerate() {
                c["sources"][i]["alias"] = (*alias).into();
                c["managed"]["stores"][i]["source"] = (*alias).into();
            }
            c.as_table_mut()
                .unwrap()
                .insert("writes".into(), value["writes"].clone());
        });
    }
    fn configure_namespace(&self) {
        self.configure_writable();
        self.edit_config(|c| c["version"] = 5.into());
    }
    fn configure_mutations(&self) {
        self.configure_namespace();
        self.edit_config(|c| c["version"] = 6.into());
    }
    fn configure_lifecycle(&self) {
        self.configure_mutations();
        self.edit_config(|c| {
            c["version"] = 7.into();
            c["managed"]
                .as_table_mut()
                .unwrap()
                .insert("windows-disposition".into(), "posix-v1".into());
        });
    }
    fn environment(&self, c: &mut Command) {
        c.env("AWS_SHARED_CREDENTIALS_FILE", &self.credentials)
            .env("AWS_CONFIG_FILE", self.root.join("no-ambient-config"))
            .env("AWS_EC2_METADATA_DISABLED", "true");
    }
    async fn child(&self, mode: &str) {
        let mut c = Command::new(std::env::current_exe().unwrap());
        c.args(["--exact", "managed_native_child", "--nocapture"])
            .env("S3DRIVE_NATIVE_MANAGED_MODE", mode)
            .env("S3DRIVE_NATIVE_MANAGED_ROOT", &self.root)
            .kill_on_drop(true);
        self.environment(&mut c);
        let out = tokio::time::timeout(Duration::from_secs(100), c.output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            out.status.success(),
            "{mode}: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    async fn mount(&self) -> Mount {
        #[cfg(target_os = "linux")]
        if !self.mount.exists() {
            std::fs::create_dir(&self.mount).unwrap();
        }
        let mut c = Command::new(env!("CARGO_BIN_EXE_s3drive"));
        c.arg("mount")
            .arg(&self.config)
            .arg(&self.mount)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        c.arg("--unmount-on-stdin-close");
        self.environment(&mut c);
        let mut m = Mount {
            child: c.spawn().unwrap(),
            path: self.mount.clone(),
            diagnostics: None,
        };
        let mut lines = BufReader::new(m.child.stderr.take().unwrap()).lines();
        tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                eprintln!("{line}");
                if line.starts_with("Mounted read-only") || line.starts_with("Mounted read-write") {
                    return;
                }
            }
            panic!("managed mount failed before readiness");
        })
        .await
        .unwrap();
        m.diagnostics = Some(tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("{line}");
            }
        }));
        m
    }
}
// Start before native file handles exist: Unix fork/exec closes inherited
// CLOEXEC descriptors, and those closes legitimately cause FUSE flush callbacks.
struct Producer {
    child: Child,
    lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
}
impl Producer {
    async fn start(h: &Harness) -> Self {
        let mut c = Command::new(std::env::current_exe().unwrap());
        c.args(["--exact", "managed_native_child", "--nocapture"])
            .env("S3DRIVE_NATIVE_MANAGED_MODE", "producer")
            .env("S3DRIVE_NATIVE_MANAGED_ROOT", &h.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        h.environment(&mut c);
        let mut child = c.spawn().unwrap();
        let lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut actor = Self { child, lines };
        actor.until("PRODUCER_READY").await;
        actor
    }
    async fn until(&mut self, expected: &str) {
        tokio::time::timeout(Duration::from_secs(100), async {
            while let Some(line) = self.lines.next_line().await.unwrap() {
                if line == expected {
                    return;
                }
            }
            panic!("producer exited before acknowledgment")
        })
        .await
        .unwrap();
    }
    async fn send(&mut self, command: &str) {
        let input = self.child.stdin.as_mut().unwrap();
        input
            .write_all(format!("{command}\n").as_bytes())
            .await
            .unwrap();
        input.flush().await.unwrap();
        self.until("PRODUCER_DONE").await;
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
}
struct Mount {
    diagnostics: Option<tokio::task::JoinHandle<()>>,
    child: Child,
    path: PathBuf,
}
impl Drop for Mount {
    fn drop(&mut self) {
        if let Some(task) = self.diagnostics.take() {
            task.abort();
        }
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("fusermount3")
            .args(["-uz"])
            .arg(&self.path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = self.child.start_kill();
    }
}
impl Mount {
    async fn kill(mut self) {
        self.child.kill().await.unwrap();
        #[cfg(target_os = "linux")]
        assert!(
            std::process::Command::new("fusermount3")
                .args(["-uz"])
                .arg(&self.path)
                .status()
                .unwrap()
                .success()
        );
        #[cfg(windows)]
        tokio::time::timeout(Duration::from_secs(20), async {
            while std::fs::symlink_metadata(&self.path).is_ok() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
    }
    async fn stop(mut self) {
        #[cfg(target_os = "linux")]
        assert!(
            Command::new("kill")
                .arg("-TERM")
                .arg(self.child.id().unwrap().to_string())
                .status()
                .await
                .unwrap()
                .success()
        );
        #[cfg(windows)]
        drop(self.child.stdin.take());
        assert!(
            tokio::time::timeout(Duration::from_secs(20), self.child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
        #[cfg(target_os = "linux")]
        assert!(std::fs::read_dir(&self.path).unwrap().next().is_none());
        #[cfg(windows)]
        assert!(std::fs::symlink_metadata(&self.path).is_err());
    }
}
fn cfg(path: &Path) -> s3drive_core::Config {
    s3drive_core::Config::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}
fn wire(c: &s3drive_core::Config) -> WireClient {
    let m = c.managed.as_ref().unwrap();
    WireClient::from_config(m, Token::from_private_file(&m.token_file).unwrap()).unwrap()
}
async fn publish(
    root: &Path,
    w: &WireClient,
    which: &str,
    store: u64,
    node: u64,
    op: u64,
    bytes: &[u8],
) {
    let c = cfg(&root.join(which).join("drive.toml"));
    let s = &c.sources[0];
    let settings = &c.writes.as_ref().unwrap().sources[0];
    let policy = s3drive_s3::uploads::WritePolicy::from_toml(
        s,
        &std::fs::read_to_string(&settings.policy).unwrap(),
        &std::fs::read_to_string(&settings.plan).unwrap(),
        &std::fs::read_to_string(&settings.report).unwrap(),
    )
    .unwrap();
    let reader = s3drive_s3::S3Reader::from_source(s).await.unwrap();
    let storage = Storage::open(
        &root.join(format!("producer-{which}")),
        cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap();
    let exec = s3drive_s3::uploads::Executor::open(reader, storage, policy)
        .await
        .unwrap();
    let v = info(w).await;
    let p = Publisher::open(exec, w.clone(), v.id.clone(), id(store))
        .await
        .unwrap();
    let Reply::Node(n) = w
        .request(&Request::Node {
            volume: v.id.clone(),
            node: id(node),
        })
        .await
        .unwrap()
    else {
        panic!("node")
    };
    let sha = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    p.stage(
        Apply {
            volume: v.id,
            operation: id(op),
            expected_sequence: v.sequence,
            mutation: Mutation::Reserve {
                node: n.id,
                expected_revision: n.revision,
                store: id(store),
                size: bytes.len() as u64,
                sha256: sha,
            },
        },
        id(op + 1000),
        Arc::from(bytes),
    )
    .await
    .unwrap();
    p.flush(&id(op)).await.unwrap();
    p.storage().close().await.unwrap();
}
pub async fn child() {
    let Ok(mode) = std::env::var("S3DRIVE_NATIVE_MANAGED_MODE") else {
        return;
    };
    let root = PathBuf::from(std::env::var("S3DRIVE_NATIVE_MANAGED_ROOT").unwrap());
    let c = cfg(&root.join("managed.toml"));
    let w = wire(&c);
    match mode.as_str() {
        "lifecycle-engine" => Box::pin(lifecycle::engine(&c)).await,
        "lifecycle-policy" => Box::pin(lifecycle::policy(&c)).await,
        "lifecycle-open-race" => Box::pin(lifecycle::open_race(c)).await,
        "producer" => {
            use std::io::BufRead;
            println!("PRODUCER_READY");
            std::io::stdout().flush().unwrap();
            for command in std::io::stdin().lock().lines() {
                match command.unwrap().as_str() {
                    "rename" => {
                        change(
                            &w,
                            3041,
                            Mutation::Move {
                                node: id(11),
                                parent: id(10),
                                name: MOVED.into(),
                            },
                        )
                        .await
                    }
                    "update" => publish(&root, &w, "first", 200, 11, 1040, &data(1)).await,
                    _ => panic!("unknown producer command"),
                }
                println!("PRODUCER_DONE");
                std::io::stdout().flush().unwrap();
            }
        }
        "seed" => {
            for (n, which) in [(200, "first"), (201, "second")] {
                let input = root.join(which).join("drive.toml");
                let local = cfg(&input);
                let m = c.managed.as_ref().unwrap();
                let mut command = Command::new(env!("CARGO_BIN_EXE_s3drive"));
                command
                    .args(["managed", "store-plan"])
                    .arg(&input)
                    .arg(&local.sources[0].alias)
                    .args([
                        "--authority",
                        &m.authority,
                        "--volume",
                        &m.volume,
                        "--root",
                        "writable/.s3drive/",
                        "--store-id",
                        &id(n),
                    ])
                    .arg("--token-file")
                    .arg(&m.token_file)
                    .arg("--directory")
                    .arg(root.join(format!("bootstrap-{n}")));
                if let Some(ca) = &m.authority_ca_file {
                    command.arg("--ca-file").arg(ca);
                }
                let out = command.output().await.unwrap();
                assert!(
                    out.status.success(),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                let plan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
                let mut command = Command::new(env!("CARGO_BIN_EXE_s3drive"));
                command
                    .args(["managed", "store-apply"])
                    .arg(&input)
                    .arg(&local.sources[0].alias)
                    .arg(plan["plan"].as_str().unwrap())
                    .arg("--token-file")
                    .arg(&m.token_file)
                    .arg("--confirm")
                    .arg(plan["plan_sha256"].as_str().unwrap());
                if let Some(ca) = &m.authority_ca_file {
                    command.arg("--ca-file").arg(ca);
                }
                let out = command.output().await.unwrap();
                assert!(
                    out.status.success(),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
                assert_eq!(result["status"], "registered");
            }
            for (n, parent, name, kind) in [
                (10, 0, DIR, Kind::Directory),
                (11, 10, FILE, Kind::EmptyFile),
                (12, 0, "other bucket.txt", Kind::EmptyFile),
                (13, 0, "empty 🦀", Kind::EmptyFile),
                (20, 0, "ambiguous", Kind::Directory),
                (21, 20, "Case", Kind::EmptyFile),
                (22, 20, "case", Kind::EmptyFile),
            ] {
                if n == 22 && info(&w).await.name_policy == "portable-collision-v1" {
                    continue;
                }
                let mutation = if info(&w).await.name_policy == "portable-collision-v1" {
                    Mutation::CreateEntry {
                        parent: id(parent),
                        parent_revision: match w
                            .request(&Request::Node {
                                volume: info(&w).await.id,
                                node: id(parent),
                            })
                            .await
                            .unwrap()
                        {
                            Reply::Node(n) => n.revision,
                            _ => panic!("parent"),
                        },
                        store: if kind == Kind::EmptyFile {
                            Some(id(if n == 12 { 201 } else { 200 }))
                        } else {
                            None
                        },
                        name: name.into(),
                        node_kind: kind,
                    }
                } else {
                    Mutation::Create {
                        parent: id(parent),
                        name: name.into(),
                        node_kind: kind,
                    }
                };
                change(&w, n, mutation).await;
            }
            publish(&root, &w, "first", 200, 11, 1000, &data(0)).await;
            publish(
                &root,
                &w,
                "second",
                201,
                12,
                1001,
                b"payload from the second bucket",
            )
            .await;
        }
        "update" => publish(&root, &w, "first", 200, 11, 1040, &data(1)).await,
        "rename" => {
            change(
                &w,
                3041,
                Mutation::Move {
                    node: id(11),
                    parent: id(10),
                    name: MOVED.into(),
                },
            )
            .await
        }
        "unlink" => change(&w, 3042, Mutation::Remove { node: id(11) }).await,
        "namespace-uncertain" => {
            let fs = s3drive_fs::ReadOnlyFs::new(&c).await.unwrap();
            fs.create_directory(s3drive_fs::ROOT, "recovered 🦀")
                .await
                .unwrap();
            panic!("fault injection failed to hold metadata reply");
        }
        "namespace-recovered" => {
            let fs = s3drive_fs::ReadOnlyFs::new(&c).await.unwrap();
            assert!(fs.lookup(s3drive_fs::ROOT, "recovered 🦀").await.is_ok());
            fs.cache_control().unwrap().close().await.unwrap();
        }
        "namespace-move-uncertain" => Box::pin(namespace_move_uncertain_mode(&c)).await,
        "namespace-move-recovered" => Box::pin(namespace_move_recovered_mode(&c)).await,
        "namespace-remove-uncertain" => Box::pin(namespace_remove_uncertain_mode(&c)).await,
        "namespace-remove-recovered" => Box::pin(namespace_remove_recovered_mode(&c)).await,
        "namespace-retained-revisions" => {
            Box::pin(namespace_retained_revisions_mode(&c, &root)).await
        }
        "namespace-cancel" => Box::pin(namespace_cancel_mode(&c, &root)).await,
        "namespace-mutations" => Box::pin(namespace_mutations_mode(&c)).await,
        "namespace-engine" => {
            let fs = s3drive_fs::ReadOnlyFs::new(&c).await.unwrap();
            assert!(fs.namespace_creation());
            let directory = fs
                .create_directory(s3drive_fs::ROOT, "new 資料 🦀")
                .await
                .unwrap();
            let file = fs
                .create_file(directory.id, "café %2F.bin", |a, b| a == b, true)
                .await
                .unwrap();
            assert!(file.read(0, 100).await.unwrap().is_empty());
            file.write(0, Arc::from(&b"new durable content"[..]), false, false)
                .await
                .unwrap();
            file.flush().await.unwrap();
            assert_eq!(
                fs.open(file.node.id)
                    .await
                    .unwrap()
                    .read(0, 100)
                    .await
                    .unwrap(),
                b"new durable content"
            );
            assert!(
                fs.create_file(directory.id, "CAFE\u{301} %2F.bin", |a, b| a == b, true)
                    .await
                    .is_err()
            );
            assert!(
                fs.create_directory(s3drive_fs::ROOT, "NEW 資料 🦀")
                    .await
                    .is_err()
            );
            assert!(
                fs.create_directory(s3drive_fs::ROOT, "con.txt")
                    .await
                    .is_err()
            );
            fs.cache_control().unwrap().close().await.unwrap();
        }
        "namespace-placement" => {
            let mut a = cfg(&root.join("managed.toml"));
            a.managed.as_mut().unwrap().default_store = Some(id(201));
            let first = s3drive_fs::ReadOnlyFs::new(&a).await.unwrap();
            let empty = first
                .create_file(s3drive_fs::ROOT, "placement 🦀", |a, b| a == b, true)
                .await
                .unwrap();
            drop(empty);
            first.cache_control().unwrap().close().await.unwrap();
            drop(first);
            let mut peer = cfg(&root.join("managed.toml"));
            peer.writes.as_mut().unwrap().directory = root.join("managed-peer-state");
            assert_eq!(
                peer.managed.as_ref().unwrap().default_store.as_deref(),
                Some(id(200).as_str())
            );
            let second = s3drive_fs::ReadOnlyFs::new(&peer).await.unwrap();
            let n = second
                .lookup(s3drive_fs::ROOT, "placement 🦀")
                .await
                .unwrap();
            let file = second.open_write(n.id).await.unwrap();
            file.write(0, Arc::from(&b"peer follows placement"[..]), false, false)
                .await
                .unwrap();
            file.flush().await.unwrap();
            drop(file);
            second.cache_control().unwrap().close().await.unwrap();
            drop(second);
            let Reply::Node(n) = w
                .request(&Request::Node {
                    volume: info(&w).await.id,
                    node: match w
                        .request(&Request::List {
                            volume: info(&w).await.id,
                            parent: id(0),
                        })
                        .await
                        .unwrap()
                    {
                        Reply::List(l) => {
                            l.children
                                .into_iter()
                                .find(|n| n.name == "placement 🦀")
                                .unwrap()
                                .id
                        }
                        _ => panic!("list"),
                    },
                })
                .await
                .unwrap()
            else {
                panic!("node")
            };
            assert_eq!(n.placement.as_ref().unwrap().id, id(201));
            assert_eq!(n.payload.as_ref().unwrap().reservation.store.id, id(201));
        }
        "namespace-check" => {
            let fs = s3drive_fs::ReadOnlyFs::new(&c).await.unwrap();
            let d = fs.lookup(s3drive_fs::ROOT, "new 資料 🦀").await.unwrap();
            let n = fs.lookup(d.id, "café %2F.bin").await.unwrap();
            assert_eq!(
                fs.open(n.id).await.unwrap().read(0, 100).await.unwrap(),
                b"new durable content"
            );
            fs.cache_control().unwrap().close().await.unwrap();
        }
        "rw-stage" => {
            let fs = s3drive_fs::ReadOnlyFs::new(&c).await.unwrap();
            assert!(!fs.is_read_only());
            let d = fs.lookup(s3drive_fs::ROOT, DIR).await.unwrap();
            let n = fs.lookup(d.id, FILE).await.unwrap();
            assert!(fs.writable(&n));
            assert!(!fs.writable_directory(&d));
            let file = fs.open_write(n.id).await.unwrap();
            let reader = fs.open(n.id).await.unwrap();
            assert_eq!(
                file.write(2, Arc::from(&b"ABC"[..]), false, false)
                    .await
                    .unwrap(),
                3
            );
            assert_eq!(reader.read(2, 3).await.unwrap(), b"ABC");
            file.truncate(5).await.unwrap();
            file.write(0, Arc::from(&b"xyz"[..]), true, false)
                .await
                .unwrap();
            file.write(12, Arc::from(&b"!"[..]), false, false)
                .await
                .unwrap();
            assert_eq!(reader.read(5, 100).await.unwrap(), b"xyz\0\0\0\0!");
            assert_eq!(file.current_metadata().unwrap().size, 13);
            assert!(file.truncate(8 * 1024 * 1024 + 1).await.is_err());
            reader.flush_on_close().await.unwrap();
            // No writable close/fsync: process exit retains the durable head.
        }
        "rw-resume" => {
            let fs = s3drive_fs::ReadOnlyFs::new(&c).await.unwrap();
            let d = fs.lookup(s3drive_fs::ROOT, DIR).await.unwrap();
            let n = fs.lookup(d.id, FILE).await.unwrap();
            assert_eq!(
                fs.metadata(n.id).await.unwrap().unwrap().size,
                13,
                "dirty size before first open after restart"
            );
            let file = fs.open_write(n.id).await.unwrap();
            assert_eq!(file.read(5, 100).await.unwrap(), b"xyz\0\0\0\0!");
            file.flush().await.unwrap();
            file.flush().await.unwrap();
            let empty = fs.lookup(s3drive_fs::ROOT, "empty 🦀").await.unwrap();
            let empty = fs.open_write(empty.id).await.unwrap();
            empty
                .write(0, Arc::from(&b"x"[..]), false, false)
                .await
                .unwrap();
            empty.truncate(0).await.unwrap();
            empty.flush().await.unwrap();
            assert!(empty.read(0, 100).await.unwrap().is_empty());
            drop(empty);
            // Two stores share the authority and local owner, without alias keys.
            let other = fs
                .lookup(s3drive_fs::ROOT, "other bucket.txt")
                .await
                .unwrap();
            let second = fs.open_write(other.id).await.unwrap();
            second.truncate(0).await.unwrap();
            second
                .write(0, Arc::from(&b"second-local"[..]), false, false)
                .await
                .unwrap();
            second.flush().await.unwrap();
            drop(second);
            // A metadata-only rename must not invalidate the content guard.
            file.write(0, Arc::from(&b"rename-safe"[..]), false, false)
                .await
                .unwrap();
            change(
                &w,
                3041,
                Mutation::Move {
                    node: id(11),
                    parent: id(10),
                    name: MOVED.into(),
                },
            )
            .await;
            file.flush().await.unwrap();
            assert_eq!(file.read(0, 11).await.unwrap(), b"rename-safe");
            let observer = fs.open(n.id).await.unwrap();
            drop(observer);
            file.write(0, Arc::from(&b"rename-safe"[..]), false, false)
                .await
                .unwrap();
            let observer = fs.open(n.id).await.unwrap();
            assert_eq!(observer.read(0, 11).await.unwrap(), b"rename-safe");
            drop(observer);
            file.flush().await.unwrap();
            drop(file);
            // Verified clean refresh adopts another publisher's content. Old
            // handles remain pinned and cannot write their stale original base.
            let old = fs.open_write(n.id).await.unwrap();
            publish(&root, &w, "first", 200, 11, 1040, &data(1)).await;
            let fresh = fs.open_write(n.id).await.unwrap();
            assert_eq!(fresh.read(0, 100).await.unwrap(), data(1)[..100]);
            assert_eq!(old.read(0, 11).await.unwrap(), b"rename-safe");
            assert!(old.flush().await.is_err());
            assert!(
                old.write(0, Arc::from(&b"stale"[..]), false, false)
                    .await
                    .is_err()
            );
            drop(old);
            fresh
                .write(0, Arc::from(&b"dirty conflict"[..]), false, false)
                .await
                .unwrap();
            publish(&root, &w, "first", 200, 11, 1050, &data(2)).await;
            assert!(fresh.flush().await.is_err());
            assert_eq!(fresh.read(0, 14).await.unwrap(), b"dirty conflict");
            assert!(fs.open(n.id).await.is_err());
        }
        "rw-conflict-reopen" => {
            let fs = s3drive_fs::ReadOnlyFs::new(&c).await.unwrap();
            let d = fs.lookup(s3drive_fs::ROOT, DIR).await.unwrap();
            let n = fs.lookup(d.id, MOVED).await.unwrap();
            assert!(fs.open_write(n.id).await.is_err());
            assert!(fs.flush_all().await.is_err());
            fs.cache_control().unwrap().close().await.unwrap();
            drop(fs);
            // Removing a store's policy cannot silently expose its old remote
            // bytes over a retained dirty local head.
            let mut c = cfg(&root.join("managed.toml"));
            c.writes
                .as_mut()
                .unwrap()
                .sources
                .retain(|s| s.alias == "docs2");
            c.managed.as_mut().unwrap().default_store = Some(id(201));
            let fs = s3drive_fs::ReadOnlyFs::new(&c).await.unwrap();
            let d = fs.lookup(s3drive_fs::ROOT, DIR).await.unwrap();
            let n = fs.lookup(d.id, MOVED).await.unwrap();
            assert!(fs.open(n.id).await.is_err());
            assert!(fs.metadata(n.id).await.is_err());
        }
        "engine" => {
            let fs = s3drive_fs::ReadOnlyFs::new(&c).await.unwrap();
            assert!(fs.is_read_only());
            assert!(fs.cache_control().is_none());
            let d = fs.lookup(s3drive_fs::ROOT, DIR).await.unwrap();
            let n = fs.lookup(d.id, FILE).await.unwrap();
            assert!(std::mem::size_of_val(&fs.open(n.id)) < 16 * 1024);
            let old = fs.open(n.id).await.unwrap();
            assert_eq!(old.read(123, 2048).await.unwrap(), data(0)[123..2171]);
            assert!(fs.open_write(n.id).await.is_err());
            let other = fs
                .lookup(s3drive_fs::ROOT, "other bucket.txt")
                .await
                .unwrap();
            assert_eq!(
                fs.open(other.id).await.unwrap().read(0, 100).await.unwrap(),
                b"payload from the second bucket"
            );
            let empty = fs.lookup(s3drive_fs::ROOT, "empty 🦀").await.unwrap();
            assert!(
                fs.open(empty.id)
                    .await
                    .unwrap()
                    .read(0, 100)
                    .await
                    .unwrap()
                    .is_empty()
            );
            // Renaming aliases and profiles does not change global store identity;
            // pointing an existing store ID at a different bucket must fail.
            let mut wrong = cfg(&root.join("managed.toml"));
            wrong.sources[0].bucket = "other-bucket".into();
            assert!(s3drive_fs::ReadOnlyFs::new(&wrong).await.is_err());
            publish(&root, &w, "first", 200, 11, 1040, &data(1)).await;
            assert_eq!(
                fs.open(n.id).await.unwrap().read(0, 100).await.unwrap(),
                data(1)[..100]
            );
            assert_eq!(old.read(0, 100).await.unwrap(), data(0)[..100]);
            change(
                &w,
                3041,
                Mutation::Move {
                    node: id(11),
                    parent: id(10),
                    name: MOVED.into(),
                },
            )
            .await;
            assert!(fs.lookup(d.id, FILE).await.is_err());
            assert_eq!(fs.lookup(d.id, MOVED).await.unwrap().id, n.id);
            change(&w, 3042, Mutation::Remove { node: id(11) }).await;
            assert!(fs.open(n.id).await.is_err());
            assert_eq!(old.read(0, 100).await.unwrap(), data(0)[..100]);
        }
        _ => panic!("unexpected helper mode"),
    }
}
fn puts(f: &Fixture) -> usize {
    f.requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.starts_with("PUT ") && r.contains("/writable/.s3drive/"))
        .count()
}
fn mutations(f: &Fixture) -> usize {
    f.requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| !r.starts_with("GET ") && !r.starts_with("HEAD "))
        .count()
}
pub async fn portable() {
    let f = Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (h, s) = Harness::new(tmp.path(), &f).await;
    let before = mutations(&f);
    h.child("engine").await;
    assert_eq!(
        mutations(&f),
        before + 1,
        "only the producer update may mutate S3"
    );
    assert_eq!(
        puts(&f),
        3,
        "two initial immutable payloads and one updated payload"
    );
    let volume = s.volume.clone();
    s.stop().await;
    let s = Service::start(tmp.path()).await;
    assert_eq!(s.volume, volume);
    h.configure(&s, &f);
    s.stop().await;
    assert!(!f.server.is_finished());
}
pub async fn native() {
    let f = Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (h, s) = Harness::new(tmp.path(), &f).await;
    let before = mutations(&f);
    let m = h.mount().await;
    let path = h.mount.join(DIR).join(FILE);
    assert_eq!(std::fs::read(&path).unwrap(), data(0));
    assert_eq!(
        std::fs::read(h.mount.join("other bucket.txt")).unwrap(),
        b"payload from the second bucket"
    );
    assert!(std::fs::read(h.mount.join("empty 🦀")).unwrap().is_empty());
    assert!(!h.mount.join("viewer-first").exists());
    assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err());
    assert!(std::fs::write(h.mount.join("new-file"), b"denied").is_err());
    let mut old = std::fs::File::open(&path).unwrap();
    let mut prefix = [0; 128];
    old.read_exact(&mut prefix).unwrap();
    assert_eq!(mutations(&f), before, "read-only mount emitted a mutation");
    h.child("update").await;
    assert_eq!(std::fs::read(&path).unwrap(), data(1));
    h.child("rename").await;
    assert!(!path.exists());
    let moved = h.mount.join(DIR).join(MOVED);
    assert_eq!(std::fs::read(&moved).unwrap(), data(1));
    f.deny_head.store(true, Ordering::SeqCst);
    assert!(std::fs::File::open(&moved).is_err());
    f.deny_head.store(false, Ordering::SeqCst);
    h.child("unlink").await;
    assert!(!moved.exists());
    old.seek(SeekFrom::Start(0)).unwrap();
    let mut bytes = Vec::new();
    old.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, data(0));
    #[cfg(target_os = "linux")]
    assert_eq!(
        std::fs::read_dir(h.mount.join("ambiguous"))
            .unwrap()
            .count(),
        2
    );
    #[cfg(windows)]
    assert!(
        std::fs::read_dir(h.mount.join("ambiguous"))
            .and_then(|mut it| it.try_for_each(|e| e.map(|_| ())))
            .is_err()
    );
    // Once opened, a verified snapshot has no dependency on authority availability.
    let mut pinned = std::fs::File::open(h.mount.join("other bucket.txt")).unwrap();
    s.stop().await;
    assert!(std::fs::File::open(h.mount.join("other bucket.txt")).is_err());
    let mut bytes = Vec::new();
    pinned.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"payload from the second bucket");
    drop(old);
    drop(pinned);
    m.stop().await;
    let s = Service::start(tmp.path()).await;
    h.configure(&s, &f);
    let m = h.mount().await;
    assert_eq!(
        std::fs::read(h.mount.join("other bucket.txt")).unwrap(),
        b"payload from the second bucket"
    );
    assert!(!h.mount.join(DIR).join(MOVED).exists());
    m.stop().await;
    s.stop().await;
    assert_eq!(puts(&f), 3);
    assert_eq!(
        mutations(&f),
        before + 1,
        "only the producer update may mutate S3"
    );
    assert_eq!(
        f.objects
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.contains("/writable/.s3drive/"))
            .count(),
        3,
        "unlinked and superseded payloads retained"
    );
    eprintln!(
        "Managed native acceptance: Unicode, two buckets, old/new handles, rename/unlink, authorization denial, authority restart; exactly 3 immutable PUTs"
    );
}

pub async fn portable_writes() {
    let f = Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (h, s) = Harness::new(tmp.path(), &f).await;
    let (h, s) = h.use_tls(s, &f).await;
    h.configure_writable();
    let before = puts(&f);
    h.child("rw-stage").await;
    assert_eq!(puts(&f), before, "local writes/read-only close emitted PUT");
    h.child("rw-resume").await;
    assert_eq!(
        puts(&f),
        before + 7,
        "empty publication, coalesced flushes, second store, rename and two producer updates"
    );
    h.child("rw-conflict-reopen").await;
    assert_eq!(puts(&f), before + 7, "conflict replay emitted PUT");
    s.stop().await;
    eprintln!(
        "PASS: managed CLI/shared-FS writes and exact recovery through native Rust HTTPS with private CA"
    );
}

pub async fn bootstrap_write_refusals() {
    let f = Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (h, s) = Harness::new_policy(tmp.path(), &f, true).await;
    let source = h.root.join("first/drive.toml");
    let input = cfg(&source);
    let plan = h.root.join("bootstrap-200/store-plan.toml");
    let output = h.root.join("must-not-generate");
    let dirty = h.root.join("retained-dirty-state");
    std::fs::create_dir(&dirty).unwrap();
    std::fs::write(dirty.join("dirty-fixture"), b"preserve bytes\0\xff").unwrap();
    let sequence = info(&wire(&cfg(&h.config))).await.sequence;
    let mutations_before = mutations(&f);
    for state in [&dirty, &input.writes.as_ref().unwrap().directory] {
        let out = Command::new(env!("CARGO_BIN_EXE_s3drive"))
            .args(["managed", "configure"])
            .arg(&source)
            .arg("--plan")
            .arg(&plan)
            .arg("--token-file")
            .arg(h.root.join("secrets/authority.token"))
            .args(["--mode", "read-write", "--default-store", &id(200)])
            .arg("--state-directory")
            .arg(state)
            .arg("--directory")
            .arg(&output)
            .output()
            .await
            .unwrap();
        assert!(!out.status.success());
        assert!(!output.exists());
    }
    let policy = h.root.join("first/policy.toml");
    let text = std::fs::read_to_string(&policy).unwrap();
    std::fs::write(
        &policy,
        text.replace("writable_prefix='writable/'", "writable_prefix='denied/'"),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_s3drive"))
        .args(["managed", "configure"])
        .arg(&source)
        .arg("--plan")
        .arg(&plan)
        .arg("--token-file")
        .arg(h.root.join("secrets/authority.token"))
        .args(["--mode", "read-write", "--default-store", &id(200)])
        .arg("--state-directory")
        .arg(h.root.join("fresh-managed-state"))
        .arg("--directory")
        .arg(&output)
        .output()
        .await
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("outside the current write policy"));
    assert!(!output.exists());
    assert_eq!(
        std::fs::read(dirty.join("dirty-fixture")).unwrap(),
        b"preserve bytes\0\xff"
    );
    assert_eq!(info(&wire(&cfg(&h.config))).await.sequence, sequence);
    assert_eq!(
        mutations(&f),
        mutations_before,
        "setup refusal performed a provider mutation"
    );
    s.stop().await;
}

pub async fn native_writes() {
    let f = Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (h, s) = Harness::new(tmp.path(), &f).await;
    let (h, s) = h.use_tls(s, &f).await;
    h.configure_writable();
    let mut producer = Producer::start(&h).await;
    let seed_puts = puts(&f);
    let m = h.mount().await;
    let empty = h.mount.join("empty 🦀");
    let mut empty = std::fs::OpenOptions::new()
        .write(true)
        .open(&empty)
        .unwrap();
    empty.write_all(b"x").unwrap();
    empty.set_len(0).unwrap();
    empty.sync_all().unwrap();
    drop(empty);
    assert_eq!(puts(&f), seed_puts + 1);
    let before = puts(&f);
    let path = h.mount.join(DIR).join(FILE);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.set_len(0).unwrap();
    file.write_all(b"abcdefghij").unwrap();
    file.seek(SeekFrom::Start(2)).unwrap();
    file.write_all(b"XY\0\xff").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"abXY\0\xffghij");
    assert_eq!(puts(&f), before, "read-only close flushed dirty file");
    file.sync_all().unwrap();
    assert_eq!(puts(&f), before + 1);
    file.set_len(5).unwrap();
    let mut append = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    append.write_all(b"tail").unwrap();
    append.sync_all().unwrap();
    drop(append);
    file.seek(SeekFrom::Start(12)).unwrap();
    file.write_all(b"!").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"abXY\0tail\0\0\0!");
    assert!(file.set_len(8 * 1024 * 1024 + 1).is_err());
    assert!(std::fs::write(h.mount.join("new file"), b"unsupported").is_err());
    let other = h.mount.join("other bucket.txt");
    let mut second = std::fs::OpenOptions::new()
        .write(true)
        .open(&other)
        .unwrap();
    second.set_len(0).unwrap();
    second.write_all(b"second-local").unwrap();
    second.sync_all().unwrap();
    drop(second);
    producer.send("rename").await;
    file.sync_all().unwrap();
    assert_eq!(puts(&f), before + 4);
    let moved = h.mount.join(DIR).join(MOVED);
    assert_eq!(std::fs::read(&moved).unwrap(), b"abXY\0tail\0\0\0!");
    file.seek(SeekFrom::Start(0)).unwrap();
    file.write_all(b"durable").unwrap();
    let expected = std::fs::read(&moved).unwrap();
    assert_eq!(puts(&f), before + 4);
    m.kill().await;
    drop(file);
    let m = h.mount().await;
    assert_eq!(std::fs::read(&moved).unwrap(), expected);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&moved)
        .unwrap();
    file.sync_all().unwrap();
    assert_eq!(puts(&f), before + 5, "dirty restart replayed writes");
    // Actual SDK PUT commits, loses its reply, and refuses observation. The
    // native write is locally durable; fsync must fail and retain its IDs.
    f.hide_committed.store(true, Ordering::SeqCst);
    file.write_all(b"lost").unwrap();
    assert_eq!(puts(&f), before + 5);
    assert!(file.sync_all().is_err());
    assert_eq!(puts(&f), before + 6);
    m.kill().await;
    drop(file);
    writes::restore_observation(&f);
    let m = h.mount().await;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&moved)
        .unwrap();
    file.sync_all().unwrap();
    assert_eq!(puts(&f), before + 6, "uncertain PUT replayed after restart");
    assert_eq!(&std::fs::read(&moved).unwrap()[..4], b"lost");
    drop(file);
    m.stop().await;
    let m = h.mount().await;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&moved)
        .unwrap();
    // Another machine wins while local edits remain dirty. Its object survives.
    file.seek(SeekFrom::Start(4)).unwrap();
    file.write_all(b"conflict").unwrap();
    assert_eq!(
        puts(&f),
        before + 6,
        "dirty local write published before external producer"
    );
    producer.send("update").await;
    assert_eq!(
        puts(&f),
        before + 7,
        "external producer must publish exactly once"
    );
    assert!(file.sync_all().is_err());
    let mut local = Vec::new();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.read_to_end(&mut local).unwrap();
    assert_eq!(&local[..4], b"lost");
    assert_eq!(&local[4..12], b"conflict");
    assert!(std::fs::File::open(&moved).is_err());
    assert_eq!(puts(&f), before + 7);
    m.kill().await;
    drop(file);
    h.child("rw-conflict-reopen").await;
    assert_eq!(puts(&f), before + 7);
    producer.stop().await;
    s.stop().await;
    eprintln!(
        "PASS: managed native HTTPS writes with private CA: Unicode, two stores, patch/truncate/append/sparse, rename, dirty restart, uncertain PUT recovery and retained conflict"
    );
}

pub async fn portable_creation() {
    let f = Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (h, s) = Harness::new_policy(tmp.path(), &f, true).await;
    h.configure_namespace();
    let before = puts(&f);
    h.child("namespace-engine").await;
    assert_eq!(
        puts(&f),
        before + 1,
        "only actual file bytes publish, not directory/empty metadata"
    );
    h.child("namespace-check").await;
    assert_eq!(puts(&f), before + 1);
    h.child("namespace-placement").await;
    assert_eq!(
        puts(&f),
        before + 2,
        "peer must follow authoritative placement"
    );
    assert!(
        f.requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.starts_with("PUT /other-bucket/desktop/writable/.s3drive/"))
    );
    let actual = wire(&cfg(&h.config));
    let volume = info(&actual).await;
    let request = |op: u64, name: &str| Request::Apply {
        request: Apply {
            volume: volume.id.clone(),
            operation: id(op),
            expected_sequence: volume.sequence,
            mutation: Mutation::CreateEntry {
                parent: id(0),
                parent_revision: 0,
                store: None,
                name: name.into(),
                node_kind: Kind::Directory,
            },
        },
    };
    let left = request(5500, "peer race");
    let right = request(5501, "PEER RACE");
    let peer = actual.clone();
    let (left, right) = tokio::join!(actual.request(&left), peer.request(&right));
    let results = [left.unwrap(), right.unwrap()];
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Reply::Applied(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Reply::Error(s3drive_authority::Error::AlreadyExists)))
            .count(),
        1
    );
    assert_eq!(puts(&f), before + 2, "metadata race emitted PUT");
    let proxy = NamespaceProxy::start(&h).await;
    proxy.route(&h);
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "managed_native_child", "--nocapture"])
        .env("S3DRIVE_NATIVE_MANAGED_MODE", "namespace-uncertain")
        .env("S3DRIVE_NATIVE_MANAGED_ROOT", &h.root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    h.environment(&mut command);
    let mut child = command.spawn().unwrap();
    proxy.committed().await;
    let committed = info(&proxy.state.upstream).await.sequence;
    child.kill().await.unwrap();
    proxy.state.release.notify_one();
    h.child("namespace-recovered").await;
    assert_eq!(
        info(&proxy.state.upstream).await.sequence,
        committed,
        "recovery created a second entry"
    );
    assert_eq!(puts(&f), before + 2, "metadata recovery emitted PUT");
    let volume = s.volume.clone();
    s.stop().await;
    let restarted = Service::start_policy(tmp.path(), "portable-collision-v1").await;
    assert_eq!(restarted.volume, volume);
    h.configure(&restarted, &f);
    h.configure_namespace();
    h.child("namespace-check").await;
    restarted.stop().await;
}

pub async fn native_creation() {
    let f = Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (h, s) = Harness::new_policy(tmp.path(), &f, true).await;
    h.configure_namespace();
    let before = puts(&f);
    let m = h.mount().await;
    let directory = h.mount.join("new 資料 🦀");
    std::fs::create_dir(&directory).unwrap();
    let nested = directory.join("nested café");
    std::fs::create_dir(&nested).unwrap();
    let path = nested.join("日本語 %2F + café.bin");
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    assert_eq!(
        puts(&f),
        before,
        "empty metadata creation uploaded an object"
    );
    file.write_all(b"created\0\xff").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"created\0\xff");
    file.sync_all().unwrap();
    drop(file);
    assert_eq!(puts(&f), before + 1);
    assert!(
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(nested.join("日本語 %2F + CAFE\u{301}.BIN"))
            .is_err()
    );
    std::fs::write(nested.join("empty 🦀"), b"").unwrap();
    assert_eq!(puts(&f), before + 1);
    assert!(std::fs::create_dir(h.mount.join("NEW 資料 🦀")).is_err());
    // Concurrent native callers must get exactly one successful exclusive create.
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let barrier = barrier.clone();
        let path = nested.join("exclusive 🦀");
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)
                .is_ok()
        }));
    }
    assert_eq!(
        workers
            .into_iter()
            .map(|t| usize::from(t.join().unwrap()))
            .sum::<usize>(),
        1
    );
    m.stop().await;
    // Existing files keep their placement when the mount's default changes;
    // new files below now choose the second bucket.
    h.edit_config(|c| c["managed"]["default_store"] = id(201).into());
    let other_before = f
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.starts_with("PUT /other-bucket/desktop/writable/.s3drive/"))
        .count();
    let m = h.mount().await;
    assert_eq!(std::fs::read(&path).unwrap(), b"created\0\xff");
    let dirty = nested.join("dirty restart 🦀");
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&dirty)
        .unwrap();
    file.write_all(b"new file retained before flush").unwrap();
    assert_eq!(puts(&f), before + 1);
    m.kill().await;
    drop(file);
    let m = h.mount().await;
    assert_eq!(
        std::fs::read(&dirty).unwrap(),
        b"new file retained before flush"
    );
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&dirty)
        .unwrap();
    file.sync_all().unwrap();
    drop(file);
    assert_eq!(puts(&f), before + 2);
    assert_eq!(
        f.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.starts_with("PUT /other-bucket/desktop/writable/.s3drive/"))
            .count(),
        other_before + 1
    );
    m.stop().await;
    // The real native create loses its reply after authority commit. Kill the
    // mount, then recover the exact durable operation on the next mount.
    let proxy = NamespaceProxy::start(&h).await;
    proxy.route(&h);
    let m = h.mount().await;
    let recovered = h.mount.join("recovered 🦀");
    let operation = tokio::task::spawn_blocking({
        let path = recovered.clone();
        move || std::fs::create_dir(path)
    });
    proxy.committed().await;
    let sequence = info(&proxy.state.upstream).await.sequence;
    m.kill().await;
    proxy.state.release.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(20), operation)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    let m = h.mount().await;
    assert!(recovered.is_dir());
    assert_eq!(
        info(&proxy.state.upstream).await.sequence,
        sequence,
        "native unknown create was duplicated"
    );
    assert_eq!(puts(&f), before + 2);
    m.stop().await;
    s.stop().await;
    eprintln!(
        "managed native creation: nested Unicode directories/files, empty metadata, exclusive/canonical collisions, orderly remount, dirty restart and lost create reply recovery passed"
    );
}

// Each OS exercises its real driver. Run native cases serially: process-spawn
// descriptor closes in another case can legitimately flush FUSE handles.
fn open_directory_handle(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_BACKUP_SEMANTICS: open a real directory handle.
        options.custom_flags(0x02000000);
    }
    options.open(path)
}
async fn native_pending_directory(h: &Harness, fixture: &Fixture) {
    let path = h.mount.join("coordinated empty 🦀");
    std::fs::create_dir(&path).unwrap();
    drop(open_directory_handle(&path).unwrap());
    let w = wire(&cfg(&h.config));
    let v = info(&w).await;
    let Reply::List(listing) = w
        .request(&Request::List {
            volume: v.id.clone(),
            parent: id(0),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    let node = listing
        .children
        .into_iter()
        .find(|n| n.name == "coordinated empty 🦀")
        .unwrap();
    let before = puts(fixture);
    let Reply::Applied(armed) = w
        .request(&Request::Apply {
            request: Apply {
                volume: v.id.clone(),
                operation: id(60_000),
                expected_sequence: v.sequence,
                mutation: Mutation::ArmRemoval {
                    node: s3drive_authority::NodeGuard {
                        id: node.id.clone(),
                        revision: node.revision,
                    },
                    parent: s3drive_authority::NodeGuard {
                        id: id(0),
                        revision: 0,
                    },
                    owner: id(9000),
                },
            },
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    let Some(s3drive_authority::Artifact::Removal(fence)) = armed.artifact else {
        panic!()
    };
    assert!(
        open_directory_handle(&path).is_err(),
        "a pending directory must refuse a new native handle"
    );
    assert!(std::fs::read_dir(&path).is_err());
    assert!(std::fs::write(path.join("must refuse"), b"").is_err());
    let v = info(&w).await;
    assert!(matches!(
        w.request(&Request::Apply {
            request: Apply {
                volume: v.id,
                operation: id(60_001),
                expected_sequence: v.sequence,
                mutation: Mutation::CancelRemoval {
                    node: node.id,
                    fence
                }
            }
        })
        .await
        .unwrap(),
        Reply::Applied(_)
    ));
    drop(open_directory_handle(&path).unwrap());
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 0);
    assert_eq!(
        puts(fixture),
        before,
        "pending metadata disposition must not upload bytes"
    );
}

pub async fn native_mutations() {
    let f = Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (h, s) = Harness::new_options(tmp.path(), &f, true, true).await;
    h.configure_bootstrap(true, false).await;
    let m = h.mount().await;
    Box::pin(native_pending_directory(&h, &f)).await;
    let directory = h.mount.join("save 資料 🦀");
    std::fs::create_dir(&directory).unwrap();
    let source = directory.join("temporary café");
    let target = directory.join("report 日本語.bin");
    std::fs::write(&target, b"previous target").unwrap();
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&source)
        .unwrap();
    file.write_all(b"atomic replacement\0\xff").unwrap();
    #[cfg(target_os = "linux")]
    let mut displaced = {
        let mut old = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&target)
            .unwrap();
        old.write_all(b"dirty displaced").unwrap();
        old
    };
    let before = puts(&f);
    std::fs::rename(&source, &target).unwrap();
    assert_eq!(
        puts(&f),
        before + 1,
        "rename must publish the dirty source exactly once"
    );
    assert!(!source.exists());
    assert_eq!(std::fs::read(&target).unwrap(), b"atomic replacement\0\xff");
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut same = Vec::new();
    file.read_to_end(&mut same).unwrap();
    assert_eq!(same, b"atomic replacement\0\xff");
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        displaced.seek(SeekFrom::Start(0)).unwrap();
        let mut old = Vec::new();
        displaced.read_to_end(&mut old).unwrap();
        assert_eq!(old, b"dirty displaced");
        assert_eq!(displaced.metadata().unwrap().nlink(), 0);
        displaced.seek(SeekFrom::End(0)).unwrap();
        displaced.write_all(b"!").unwrap();
        displaced.sync_all().unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"atomic replacement\0\xff");
        drop(displaced);
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(b"retained").unwrap();
        std::fs::remove_file(&target).unwrap();
        assert!(!target.exists());
        assert_eq!(file.metadata().unwrap().nlink(), 0);
        std::fs::write(&target, b"new identity").unwrap();
        file.sync_all().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut old = Vec::new();
        file.read_to_end(&mut old).unwrap();
        assert!(old.starts_with(b"retained"));
        assert_eq!(std::fs::read(&target).unwrap(), b"new identity");
        assert!(std::fs::remove_dir(&directory).is_err());
        assert!(std::fs::remove_file(&directory).is_err());
        assert!(std::fs::remove_dir(&target).is_err());
        std::fs::File::open(&directory).unwrap().sync_all().unwrap();
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        assert!(
            std::fs::remove_file(&target).is_err(),
            "deferred Windows deletion remains unsupported"
        );
        assert!(
            std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(0x04000000)
                .open(&target)
                .is_err(),
            "FILE_FLAG_DELETE_ON_CLOSE must remain unsupported"
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"atomic replacement\0\xff");
    }
    drop(file);
    let renamed = h.mount.join("moved 資料 🦀");
    std::fs::rename(&directory, &renamed).unwrap();
    assert!(!directory.exists());
    assert!(renamed.join("report 日本語.bin").exists());
    // Rename changes metadata only, including files placed in the other bucket.
    let before = puts(&f);
    std::fs::rename(
        h.mount.join("other bucket.txt"),
        renamed.join("other bucket.txt"),
    )
    .unwrap();
    assert_eq!(puts(&f), before);
    assert_eq!(
        std::fs::read(renamed.join("other bucket.txt")).unwrap(),
        b"payload from the second bucket"
    );
    let sub = renamed.join("child");
    std::fs::create_dir(&sub).unwrap();
    assert!(std::fs::rename(&renamed, sub.join("cycle")).is_err());
    #[cfg(target_os = "linux")]
    {
        std::fs::remove_dir(&sub).unwrap();
        let empty = h.mount.join("empty directory");
        std::fs::create_dir(&empty).unwrap();
        std::fs::remove_dir(&empty).unwrap();
        assert!(!empty.exists());
    }
    m.stop().await;
    let m = h.mount().await;
    assert!(renamed.join("report 日本語.bin").exists());
    m.stop().await;
    // Kill a real mount after a successful authority effect but before its
    // reply; the blocked OS call fails, and remount replays the same ID.
    for kind in [2, 3] {
        if kind == 3 && cfg!(windows) {
            continue;
        }
        let remove = h.mount.join("unknown remove 🦀");
        if kind == 3 {
            let m = h.mount().await;
            std::fs::create_dir(&remove).unwrap();
            m.stop().await;
        }
        let proxy = NamespaceProxy::start_kind(&h, kind).await;
        proxy.route(&h);
        let m = h.mount().await;
        let moved = h.mount.join(DIR).join("unknown rename 🦀");
        let from = h.mount.join(DIR).join(FILE);
        let operation = tokio::task::spawn_blocking({
            let moved = moved.clone();
            let remove = remove.clone();
            move || {
                if kind == 2 {
                    std::fs::rename(from, moved)
                } else {
                    std::fs::remove_dir(remove)
                }
            }
        });
        proxy.committed().await;
        let sequence = info(&proxy.state.upstream).await.sequence;
        let before = puts(&f);
        m.kill().await;
        proxy.state.release.notify_one();
        assert!(
            tokio::time::timeout(Duration::from_secs(20), operation)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        let m = h.mount().await;
        if kind == 2 {
            assert_eq!(std::fs::read(&moved).unwrap(), data(0));
        } else {
            assert!(!remove.exists());
        }
        assert_eq!(
            info(&proxy.state.upstream).await.sequence,
            sequence,
            "native recovery repeated an effect"
        );
        assert_eq!(
            puts(&f),
            before,
            "native recovery repeated content publication"
        );
        m.stop().await;
        h.configure(&s, &f);
        h.configure_mutations();
        drop(proxy);
    }
    s.stop().await;
    eprintln!(
        "managed native mutations: dirty-source save/replace, Unicode directory moves, two stores, cycle refusal, remount and lost rename reply recovery passed; Linux additionally tests retained replaced/unlinked handles, name reuse, unlink/rmdir and lost removal reply; Windows deletion stays unsupported"
    );
}

pub async fn portable_mutations() {
    let f = Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (h, s) = Harness::new_options(tmp.path(), &f, true, true).await;
    h.configure_bootstrap(true, false).await;
    let before = puts(&f);
    h.child("namespace-mutations").await;
    assert_eq!(
        puts(&f),
        before + 3,
        "source flush plus two retained unlinked generations only"
    );
    for (kind, mode, recover) in [
        (2, "namespace-move-uncertain", "namespace-move-recovered"),
        (
            3,
            "namespace-remove-uncertain",
            "namespace-remove-recovered",
        ),
    ] {
        let proxy = NamespaceProxy::start_kind(&h, kind).await;
        proxy.route(&h);
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "managed_native_child", "--nocapture"])
            .env("S3DRIVE_NATIVE_MANAGED_MODE", mode)
            .env("S3DRIVE_NATIVE_MANAGED_ROOT", &h.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        h.environment(&mut command);
        let mut child = command.spawn().unwrap();
        proxy.committed().await;
        let sequence = info(&proxy.state.upstream).await.sequence;
        child.kill().await.unwrap();
        proxy.state.release.notify_one();
        h.child(recover).await;
        assert_eq!(
            info(&proxy.state.upstream).await.sequence,
            sequence,
            "namespace effect repeated"
        );
        assert_eq!(puts(&f), before + 3, "metadata recovery uploaded content");
        h.configure(&s, &f);
        h.configure_mutations();
        drop(proxy);
    }
    let proxy = NamespaceProxy::start_kind(&h, 2).await;
    proxy.route(&h);
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "managed_native_child", "--nocapture"])
        .env("S3DRIVE_NATIVE_MANAGED_MODE", "namespace-cancel")
        .env("S3DRIVE_NATIVE_MANAGED_ROOT", &h.root)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    h.environment(&mut command);
    let mut child = command.spawn().unwrap();
    proxy.committed().await;
    let sequence = info(&proxy.state.upstream).await.sequence;
    std::fs::write(h.root.join("cancel-now"), b"ready").unwrap();
    wait_marker(&h.root, "drain-waiting").await;
    proxy.state.release.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(30), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert_eq!(
        info(&proxy.state.upstream).await.sequence,
        sequence,
        "cancelled caller repeated namespace effect"
    );
    assert_eq!(puts(&f), before + 3);
    h.configure(&s, &f);
    h.configure_mutations();
    h.child("namespace-retained-revisions").await;
    assert_eq!(puts(&f), before + 4, "exactly one external clean refresh");
    s.stop().await;
}
async fn wait_marker(root: &Path, name: &str) {
    tokio::time::timeout(Duration::from_secs(15), async {
        while !root.join(name).exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
async fn namespace_move_uncertain_mode(c: &s3drive_core::Config) {
    let fs = s3drive_fs::ReadOnlyFs::new(c).await.unwrap();
    let d = fs.lookup(s3drive_fs::ROOT, DIR).await.unwrap();
    fs.rename_entry(d.id, FILE, d.id, "recovered move 🦀", false, |a, b| {
        a == b
    })
    .await
    .unwrap();
    panic!("rename reply fault did not hold");
}
async fn namespace_move_recovered_mode(c: &s3drive_core::Config) {
    let fs = s3drive_fs::ReadOnlyFs::new(c).await.unwrap();
    let d = fs.lookup(s3drive_fs::ROOT, DIR).await.unwrap();
    let n = fs.lookup(d.id, "recovered move 🦀").await.unwrap();
    assert_eq!(
        fs.open(n.id).await.unwrap().read(0, 64).await.unwrap(),
        data(0)[..64]
    );
    fs.cache_control().unwrap().close().await.unwrap();
}
async fn namespace_remove_uncertain_mode(c: &s3drive_core::Config) {
    let fs = s3drive_fs::ReadOnlyFs::new(c).await.unwrap();
    fs.create_directory(s3drive_fs::ROOT, "remove once 🦀")
        .await
        .unwrap();
    fs.remove_entry(s3drive_fs::ROOT, "remove once 🦀", true)
        .await
        .unwrap();
    panic!("remove reply fault did not hold");
}
async fn namespace_remove_recovered_mode(c: &s3drive_core::Config) {
    let fs = s3drive_fs::ReadOnlyFs::new(c).await.unwrap();
    assert!(fs.lookup(s3drive_fs::ROOT, "remove once 🦀").await.is_err());
    fs.cache_control().unwrap().close().await.unwrap();
}
async fn namespace_retained_revisions_mode(c: &s3drive_core::Config, root: &Path) {
    let fs = s3drive_fs::ReadOnlyFs::new(c).await.unwrap();
    let d = fs.lookup(s3drive_fs::ROOT, DIR).await.unwrap();
    let n = fs.lookup(d.id, "cancelled caller completed").await.unwrap();
    let original = fs.open(n.id).await.unwrap();
    assert_eq!(original.read(0, 64).await.unwrap(), data(0)[..64]);
    publish(root, &wire(c), "first", 200, 11, 8100, &data(1)).await;
    let refreshed = fs.open(n.id).await.unwrap();
    assert_eq!(refreshed.read(0, 64).await.unwrap(), data(1)[..64]);
    drop(refreshed);
    // The newest Weak<File> is now expired. The old revision still carries the
    // same namespace identity and must learn its zero-link state.
    fs.remove_entry(d.id, "cancelled caller completed", false)
        .await
        .unwrap();
    assert!(original.is_unlinked().unwrap());
    assert_eq!(original.read(0, 64).await.unwrap(), data(0)[..64]);
    assert!(fs.lookup(d.id, "cancelled caller completed").await.is_err());
    fs.cache_control().unwrap().close().await.unwrap();
}

async fn namespace_cancel_mode(c: &s3drive_core::Config, root: &Path) {
    let fs = Arc::new(s3drive_fs::ReadOnlyFs::new(c).await.unwrap());
    let d = fs.lookup(s3drive_fs::ROOT, DIR).await.unwrap();
    let other = fs.clone();
    let request = tokio::spawn(async move {
        other
            .rename_entry(
                d.id,
                "recovered move 🦀",
                d.id,
                "cancelled caller completed",
                false,
                |a, b| a == b,
            )
            .await
    });
    wait_marker(root, "cancel-now").await;
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    let mut drain = tokio::spawn(fs.shutdown_writes());
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut drain)
            .await
            .is_err(),
        "shutdown forgot accepted I/O"
    );
    assert!(matches!(
        fs.create_directory(s3drive_fs::ROOT, "must refuse").await,
        Err(s3drive_fs::Error::Busy)
    ));
    std::fs::write(root.join("drain-waiting"), b"ready").unwrap();
    drain.await.unwrap().unwrap();
    fs.cache_control().unwrap().close().await.unwrap();
    let reopened = s3drive_fs::ReadOnlyFs::new(c).await.unwrap();
    let d = reopened.lookup(s3drive_fs::ROOT, DIR).await.unwrap();
    assert!(
        reopened
            .lookup(d.id, "cancelled caller completed")
            .await
            .is_ok()
    );
    reopened.cache_control().unwrap().close().await.unwrap();
}

async fn namespace_mutations_mode(c: &s3drive_core::Config) {
    let fs = s3drive_fs::ReadOnlyFs::new(c).await.unwrap();
    assert!(fs.namespace_mutation());
    let directory = fs
        .create_directory(s3drive_fs::ROOT, "moves 🦀")
        .await
        .unwrap();
    let source = fs
        .create_file(directory.id, "temporary", |a, b| a == b, true)
        .await
        .unwrap();
    let target = fs
        .create_file(directory.id, "report.txt", |a, b| a == b, true)
        .await
        .unwrap();
    source
        .write(0, Arc::from(&b"saved source"[..]), false, false)
        .await
        .unwrap();
    target
        .write(0, Arc::from(&b"dirty displaced"[..]), false, false)
        .await
        .unwrap();
    let moved = fs
        .rename_entry(
            directory.id,
            "temporary",
            directory.id,
            "report.txt",
            true,
            |a, b| a == b,
        )
        .await
        .unwrap();
    assert_eq!(moved.id, source.node.id);
    assert_eq!(
        fs.lookup(directory.id, "report.txt").await.unwrap().id,
        source.node.id
    );
    assert_eq!(source.read(0, 100).await.unwrap(), b"saved source");
    assert_eq!(target.read(0, 100).await.unwrap(), b"dirty displaced");
    assert!(target.is_unlinked().unwrap());
    target
        .write(15, Arc::from(&b"!"[..]), false, false)
        .await
        .unwrap();
    target.flush().await.unwrap();
    assert_eq!(
        fs.open(moved.id).await.unwrap().read(0, 100).await.unwrap(),
        b"saved source"
    );
    assert!(
        fs.rename_entry(
            directory.id,
            "report.txt",
            directory.id,
            "report.txt",
            false,
            |a, b| a == b
        )
        .await
        .is_ok()
    );
    let collision = fs
        .create_file(directory.id, "occupied", |a, b| a == b, true)
        .await
        .unwrap();
    drop(collision);
    assert!(matches!(
        fs.rename_entry(
            directory.id,
            "report.txt",
            directory.id,
            "occupied",
            false,
            |a, b| a == b
        )
        .await,
        Err(s3drive_fs::Error::AlreadyExists)
    ));
    assert!(matches!(
        fs.rename_entry(
            directory.id,
            "report.txt",
            directory.id,
            "OCCUPIED",
            true,
            |a, b| a == b
        )
        .await,
        Err(s3drive_fs::Error::AlreadyExists)
    ));
    let sub = fs.create_directory(directory.id, "nested").await.unwrap();
    assert!(matches!(
        fs.remove_entry(s3drive_fs::ROOT, "moves 🦀", true).await,
        Err(s3drive_fs::Error::NotEmpty)
    ));
    assert!(matches!(
        fs.rename_entry(
            s3drive_fs::ROOT,
            "moves 🦀",
            sub.id,
            "cycle",
            false,
            |a, b| a == b
        )
        .await,
        Err(s3drive_fs::Error::InvalidOperation)
    ));
    let other = fs
        .lookup(s3drive_fs::ROOT, "other bucket.txt")
        .await
        .unwrap();
    let moved_other = fs
        .rename_entry(
            s3drive_fs::ROOT,
            "other bucket.txt",
            sub.id,
            "other bucket.txt",
            false,
            |a, b| a == b,
        )
        .await
        .unwrap();
    assert_eq!(moved_other.id, other.id);
    assert_eq!(
        fs.open(other.id).await.unwrap().read(0, 100).await.unwrap(),
        b"payload from the second bucket"
    );
    source
        .write(0, Arc::from(&b"after removal"[..]), false, false)
        .await
        .unwrap();
    fs.remove_entry(directory.id, "report.txt", false)
        .await
        .unwrap();
    assert!(source.is_unlinked().unwrap());
    assert!(fs.lookup(directory.id, "report.txt").await.is_err());
    let reused = fs
        .create_file(directory.id, "report.txt", |a, b| a == b, true)
        .await
        .unwrap();
    assert_ne!(reused.node.id, source.node.id);
    source.flush().await.unwrap();
    assert!(reused.read(0, 100).await.unwrap().is_empty());
    fs.remove_entry(directory.id, "report.txt", false)
        .await
        .unwrap();
    fs.remove_entry(directory.id, "occupied", false)
        .await
        .unwrap();
    fs.remove_entry(sub.id, "other bucket.txt", false)
        .await
        .unwrap();
    fs.remove_entry(directory.id, "nested", true).await.unwrap();
    fs.remove_entry(s3drive_fs::ROOT, "moves 🦀", true)
        .await
        .unwrap();
    fs.sync_namespace().await.unwrap();
    fs.cache_control().unwrap().close().await.unwrap();
}

pub async fn portable_lifecycle() {
    lifecycle::portable().await;
}

#[cfg(windows)]
pub async fn native_lifecycle() {
    lifecycle::native().await;
}
