//! Actual CLI authority, SDK fixture and native adapters; no live cloud writes.
use super::{fixture::Fixture, writes};
use s3drive_authority::{Apply, Kind, Mutation, Reply, Request, Token, WireClient};
use s3drive_managed::Publisher;
use s3drive_state::{cache, journal, worker::Storage};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
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
struct Service {
    child: Child,
    endpoint: String,
    volume: String,
}
impl Service {
    async fn start(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_s3drive"))
            .arg("authority-serve")
            .arg("--directory")
            .arg(root.join("authority"))
            .arg("--token-file")
            .arg(root.join("secrets/authority.token"))
            .arg("--stop-on-stdin-close")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
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
    async fn new(root: &Path, f: &Fixture) -> (Self, Service) {
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
        let server = Service::start(root).await;
        h.configure(&server, f);
        h.child("seed").await;
        (h, server)
    }
    fn configure(&self, s: &Service, f: &Fixture) {
        let mut text = format!(
            "version=3\nmode='read-only'\n[managed]\nauthority={:?}\nvolume={:?}\ntoken_file={:?}\n",
            s.endpoint,
            s.volume,
            self.root.join("secrets/authority.token").to_str().unwrap()
        );
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
    fn configure_writable(&self) {
        let old = std::fs::read_to_string(&self.config).unwrap();
        let mut text = old
            .replace("version=3", "version=4")
            .replace("mode='read-only'", "mode='read-write'")
            .replace(
                "[managed]",
                &format!("[managed]\ndefault_store={:?}", id(200)),
            )
            .replace("viewer-first", "docs")
            .replace("viewer-second", "docs2");
        text += &format!(
            "\n[writes]\ndirectory={:?}\n",
            self.root.join("managed-write-state").to_str().unwrap()
        );
        for (which, alias) in [("first", "docs"), ("second", "docs2")] {
            text += &format!(
                "[[writes.sources]]\nalias={alias:?}\npolicy={:?}\nplan={:?}\nreport={:?}\n",
                self.root.join(which).join("policy.toml").to_str().unwrap(),
                self.root.join(which).join("plan.toml").to_str().unwrap(),
                self.root.join(which).join("report.toml").to_str().unwrap()
            );
        }
        std::fs::write(&self.config, text).unwrap();
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
    WireClient::loopback(
        &m.authority,
        Token::from_private_file(&m.token_file).unwrap(),
    )
    .unwrap()
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
        "seed" => {
            for (n, which) in [(200, "first"), (201, "second")] {
                let c = cfg(&root.join(which).join("drive.toml"));
                let r = s3drive_s3::S3Reader::from_source(&c.sources[0])
                    .await
                    .unwrap();
                change(
                    &w,
                    n,
                    Mutation::RegisterStore {
                        route: r.storage_fingerprint(),
                        root: "writable/.s3drive/".into(),
                    },
                )
                .await;
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
                change(
                    &w,
                    n,
                    Mutation::Create {
                        parent: id(parent),
                        name: name.into(),
                        node_kind: kind,
                    },
                )
                .await;
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
}

pub async fn native_writes() {
    let f = Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (h, s) = Harness::new(tmp.path(), &f).await;
    h.configure_writable();
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
    h.child("rename").await;
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
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&moved)
        .unwrap();
    file.sync_all().unwrap();
    assert_eq!(puts(&f), before + 6, "uncertain PUT replayed after restart");
    assert_eq!(&std::fs::read(&moved).unwrap()[..4], b"lost");
    // Another machine wins while local edits remain dirty. Its object survives.
    file.seek(SeekFrom::Start(4)).unwrap();
    file.write_all(b"conflict").unwrap();
    h.child("update").await;
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
    s.stop().await;
    eprintln!(
        "managed native writes: Unicode, two stores, patch/truncate/append/sparse, rename, dirty restart, uncertain PUT recovery and retained conflict passed"
    );
}
