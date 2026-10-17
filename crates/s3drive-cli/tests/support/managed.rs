//! Actual CLI authority, SDK fixture and native adapters; no live cloud writes.
use super::{fixture::Fixture, writes};
use s3drive_authority::{Apply, Kind, Mutation, Reply, Request, Token, WireClient};
use s3drive_managed::Publisher;
use s3drive_state::{cache, journal, worker::Storage};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom},
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
        };
        let mut lines = BufReader::new(m.child.stderr.take().unwrap()).lines();
        tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                eprintln!("{line}");
                if line.starts_with("Mounted read-only") {
                    return;
                }
            }
            panic!("managed mount failed before readiness");
        })
        .await
        .unwrap();
        m
    }
}
struct Mount {
    child: Child,
    path: PathBuf,
}
impl Drop for Mount {
    fn drop(&mut self) {
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
