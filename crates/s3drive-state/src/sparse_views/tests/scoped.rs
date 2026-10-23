use super::*;

#[test]
fn scoped_mode_is_explicit_permanent_and_preserves_exact_legacy_replays() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("scoped café 🦀");
    let mut state = State::open_managed(&path).unwrap();
    let b = binding(2, 17);
    let token = ScopedOperation::new(&b, [42; 32]).unwrap();
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    failure(
        j.begin_scoped(token, b.clone(), 17),
        Failure::ScopedDisabled,
    );
    let old = j.begin([7; 32], b.clone(), 17).unwrap();
    let before = rows(&state.connection);
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    j.enable_scoped().unwrap();
    j.enable_scoped().unwrap();
    assert_eq!(j.begin([7; 32], b.clone(), 17).unwrap(), old);
    failure(
        j.write([8; 32], old.id(), 0, b"lost"),
        Failure::ScopedRequired,
    );
    failure(j.resize([9; 32], old.id(), 0), Failure::ScopedRequired);
    assert_eq!(rows(&state.connection), before);
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    let written = j
        .write_scoped(token, old.id(), 3, "café 🦀".as_bytes())
        .unwrap();
    let resize = ScopedOperation::new(&b, [43; 32]).unwrap();
    let resized = j.resize_scoped(resize, written.id(), 8).unwrap();
    assert_eq!(
        j.write_scoped(token, old.id(), 3, "café 🦀".as_bytes())
            .unwrap(),
        written
    );
    failure(
        j.write_scoped(token, old.id(), 3, b"different"),
        Failure::Conflict,
    );
    assert_eq!(j.head(&b).unwrap(), resized);
    drop(state);
    let mut state = State::open_managed(&path).unwrap();
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    assert_eq!(j.resize_scoped(resize, written.id(), 8).unwrap(), resized);
    failure(j.resize([10; 32], resized.id(), 0), Failure::ScopedRequired);
}

#[test]
fn scoped_initial_and_wrong_file_tokens_are_atomic_and_domain_separated() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let a = binding(2, 0);
    let b = binding(3, 0);
    let ta = ScopedOperation::new(&a, [42; 32]).unwrap();
    let tb = ScopedOperation::new(&b, [42; 32]).unwrap();
    assert_ne!(ta.id().unwrap(), tb.id().unwrap());
    assert!(ScopedOperation::new(&a, [0; 32]).is_err());
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    j.enable_scoped().unwrap();
    failure(j.begin([7; 32], a.clone(), 0), Failure::ScopedRequired);
    failure(j.begin_scoped(tb, a.clone(), 0), Failure::EpochClosed);
    let v = j.begin_scoped(ta, a.clone(), 0).unwrap();
    assert_eq!(j.begin_scoped(ta, a.clone(), 0).unwrap(), v);
    let before = rows(&state.connection);
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    failure(
        j.write_scoped(tb, v.id(), 0, b"wrong"),
        Failure::EpochClosed,
    );
    failure(j.resize_scoped(tb, v.id(), 1), Failure::EpochClosed);
    assert_eq!(rows(&state.connection), before);
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    let other = j.begin_scoped(tb, b, 0).unwrap();
    assert_ne!(other.id(), v.id());
}

#[test]
fn scoped_marker_missing_or_corrupt_fails_closed_before_first_file() {
    for sql in [
        "DROP TABLE sparse_scoped_mode",
        "INSERT INTO sparse_scoped_mode VALUES(1,1,zeroblob(32))",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state");
        let state = State::open_managed(&path).unwrap();
        state.connection.execute_batch(sql).unwrap();
        drop(state);
        assert!(State::open_managed(&path).is_err(), "accepted {sql}");
    }
}

#[test]
fn scoped_mode_refuses_trigger_that_could_silently_disable_admission_rules() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    Journal::new(&mut state, Options::default())
        .unwrap()
        .enable_scoped()
        .unwrap();
    state.connection.execute_batch("CREATE TRIGGER disable_mode AFTER INSERT ON sparse_views BEGIN DELETE FROM sparse_scoped_mode; END;").unwrap();
    assert!(Journal::new(&mut state, Options::default()).is_err());
    let count: i64 = state
        .connection
        .query_row("SELECT count(*) FROM sparse_scoped_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert!(rows(&state.connection).iter().all(Vec::is_empty));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scoped_same_owner_race_replays_once_and_remains_enabled_on_reopen() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("scoped worker");
        let storage = crate::worker::Storage::open(
            &path,
            crate::cache::Options::default(),
            crate::journal::Options::default(),
        )
        .await
        .unwrap();
        let a = storage.sparse_views(Options::default()).unwrap();
        let b = a.clone();
        a.enable_scoped().await.unwrap();
        let binding = binding(2, 0);
        let token = ScopedOperation::new(&binding, [41; 32]).unwrap();
        let v = a.begin_scoped(token, binding.clone(), 0).await.unwrap();
        let token = ScopedOperation::new(&binding, [42; 32]).unwrap();
        let (left, right) = tokio::join!(
            a.write_scoped(token, v.id(), 0, std::sync::Arc::from(b"same".as_slice())),
            b.write_scoped(token, v.id(), 0, std::sync::Arc::from(b"same".as_slice()))
        );
        let next = left.unwrap();
        assert_eq!(right.unwrap(), next);
        storage.close().await.unwrap();
        let mut state = State::open_managed(&path).unwrap();
        let mut j = Journal::new(&mut state, Options::default()).unwrap();
        assert_eq!(j.write_scoped(token, v.id(), 0, b"same").unwrap(), next);
        failure(
            j.write([8; 32], next.id(), 0, b"raw"),
            Failure::ScopedRequired,
        );
    })
    .await
    .expect("scoped worker timed out");
}

#[test]
fn schema35_migration_preserves_literal_source_rows_and_mode_collisions_rollback() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("migration");
    let mut state = State::open_managed(&path).unwrap();
    let b = binding(2, 17);
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    let v = j.begin([7; 32], b, 17).unwrap();
    j.write([8; 32], v.id(), 2, b"original").unwrap();
    let before = rows(&state.connection);
    state
        .connection
        .execute_batch("DROP TABLE sparse_retry_mode; DROP TABLE chunk_mount_role; DROP TABLE manifest_creations; DROP TABLE manifest_creation_role; DROP TABLE source_read_pins; DROP TABLE source_read_pin_session; DROP TABLE sparse_scoped_mode; PRAGMA user_version=35")
        .unwrap();
    drop(state);
    let mut state = State::open_managed(&path).unwrap();
    assert_eq!(state.info().unwrap().schema_version, crate::SCHEMA_VERSION);
    assert_eq!(rows(&state.connection), before);
    Journal::new(&mut state, Options::default())
        .unwrap()
        .enable_scoped()
        .unwrap();
    crate::test_schema::without_creation(&state.connection);
    crate::test_schema::demote(&state.connection, 35).unwrap();
    drop(state);
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        35
    );
    assert_eq!(rows(&db), before);
}

#[test]
fn ignored_mode_enable_rolls_back_and_never_changes_legacy_rows() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    state.connection.execute_batch("CREATE TRIGGER ignore_mode BEFORE INSERT ON sparse_scoped_mode BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    assert!(
        Journal::new(&mut state, Options::default())
            .unwrap()
            .enable_scoped()
            .is_err()
    );
    let count: i64 = state
        .connection
        .query_row("SELECT count(*) FROM sparse_scoped_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    assert!(rows(&state.connection).iter().all(Vec::is_empty));
    state
        .connection
        .execute_batch("DROP TRIGGER ignore_mode")
        .unwrap();
    Journal::new(&mut state, Options::default())
        .unwrap()
        .enable_scoped()
        .unwrap();
}

mod mode_crash;

#[test]
fn empty_scoped_owner_already_excludes_legacy_publication_and_authority_roles() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source");
    let mut state = State::open_managed(&path).unwrap();
    Journal::new(&mut state, Options::default())
        .unwrap()
        .enable_scoped()
        .unwrap();
    assert!(crate::chunk_publication::Journal::new(&mut state).is_err());
    assert!(crate::logical::Journal::new(&mut state, crate::journal::Options::default()).is_err());
    assert!(state.namespace_initialize().is_err());
    assert!(rows(&state.connection).iter().all(Vec::is_empty));
    drop(state);
    assert!(
        State::inspect_existing_authority(&path, &"01".repeat(16), s3drive_core::names::PORTABLE)
            .is_err()
    );
    let mut authority = State::open_managed(&temp.path().join("authority")).unwrap();
    authority.namespace_initialize().unwrap();
    assert!(Journal::new(&mut authority, Options::default()).is_err());
}

#[test]
fn actual_sqlite_full_preserves_scoped_head_marker_and_exact_retry_token() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let b = binding(2, 0);
    let initial = ScopedOperation::new(&b, [41; 32]).unwrap();
    let write = ScopedOperation::new(&b, [42; 32]).unwrap();
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    j.enable_scoped().unwrap();
    let v = j.begin_scoped(initial, b, 0).unwrap();
    let before = rows(&state.connection);
    let pages: i64 = state
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    let original_ceiling: i64 = state
        .connection
        .pragma_query_value(None, "max_page_count", |r| r.get(0))
        .unwrap();
    state
        .connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let data: Vec<u8> = (0..MAX_RANGE_BYTES)
        .map(|i| (i / PATCH_BYTES + 7) as u8)
        .collect();
    let err = Journal::new(&mut state, Options::default())
        .unwrap()
        .write_scoped(write, v.id(), 0, &data)
        .unwrap_err();
    assert!(
        crate::cache::is_disk_full(err.as_ref()),
        "expected SQLITE_FULL: {err}"
    );
    assert_eq!(rows(&state.connection), before);
    assert!(crate::sparse_views::scoped::enabled(&state.connection).unwrap());
    state
        .connection
        .pragma_update(None, "max_page_count", original_ceiling)
        .unwrap();
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    assert_eq!(j.head(v.binding()).unwrap(), v);
    let next = j.write_scoped(write, v.id(), 0, &data).unwrap();
    assert_eq!(j.write_scoped(write, v.id(), 0, &data).unwrap(), next);
    assert_eq!(j.head(v.binding()).unwrap(), next);
}
