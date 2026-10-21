//! Actual shared filesystem dispatch + SDK fixtures. OS drivers have their own gate.
use super::*;
use s3drive_fs::{ROOT, ReadOnlyFs};
async fn stop(fs: ReadOnlyFs) {
    let cache = fs.cache_control().unwrap();
    fs.shutdown_writes().await.unwrap();
    drop(fs);
    cache.close().await.unwrap();
}
async fn configuration(
    config: &s3drive_core::Config,
    root: &Path,
    s: &Server,
) -> s3drive_core::Config {
    let seed = setup(config, root, s).await;
    seed.storage().close().await.unwrap();
    drop(seed);
    let mut c = s3drive_core::Config::parse(
        &std::fs::read_to_string(std::env::var("S3DRIVE_CHUNK_CONFIG").unwrap()).unwrap(),
    )
    .unwrap();
    c.version = 8;
    c.writes.as_mut().unwrap().directory = root.join("native-state");
    let token = s3drive_state::create_private_secret(&root.join("native-token"), SECRET.as_bytes())
        .unwrap();
    c.managed = Some(s3drive_core::ManagedConfig {
        authority_ca_file: None,
        windows_disposition: None,
        default_store: Some(id(200)),
        authority: s.url.clone(),
        volume: info(&s.direct).await.id,
        token_file: token,
        stores: vec![s3drive_core::ManagedStore {
            id: id(200),
            source: c.sources[0].alias.clone(),
        }],
    });
    c.validate().unwrap();
    c
}
pub async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    assert!(!fs.is_read_only());
    assert!(!fs.namespace_mutation());
    assert!(!fs.native_lifecycle_enabled());
    let legacy = fs.lookup(ROOT, "café 日本語 🦀.bin").await.unwrap();
    assert!(fs.open_write(legacy.id).await.is_err());
    assert!(
        fs.open(legacy.id)
            .await
            .unwrap()
            .read(0, 1)
            .await
            .unwrap()
            .is_empty()
    );
    let name = "native 日本語 🦀 %2F.bin";
    let file = fs
        .create_file(ROOT, name, |a, b| a == b, true)
        .await
        .unwrap();
    let inode = file.node.id;
    let at = 8 * 1024 * 1024 + 3;
    assert_eq!(
        file.write(at, Arc::from("日本語".as_bytes()), false, false)
            .await
            .unwrap(),
        9
    );
    assert_eq!(file.current_metadata().unwrap().size, at + 9);
    assert_eq!(fs.metadata(inode).await.unwrap().unwrap().size, at + 9);
    let other = fs.open(inode).await.unwrap();
    assert_eq!(
        other.read(at - 2, 11).await.unwrap(),
        [&[0, 0][..], "日本語".as_bytes()].concat()
    );
    s.proxy.mode.store(2, Ordering::SeqCst);
    assert!(
        file.flush().await.is_err(),
        "lost Publish reply must remain unknown"
    );
    let saved = s
        .proxy
        .applies()
        .into_iter()
        .filter(|r| matches!(r.mutation, Mutation::Publish { .. }))
        .collect::<Vec<_>>();
    assert_eq!(saved.len(), 1);
    // Simulate an owner restart without a graceful drain that would recover it.
    let cache = fs.cache_control().unwrap();
    drop((file, other, fs));
    cache.close().await.unwrap();
    drop(cache);
    s.proxy.clear();
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let replay = s
        .proxy
        .applies()
        .into_iter()
        .filter(|r| matches!(r.mutation, Mutation::Publish { .. }))
        .collect::<Vec<_>>();
    assert_eq!(replay, saved, "startup changed saved root request");
    let node = fs.lookup(ROOT, name).await.unwrap();
    let file = fs.open_write(node.id).await.unwrap();
    assert_eq!(file.read(at, 9).await.unwrap(), "日本語".as_bytes());
    file.flush().await.unwrap();
    file.flush().await.unwrap();
    assert_eq!(
        file.write(0, Arc::from(&b"xy"[..]), true, false)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        file.write(at + 10, Arc::from(&b"ABC"[..]), false, true)
            .await
            .unwrap(),
        1
    );
    assert_eq!(file.current_metadata().unwrap().size, at + 11);
    assert_eq!(file.read(at + 9, 3).await.unwrap(), b"xA");
    file.truncate(5).await.unwrap();
    file.truncate(at + 11).await.unwrap();
    assert_eq!(file.read(at - 1, 12).await.unwrap(), vec![0; 12]);
    file.flush().await.unwrap();
    assert!(matches!(
        fs.create_file(ROOT, name, |a, b| a == b, true).await,
        Err(s3drive_fs::Error::AlreadyExists)
    ));
    fs.sync_namespace().await.unwrap();
    drop(file);
    stop(fs).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let node = fs.lookup(ROOT, name).await.unwrap();
    let file = fs.open(node.id).await.unwrap();
    assert_eq!(file.current_metadata().unwrap().size, at + 11);
    assert_eq!(file.read(at, 11).await.unwrap(), vec![0; 11]);
    drop(file);
    stop(fs).await;
}
pub async fn policy(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let name = "policy 日本語 🦀.bin";
    let dirty = fs
        .create_file(ROOT, name, |a, b| a == b, true)
        .await
        .unwrap();
    dirty
        .write(6, Arc::from(&b"pending"[..]), false, false)
        .await
        .unwrap();
    let cache = fs.cache_control().unwrap();
    drop((dirty, fs));
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
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let node = fs.lookup(ROOT, name).await.unwrap();
    assert!(!fs.writable(&node));
    assert!(matches!(
        fs.open_write(node.id).await,
        Err(s3drive_fs::Error::ReadOnly)
    ));
    let held = fs.open(node.id).await.unwrap();
    assert_eq!(held.read(6, 7).await.unwrap(), b"pending");
    assert!(matches!(
        fs.flush_all().await,
        Err(s3drive_fs::Error::ReadOnly)
    ));
    let cache = fs.cache_control().unwrap();
    drop((held, fs));
    cache.close().await.unwrap();
    drop(cache);
    s.proxy.mode.store(14, Ordering::SeqCst);
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let node = fs.lookup(ROOT, name).await.unwrap();
    assert!(
        matches!(fs.open(node.id).await, Err(s3drive_fs::Error::Conflict)),
        "changed live format concealed retained dirty bytes"
    );
    let cache = fs.cache_control().unwrap();
    drop(fs);
    cache.close().await.unwrap();
    drop(cache);
    s.proxy.mode.store(0, Ordering::SeqCst);
    std::fs::write(policy, original).unwrap();
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    fs.flush_all().await.unwrap();
    let node = fs.lookup(ROOT, name).await.unwrap();
    let file = fs.open(node.id).await.unwrap();
    assert_eq!(file.read(6, 7).await.unwrap(), b"pending");
    drop(file);
    stop(fs).await;
}
pub async fn concurrency(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let newer = fs
        .create_file(ROOT, "concurrent 🦀.bin", |a, b| a == b, true)
        .await
        .unwrap();
    newer
        .write(0, Arc::from(&b"older"[..]), false, false)
        .await
        .unwrap();
    s.proxy.mode.store(15, Ordering::SeqCst);
    let flush = Box::pin(fs.flush_all());
    let change = async {
        s.proxy.publish_gate.wait().await;
        newer
            .write(0, Arc::from(&b"newer!"[..]), false, false)
            .await
            .unwrap();
        s.proxy.mode.store(0, Ordering::SeqCst);
        s.proxy.publish_gate.wait().await;
    };
    let (finished, ()) = tokio::join!(flush, change);
    finished.unwrap();
    assert_eq!(newer.read(0, 6).await.unwrap(), b"newer!");
    assert_eq!(newer.current_metadata().unwrap().size, 6);
    newer.flush().await.unwrap();
    drop(newer);
    stop(fs).await;
}

#[cfg(any(target_os = "linux", windows))]
pub async fn mount(config: &s3drive_core::Config, root: &Path, s: &Server) {
    use super::native_mount::{Mount, blocking};
    use std::io::{Read, Seek, SeekFrom, Write};
    let seed = setup(config, root, s).await;
    seed.storage().close().await.unwrap();
    drop(seed);
    let original = std::fs::read_to_string(std::env::var("S3DRIVE_CHUNK_CONFIG").unwrap()).unwrap();
    let mut cfg: toml::Value = toml::from_str(&original).unwrap();
    cfg["version"] = 8.into();
    cfg["writes"]["directory"] = root.join("native-state").to_str().unwrap().into();
    let token = s3drive_state::create_private_secret(&root.join("native-token"), SECRET.as_bytes())
        .unwrap();
    let mut managed = toml::Table::new();
    managed.insert("authority".into(), s.url.clone().into());
    managed.insert("volume".into(), info(&s.direct).await.id.into());
    managed.insert("token_file".into(), token.to_str().unwrap().into());
    managed.insert("default_store".into(), id(200).into());
    let mut mapping = toml::Table::new();
    mapping.insert("id".into(), id(200).into());
    mapping.insert("source".into(), config.sources[0].alias.clone().into());
    managed.insert(
        "stores".into(),
        toml::Value::Array(vec![toml::Value::Table(mapping)]),
    );
    cfg.as_table_mut()
        .unwrap()
        .insert("managed".into(), toml::Value::Table(managed));
    let text = toml::to_string(&cfg).unwrap();
    s3drive_core::Config::parse(&text).unwrap();
    std::fs::write(root.join("native-drive.toml"), text).unwrap();
    let name = "native OS 日本語 café 🦀.bin";
    let at = 9 * 1024 * 1024u64;
    let bytes: Vec<u8> = (0..2 * 1024 * 1024).map(|n| (n % 251) as u8).collect();
    let mut m = Mount::start(root).await;
    let path = m.path.join(name);
    let expected = bytes.clone();
    blocking(&mut m, move || {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.seek(SeekFrom::Start(at)).unwrap();
        file.write_all(&expected).unwrap();
        file.sync_all().unwrap();
        assert_eq!(file.metadata().unwrap().len(), at + expected.len() as u64);
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut hole = [1; 97];
        file.read_exact(&mut hole).unwrap();
        assert_eq!(hole, [0; 97]);
        file.seek(SeekFrom::Start(at)).unwrap();
        let mut actual = vec![0; expected.len()];
        file.read_exact(&mut actual).unwrap();
        assert_eq!(actual, expected);
        file.sync_all().unwrap();
    })
    .await;
    m.stop().await;
    drop(m);
    let mut m = Mount::start(root).await;
    let path = m.path.join(name);
    let expected = bytes;
    blocking(&mut m, move || {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.seek(SeekFrom::Start(at)).unwrap();
        let mut actual = vec![0; expected.len()];
        file.read_exact(&mut actual).unwrap();
        assert_eq!(actual, expected);
        file.set_len(5).unwrap();
        file.seek(SeekFrom::Start(1)).unwrap();
        file.write_all(b"XYZ").unwrap();
        file.set_len(at + expected.len() as u64 + 8).unwrap();
        file.sync_all().unwrap();
        file.seek(SeekFrom::Start(at)).unwrap();
        let mut zeros = vec![1; expected.len() + 8];
        file.read_exact(&mut zeros).unwrap();
        assert!(zeros.iter().all(|b| *b == 0));
    })
    .await;
    m.stop().await;
    drop(m);
    println!(
        "PASS: actual native chunk writer Unicode create, sparse11MiB file, binary2MiB data, exact flush, shrink/regrow and remount; owned loopback only"
    );
}

/// Portable pressure control, separate from actual WinFsp acceptance. The owned
/// child contains a stack overflow if the production dispatch regresses.
pub async fn small_stack(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = configuration(config, root, s).await;
    let fs = Arc::new(ReadOnlyFs::new(&c).await.unwrap());
    let worker = fs.clone();
    let runtime = tokio::runtime::Handle::current();
    let driver = tokio::task::spawn_blocking(move || {
        std::thread::Builder::new()
            .name("small-driver-callback".into())
            .stack_size(128 * 1024)
            .spawn(move || {
                eprintln!("SMALL_STACK_STAGE entered");
                eprintln!("SMALL_STACK_STAGE create");
                let file = runtime
                    .block_on(Box::pin(worker.create_file(
                        ROOT,
                        "callback 日本語 🦀.bin",
                        |a, b| a == b,
                        true,
                    )))
                    .unwrap();
                let at = 8 * 1024 * 1024 + 3;
                eprintln!("SMALL_STACK_STAGE write");
                runtime
                    .block_on(file.write(at, Arc::from(&b"stack"[..]), false, false))
                    .unwrap();
                eprintln!("SMALL_STACK_STAGE truncate");
                runtime.block_on(file.truncate(at + 6)).unwrap();
                assert_eq!(runtime.block_on(file.read(at, 6)).unwrap(), b"stack\0");
                eprintln!("SMALL_STACK_STAGE flush");
                runtime.block_on(file.flush()).unwrap();
                let read = runtime.block_on(worker.open(file.node.id)).unwrap();
                assert_eq!(runtime.block_on(read.read(at, 6)).unwrap(), b"stack\0");
                runtime.block_on(worker.flush_all()).unwrap();
                runtime.block_on(worker.sync_namespace()).unwrap();
                drop((file, read));
                runtime.block_on(worker.shutdown_writes()).unwrap();
            })
            .unwrap()
            .join()
            .unwrap();
    });
    tokio::time::timeout(Duration::from_secs(120), driver)
        .await
        .unwrap()
        .unwrap();
    let cache = fs.cache_control().unwrap();
    drop(fs);
    cache.close().await.unwrap();
}
