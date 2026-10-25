//! Opt-in shared-engine gates. These do not substitute for real OS callbacks.
use super::*;
use s3drive_fs::{ROOT, ReadOnlyFs};
async fn configuration(
    config: &s3drive_core::Config,
    root: &Path,
    s: &Server,
) -> s3drive_core::Config {
    let mut c = native_writer::configuration(config, root, s).await;
    c.managed.as_mut().unwrap().sparse_lifecycle =
        Some(if root.join("component-retention-fixture").exists() {
            s3drive_core::SparseLifecycle::ComponentRetentionV1
        } else {
            s3drive_core::SparseLifecycle::OwnedUnlinkedV1
        });
    c.validate().unwrap();
    c
}
#[cfg(any(target_os = "linux", windows))]
async fn mount_configuration(config: &s3drive_core::Config, root: &Path, s: &Server) {
    native_writer::mount_configuration(config, root, s).await;
    let path = root.join("native-drive.toml");
    let mut c: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    c.get_mut("managed")
        .unwrap()
        .as_table_mut()
        .unwrap()
        .insert(
            "sparse-lifecycle".into(),
            if root.join("component-retention-fixture").exists() {
                "component-retention-v1"
            } else {
                "owned-unlinked-v1"
            }
            .into(),
        );
    let text = toml::to_string(&c).unwrap();
    s3drive_core::Config::parse(&text).unwrap();
    std::fs::write(path, text).unwrap();
}

fn inspect_component_permanent(db: &rusqlite::Connection) {
    for (table, maximum) in [
        ("windows_mount_sessions", 2),
        ("source_bindings", 1),
        ("client_identity", 1),
        ("retained_client_identity", 1),
        ("chunk_mount_role", 1),
        ("source_read_pin_session", 1),
        ("sparse_scoped_mode", 1),
        ("sparse_retry_mode", 1),
        ("sqlite_sequence", 16),
    ] {
        let count = db
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap();
        assert!(
            count <= maximum,
            "permanent {table}={count} exceeds {maximum}"
        );
        eprintln!("COMPONENT_PERMANENT table={table} rows={count} maximum={maximum}");
    }
    let payload: i64 = db
        .query_row(
            "SELECT coalesce(sum(length(data)),0) FROM staged_chunks",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        payload <= 2 * 1024 * 1024,
        "physical payload exceeded bound"
    );
    eprintln!("COMPONENT_PHYSICAL payload={payload} maximum=2097152");
}

fn inspect_component_closed(root: &Path) {
    if !root.join("component-retention-fixture").exists() {
        return;
    }
    let db = rusqlite::Connection::open_with_flags(
        root.join("native-state/metadata.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    inspect_component_permanent(&db);
    let integrity: Vec<String> = db
        .prepare("PRAGMA integrity_check")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(integrity, ["ok"], "closed component SQLite integrity");
    let foreign: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!foreign, "closed component foreign-key violation");
    eprintln!("COMPONENT_SQLITE integrity=ok foreign_keys=ok");
    for table in [
        "windows_open_journal",
        "windows_open_events",
        "retained_windows_opens",
        "retired_windows_opens",
        "recovered_windows_opens",
        "source_read_pins",
        "staged_chunks",
        "staged_extents",
        "staged_operations",
        "staged_heads",
        "write_journal",
        "remote_intents",
        "retained_remote_intents",
        "remote_flushes",
        "write_settlements",
        "sparse_view_files",
        "sparse_view_operations",
        "sparse_view_blocks",
        "sparse_preparation_blobs",
        "sparse_preparation_objects",
        "sparse_publication_checks",
        "namespace_intents",
        "manifest_creations",
        "sparse_views",
        "sparse_preparations",
        "sparse_publications",
        "staged_generations",
        "windows_session_recoveries",
    ] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0,
            "component closed forest retained {table}"
        );
    }
    eprintln!("COMPONENT_RETENTION closed forest zero; permanent identities preserved");
}
pub async fn unlink(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    assert!(fs.native_posix_unlink_rename_enabled());
    assert!(!fs.native_lifecycle_enabled());
    let file = fs
        .create_file(ROOT, "held 日本語.bin", |a, b| a == b, true)
        .await
        .unwrap();
    let other = fs.open(file.node.id).await.unwrap();
    file.write(3, Arc::from("日本語 🦀\0".as_bytes()), false, false)
        .await
        .unwrap();
    assert_eq!(other.read(3, 64).await.unwrap(), "日本語 🦀\0".as_bytes());
    fs.remove_entry(ROOT, "held 日本語.bin", false)
        .await
        .unwrap();
    assert!(fs.lookup(ROOT, "held 日本語.bin").await.is_err());
    assert!(file.is_unlinked().unwrap());
    file.write(1, Arc::from(&b"z"[..]), false, false)
        .await
        .unwrap();
    let original = file.native_object().unwrap();
    let close = original.close();
    assert!(
        file.write(0, Arc::from(&b"forbidden"[..]), false, false)
            .await
            .is_err()
    );
    close.await.unwrap();
    assert_eq!(other.read(0, 3).await.unwrap(), &[0, b'z', 0]);
    other.native_object().unwrap().close().await.unwrap();
    assert!(matches!(
        ask(
            &s.direct,
            Request::PendingRemoval {
                volume: c.managed.as_ref().unwrap().volume.clone(),
                node: s
                    .proxy
                    .applies()
                    .into_iter()
                    .find_map(|a| match a.mutation {
                        Mutation::ArmRemoval { node, .. } => Some(node.id),
                        _ => None,
                    })
                    .unwrap()
            }
        )
        .await,
        Reply::PendingRemoval(None)
    ));
    drop((file, other, original));
    native_writer::stop(fs).await;
    inspect_component_closed(root);
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    assert!(fs.lookup(ROOT, "held 日本語.bin").await.is_err());
    native_writer::stop(fs).await;
    inspect_component_closed(root);
}
pub async fn replace(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let old = fs
        .create_file(ROOT, "saved 日本語.bin", |a, b| a == b, true)
        .await
        .unwrap();
    old.write(0, Arc::from(&b"old body"[..]), false, false)
        .await
        .unwrap();
    let temp = fs
        .create_file(ROOT, "temporary.bin", |a, b| a == b, true)
        .await
        .unwrap();
    temp.write(0, Arc::from(&b"new body"[..]), false, false)
        .await
        .unwrap();
    fs.rename_entry(
        ROOT,
        "temporary.bin",
        ROOT,
        "saved 日本語.bin",
        true,
        |a, b| a == b,
    )
    .await
    .unwrap();
    assert!(old.is_unlinked().unwrap());
    assert_eq!(old.read(0, 64).await.unwrap(), b"old body");
    let saved = fs.lookup(ROOT, "saved 日本語.bin").await.unwrap();
    assert_ne!(saved.id, old.node.id);
    let current = fs.open(saved.id).await.unwrap();
    assert_eq!(current.read(0, 64).await.unwrap(), b"new body");
    old.write(0, Arc::from(&b"OLD"[..]), false, false)
        .await
        .unwrap();
    old.native_object().unwrap().close().await.unwrap();
    assert_eq!(current.read(0, 64).await.unwrap(), b"new body");
    temp.native_object().unwrap().close().await.unwrap();
    current.native_object().unwrap().close().await.unwrap();
    drop((old, temp, current));
    native_writer::stop(fs).await;
    inspect_component_closed(root);
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let saved = fs.lookup(ROOT, "saved 日本語.bin").await.unwrap();
    let current = fs.open(saved.id).await.unwrap();
    assert_eq!(current.read(0, 64).await.unwrap(), b"new body");
    current.native_object().unwrap().close().await.unwrap();
    drop(current);
    native_writer::stop(fs).await;
    inspect_component_closed(root);
}

#[cfg(any(target_os = "linux", windows))]
pub async fn mounted(config: &s3drive_core::Config, root: &Path, s: &Server) {
    use native_mount::{Mount, blocking};
    use std::io::{Read, Seek, SeekFrom, Write};
    mount_configuration(config, root, s).await;
    let mut mount = Mount::start(root).await;
    let base = mount.path.clone();
    // Keep the original single 120-second bound for the entire live-mount
    // sequence. Per-call measurements distinguish validation cost from waits;
    // splitting diagnostic phases is not acceptance of the original regression.
    blocking(&mut mount, move || {
        let sequence = std::time::Instant::now();
        macro_rules! call {
            ($name:literal, $operation:expr) => {{
                let started = std::time::Instant::now();
                let result = $operation;
                eprintln!(
                    "NATIVE_LIFECYCLE_CALL {} duration={:?} sequence={:?}",
                    $name,
                    started.elapsed(),
                    sequence.elapsed()
                );
                result
            }};
        }
        let path = base.join("unlink 日本語 🦀.bin");
        let mut file = call!(
            "unlink-create",
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
        )
        .unwrap();
        let mut other = call!("unlink-open-other", std::fs::File::open(&path)).unwrap();
        call!("unlink-seek-writer", file.seek(SeekFrom::Start(3))).unwrap();
        call!("unlink-write", file.write_all("日本語 🦀\0".as_bytes())).unwrap();
        let mut bytes = vec![0; "日本語 🦀\0".len()];
        call!("unlink-seek-reader", other.seek(SeekFrom::Start(3))).unwrap();
        call!("unlink-dirty-read", other.read_exact(&mut bytes)).unwrap();
        assert_eq!(bytes, "日本語 🦀\0".as_bytes());
        call!("unlink-remove", std::fs::remove_file(&path)).unwrap();
        assert!(!call!("unlink-exists", path.exists()));
        call!("unlinked-seek-writer", file.seek(SeekFrom::Start(1))).unwrap();
        call!("unlinked-write", file.write_all(b"z")).unwrap();
        call!("unlinked-sync", file.sync_all()).unwrap();
        call!("unlinked-close-writer", drop(file));
        call!("unlinked-seek-reader", other.seek(SeekFrom::Start(0))).unwrap();
        let mut bytes = [9; 3];
        call!("unlinked-read", other.read_exact(&mut bytes)).unwrap();
        assert_eq!(bytes, [0, b'z', 0]);
        call!("unlinked-last-close", drop(other));
        let saved = base.join("saved 日本語.bin");
        let temporary = base.join("temporary.bin");
        let mut old = call!(
            "replace-create-old",
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&saved)
        )
        .unwrap();
        call!("replace-write-old", old.write_all(b"old body")).unwrap();
        let mut temp = call!(
            "replace-create-temp",
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&temporary)
        )
        .unwrap();
        call!("replace-write-temp", temp.write_all(b"new body")).unwrap();
        call!("replace-rename", std::fs::rename(&temporary, &saved)).unwrap();
        assert!(!call!("replace-temp-exists", temporary.exists()));
        call!("replace-seek-old", old.seek(SeekFrom::Start(0))).unwrap();
        let mut bytes = [0; 8];
        call!("replace-read-old", old.read_exact(&mut bytes)).unwrap();
        assert_eq!(&bytes, b"old body");
        assert_eq!(
            call!("replace-read-path", std::fs::read(&saved)).unwrap(),
            b"new body"
        );
        call!("replace-seek-detached", old.seek(SeekFrom::Start(0))).unwrap();
        call!("replace-write-detached", old.write_all(b"OLD")).unwrap();
        call!("replace-sync-detached", old.sync_all()).unwrap();
        call!("replace-close-old", drop(old));
        call!("replace-close-temp", drop(temp));
        let mut file = call!("final-reopen", std::fs::File::open(saved)).unwrap();
        let mut bytes = Vec::new();
        call!("final-read", file.read_to_end(&mut bytes)).unwrap();
        assert_eq!(bytes, b"new body");
        call!("final-close", drop(file));
    })
    .await;
    mount.stop().await;
    drop(mount);
    inspect_component_closed(root);
    let mut mount = Mount::start(root).await;
    let base = mount.path.clone();
    blocking(&mut mount, move || {
        assert!(!base.join("unlink 日本語 🦀.bin").exists());
        assert_eq!(
            std::fs::read(base.join("saved 日本語.bin")).unwrap(),
            b"new body"
        );
    })
    .await;
    mount.stop().await;
}

pub async fn cold_known(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let file = fs
        .create_file(ROOT, "cold known.bin", |a, b| a == b, true)
        .await
        .unwrap();
    file.write(0, Arc::from(&b"known root after crash"[..]), false, false)
        .await
        .unwrap();
    s.proxy.mode.store(2, Ordering::SeqCst);
    assert!(file.native_object().unwrap().close().await.is_err());
    let saved = s
        .proxy
        .applies()
        .into_iter()
        .filter(|a| matches!(a.mutation, Mutation::Publish { .. }))
        .collect::<Vec<_>>();
    assert_eq!(saved.len(), 1);
    let cache = fs.cache_control().unwrap();
    drop((file, fs));
    cache.close().await.unwrap();
    drop(cache);
    s.proxy.clear();
    let runtime = tokio::runtime::Handle::current();
    let fs = tokio::task::spawn_blocking(move || {
        std::thread::Builder::new()
            .name("native-cold-small-stack".into())
            .stack_size(128 * 1024)
            .spawn(move || runtime.block_on(Box::pin(ReadOnlyFs::new(&c))).unwrap())
            .unwrap()
            .join()
            .unwrap()
    })
    .await
    .unwrap();
    let replay = s
        .proxy
        .applies()
        .into_iter()
        .filter(|a| matches!(a.mutation, Mutation::Publish { .. }))
        .collect::<Vec<_>>();
    assert_eq!(
        replay, saved,
        "cold startup changed the saved root operation"
    );
    let node = fs.lookup(ROOT, "cold known.bin").await.unwrap();
    let file = fs.open(node.id).await.unwrap();
    assert_eq!(file.read(0, 64).await.unwrap(), b"known root after crash");
    file.native_object().unwrap().close().await.unwrap();
    drop(file);
    native_writer::stop(fs).await;
}
pub async fn cold_unknown(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let mut c = configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let file = fs
        .create_file(ROOT, "unprepared retained.bin", |a, b| a == b, true)
        .await
        .unwrap();
    file.write(
        0,
        Arc::from(&b"must not adopt this original epoch"[..]),
        false,
        false,
    )
    .await
    .unwrap();
    let cache = fs.cache_control().unwrap();
    drop((file, fs));
    cache.close().await.unwrap();
    drop(cache);
    fn retained(path: &Path) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let db =
            rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        let query = |sql| {
            db.prepare(sql)
                .unwrap()
                .query_map([], |r| r.get::<_, Vec<u8>>(0))
                .unwrap()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap()
        };
        (
            query("SELECT data FROM sparse_view_blocks ORDER BY block"),
            query("SELECT action FROM windows_open_events ORDER BY event_id"),
        )
    }
    let database = c
        .writes
        .as_ref()
        .unwrap()
        .directory
        .join("metadata.sqlite3");
    let before = retained(&database);
    s.proxy.clear();
    assert!(ReadOnlyFs::new(&c).await.is_err());
    c.managed.as_mut().unwrap().sparse_lifecycle = None;
    assert!(
        matches!(
            ReadOnlyFs::new(&c).await,
            Err(s3drive_fs::Error::Configuration)
        ),
        "removing opt-in adopted a native epoch"
    );
    assert_eq!(
        retained(&database),
        before,
        "cold refusal changed original source/callback evidence"
    );
    assert!(
        !s.proxy
            .applies()
            .iter()
            .any(|a| matches!(a.mutation, Mutation::Publish { .. })),
        "cold startup minted fresh publication"
    );
}

#[cfg(any(target_os = "linux", windows))]
pub async fn mounted_crash(config: &s3drive_core::Config, root: &Path, s: &Server, known: bool) {
    use native_mount::{Mount, blocking};
    use std::io::Write;
    mount_configuration(config, root, s).await;
    let mut mount = Mount::start(root).await;
    let path = mount.path.join("killed owner 日本語.bin");
    let file = blocking(&mut mount, move || {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(b"retained original bytes").unwrap();
        file
    })
    .await;
    if known {
        s.proxy.mode.store(2, Ordering::SeqCst);
        let file = blocking(&mut mount, move || {
            assert!(file.sync_all().is_err());
            file
        })
        .await;
        mount.crash().await;
        drop(file);
    } else {
        mount.crash().await;
        drop(file);
    }
    drop(mount);
    let saved = s
        .proxy
        .applies()
        .into_iter()
        .filter(|a| matches!(a.mutation, Mutation::Publish { .. }))
        .collect::<Vec<_>>();
    assert_eq!(saved.len(), usize::from(known));
    s.proxy.clear();
    if known {
        let mut mount = Mount::start(root).await;
        let path = mount.path.join("killed owner 日本語.bin");
        blocking(&mut mount, move || {
            assert_eq!(std::fs::read(path).unwrap(), b"retained original bytes");
        })
        .await;
        let replay = s
            .proxy
            .applies()
            .into_iter()
            .filter(|a| matches!(a.mutation, Mutation::Publish { .. }))
            .collect::<Vec<_>>();
        assert_eq!(replay, saved);
        mount.stop().await;
    } else {
        fn source(root: &Path) -> Vec<Vec<u8>> {
            let db = rusqlite::Connection::open_with_flags(
                root.join("native-state/metadata.sqlite3"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            db.prepare("SELECT data FROM sparse_view_blocks ORDER BY block")
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .collect::<std::result::Result<_, _>>()
                .unwrap()
        }
        let before = source(root);
        assert!(!before.is_empty());
        Mount::refused(root).await;
        assert_eq!(source(root), before);
        assert!(
            !s.proxy
                .applies()
                .iter()
                .any(|a| matches!(a.mutation, Mutation::Publish { .. }))
        );
    }
}

pub async fn profile_transition(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let mut c = native_writer::configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    assert!(!fs.namespace_mutation());
    assert!(!fs.native_posix_unlink_rename_enabled());
    let file = fs
        .create_file(ROOT, "legacy epoch.bin", |a, b| a == b, true)
        .await
        .unwrap();
    assert!(file.native_object().is_none());
    file.write(0, Arc::from(&b"legacy retained epoch"[..]), false, false)
        .await
        .unwrap();
    let cache = fs.cache_control().unwrap();
    drop((file, fs));
    cache.close().await.unwrap();
    drop(cache);
    c.managed.as_mut().unwrap().sparse_lifecycle =
        Some(s3drive_core::SparseLifecycle::OwnedUnlinkedV1);
    s.proxy.clear();
    assert!(
        matches!(ReadOnlyFs::new(&c).await, Err(s3drive_fs::Error::Conflict)),
        "opt-in adopted an old unprepared epoch"
    );
    assert!(
        !s.proxy
            .applies()
            .iter()
            .any(|a| matches!(a.mutation, Mutation::Publish { .. }))
    );
    c.managed.as_mut().unwrap().sparse_lifecycle = None;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let node = fs.lookup(ROOT, "legacy epoch.bin").await.unwrap();
    let file = fs.open(node.id).await.unwrap();
    assert_eq!(file.read(0, 64).await.unwrap(), b"legacy retained epoch");
    fs.flush_all().await.unwrap();
    drop(file);
    native_writer::stop(fs).await;
    c.managed.as_mut().unwrap().sparse_lifecycle =
        Some(s3drive_core::SparseLifecycle::OwnedUnlinkedV1);
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    assert!(fs.namespace_mutation());
    let node = fs.lookup(ROOT, "legacy epoch.bin").await.unwrap();
    let file = fs.open(node.id).await.unwrap();
    assert_eq!(file.read(0, 64).await.unwrap(), b"legacy retained epoch");
    file.native_object().unwrap().close().await.unwrap();
    drop(file);
    native_writer::stop(fs).await;
    c.managed.as_mut().unwrap().sparse_lifecycle = None;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    assert!(!fs.namespace_mutation());
    let node = fs.lookup(ROOT, "legacy epoch.bin").await.unwrap();
    let file = fs.open_write(node.id).await.unwrap();
    assert!(file.native_object().is_none());
    file.write(0, Arc::from(&b"settled"[..]), false, false)
        .await
        .unwrap();
    file.flush().await.unwrap();
    assert_eq!(file.read(0, 7).await.unwrap(), b"settled");
    drop(file);
    native_writer::stop(fs).await;
}

pub async fn ordered_flush(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = configuration(config, root, s).await;
    let fs = Arc::new(ReadOnlyFs::new(&c).await.unwrap());
    let file = Arc::new(
        fs.create_file(ROOT, "ordered epoch.bin", |a, b| a == b, true)
            .await
            .unwrap(),
    );
    file.write(0, Arc::from(&b"older"[..]), false, false)
        .await
        .unwrap();
    s.proxy.mode.store(15, Ordering::SeqCst);
    let owner = fs.clone();
    let flush = tokio::spawn(async move { owner.flush_all().await });
    s.proxy.publish_gate.wait().await;
    let held = file.clone();
    let mut write =
        tokio::spawn(async move { held.write(0, Arc::from(&b"newer!"[..]), false, false).await });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut write)
            .await
            .is_err(),
        "opted-in write bypassed the publication barrier"
    );
    s.proxy.mode.store(0, Ordering::SeqCst);
    s.proxy.publish_gate.wait().await;
    flush.await.unwrap().unwrap();
    assert_eq!(write.await.unwrap().unwrap(), 6);
    assert_eq!(file.read(0, 6).await.unwrap(), b"newer!");
    let object = file.native_object().unwrap();
    let close = object.close();
    assert!(
        file.write(0, Arc::from(&b"late"[..]), false, false)
            .await
            .is_err()
    );
    close.await.unwrap();
    drop((object, file));
    let fs = Arc::try_unwrap(fs).ok().unwrap();
    native_writer::stop(fs).await;
}

// Shared adapter/controller control, not a claim of actual Windows callbacks.
pub async fn disposition(config: &s3drive_core::Config, root: &Path, s: &Server, concurrent: bool) {
    let c = configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let file = fs
        .create_file(ROOT, "callback unlink.bin", |a, b| a == b, true)
        .await
        .unwrap();
    let other = fs.open(file.node.id).await.unwrap();
    file.write(0, Arc::from(&b"original"[..]), false, false)
        .await
        .unwrap();
    let deleting = fs.open(file.node.id).await.unwrap();
    let object = deleting.native_object().unwrap();
    object.set_delete(true).await.unwrap();
    object.cleanup(1).await.unwrap();
    let mut closing = Some(object.close());
    if !concurrent {
        closing.take().unwrap().await.unwrap();
    }
    // Actual Close is already synchronously fenced. A different still-open
    // handle waits for that original job, rather than racing its state fence.
    file.write(0, Arc::from(&b"later"[..]), false, false)
        .await
        .unwrap();
    if let Some(pending) = closing.take() {
        pending.await.unwrap();
    }
    drop(closing);
    drop((deleting, object));
    assert!(fs.lookup(ROOT, "callback unlink.bin").await.is_err());
    assert_eq!(other.read(0, 8).await.unwrap(), b"laternal");
    file.native_object().unwrap().close().await.unwrap();
    other.native_object().unwrap().close().await.unwrap();
    drop((file, other));
    native_writer::stop(fs).await;
}

// Three acknowledged epochs from two original publishers, including a detached
// epoch. Repeated clean flushes/Close must never replay already completed PUTs.
pub async fn acknowledged_history(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let first = fs
        .create_file(ROOT, "history.bin", |a, b| a == b, true)
        .await
        .unwrap();
    let second = fs.open_write(first.node.id).await.unwrap();
    first
        .write(0, Arc::from(&b"epoch-one"[..]), false, false)
        .await
        .unwrap();
    first.flush().await.unwrap();
    second
        .write(0, Arc::from(&b"epoch-two"[..]), false, false)
        .await
        .unwrap();
    second.flush().await.unwrap();
    fs.remove_entry(ROOT, "history.bin", false).await.unwrap();
    first
        .write(0, Arc::from(&b"epoch-end"[..]), false, false)
        .await
        .unwrap();
    first.flush().await.unwrap();
    for _ in 0..2 {
        second.flush().await.unwrap();
        first.flush().await.unwrap();
        assert_eq!(second.read(0, 9).await.unwrap(), b"epoch-end");
    }
    first.native_object().unwrap().close().await.unwrap();
    second.native_object().unwrap().close().await.unwrap();
    drop((first, second));
    native_writer::stop(fs).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    assert!(fs.lookup(ROOT, "history.bin").await.is_err());
    native_writer::stop(fs).await;
}

/// Repeated real SDK create/rename/unlink closure while unrelated dirty handles
/// and their cached reader remain live. No test-only collector calls.
pub async fn component_repeated(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let protected = fs
        .create_file(ROOT, "protected.bin", |a, b| a == b, true)
        .await
        .unwrap();
    protected
        .write(0, Arc::from(&b"unsaved protected"[..]), false, false)
        .await
        .unwrap();
    let reader = fs.open(protected.node.id).await.unwrap();
    assert_eq!(reader.read(0, 64).await.unwrap(), b"unsaved protected");
    for cycle in 0..4 {
        let from = format!("temporary-{cycle}.bin");
        let to = format!("renamed-{cycle}.bin");
        let file = fs
            .create_file(ROOT, &from, |a, b| a == b, true)
            .await
            .unwrap();
        file.write(0, Arc::from(&b"initial content"[..]), false, false)
            .await
            .unwrap();
        fs.rename_entry(ROOT, &from, ROOT, &to, false, |a, b| a == b)
            .await
            .unwrap();
        assert!(fs.lookup(ROOT, &from).await.is_err());
        assert_eq!(file.read(0, 64).await.unwrap(), b"initial content");
        fs.remove_entry(ROOT, &to, false).await.unwrap();
        assert!(fs.lookup(ROOT, &to).await.is_err());
        file.write(0, Arc::from(&b"FINAL"[..]), false, false)
            .await
            .unwrap();
        assert_eq!(file.read(0, 64).await.unwrap(), b"FINALal content");
        file.native_object().unwrap().close().await.unwrap();
        drop(file);
        assert_eq!(reader.read(0, 64).await.unwrap(), b"unsaved protected");
        let db = rusqlite::Connection::open_with_flags(
            root.join("native-state/metadata.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        for (table, count) in [
            ("windows_open_journal", 2),
            ("sparse_view_files", 1),
            ("namespace_intents", 0),
            ("manifest_creations", 0),
            ("sparse_preparations", 0),
            ("staged_generations", 0),
        ] {
            assert_eq!(
                db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                count,
                "cycle {cycle} retained {table}"
            );
        }
        eprintln!(
            "COMPONENT_REPEAT cycle={cycle} unrelated_dirty_read=exact retained_files=1 retained_opens=2 physical=0"
        );
    }
    protected.native_object().unwrap().close().await.unwrap();
    assert_eq!(reader.read(0, 64).await.unwrap(), b"unsaved protected");
    reader.native_object().unwrap().close().await.unwrap();
    drop((protected, reader));
    native_writer::stop(fs).await;
    inspect_component_closed(root);
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let node = fs.lookup(ROOT, "protected.bin").await.unwrap();
    let file = fs.open(node.id).await.unwrap();
    assert_eq!(file.read(0, 64).await.unwrap(), b"unsaved protected");
    file.native_object().unwrap().close().await.unwrap();
    drop(file);
    native_writer::stop(fs).await;
    inspect_component_closed(root);
}

#[cfg(any(target_os = "linux", windows))]
pub async fn component_repeated_mounted(config: &s3drive_core::Config, root: &Path, s: &Server) {
    use native_mount::{Mount, blocking};
    use std::io::{Read, Seek, SeekFrom, Write};
    let completed = match std::fs::read_to_string(root.join("component-completed")) {
        Ok(value) => value.parse::<u64>().unwrap(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => panic!("component progress: {error}"),
    };
    if completed == 0 {
        mount_configuration(config, root, s).await;
    } else {
        let path = root.join("native-drive.toml");
        let mut value: toml::Value =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        value["managed"]["authority"] = s.url.clone().into();
        let text = toml::to_string(&value).unwrap();
        s3drive_core::Config::parse(&text).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut mount = Mount::start(root).await;
    let base = mount.path.clone();
    let database = root.join("native-state/metadata.sqlite3");
    blocking(&mut mount, move || {
        if completed != 0 {
            assert_eq!(std::fs::read(base.join("protected.bin")).unwrap(), b"unsaved protected");
        }
        let mut protected = std::fs::OpenOptions::new().create_new(completed == 0).write(true).read(true).open(base.join("protected.bin")).unwrap();
        protected.write_all(b"unsaved protected").unwrap();
        let mut reader = std::fs::File::open(base.join("protected.bin")).unwrap();
        for cycle in completed..completed+4 {
            let from = base.join(format!("temporary-{cycle}.bin"));
            let to = base.join(format!("renamed-{cycle}.bin"));
            let mut file = std::fs::OpenOptions::new().create_new(true).write(true).read(true).open(&from).unwrap();
            file.write_all(b"initial content").unwrap();
            std::fs::rename(&from, &to).unwrap();
            assert!(!from.exists());
            std::fs::remove_file(&to).unwrap();
            assert!(!to.exists());
            file.seek(SeekFrom::Start(0)).unwrap();
            file.write_all(b"FINAL").unwrap();
            file.sync_all().unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes,b"FINALal content");
            drop(file);
            reader.seek(SeekFrom::Start(0)).unwrap();
            bytes.clear();
            reader.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes,b"unsaved protected");
            let db = rusqlite::Connection::open_with_flags(&database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
            // OS Close dispatch may still be in flight. Bounded overlap is not
            // mistaken for an observed terminal Close or forced reclamation.
            for (table, maximum) in [("windows_open_journal",8), ("sparse_view_files",3), ("staged_generations",8), ("namespace_intents",16)] {
                let count = db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get::<_,i64>(0)).unwrap();
                assert!(count<=maximum,"cycle {cycle}: {table}={count} exceeds {maximum}");
            }
            inspect_component_permanent(&db);
            let bytes = std::fs::metadata(&database).unwrap().len();
            let wal = std::fs::metadata(database.with_file_name("metadata.sqlite3-wal")).map(|m|m.len()).unwrap_or(0);
            assert!(bytes<=4*1024*1024 && wal<=8*1024*1024);
            eprintln!("COMPONENT_NATIVE_REPEAT cycle={cycle} db={bytes} wal={wal} unrelated_dirty_read=exact");
        }
        protected.sync_all().unwrap();
        drop(protected);
        reader.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes,b"unsaved protected");
        drop(reader);
    }).await;
    mount.stop().await;
    drop(mount);
    inspect_component_closed(root);
    let mut mount = Mount::start(root).await;
    let base = mount.path.clone();
    blocking(&mut mount, move || {
        assert_eq!(
            std::fs::read(base.join("protected.bin")).unwrap(),
            b"unsaved protected"
        );
        for cycle in 0..completed + 4 {
            assert!(!base.join(format!("temporary-{cycle}.bin")).exists());
            assert!(!base.join(format!("renamed-{cycle}.bin")).exists());
        }
    })
    .await;
    mount.stop().await;
    drop(mount);
    inspect_component_closed(root);
    std::fs::write(
        root.join("component-completed"),
        (completed + 4).to_string(),
    )
    .unwrap();
    eprintln!(
        "COMPONENT_NATIVE_SOAK completed={} cold_read=exact closed_forest=zero",
        completed + 4
    );
}

// Synthetic test data only. No raw database/archive is uploaded or emitted.
pub fn inspect_component_failure(root: &Path) {
    let path = root.join("native-state/metadata.sqlite3");
    let db = match rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(db) => db,
        Err(error) => {
            eprintln!("COMPONENT_FAILURE open={error:?}");
            return;
        }
    };
    db.busy_timeout(Duration::from_secs(2)).unwrap();
    eprintln!("COMPONENT_FAILURE sqlite={}", rusqlite::version());
    for sql in ["PRAGMA quick_check(8)", "PRAGMA integrity_check(8)"] {
        let result = (|| -> rusqlite::Result<Vec<String>> {
            db.prepare(sql)?
                .query_map([], |r| r.get(0))?
                .take(8)
                .collect()
        })();
        eprintln!("COMPONENT_FAILURE {sql}: {result:?}");
    }
    for table in [
        "sparse_view_files",
        "sparse_views",
        "sparse_preparations",
        "sparse_publications",
        "staged_generations",
        "staged_chunks",
        "windows_open_journal",
        "namespace_intents",
        "manifest_creations",
    ] {
        let result = db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
            r.get::<_, i64>(0)
        });
        eprintln!("COMPONENT_FAILURE count {table}: {result:?}");
    }
    let foreign: rusqlite::Result<bool> = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
        [],
        |r| r.get(0),
    );
    eprintln!("COMPONENT_FAILURE foreign_key_violation={foreign:?}");
}
