use super::*;
use std::{
    fs,
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn key(name: &str, size: u64) -> Key {
    Key::new(
        [1; 32],
        &"a".repeat(64),
        name,
        Some("\"revision\""),
        None,
        size,
    )
    .unwrap()
}
fn options(bytes: u64, entries: usize) -> Options {
    Options {
        max_bytes: bytes,
        max_file_bytes: bytes.min(MAX_FILE_BYTES as u64) as usize,
        max_entries: entries,
    }
}
fn insert(store: &mut Store, name: &str, data: &[u8]) -> Option<Pinned> {
    store
        .put(&key(name, data.len() as u64), Arc::from(data))
        .unwrap()
}

#[test]
fn cache_addresses_bind_every_identity_component_and_preserve_unicode() {
    let base = key("é", 3);
    for other in [
        Key::new([2; 32], &"a".repeat(64), "é", Some("\"revision\""), None, 3).unwrap(),
        Key::new([1; 32], &"b".repeat(64), "é", Some("\"revision\""), None, 3).unwrap(),
        key("e\u{301}", 3),
        key("é", 4),
        Key::new([1; 32], &"a".repeat(64), "é", Some("changed"), None, 3).unwrap(),
        Key::new([1; 32], &"a".repeat(64), "é", None, Some("\"revision\""), 3).unwrap(),
        Key::new(
            [1; 32],
            &"a".repeat(64),
            "é",
            Some("\"revision\""),
            Some("v1"),
            3,
        )
        .unwrap(),
    ] {
        assert_ne!(base.digest, other.digest);
    }
    assert_eq!(base.digest, key("é", 3).digest);
    assert!(!format!("{base:?}").contains('é'));
    assert!(Key::new([1; 32], &"a".repeat(64), "file", None, None, 0).is_err());
    assert!(Key::new([1; 32], &"a".repeat(64), "file", Some(""), None, 0).is_err());
}

#[tokio::test]
async fn invalid_limits_do_not_create_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state");
    for bad in [
        Options {
            max_bytes: 0,
            ..Options::default()
        },
        Options {
            max_entries: 0,
            ..Options::default()
        },
        Options {
            max_file_bytes: MAX_FILE_BYTES + 1,
            ..Options::default()
        },
        Options {
            max_bytes: 2,
            max_file_bytes: 3,
            max_entries: 1,
        },
    ] {
        assert!(Cache::open(&path, bad).await.is_err());
        assert!(!path.exists());
    }
}

#[test]
fn quota_eviction_respects_lru_pins_and_empty_entry_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state");
    let mut store = Store::open(&path, options(6, 2)).unwrap();
    let pinned = insert(&mut store, "a", b"abc").unwrap();
    drop(insert(&mut store, "b", b"def"));
    let same = store.get(&key("a", 3)).unwrap().unwrap();
    assert!(Arc::ptr_eq(&pinned.0, &same.0));
    let c = insert(&mut store, "c", b"ghi").unwrap();
    assert!(store.get(&key("b", 3)).unwrap().is_none());
    // Both remaining entries are pinned. No eviction or partial insertion.
    assert!(insert(&mut store, "d", b"jkl").is_none());
    assert_eq!(pinned.bytes(), b"abc");
    drop(pinned);
    drop(same);
    drop(c);
    drop(insert(&mut store, "empty-1", b""));
    drop(insert(&mut store, "empty-2", b""));
    let count: i64 = store
        .state
        .connection
        .query_row("SELECT count(*) FROM clean_blobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
    assert!(store.get(&key("c", 3)).unwrap().is_none());
    assert_eq!(store.get(&key("empty-1", 0)).unwrap().unwrap().bytes(), b"");
}

#[test]
fn restart_reuses_verified_bytes_and_applies_smaller_limits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state");
    {
        let mut store = Store::open(&path, options(10, 3)).unwrap();
        drop(insert(&mut store, "older", b"abc"));
        drop(insert(&mut store, "newer", b"def"));
    }
    let mut store = Store::open(&path, options(3, 1)).unwrap();
    assert!(store.get(&key("older", 3)).unwrap().is_none());
    assert_eq!(
        store.get(&key("newer", 3)).unwrap().unwrap().bytes(),
        b"def"
    );
    assert!(insert(&mut store, "too-big", b"abcd").is_none());
    assert!(
        store
            .put(&key("wrong-size", 3), Arc::from(b"ab".as_slice()))
            .is_err()
    );
}

#[test]
fn corruption_missing_rows_and_rebinding_never_return_wrong_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("state"), options(30, 10)).unwrap();
    for mutation in [
        "UPDATE clean_blobs SET data=x'78797a'", // same length, wrong checksum
        "UPDATE clean_blobs SET checksum=zeroblob(32)",
        "PRAGMA ignore_check_constraints=ON; UPDATE clean_blobs SET size_bytes=1000000000; PRAGMA ignore_check_constraints=OFF;",
        "PRAGMA ignore_check_constraints=ON; UPDATE clean_blobs SET data=x'00'; PRAGMA ignore_check_constraints=OFF;",
        "DELETE FROM clean_blobs",
    ] {
        drop(insert(&mut store, "file", b"abc"));
        store.state.connection.execute_batch(mutation).unwrap();
        assert!(store.get(&key("file", 3)).unwrap().is_none(), "{mutation}");
        assert_eq!(insert(&mut store, "file", b"abc").unwrap().bytes(), b"abc");
    }
    let wrong = Key::new(
        [1; 32],
        &"b".repeat(64),
        "file",
        Some("\"revision\""),
        None,
        3,
    )
    .unwrap();
    assert!(store.get(&wrong).is_err());
    assert_eq!(store.get(&key("file", 3)).unwrap().unwrap().bytes(), b"abc");
}

#[test]
fn sqlite_full_rolls_back_eviction_and_payload_together() {
    let dir = tempfile::tempdir().unwrap();
    let mut store =
        Store::open(&dir.path().join("state"), options(MAX_FILE_BYTES as u64, 1)).unwrap();
    drop(insert(&mut store, "keep", b"original"));
    let pages: i64 = store
        .state
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    store
        .state
        .connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let error = store
        .put(
            &key("too-large-for-disk", MAX_FILE_BYTES as u64),
            Arc::from(vec![7; MAX_FILE_BYTES]),
        )
        .err()
        .expect("expected SQLITE_FULL");
    let sql = error.downcast_ref::<rusqlite::Error>().expect("SQL error");
    assert!(is_disk_full(error.as_ref()));
    assert_eq!(sql.sqlite_error_code(), Some(rusqlite::ErrorCode::DiskFull));
    assert_eq!(
        store.get(&key("keep", 8)).unwrap().unwrap().bytes(),
        b"original"
    );
    assert!(
        store
            .get(&key("too-large-for-disk", MAX_FILE_BYTES as u64))
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn worker_is_exclusive_async_and_releases_ownership_on_close() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state");
    let cache = Cache::open(&path, options(20, 3)).await.unwrap();
    assert!(Cache::open(&path, options(20, 3)).await.is_err());
    // Hold a real SQLite writer lock. The cache worker must wait, but this
    // single-threaded Tokio runtime must remain able to run the release timer.
    let other = rusqlite::Connection::open(path.join("metadata.sqlite3")).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let clone = cache.clone();
    let mut pending = tokio::spawn(async move { clone.get(key("missing", 2)).await });
    tokio::select! {
        result = &mut pending => panic!("cache completed while writer lock was held: {}", result.is_ok()),
        _ = tokio::time::sleep(Duration::from_millis(100)) => {},
    }
    other.execute_batch("ROLLBACK").unwrap();
    assert!(pending.await.unwrap().unwrap().is_none());
    drop(other);
    let pinned = cache
        .put(key("file", 3), Arc::from(b"abc".as_slice()))
        .await
        .unwrap()
        .unwrap();
    cache.close().await.unwrap();
    assert!(cache.get(key("file", 3)).await.is_err());
    // A snapshot may outlive a closed worker; it remains immutable and valid.
    assert_eq!(pinned.bytes(), b"abc");
    let reopened = Cache::open(&path, options(20, 3)).await.unwrap();
    assert_eq!(
        reopened.get(key("file", 3)).await.unwrap().unwrap().bytes(),
        b"abc"
    );
    reopened.close().await.unwrap();
}

#[test]
fn migration_two_preserves_bindings_and_rolls_back_conflicting_cache_schema() {
    for conflict in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        let mut state = State::open(&path).unwrap();
        state.register_source(&[1; 32], &"a".repeat(64)).unwrap();
        state
            .connection
            .execute_batch("DROP TABLE namespace_removals; DROP TABLE namespace_intents; DROP INDEX namespace_portable_names; ALTER TABLE namespace_nodes DROP COLUMN name_key; ALTER TABLE namespace_nodes DROP COLUMN placement; DROP TABLE logical_publication_links; DROP TABLE logical_edits; DROP TABLE logical_files; DROP TABLE managed_publications; DROP TABLE namespace_uploads; DROP TABLE namespace_stores; DROP TABLE namespace_operations; DROP TABLE namespace_nodes; DROP TABLE namespace_volume; DROP TABLE write_settlements; DROP TABLE remote_flushes; DROP TABLE retained_remote_intents; DROP TABLE remote_intents; DROP TABLE staged_operations; DROP TABLE write_journal; DROP TABLE staged_heads; DROP TABLE staged_extents; DROP TABLE retained_staged_chunks; DROP TABLE staged_chunks; DROP TABLE staged_generations; DROP TABLE clean_blobs; PRAGMA user_version=2;")
            .unwrap();
        if conflict {
            state.connection.execute_batch("CREATE TABLE clean_blobs (personal TEXT); INSERT INTO clean_blobs VALUES ('keep');").unwrap();
        }
        drop(state);
        let result = State::open(&path);
        assert_eq!(result.is_err(), conflict);
        drop(result);
        let db = rusqlite::Connection::open(&path).unwrap();
        let version: i32 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, if conflict { 2 } else { crate::SCHEMA_VERSION });
        let count: i64 = db
            .query_row("SELECT count(*) FROM source_bindings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        if conflict {
            assert_eq!(
                db.query_row("SELECT personal FROM clean_blobs", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "keep"
            );
        }
    }
}

#[test]
fn child_process_holds_cache_transaction() {
    let Some(path) = std::env::var_os("S3DRIVE_TEST_CACHE_DIRECTORY") else {
        return;
    };
    let ready = std::env::var_os("S3DRIVE_TEST_CACHE_READY").unwrap();
    let mut store = Store::open(Path::new(&path), options(30, 10)).unwrap();
    drop(insert(&mut store, "committed", b"acknowledged"));
    if std::env::var_os("S3DRIVE_TEST_CACHE_UNCOMMITTED").is_some() {
        store
            .state
            .connection
            .execute_batch("BEGIN IMMEDIATE; DELETE FROM clean_blobs;")
            .unwrap();
    }
    fs::write(ready, b"ready").unwrap();
    let _ = std::io::stdin().read(&mut [0u8]);
}

#[test]
fn process_death_recovers_committed_bytes_and_rolls_back_uncommitted_eviction() {
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for uncommitted in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state");
        let ready = dir.path().join("ready");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "cache::tests::child_process_holds_cache_transaction",
                "--nocapture",
            ])
            .env("S3DRIVE_TEST_CACHE_DIRECTORY", &path)
            .env("S3DRIVE_TEST_CACHE_READY", &ready)
            .env_remove("S3DRIVE_TEST_CACHE_UNCOMMITTED")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        if uncommitted {
            command.env("S3DRIVE_TEST_CACHE_UNCOMMITTED", "1");
        }
        let mut child = ChildGuard(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(15);
        while !ready.exists() {
            assert!(
                Instant::now() < deadline,
                "cache child did not become ready"
            );
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "cache child exited early"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let mut store = Store::open(&path, options(30, 10)).unwrap();
        assert_eq!(
            store.get(&key("committed", 12)).unwrap().unwrap().bytes(),
            b"acknowledged"
        );
    }
}
