use super::*;
use crate::journal;
#[test]
fn schema_twenty_nine_keeps_old_dirty_bytes_and_migration_collisions_roll_back() {
    for collision in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut s = State::open_managed(&path).unwrap();
        let target = journal::Target::new([9; 32], &"a".repeat(64), "old café 🦀").unwrap();
        let mut j = journal::Journal::new(&mut s, journal::Options::default()).unwrap();
        let generation = j
            .begin_operation(&[10; 32], &target, journal::Base::Missing, None, 3)
            .unwrap();
        j.append(generation, b"\0\xffx").unwrap();
        j.seal(generation).unwrap();
        s.connection
            .execute_batch(&crate::test_schema::legacy_sql("PRAGMA user_version=28"))
            .unwrap();
        if collision {
            s.connection.execute_batch("CREATE TABLE sparse_views(keep TEXT); INSERT INTO sparse_views VALUES('untouched')").unwrap();
        }
        drop(s);
        if collision {
            assert!(State::open_managed(&path).is_err());
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value::<i32, _>(None, "user_version", |r| r.get(0))
                    .unwrap(),
                28
            );
            let value: String = db
                .query_row("SELECT keep FROM sparse_views", [], |r| r.get(0))
                .unwrap();
            assert_eq!(value, "untouched");
            assert!(db.prepare("SELECT * FROM sparse_view_role").is_err());
            let value: Vec<u8> = db
                .query_row("SELECT data FROM staged_chunks", [], |r| r.get(0))
                .unwrap();
            assert_eq!(value, b"\0\xffx");
        } else {
            let mut s = State::open_managed(&path).unwrap();
            assert_eq!(s.info().unwrap().schema_version, 31);
            assert_eq!(
                journal::Journal::new(&mut s, journal::Options::default())
                    .unwrap()
                    .read(generation, 0, 8)
                    .unwrap(),
                b"\0\xffx"
            );
            Journal::new(&mut s, Options::default())
                .unwrap()
                .begin([7; 32], binding(2, 0), MAX_FILE_BYTES)
                .unwrap();
        }
    }
}
#[test]
fn sparse_role_is_reciprocally_isolated_while_generic_staging_remains_available() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    let v = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin([7; 32], binding(2, 17), 17)
        .unwrap();
    let before = rows(&s.connection);
    assert!(crate::chunk_publication::Journal::new(&mut s).is_err());
    assert!(crate::logical::Journal::new(&mut s, journal::Options::default()).is_err());
    assert!(s.namespace_initialize().is_err());
    assert_eq!(rows(&s.connection), before);
    let target = journal::Target::new([9; 32], &"b".repeat(64), "future-local-object").unwrap();
    let mut j = journal::Journal::new(&mut s, journal::Options::default()).unwrap();
    let g = j
        .begin_operation(&[10; 32], &target, journal::Base::Missing, None, 3)
        .unwrap();
    j.append(g, b"new").unwrap();
    j.seal(g).unwrap();
    assert_eq!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .view(v.id())
            .unwrap(),
        v
    );
    drop(s);
    assert!(
        State::inspect_existing_authority(&path, &"01".repeat(16), s3drive_core::names::PORTABLE)
            .is_err()
    );
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .view(v.id())
            .unwrap(),
        v
    );
    let mut authority = State::open_managed(&tmp.path().join("authority")).unwrap();
    let volume = authority.namespace_initialize().unwrap();
    assert!(Journal::new(&mut authority, Options::default()).is_err());
    assert_eq!(authority.namespace_volume().unwrap(), volume);
    // Even a retained/orphaned foreign role is not an empty reusable database.
    let mut foreign = State::open_managed(&tmp.path().join("foreign")).unwrap();
    foreign
        .connection
        .execute("INSERT INTO chunk_client_role VALUES(1,1,zeroblob(32))", [])
        .unwrap();
    assert!(Journal::new(&mut foreign, Options::default()).is_err());
    assert!(rows(&foreign.connection).iter().all(Vec::is_empty));
}

fn original_twenty_nine(path: &std::path::Path) -> (State, Vec<View>) {
    let mut s = State::open_managed(path).unwrap();
    // Construct the actual old DDL, including target UNIQUE and cyclic FKs.
    s.connection
        .execute_batch(&crate::test_schema::legacy_sql(include_str!(
            "../../../migrations/029.sql"
        )))
        .unwrap();
    s.connection
        .execute_batch(include_str!("../../../migrations/031.sql"))
        .unwrap();
    s.connection
        .pragma_update(None, "user_version", 29)
        .unwrap();
    let mut j = Journal::new(&mut s, Options::default()).unwrap();
    let a = j.begin([7; 32], binding(2, 17), MAX_FILE_BYTES).unwrap();
    let b = j.begin([8; 32], binding(3, 0), 0).unwrap();
    (s, vec![a, b])
}

fn assert_foreign_keys(db: &Connection) {
    let enabled: bool = db
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .unwrap();
    assert!(enabled);
    let bad: u32 = db
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(bad, 0);
}

#[test]
fn schema_thirty_preserves_populated_cyclic_views_byte_for_byte_and_removes_only_target_uniqueness()
{
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let (s, views) = original_twenty_nine(&path);
    let before = rows(&s.connection);
    assert_foreign_keys(&s.connection);
    let indexes = |db: &Connection| -> u32 {
        db.query_row("SELECT count(*) FROM pragma_index_list('sparse_views') AS i JOIN pragma_index_info(i.name) AS c WHERE i.\"unique\"=1 AND c.name='target'", [], |r| r.get(0)).unwrap()
    };
    assert_eq!(indexes(&s.connection), 1);
    s.connection
        .execute_batch("DROP TABLE sparse_view_blocks")
        .unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, 31);
    assert_eq!(
        rows(&s.connection),
        before,
        "migration changed a persisted BLOB"
    );
    assert_eq!(indexes(&s.connection), 0);
    assert_foreign_keys(&s.connection);
    let deferred: bool = s
        .connection
        .pragma_query_value(None, "defer_foreign_keys", |r| r.get(0))
        .unwrap();
    assert!(!deferred, "deferral leaked beyond migration transaction");
    for v in &views {
        let mut j = Journal::new(&mut s, Options::default()).unwrap();
        assert_eq!(j.view(v.id()).unwrap(), *v);
        assert_eq!(
            j.begin(v.operation(), v.binding().clone(), v.size())
                .unwrap(),
            *v
        );
    }
    // Exercise SQL constraints, without admitting an unsupported child record.
    {
        let tx = s.connection.transaction().unwrap();
        assert_eq!(tx.execute("INSERT INTO sparse_views SELECT ?1,target,record,state_hash FROM sparse_views WHERE view_id=?2", params![[99u8;32].as_slice(), views[0].id().as_slice()]).unwrap(),1);
        assert_foreign_keys(&tx);
        assert!(
            tx.execute(
                "INSERT INTO sparse_views SELECT * FROM sparse_views WHERE view_id=?1",
                [views[0].id().as_slice()]
            )
            .is_err(),
            "view identity lost uniqueness"
        );
        assert!(
            tx.execute(
                "INSERT INTO sparse_views VALUES(?1,?2,x'7b',zeroblob(32))",
                params![[98u8; 32].as_slice(), [97u8; 32].as_slice()]
            )
            .is_err(),
            "target foreign key lost"
        );
        tx.rollback().unwrap();
    }
    assert_eq!(rows(&s.connection), before);
    drop(s);
    let s = State::open_managed(&path).unwrap();
    assert_foreign_keys(&s.connection);
    assert_eq!(rows(&s.connection), before);
}

#[test]
fn schema_thirty_migration_sql_failure_rolls_back_copies_and_refuses_attached_triggers() {
    for trigger in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let (mut s, _) = original_twenty_nine(&path);
        let before = rows(&s.connection);
        if trigger {
            s.connection.execute_batch("CREATE TRIGGER migration_must_not_execute AFTER DELETE ON sparse_views BEGIN DELETE FROM sparse_view_operations; END;").unwrap();
            drop(s);
            assert!(State::open_managed(&path).is_err());
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(rows(&db), before);
            assert_eq!(
                db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))
                    .unwrap(),
                29
            );
            let n: u32 = db
                .query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name='migration_must_not_execute'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1);
        } else {
            s.connection.execute_batch("CREATE TEMP TABLE sparse_views_copy_v30(keep TEXT); INSERT INTO sparse_views_copy_v30 VALUES('untouched');").unwrap();
            {
                let tx = s
                    .connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .unwrap();
                assert!(
                    migrate_thirty(&tx).is_err(),
                    "mid-copy SQL collision was ignored"
                );
                // Drop rolls back the first copy and PRAGMA version remains29.
            }
            assert_eq!(rows(&s.connection), before);
            assert_foreign_keys(&s.connection);
            let n: u32 = s
                .connection
                .query_row(
                    "SELECT count(*) FROM sqlite_temp_schema WHERE name='sparse_files_copy_v30'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 0);
            let value: String = s
                .connection
                .query_row("SELECT keep FROM sparse_views_copy_v30", [], |r| r.get(0))
                .unwrap();
            assert_eq!(value, "untouched");
            assert_eq!(s.info().unwrap().schema_version, 29);
            let deferred: bool = s
                .connection
                .pragma_query_value(None, "defer_foreign_keys", |r| r.get(0))
                .unwrap();
            assert!(!deferred, "failed migration leaked FK deferral");
            s.connection
                .execute_batch("DROP TABLE temp.sparse_views_copy_v30")
                .unwrap();
            s.connection
                .execute_batch("DROP TABLE sparse_view_blocks")
                .unwrap();
            drop(s);
            let s = State::open_managed(&path).unwrap();
            assert_eq!(s.info().unwrap().schema_version, 31);
            assert_eq!(rows(&s.connection), before);
        }
    }
}
