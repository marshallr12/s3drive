use super::native_writer::{configuration, stop};
use super::*;
use s3drive_fs::{Error, ROOT, ReadOnlyFs};
fn directories(s: &Server) -> Vec<Apply> {
    s.proxy
        .applies()
        .into_iter()
        .filter(|r| {
            matches!(
                r.mutation,
                Mutation::CreateEntry {
                    node_kind: Kind::Directory,
                    ..
                }
            )
        })
        .collect()
}
async fn remote(s: &Server, name: &str) -> Node {
    let Reply::List(list) = s
        .direct
        .request(&Request::List {
            volume: info(&s.direct).await.id,
            parent: id(0),
        })
        .await
        .unwrap()
    else {
        panic!("root list");
    };
    list.children.into_iter().find(|n| n.name == name).unwrap()
}
pub async fn only(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let top = fs
        .create_directory(ROOT, "only folders 日本語")
        .await
        .unwrap();
    fs.create_directory(top.id, "café 🦀").await.unwrap();
    assert!(matches!(
        fs.create_directory(top.id, "café 🦀").await,
        Err(Error::AlreadyExists)
    ));
    for mode in [7, 9] {
        s.proxy.mode.store(mode, Ordering::SeqCst);
        assert!(matches!(
            fs.create_directory(ROOT, "wrong authority").await,
            Err(Error::Configuration)
        ));
    }
    s.proxy.mode.store(0, Ordering::SeqCst);
    let racy = fs.create_directory(ROOT, "racy parent").await.unwrap();
    s.proxy.mode.store(18, Ordering::SeqCst);
    assert!(matches!(
        fs.create_directory(racy.id, "raced child").await,
        Err(Error::Conflict)
    ));
    assert!(fs.lookup(racy.id, "raced child").await.is_err());
    assert_eq!(
        fs.lookup(ROOT, "rival directory").await.unwrap().kind,
        s3drive_fs::Kind::Directory
    );
    fs.create_directory(racy.id, "raced child").await.unwrap();
    stop(fs).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let top = fs.lookup(ROOT, "only folders 日本語").await.unwrap();
    assert_eq!(fs.list(top.id).await.unwrap().len(), 1);
    assert_eq!(
        fs.lookup(top.id, "café 🦀").await.unwrap().kind,
        s3drive_fs::Kind::Directory
    );
    stop(fs).await;
}
pub async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let f = fs
        .create_file(ROOT, "dirty 🦀.bin", |a, b| a == b, true)
        .await
        .unwrap();
    let at = 8 * 1024 * 1024 + 3;
    f.write(at, Arc::from(&b"pending"[..]), false, false)
        .await
        .unwrap();
    let original_remote = remote(s, "dirty 🦀.bin").await;
    s.proxy.clear();
    s.proxy.mode.store(16, Ordering::SeqCst);
    assert!(fs.create_directory(ROOT, "資料 café 🦀").await.is_err());
    assert_eq!(
        remote(s, "dirty 🦀.bin").await,
        original_remote,
        "mkdir flushed dirty file"
    );
    let saved = directories(s);
    assert_eq!(saved.len(), 1);
    let cache = fs.cache_control().unwrap();
    drop((f, fs));
    cache.close().await.unwrap();
    drop(cache);
    let policy = &c.writes.as_ref().unwrap().sources[0].policy;
    let original = std::fs::read_to_string(policy).unwrap();
    std::fs::write(
        policy,
        original.replace(
            "writable_prefix='writable/'",
            "writable_prefix='writable/.s3drive/other/'",
        ),
    )
    .unwrap();
    s.proxy.clear();
    s.proxy.mode.store(17, Ordering::SeqCst);
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    assert_eq!(directories(s), saved, "recovery changed directory request");
    let dir = fs.lookup(ROOT, "資料 café 🦀").await.unwrap();
    let node = fs.lookup(ROOT, "dirty 🦀.bin").await.unwrap();
    let read = fs.open(node.id).await.unwrap();
    assert_eq!(read.read(at, 7).await.unwrap(), b"pending");
    assert!(matches!(
        fs.create_directory(dir.id, "denied").await,
        Err(Error::ReadOnly)
    ));
    assert_eq!(directories(s), saved);
    let cache = fs.cache_control().unwrap();
    drop((read, fs));
    cache.close().await.unwrap();
    drop(cache);
    std::fs::write(policy, original).unwrap();
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    assert!(matches!(
        fs.create_directory(ROOT, "feature denied").await,
        Err(Error::ReadOnly)
    ));
    s.proxy.mode.store(0, Ordering::SeqCst);
    let dir = fs.lookup(ROOT, "資料 café 🦀").await.unwrap();
    let nested = fs
        .create_directory(dir.id, "nested %2F 日本語")
        .await
        .unwrap();
    assert!(matches!(
        fs.create_directory(dir.id, "nested %2F 日本語").await,
        Err(Error::AlreadyExists)
    ));
    assert!(matches!(
        fs.create_directory(node.id, "invalid parent").await,
        Err(Error::NotDirectory)
    ));
    assert_eq!(fs.list(dir.id).await.unwrap().len(), 1);
    assert_eq!(fs.metadata(node.id).await.unwrap().unwrap().size, at + 7);
    // Recover another exact unknown directory outcome with expired admission.
    s.proxy.clear();
    s.proxy.mode.store(16, Ordering::SeqCst);
    assert!(
        fs.create_directory(ROOT, "expired replay 🦀")
            .await
            .is_err()
    );
    let expired_saved = directories(s);
    assert_eq!(expired_saved.len(), 1);
    let cache = fs.cache_control().unwrap();
    drop(fs);
    cache.close().await.unwrap();
    drop(cache);
    let original = std::fs::read_to_string(policy).unwrap();
    let expired = original.replace(
        "evidence_max_age_seconds=86400",
        "evidence_max_age_seconds=1",
    );
    assert_ne!(expired, original);
    std::fs::write(policy, expired).unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    s.proxy.clear();
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    assert_eq!(directories(s), expired_saved);
    fs.lookup(ROOT, "expired replay 🦀").await.unwrap();
    assert!(matches!(
        fs.create_directory(ROOT, "expired fresh").await,
        Err(Error::ReadOnly)
    ));
    let node = fs.lookup(ROOT, "dirty 🦀.bin").await.unwrap();
    let read = fs.open(node.id).await.unwrap();
    assert_eq!(read.read(at, 7).await.unwrap(), b"pending");
    let cache = fs.cache_control().unwrap();
    drop((read, fs));
    cache.close().await.unwrap();
    drop(cache);
    std::fs::write(policy, original).unwrap();
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    // Dir-only effects never flush the unrelated file; the parent verifies the
    // final exact3 PUTs (canonical empty root + one dirty chunk + its manifest).
    fs.flush_all().await.unwrap();
    stop(fs).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let dir = fs.lookup(ROOT, "資料 café 🦀").await.unwrap();
    assert_eq!(
        fs.lookup(dir.id, "nested %2F 日本語").await.unwrap().name,
        nested.name
    );
    stop(fs).await;
}
pub async fn concurrency(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let f = fs
        .create_file(ROOT, "pending root.bin", |a, b| a == b, true)
        .await
        .unwrap();
    f.write(0, Arc::from(&b"older"[..]), false, false)
        .await
        .unwrap();
    s.proxy.mode.store(15, Ordering::SeqCst);
    let flush = Box::pin(fs.flush_all());
    let mkdir = async {
        s.proxy.publish_gate.wait().await;
        s.proxy.mode.store(0, Ordering::SeqCst);
        f.write(0, Arc::from(&b"newer!"[..]), false, false)
            .await
            .unwrap();
        let dir = fs
            .create_directory(ROOT, "while root unknown 日本語")
            .await
            .unwrap();
        assert_eq!(f.read(0, 6).await.unwrap(), b"newer!");
        assert_eq!(fs.list(dir.id).await.unwrap().len(), 0);
        assert_eq!(
            s.proxy
                .applies()
                .iter()
                .filter(|r| matches!(r.mutation, Mutation::Publish { .. }))
                .count(),
            1,
            "mkdir interfered with unknown root"
        );
        s.proxy.publish_gate.wait().await;
    };
    let (result, ()) = tokio::join!(flush, mkdir);
    result.unwrap();
    assert_eq!(f.read(0, 6).await.unwrap(), b"newer!");
    f.flush().await.unwrap();
    drop(f);
    stop(fs).await;
}
