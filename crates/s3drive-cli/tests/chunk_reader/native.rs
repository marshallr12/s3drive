//! Filesystem bridge controls; real native acceptance is added separately.
use super::*;
use s3drive_fs::{Error, ROOT, ReadOnlyFs};
use std::{path::Path, sync::Arc};
const NAME: &str = "café 日本語 🦀.bin";
const LEGACY: &[u8] = b"legacy binary\0\xff";
const OTHER: &[u8] = b"second store\0\xfe";

async fn prepare(f: &Fixture, path: &Path) -> (s3drive_core::Config, Tree) {
    let mut map = BTreeMap::new();
    map.insert(127, pattern(127, C));
    map.insert(128, pattern(128, C));
    let tree = f.tree(10, 130 * C, map).await;
    f.create(11, "empty 日本語", 200, false).await;
    f.create(12, "published empty 🦀", 200, false).await;
    f.tree(12, 0, BTreeMap::new()).await;
    f.create(13, "legacy.bin", 200, false).await;
    let legacy_bytes = LEGACY;
    let n = f.node(13).await;
    let receipt = f
        .change(
            180,
            Mutation::ReserveContent {
                node: n.id,
                expected_revision: n.revision,
                store: id(200),
                size: legacy_bytes.len() as u64,
                sha256: Sha256::digest(legacy_bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect(),
                expected_payload: s3drive_authority::payload_reference(&n.payload),
            },
        )
        .await;
    let Some(s3drive_authority::Artifact::Reservation(reservation)) = receipt.artifact else {
        panic!("fixture reservation receipt missing");
    };
    f.s3.put(
        f.absolute(&reservation.key),
        legacy_bytes.to_vec(),
        "legacy",
    );
    f.change(
        181,
        Mutation::Publish {
            reservation: id(180),
            upload_operation: "d".repeat(64),
            etag: "\"legacy\"".into(),
            version: None,
        },
    )
    .await;
    f.create(14, "64GiB sparse 🦀", 200, false).await;
    f.tree(14, 65536 * C, BTreeMap::new()).await;
    let mut other = f.source.clone();
    other.alias = "other".into();
    other.bucket = "other-bucket".into();
    other.prefix = "secondary/".into();
    f.change(
        201,
        Mutation::RegisterStore {
            route: s3drive_s3::storage_fingerprint(&other).unwrap(),
            root: "managed/".into(),
        },
    )
    .await;
    f.create(15, "second store 日本語", 201, false).await;
    let reference = Reference::new(
        bytes(&id(201)),
        bytes(&id(300)),
        0,
        Sha256::digest(OTHER).into(),
    )
    .unwrap();
    let ns = Namespace::new(
        "managed/".into(),
        bytes(&f.volume),
        bytes(&id(15)),
        bytes(&id(201)),
    )
    .unwrap();
    f.s3.put(
        format!(
            "/{}/{}{}",
            other.bucket,
            other.prefix,
            ns.key(Role::Data, &reference).unwrap().as_str()
        ),
        OTHER.to_vec(),
        "other",
    );
    let page = Page::new(
        bytes(&f.volume),
        bytes(&id(15)),
        0,
        0,
        vec![Entry::Data {
            length: OTHER.len() as u64,
            object: reference,
        }],
    )
    .unwrap();
    let published = f.publish_page(15, 201, page, None).await;
    let prefix = format!("/{}/{}", f.source.bucket, f.source.prefix);
    let moved = format!(
        "/{}/{}{}",
        other.bucket,
        other.prefix,
        published.root.strip_prefix(&prefix).unwrap()
    );
    {
        let mut objects = f.s3.objects.lock().unwrap();
        let root = objects.remove(&published.root).unwrap();
        objects.insert(moved, root);
    }
    let cfg = f.config(path, &[(201, other)]);
    (cfg, tree)
}
pub async fn engine(f: &Fixture, path: &Path) {
    let (cfg, tree) = prepare(f, path).await;
    let before = f.s3.events().len();
    let fs = ReadOnlyFs::new(&cfg).await.unwrap();
    let names: Vec<_> = fs
        .list(ROOT)
        .await
        .unwrap()
        .iter()
        .map(|n| n.name.clone())
        .collect();
    assert_eq!(names.len(), 6);
    assert!(names.iter().any(|n| n == NAME));
    assert_eq!(f.s3.events().len(), before, "namespace fetched S3 data");
    let node = fs.lookup(ROOT, NAME).await.unwrap();
    assert_eq!(fs.metadata(node.id).await.unwrap().unwrap().size, 130 * C);
    assert_eq!(f.s3.events().len(), before);
    let old = fs.open(node.id).await.unwrap();
    assert_eq!(old.current_metadata().unwrap().size, 130 * C);
    assert_eq!(
        f.s3.events().len() - before,
        2,
        "open fetched children/data"
    );
    let expected = [pattern(127, C)[C as usize - 3..].to_vec(), pattern(128, 5)].concat();
    let before = f.s3.events().len();
    assert!(old.read(128 * C - 3, 8).await.unwrap() == expected);
    assert_eq!(f.s3.events().len() - before, 8);
    assert!(
        old.read(129 * C + 7, 100)
            .await
            .unwrap()
            .iter()
            .all(|b| *b == 0)
    );
    assert!(matches!(fs.open_write(node.id).await, Err(Error::ReadOnly)));
    assert!(matches!(
        old.write(0, Arc::from(&b"bad"[..]), false, false).await,
        Err(Error::ReadOnly)
    ));
    assert!(matches!(old.truncate(0).await, Err(Error::ReadOnly)));
    let before = f.s3.events().len();
    old.flush().await.unwrap();
    old.flush_on_close().await.unwrap();
    assert!(old.read(u64::MAX, 1).await.unwrap().is_empty());
    assert!(matches!(
        old.read(0, 8 * C as usize + 1).await,
        Err(Error::TooLarge)
    ));
    assert_eq!(f.s3.events().len(), before);
    for name in ["empty 日本語", "published empty 🦀", "legacy.bin"] {
        let node = fs.lookup(ROOT, name).await.unwrap();
        let file = fs.open(node.id).await.unwrap();
        assert!(
            file.read(0, 100).await.unwrap() == if name == "legacy.bin" { LEGACY } else { &[] }
        );
    }
    let large = fs.lookup(ROOT, "64GiB sparse 🦀").await.unwrap();
    assert_eq!(
        fs.metadata(large.id).await.unwrap().unwrap().size,
        65536 * C
    );
    let large = fs.open(large.id).await.unwrap();
    assert_eq!(large.read(65536 * C - 3, 99).await.unwrap(), vec![0; 3]);
    let other = fs.lookup(ROOT, "second store 日本語").await.unwrap();
    assert!(fs.open(other.id).await.unwrap().read(0, 99).await.unwrap() == OTHER);
    // A new publication and rename leave the old file identity/root pinned.
    f.tree(10, 4, BTreeMap::from([(0, b"new!".to_vec())])).await;
    f.change(
        182,
        Mutation::Move {
            node: id(10),
            parent: s3drive_authority::ROOT.into(),
            name: "renamed 日本語.bin".into(),
        },
    )
    .await;
    let fresh = fs.lookup(ROOT, "renamed 日本語.bin").await.unwrap();
    assert_eq!(fs.metadata(fresh.id).await.unwrap().unwrap().size, 4);
    assert!(fs.open(fresh.id).await.unwrap().read(0, 100).await.unwrap() == b"new!");
    assert_eq!(old.current_metadata().unwrap().size, 130 * C);
    assert!(old.read(128 * C - 3, 8).await.unwrap() == expected);
    // A late chunk failure never returns a successful verified prefix.
    f.s3.fault(Target::Exact(tree.data[1].clone()), Fault::GetDenied);
    assert!(
        matches!(old.read(128*C-3,8).await,Err(Error::Storage(e)) if e.kind==s3drive_s3::ErrorKind::PermissionDenied)
    );
    f.s3.clear();
    {
        let mut objects = f.s3.objects.lock().unwrap();
        objects.get_mut(&tree.data[1]).unwrap().bytes[500] ^= 1;
    }
    assert!(old.read(128 * C - 3, 8).await.is_err());
    {
        let mut objects = f.s3.objects.lock().unwrap();
        objects.get_mut(&tree.data[1]).unwrap().bytes[500] ^= 1;
    }
    assert!(old.read(128 * C - 3, 8).await.unwrap() == expected);
    let before = f.s3.events().len();
    for mode in 20..=27 {
        f.mode.store(mode, Ordering::SeqCst);
        assert!(
            fs.list(ROOT).await.is_err(),
            "malformed listing accepted: {mode}"
        );
        assert_eq!(before, f.s3.events().len());
    }
    for mode in [6, 7, 8, 9, 10, 11, 12, 14] {
        f.mode.store(mode, Ordering::SeqCst);
        assert!(
            fs.metadata(fresh.id).await.is_err(),
            "malformed namespace node accepted: {mode}"
        );
        assert_eq!(before, f.s3.events().len());
    }
    f.mode.store(17, Ordering::SeqCst);
    assert!(matches!(
        ReadOnlyFs::new(&cfg).await,
        Err(Error::Configuration)
    ));
    f.mode.store(1, Ordering::SeqCst);
    let fallback = ReadOnlyFs::new(&cfg).await.unwrap();
    assert!(
        fallback.list(ROOT).await.is_err(),
        "missing capability silently hid manifests"
    );
    drop(fallback);
    f.mode.store(0, Ordering::SeqCst);
    drop(old);
    drop(fs);
    for (op, node) in [10, 12, 14, 15].into_iter().enumerate() {
        f.change(230 + op as u64, Mutation::Remove { node: id(node) })
            .await;
    }
    f.mode.store(1, Ordering::SeqCst);
    let fallback = ReadOnlyFs::new(&cfg).await.unwrap();
    assert_eq!(fallback.list(ROOT).await.unwrap().len(), 2);
    let legacy = fallback.lookup(ROOT, "legacy.bin").await.unwrap();
    assert!(
        fallback
            .open(legacy.id)
            .await
            .unwrap()
            .read(0, 99)
            .await
            .unwrap()
            == LEGACY
    );
    f.mode.store(0, Ordering::SeqCst);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manifest_filesystem_mixed_namespace_logical_sizes_pinned_ranges_and_refusals() {
    super::run("engine").await;
}

#[cfg(any(target_os = "linux", windows))]
struct Mount {
    child: tokio::process::Child,
    log: tokio::task::JoinHandle<String>,
    path: std::path::PathBuf,
}
#[cfg(target_os = "linux")]
impl Drop for Mount {
    fn drop(&mut self) {
        // Last resort only. Normal teardown below verifies successful unmount.
        if let Ok(mut process) = std::process::Command::new("fusermount3")
            .arg("-uz")
            .arg(&self.path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            let until = std::time::Instant::now() + Duration::from_secs(3);
            while process.try_wait().ok().flatten().is_none() && std::time::Instant::now() < until {
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}
#[cfg(any(target_os = "linux", windows))]
impl Mount {
    async fn start(root: &Path) -> Self {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let path = root.join("native mount 日本語");
        #[cfg(target_os = "linux")]
        std::fs::create_dir_all(&path).unwrap();
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_s3drive"));
        command
            .arg("mount")
            .arg(root.join("drive.toml"))
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.arg("--unmount-on-stdin-close");
        let mut child = command.spawn().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (send, receive) = tokio::sync::oneshot::channel();
        let log = tokio::spawn(async move {
            let mut send = Some(send);
            let mut output = String::new();
            let mut lines = BufReader::new(stderr.take(65537)).lines();
            while let Some(line) = lines.next_line().await.unwrap() {
                if line.starts_with("Mounted ")
                    && let Some(send) = send.take()
                {
                    let _ = send.send(());
                }
                output.push_str(&line);
                output.push('\n');
            }
            output
        });
        let mut mount = Self { child, log, path };
        if !matches!(
            tokio::time::timeout(Duration::from_secs(30), receive).await,
            Ok(Ok(()))
        ) {
            let _ = tokio::time::timeout(Duration::from_secs(10), mount.child.kill()).await;
            let _ = tokio::time::timeout(Duration::from_secs(10), mount.child.wait()).await;
            // Borrow join handle because Linux Drop owns last-resort unmount.
            let log = tokio::time::timeout(Duration::from_secs(5), &mut mount.log)
                .await
                .unwrap()
                .unwrap();
            panic!("native manifest mount never became ready: {log}");
        }
        mount
    }
    async fn stop(&mut self) {
        #[cfg(target_os = "linux")]
        {
            let status = tokio::time::timeout(
                Duration::from_secs(15),
                tokio::process::Command::new("fusermount3")
                    .arg("-u")
                    .arg(&self.path)
                    .status(),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(status.success(), "native manifest unmount failed");
        }
        #[cfg(windows)]
        drop(self.child.stdin.take());
        let status = tokio::time::timeout(Duration::from_secs(15), self.child.wait())
            .await
            .unwrap()
            .unwrap();
        let log = tokio::time::timeout(Duration::from_secs(5), &mut self.log)
            .await
            .unwrap()
            .unwrap();
        assert!(
            status.success() && log.len() <= 65536,
            "native manifest daemon failed: {status}: {log}"
        );
    }
}
#[cfg(any(target_os = "linux", windows))]
async fn blocking<T: Send + 'static>(
    mount: &mut Mount,
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    let mut task = tokio::task::spawn_blocking(f);
    let result = tokio::time::timeout(Duration::from_secs(25), &mut task).await;
    if let Ok(Ok(value)) = result {
        return value;
    }
    // A timed-out blocking filesystem call cannot be aborted by Tokio. Tear
    // down the owned daemon first, then give the syscall/thread time to return.
    drop(mount.child.stdin.take());
    let _ = tokio::time::timeout(Duration::from_secs(10), mount.child.kill()).await;
    let _ = tokio::time::timeout(Duration::from_secs(10), mount.child.wait()).await;
    #[cfg(target_os = "linux")]
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new("fusermount3")
            .arg("-uz")
            .arg(&mount.path)
            .status(),
    )
    .await;
    if result.is_err() {
        let reaped = tokio::time::timeout(Duration::from_secs(5), &mut task).await;
        assert!(
            reaped.is_ok(),
            "native filesystem worker failed to return after owned daemon teardown"
        );
    }
    let log = tokio::time::timeout(Duration::from_secs(5), &mut mount.log).await;
    panic!("native filesystem worker failed or exceeded its bound; daemon reaped: {log:?}");
}

#[cfg(any(target_os = "linux", windows))]
pub async fn native(f: &Fixture, root: &Path) {
    use std::io::{Read, Seek, SeekFrom};
    let (_cfg, _tree) = prepare(f, root).await;
    let before = f.s3.served.load(Ordering::SeqCst);
    let mut mount = Mount::start(root).await;
    let file_path = mount.path.join(NAME);
    let data_before = data_gets(f);
    let opened = blocking(&mut mount, move || std::fs::File::open(file_path).unwrap()).await;
    assert_eq!(data_gets(f), data_before, "native open fetched data chunks");
    let path = mount.path.clone();
    let mut old = blocking(&mut mount, move || {
        let names: Vec<_> = std::fs::read_dir(&path)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names.len(), 6);
        assert!(names.iter().any(|n| n == NAME));
        assert_eq!(std::fs::metadata(path.join(NAME)).unwrap().len(), 130 * C);
        assert_eq!(
            std::fs::metadata(path.join("64GiB sparse 🦀"))
                .unwrap()
                .len(),
            65536 * C
        );
        for name in ["empty 日本語", "published empty 🦀"] {
            assert!(std::fs::read(path.join(name)).unwrap().is_empty());
        }
        assert_eq!(std::fs::read(path.join("legacy.bin")).unwrap(), LEGACY);
        assert_eq!(
            std::fs::read(path.join("second store 日本語")).unwrap(),
            OTHER
        );
        let mut sparse = std::fs::File::open(path.join("64GiB sparse 🦀")).unwrap();
        sparse.seek(SeekFrom::Start(65536 * C - 3)).unwrap();
        let mut zeros = Vec::new();
        sparse.read_to_end(&mut zeros).unwrap();
        assert_eq!(zeros, vec![0; 3]);
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(path.join(NAME))
                .is_err()
        );
        let mut file = opened;
        file.seek(SeekFrom::Start(128 * C - 3)).unwrap();
        let mut bytes = [0; 8];
        file.read_exact(&mut bytes).unwrap();
        assert!(
            bytes.to_vec()
                == [pattern(127, C)[C as usize - 3..].to_vec(), pattern(128, 5)].concat()
        );
        file
    })
    .await;
    assert!(
        f.s3.served.load(Ordering::SeqCst) - before < 16 * C,
        "native reads downloaded too much data"
    );
    f.tree(10, 4, BTreeMap::from([(0, b"new!".to_vec())])).await;
    f.change(
        182,
        Mutation::Move {
            node: id(10),
            parent: s3drive_authority::ROOT.into(),
            name: "renamed 日本語.bin".into(),
        },
    )
    .await;
    let path = mount.path.clone();
    blocking(&mut mount, move || {
        assert_eq!(
            std::fs::read(path.join("renamed 日本語.bin")).unwrap(),
            b"new!"
        );
        assert_eq!(old.metadata().unwrap().len(), 130 * C);
        old.seek(SeekFrom::Start(128 * C - 3)).unwrap();
        let mut bytes = [0; 8];
        old.read_exact(&mut bytes).unwrap();
        assert!(
            bytes.to_vec()
                == [pattern(127, C)[C as usize - 3..].to_vec(), pattern(128, 5)].concat()
        );
    })
    .await;
    // Previously unread files prevent an OS cache hit from hiding a fault.
    for (node, name, fault) in [
        (16, "corrupt unseen 日本語", 0),
        (17, "missing unseen 日本語", 1),
        (18, "denied unseen 日本語", 2),
    ] {
        f.create(node, name, 200, false).await;
        let tree = f.tree(node, C, BTreeMap::from([(0, pattern(0, C))])).await;
        match fault {
            0 => {
                f.s3.objects
                    .lock()
                    .unwrap()
                    .get_mut(&tree.data[0])
                    .unwrap()
                    .bytes[500] ^= 1
            }
            1 => {
                f.s3.objects.lock().unwrap().remove(&tree.data[0]);
            }
            _ => {
                f.s3.fault(Target::Exact(tree.root.clone()), Fault::HeadDenied)
            }
        }
        let path = mount.path.join(name);
        blocking(&mut mount, move || {
            let result = std::fs::File::open(path).and_then(|mut file| {
                let mut bytes = [0; 4];
                file.read_exact(&mut bytes)
            });
            assert!(
                result.is_err(),
                "native read hid corrupt/missing/denied bytes"
            );
        })
        .await;
        f.s3.clear();
    }
    mount.stop().await;
    drop(mount);
    let mut mount = Mount::start(root).await;
    let path = mount.path.join("renamed 日本語.bin");
    blocking(&mut mount, move || {
        assert_eq!(std::fs::read(path).unwrap(), b"new!")
    })
    .await;
    mount.stop().await;
    println!(
        "PASS: actual native manifest Unicode/two-store/legacy/empty ranges, logical64GiB size, old roots after publication+rename, corruption/missing/denial, read-only refusal and remount; owned loopback only"
    );
}
#[cfg(any(target_os = "linux", windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native manifest acceptance: requires real FUSE or WinFsp; no mock substitute"]
async fn actual_native_manifest_reads_and_remount() {
    super::run("native").await;
}
