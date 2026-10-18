//! Paired native Linux login supervisors with the actual SDK/publication engine.
//! All process control is delegated to a child started before native handles.
use super::*;

struct Controller {
    child: Child,
    lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    mount: PathBuf,
}
impl Controller {
    async fn start(h: &Harness, service: Service) -> Self {
        let c = cfg(&h.config);
        let m = c.managed.as_ref().unwrap();
        let manifest = h.root.join("paired-startup.json");
        let value = serde_json::json!({
            "root":h.root,"executable":env!("CARGO_BIN_EXE_s3drive"),
            "mount":h.mount,"drive_config":h.config,"endpoint":service.endpoint,
            "volume":service.volume,"policy":info(&wire(&c)).await.name_policy,
            "database":h.root.join("authority"),"token":m.token_file,
            "cert":service.ca_file,"key":h.root.join("tls-key/authority.token"),
        });
        std::fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
        service.stop().await;
        let mut command = Command::new("python3");
        command
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/test-linux-managed-startup.py"),
            )
            .arg(manifest)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        h.environment(&mut command);
        let mut child = command.spawn().unwrap();
        let lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut controller = Self {
            child,
            lines,
            mount: h.mount.clone(),
        };
        controller.event("ready").await;
        controller
    }
    async fn event(&mut self, expected: &str) {
        let line = tokio::time::timeout(Duration::from_secs(100), self.lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap_or_else(|| panic!("paired startup controller exited before {expected}"));
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["event"], expected, "{v}");
    }
    async fn send(&mut self, command: &str, expected: &str) {
        let input = self.child.stdin.as_mut().unwrap();
        input
            .write_all(format!("{command}\n").as_bytes())
            .await
            .unwrap();
        input.flush().await.unwrap();
        self.event(expected).await;
    }
    async fn finish(mut self) {
        self.send("remove", "removed").await;
        drop(self.child.stdin.take());
        assert!(
            tokio::time::timeout(Duration::from_secs(50), self.child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        // EOF runs the controller's owned-registration cleanup. On assertion
        // failure only, detach this fixture mount if open kernel handles delay
        // that cleanup; never leave a mounted path for TempDir to traverse.
        drop(self.child.stdin.take());
        let deadline = std::time::Instant::now() + Duration::from_secs(45);
        while self.child.try_wait().unwrap().is_none() {
            if std::time::Instant::now() >= deadline {
                let _ = std::process::Command::new("fusermount3")
                    .args(["-uz"])
                    .arg(&self.mount)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let _ = self.child.start_kill();
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Attempt {
    operation: Vec<u8>,
    intent_hash: Vec<u8>,
    key: String,
    phase: String,
}
type PublicationIdentity = (String, Vec<u8>, Option<Vec<u8>>);
fn publication_identities(h: &Harness) -> Vec<PublicationIdentity> {
    let directory = cfg(&h.config).writes.unwrap().directory;
    let db = rusqlite::Connection::open_with_flags(
        directory.join("metadata.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
            | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .unwrap();
    db.prepare(
        "SELECT operation_id,input,reservation FROM managed_publications ORDER BY operation_id",
    )
    .unwrap()
    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}
fn attempts(h: &Harness) -> Vec<Attempt> {
    let directory = cfg(&h.config).writes.unwrap().directory;
    let db = rusqlite::Connection::open_with_flags(
        directory.join("metadata.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
            | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .unwrap();
    db.prepare("SELECT operation_id,intent_hash,object_key,phase FROM remote_intents ORDER BY operation_id")
        .unwrap()
        .query_map([], |row| {
            Ok(Attempt {
                operation: row.get(0)?,
                intent_hash: row.get(1)?,
                key: row.get(2)?,
                phase: row.get(3)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

pub(super) async fn native() {
    let f = Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (h, service) = Harness::new_options(tmp.path(), &f, false, true).await;
    h.configure_bootstrap(true, false).await;
    let mut producer = Producer::start(&h).await;
    let mut startup = Controller::start(&h, service).await;
    assert_eq!(
        std::fs::read(h.mount.join(DIR).join(FILE)).unwrap(),
        data(0)
    );
    assert_eq!(std::fs::read(h.mount.join("empty 🦀")).unwrap(), b"");
    assert_eq!(
        std::fs::read(h.mount.join("other bucket.txt")).unwrap(),
        b"payload from the second bucket"
    );
    let before = puts(&f);
    let path = h.mount.join(DIR).join(FILE);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.set_len(0).unwrap();
    file.write_all(b"paired Unicode binary\0\xff bytes")
        .unwrap();
    let expected = b"paired Unicode binary\0\xff bytes";
    f.hide_committed.store(true, Ordering::SeqCst);
    assert!(
        file.sync_all().is_err(),
        "lost PUT reply falsely acknowledged"
    );
    assert_eq!(puts(&f), before + 1);
    let retained = attempts(&h);
    let logical = publication_identities(&h);
    assert_eq!(logical.len(), 1);
    assert!(logical[0].2.is_some());
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].phase, "uncertain");
    startup.send("crash-authority", "authority-down").await;
    assert!(file.sync_all().is_err());
    let mut local = Vec::new();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.read_to_end(&mut local).unwrap();
    assert_eq!(local, expected);
    assert_eq!(
        attempts(&h),
        retained,
        "offline flush changed unknown intent"
    );
    writes::restore_observation(&f);
    startup
        .send("recover-authority", "authority-recovered")
        .await;
    file.sync_all().unwrap();
    assert_eq!(puts(&f), before + 1, "coordinator restart repeated PUT");
    let after = attempts(&h);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].operation, retained[0].operation);
    assert_eq!(after[0].intent_hash, retained[0].intent_hash);
    assert_eq!(after[0].key, retained[0].key);
    assert_eq!(after[0].phase, "verified");
    assert_eq!(
        publication_identities(&h),
        logical,
        "logical request identity changed during recovery"
    );
    assert_eq!(
        f.objects.lock().unwrap()[&format!("/test-bucket/{}", after[0].key)].0,
        expected,
        "actual immutable S3 fixture bytes differ from local cache"
    );
    drop(file);
    // A fresh daemon must read the exact uploaded bytes from durable state/S3.
    startup.send("crash-mount-stop", "mount-stopped").await;
    startup.send("start-mount", "mount-ready").await;
    assert_eq!(std::fs::read(&path).unwrap(), expected);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    producer.send("rename").await;
    file.seek(SeekFrom::Start(0)).unwrap();
    file.write_all(b"dirty conflict").unwrap();
    let mut local = Vec::new();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.read_to_end(&mut local).unwrap();
    producer.send("update").await;
    assert_eq!(puts(&f), before + 2);
    assert!(
        file.sync_all().is_err(),
        "rival publication silently overwritten"
    );
    startup.send("crash-authority", "authority-down").await;
    startup
        .send("recover-authority", "authority-recovered")
        .await;
    assert!(
        file.sync_all().is_err(),
        "restart discarded retained conflict"
    );
    let mut still_local = Vec::new();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.read_to_end(&mut still_local).unwrap();
    assert_eq!(still_local, local);
    assert_eq!(puts(&f), before + 2);
    let moved = h.mount.join(DIR).join(MOVED);
    assert!(std::fs::File::open(&moved).is_err());
    startup.send("crash-mount-stop", "mount-stopped").await;
    drop(file);
    h.child("rw-conflict-reopen").await;
    assert_eq!(puts(&f), before + 2);
    producer.stop().await;
    startup.finish().await;
    eprintln!(
        "PASS: paired no-systemd login supervisors, HTTPS/private CA, real FUSE Unicode/binary/empty reads, terminal loss, coordinator crashes, exact uncertain IDs without repeated PUT, durable rival conflict and independent removal"
    );
}
