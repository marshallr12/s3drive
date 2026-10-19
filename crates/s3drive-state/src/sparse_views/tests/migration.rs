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
            assert_eq!(s.info().unwrap().schema_version, 29);
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
