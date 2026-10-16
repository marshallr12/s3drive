//! Real mount cache acceptance shared by Linux and Windows native gates.
use super::fixture::Fixture;
use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::Ordering,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
};

struct Harness {
    root: PathBuf,
    config: PathBuf,
    credentials: PathBuf,
    mount: PathBuf,
    state: PathBuf,
}
struct Session {
    child: Child,
    mount: PathBuf,
}
impl Drop for Session {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("fusermount3")
            .args(["-uz"])
            .arg(&self.mount)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = self.child.start_kill();
    }
}
impl Harness {
    fn new(parent: &Path, fixture: &Fixture) -> Self {
        let root = parent.join("cache acceptance café ' $ %");
        std::fs::create_dir(&root).unwrap();
        let harness = Self {
            config: root.join("drive.toml"),
            credentials: root.join("credentials"),
            mount: root.join("mount"),
            state: root.join("local-state"),
            root,
        };
        #[cfg(target_os = "linux")]
        std::fs::create_dir(&harness.mount).unwrap();
        harness.configure(fixture, 128 * 1024, 64 * 1024, 32);
        harness.credentials("TESTPROFILESECRET");
        harness
    }
    fn credentials(&self, secret: &str) {
        std::fs::write(
            &self.credentials,
            format!("[fixture]\naws_access_key_id=TESTPROFILE\naws_secret_access_key={secret}\n"),
        )
        .unwrap();
    }
    fn configure(&self, fixture: &Fixture, bytes: usize, threshold: usize, entries: usize) {
        let path = self
            .state
            .to_str()
            .unwrap()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        std::fs::write(&self.config, format!(
            "version=1\nmode='read-only'\n[cache]\ndirectory=\"{path}\"\nmax_bytes={bytes}\nmax_file_bytes={threshold}\nmax_entries={entries}\n[[sources]]\nalias='cached'\nbucket='test-bucket'\nprefix='cache/'\nregion='us-east-1'\nendpoint='{}'\nprofile='fixture'\npath_style=true\nallow_http_loopback=true\n", fixture.endpoint
        )).unwrap();
    }
    async fn start(&self) -> Session {
        let mut command = Command::new(env!("CARGO_BIN_EXE_s3drive"));
        command
            .arg("mount")
            .arg(&self.config)
            .arg(&self.mount)
            .env("AWS_SHARED_CREDENTIALS_FILE", &self.credentials)
            .env("AWS_CONFIG_FILE", self.root.join("no-ambient-config"))
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .stdout(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.arg("--unmount-on-stdin-close");
        let child = command.spawn().unwrap();
        let mut session = Session {
            child,
            mount: self.mount.clone(),
        };
        let mut lines = BufReader::new(session.child.stderr.take().unwrap()).lines();
        tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                eprintln!("{line}");
                if line.starts_with("Mounted read-only") {
                    return;
                }
            }
            panic!("cache mount exited before ready");
        })
        .await
        .expect("cache mount readiness timeout");
        session
    }
    fn db(&self) -> rusqlite::Connection {
        // Only called after stop() confirms worker ownership has been released.
        rusqlite::Connection::open(self.state.join("metadata.sqlite3")).unwrap()
    }
}
impl Session {
    async fn kill(mut self) {
        self.child.kill().await.unwrap();
        #[cfg(target_os = "linux")]
        assert!(
            Command::new("fusermount3")
                .args(["-uz"])
                .arg(&self.mount)
                .status()
                .await
                .unwrap()
                .success()
        );
        #[cfg(windows)]
        tokio::time::timeout(Duration::from_secs(10), async {
            while std::fs::symlink_metadata(&self.mount).is_ok() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("cache mount survived process death");
    }
    async fn stop(mut self) {
        // The daemon cancels callbacks before unmounting. An external ordinary
        // fusermount -u correctly refuses a busy mount with an in-flight read.
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
        let status = tokio::time::timeout(Duration::from_secs(20), self.child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(status.success(), "cache daemon teardown failed: {status}");
        #[cfg(target_os = "linux")]
        assert!(std::fs::read_dir(&self.mount).unwrap().next().is_none());
        #[cfg(windows)]
        assert!(std::fs::symlink_metadata(&self.mount).is_err());
    }
}
fn gets(fixture: &Fixture, key: &str) -> usize {
    let path = format!("/test-bucket/cache/{key}");
    fixture
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| {
            let mut words = request.split_whitespace();
            if words.next() != Some("GET") {
                return false;
            }
            let target = words.next().unwrap().split('?').next().unwrap();
            percent_encoding::percent_decode_str(target)
                .decode_utf8()
                .unwrap()
                == path
        })
        .count()
}
async fn read(path: PathBuf) -> std::io::Result<Vec<u8>> {
    tokio::task::spawn_blocking(move || std::fs::read(path))
        .await
        .unwrap()
}

pub async fn verify(fixture: &Fixture, parent: &Path) {
    let h = Harness::new(parent, fixture);
    let small = h.mount.join("cached/small.txt");
    let original = vec![b'a'; 4096];
    let replacement = vec![b'b'; 4096];
    fixture.put("/test-bucket/cache/small.txt", &original, "small-v1");
    fixture.put("/test-bucket/cache/coalesce", &vec![b'c'; 8000], "coalesce");
    let large = vec![b'L'; 70_000];
    fixture.put("/test-bucket/cache/large.bin", &large, "large");
    let unicode = [
        ("日本語.txt", "Japanese"),
        ("café.txt", "NFC"),
        ("cafe\u{301}.txt", "NFD"),
    ];
    for (name, value) in unicode {
        fixture.put(
            &format!("/test-bucket/cache/{name}"),
            value.as_bytes(),
            value,
        );
    }

    let session = h.start().await;
    assert_eq!(read(small.clone()).await.unwrap(), original);
    assert_eq!(read(small.clone()).await.unwrap(), original);
    assert_eq!(gets(fixture, "small.txt"), 1, "warm reads downloaded again");
    for (name, value) in unicode {
        assert_eq!(
            read(h.mount.join("cached").join(name)).await.unwrap(),
            value.as_bytes()
        );
    }
    let mut readers = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let path = h.mount.join("cached/coalesce");
        readers.spawn(async move {
            assert_eq!(read(path).await.unwrap(), vec![b'c'; 8000]);
        });
    }
    while let Some(result) = readers.join_next().await {
        result.unwrap();
    }
    assert_eq!(
        gets(fixture, "coalesce"),
        1,
        "concurrent misses were not coalesced"
    );

    // A fully cached open handle remains an immutable snapshot. A fresh open
    // authorizes again and sees a new revision, even with the same byte length.
    let path = small.clone();
    let mut pinned = tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(path).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, vec![b'a'; 4096]);
        file
    })
    .await
    .unwrap();
    fixture.put("/test-bucket/cache/small.txt", &replacement, "small-v2");
    let old = tokio::task::spawn_blocking(move || {
        pinned.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        pinned.read_to_end(&mut bytes).unwrap();
        (pinned, bytes)
    })
    .await
    .unwrap();
    assert_eq!(old.1, original);
    assert_eq!(read(small.clone()).await.unwrap(), replacement);
    assert_eq!(gets(fixture, "small.txt"), 2);

    fixture.deny_get.store(true, Ordering::SeqCst);
    assert_eq!(read(small.clone()).await.unwrap(), replacement);
    assert_eq!(
        gets(fixture, "small.txt"),
        2,
        "cache hit reached forbidden GET"
    );
    fixture
        .objects
        .lock()
        .unwrap()
        .remove("/test-bucket/cache/small.txt");
    assert!(
        read(small.clone()).await.is_err(),
        "deleted remote file used cache"
    );
    fixture.put("/test-bucket/cache/small.txt", &replacement, "small-v2");
    fixture.deny_head.store(true, Ordering::SeqCst);
    assert!(
        read(small.clone()).await.is_err(),
        "cached bytes bypassed denied HEAD"
    );
    fixture.deny_head.store(false, Ordering::SeqCst);
    fixture.offline.store(true, Ordering::SeqCst);
    assert!(
        read(small.clone()).await.is_err(),
        "new offline open was allowed"
    );
    let mut pinned = old.0;
    tokio::task::spawn_blocking(move || {
        pinned.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        pinned.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, vec![b'a'; 4096]);
    })
    .await
    .unwrap();
    fixture.offline.store(false, Ordering::SeqCst);
    assert!(
        read(h.mount.join("cached/large.bin")).await.is_err(),
        "oversized file used cache"
    );
    fixture.deny_get.store(false, Ordering::SeqCst);
    for _ in 0..2 {
        assert_eq!(read(h.mount.join("cached/large.bin")).await.unwrap(), large);
    }
    assert!(
        gets(fixture, "large.bin") >= 3,
        "large file did not retain direct GET reads"
    );
    session.stop().await;

    fixture.deny_get.store(true, Ordering::SeqCst);
    let session = h.start().await;
    assert_eq!(read(small.clone()).await.unwrap(), replacement);
    for (name, value) in unicode {
        assert_eq!(
            read(h.mount.join("cached").join(name)).await.unwrap(),
            value.as_bytes()
        );
    }
    assert_eq!(gets(fixture, "small.txt"), 2, "restart did not reuse cache");
    session.kill().await;
    let session = h.start().await;
    assert_eq!(read(small.clone()).await.unwrap(), replacement);
    assert_eq!(
        gets(fixture, "small.txt"),
        2,
        "process death lost committed cache"
    );
    session.stop().await;
    fixture.deny_get.store(false, Ordering::SeqCst);
    h.db()
        .execute("UPDATE clean_blobs SET checksum=zeroblob(32)", [])
        .unwrap();
    let session = h.start().await;
    assert_eq!(read(small.clone()).await.unwrap(), replacement);
    assert_eq!(gets(fixture, "small.txt"), 3, "corruption did not refetch");
    session.stop().await;
    h.db().execute("DELETE FROM clean_blobs", []).unwrap();
    let session = h.start().await;
    assert_eq!(read(small.clone()).await.unwrap(), replacement);
    assert_eq!(
        gets(fixture, "small.txt"),
        4,
        "missing entry did not refetch"
    );
    session.stop().await;

    // Same profile name, access ID, endpoint, key and ETag; only the secret
    // generation changes. A new process must not adopt the previous cache.
    h.credentials("ROTATEDTESTSECRET");
    fixture.deny_get.store(true, Ordering::SeqCst);
    let session = h.start().await;
    assert!(
        read(small.clone()).await.is_err(),
        "rotated credentials reused old cached bytes"
    );
    fixture.deny_get.store(false, Ordering::SeqCst);
    assert_eq!(read(small.clone()).await.unwrap(), replacement);
    session.stop().await;
    let sources: i64 = h
        .db()
        .query_row("SELECT count(*) FROM source_bindings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(sources, 2, "credential generations were not isolated");

    // One-entry quota. A live pinned snapshot prevents eviction; another file
    // can still be read with explicit per-handle range-read bypass.
    h.configure(fixture, 4096, 4096, 1);
    h.db().execute("DELETE FROM clean_blobs", []).unwrap();
    fixture.put("/test-bucket/cache/pressure", &vec![b'p'; 4096], "pressure");
    fixture.put("/test-bucket/cache/last", &vec![b'z'; 4096], "last");
    let session = h.start().await;
    let path = small.clone();
    let pinned = tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(path).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, vec![b'b'; 4096]);
        file
    })
    .await
    .unwrap();
    let pressure = h.mount.join("cached/pressure");
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(pressure).unwrap();
        for _ in 0..2 {
            file.seek(SeekFrom::Start(0)).unwrap();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, vec![b'p'; 4096]);
        }
    })
    .await
    .unwrap();
    assert!(
        gets(fixture, "pressure") >= 2,
        "pinned quota pressure did not bypass"
    );
    drop(pinned);
    assert_eq!(
        read(h.mount.join("cached/last")).await.unwrap(),
        vec![b'z'; 4096]
    );
    session.stop().await;
    let (count, bytes): (i64, i64) = h
        .db()
        .query_row(
            "SELECT count(*),sum(size_bytes) FROM clean_blobs",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((count, bytes), (1, 4096));
    let kept: Vec<u8> = h
        .db()
        .query_row("SELECT data FROM clean_blobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        kept,
        vec![b'z'; 4096],
        "released snapshots stayed pinned against eviction"
    );
    // Install a fault only in the stopped, isolated test database. A real SQL
    // insertion error must reach the native read and roll back its eviction.
    h.db().execute_batch("CREATE TRIGGER cache_test_failure BEFORE INSERT ON clean_blobs BEGIN SELECT RAISE(ABORT,'injected cache write failure'); END;").unwrap();
    fixture.put("/test-bucket/cache/fault", &vec![b'f'; 4096], "fault");
    let session = h.start().await;
    assert!(
        read(h.mount.join("cached/fault")).await.is_err(),
        "failed cache transaction was reported as successful read"
    );
    session.stop().await;
    let kept: Vec<u8> = h
        .db()
        .query_row("SELECT data FROM clean_blobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        kept,
        vec![b'z'; 4096],
        "failed insert evicted committed cache bytes"
    );
    h.db()
        .execute_batch("DROP TRIGGER cache_test_failure")
        .unwrap();
    fixture.put("/test-bucket/cache/stalled", b"stalled", "stalled");
    let session = h.start().await;
    let path = h.mount.join("cached/stalled");
    let reading = tokio::spawn(async move { read(path).await });
    tokio::time::timeout(Duration::from_secs(15), fixture.stalled.notified())
        .await
        .unwrap();
    session.stop().await;
    assert!(
        tokio::time::timeout(Duration::from_secs(10), reading)
            .await
            .unwrap()
            .unwrap()
            .is_err(),
        "cache fill survived native cancellation"
    );
    eprintln!(
        "PASS: mounted cache hits, coalescing, Unicode, revision snapshots, online authorization, restart/process death, corruption/missing repair, credential isolation, pinned quota bypass and SQL failure propagation"
    );
}
