use super::*;
#[test]
fn corrupt_missing_oversized_and_substituted_rows_never_replay_or_repair() {
    for case in 0..24 {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut s = State::open_managed(&path).unwrap();
        let v = Journal::new(&mut s, Options::default())
            .unwrap()
            .begin([7; 32], binding(2, 17), 17)
            .unwrap();
        s.connection
            .execute_batch("PRAGMA foreign_keys=OFF; PRAGMA ignore_check_constraints=ON")
            .unwrap();
        let sql = match case {
            0 => "UPDATE sparse_view_role SET state_hash=zeroblob(32)",
            1 => "UPDATE sparse_view_files SET state_hash=zeroblob(32)",
            2 => "UPDATE sparse_view_files SET head=zeroblob(32)",
            3 => "UPDATE sparse_views SET target=zeroblob(32)",
            4 => "UPDATE sparse_views SET state_hash=zeroblob(32)",
            5 => "UPDATE sparse_view_operations SET state_hash=zeroblob(32)",
            6 => "UPDATE sparse_view_operations SET operation=zeroblob(32)",
            7 => "UPDATE sparse_view_operations SET request=x'7b7d'",
            8 => "UPDATE sparse_views SET record=x'7b7d'",
            9 => "UPDATE sparse_view_files SET binding=x'7b7d'",
            10 => "DELETE FROM sparse_view_role",
            11 => "DELETE FROM sparse_view_files",
            12 => "DELETE FROM sparse_views",
            13 => "DELETE FROM sparse_view_operations",
            14 => {
                "UPDATE sparse_view_files SET binding=CAST(CAST(binding AS TEXT)||printf('%*s',4097-length(binding),'') AS BLOB)"
            }
            15 => {
                "UPDATE sparse_views SET record=CAST(CAST(record AS TEXT)||printf('%*s',1048577-length(record),'') AS BLOB)"
            }
            16 => {
                "UPDATE sparse_view_operations SET request=CAST(CAST(request AS TEXT)||printf('%*s',4097-length(request),'') AS BLOB)"
            }
            21 => "UPDATE sparse_view_files SET target=zeroblob(33)",
            22 => {
                "DELETE FROM sparse_view_operations; DELETE FROM sparse_views; DELETE FROM sparse_view_files"
            }
            23 => {
                "ALTER TABLE sparse_view_role RENAME TO owned_old_role; CREATE TABLE sparse_view_role(singleton,format,state_hash); INSERT INTO sparse_view_role SELECT singleton,NULL,state_hash FROM owned_old_role; DROP TABLE owned_old_role"
            }
            _ => "",
        };
        if !sql.is_empty() {
            s.connection.execute_batch(sql).unwrap();
        }
        if case == 17 || case == 18 {
            let mut r = v.0.clone();
            if case == 17 {
                r.parent = Some([8; 32]);
            } else {
                r.spans = vec![model::Span::Base {
                    offset: 1,
                    length: 17,
                }];
            }
            s.connection
                .execute(
                    "UPDATE sparse_views SET record=?1,state_hash=?2",
                    params![
                        encode(&r).unwrap(),
                        hash(b"s3drive-sparse-view-v1\0", &r).unwrap().as_slice()
                    ],
                )
                .unwrap();
        }
        if case == 19 {
            let mut r = v.0.request.clone();
            r.binding.root = "other/".into();
            s.connection
                .execute(
                    "UPDATE sparse_view_operations SET request=?1,state_hash=?2",
                    params![
                        encode(&r).unwrap(),
                        hash(b"s3drive-sparse-request-v1\0", &(v.operation(), &r))
                            .unwrap()
                            .as_slice()
                    ],
                )
                .unwrap();
        }
        if case == 20 {
            let mut b = v.binding().clone();
            b.fingerprint = "b".repeat(64);
            s.connection
                .execute(
                    "UPDATE sparse_view_files SET binding=?1,state_hash=?2",
                    params![
                        encode(&b).unwrap(),
                        hash(b"s3drive-sparse-binding-v1\0", &b).unwrap().as_slice()
                    ],
                )
                .unwrap();
        }
        let before = rows(&s.connection);
        assert!(
            Journal::new(&mut s, Options::default()).is_err(),
            "case{case}"
        );
        assert_eq!(rows(&s.connection), before, "case{case}");
        drop(s);
        assert!(State::open_managed(&path).is_err(), "reopen case{case}");
        let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
        assert_eq!(rows(&db), before, "reopen repaired case{case}");
    }
}
#[test]
fn ignored_or_foreign_trigger_effects_roll_back_all_initial_admission_rows() {
    for (table, effect) in [
        ("sparse_view_role", "SELECT RAISE(IGNORE)"),
        ("sparse_view_files", "SELECT RAISE(IGNORE)"),
        ("sparse_views", "SELECT RAISE(IGNORE)"),
        ("sparse_view_operations", "SELECT RAISE(IGNORE)"),
        (
            "sparse_view_files",
            "UPDATE sparse_view_files SET state_hash=zeroblob(32)",
        ),
        ("sparse_views", "DELETE FROM sparse_view_role"),
        ("sparse_view_operations", "DELETE FROM sparse_views"),
        (
            "sparse_view_operations",
            "UPDATE sparse_view_operations SET request=x'7b7d'",
        ),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let before = rows(&s.connection);
        let timing = if effect.contains("RAISE") {
            "BEFORE"
        } else {
            "AFTER"
        };
        s.connection
            .execute_batch(&format!(
                "CREATE TRIGGER owned_fault {timing} INSERT ON {table} BEGIN {effect}; END"
            ))
            .unwrap();
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .begin([7; 32], binding(2, 17), 17)
                .is_err(),
            "{table}: {effect}"
        );
        assert_eq!(rows(&s.connection), before);
        s.connection
            .execute_batch("DROP TRIGGER owned_fault")
            .unwrap();
        Journal::new(&mut s, Options::default())
            .unwrap()
            .begin([7; 32], binding(2, 17), 17)
            .unwrap();
    }
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    Journal::new(&mut s, Options::default())
        .unwrap()
        .begin([7; 32], binding(2, 17), 17)
        .unwrap();
    let before = rows(&s.connection);
    s.connection.execute_batch("CREATE TRIGGER owned_fault AFTER INSERT ON sparse_view_operations BEGIN UPDATE sparse_views SET state_hash=zeroblob(32) WHERE operation!=NEW.operation; END").unwrap();
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .begin([8; 32], binding(3, 0), 0)
            .is_err()
    );
    assert_eq!(rows(&s.connection), before);
}
#[test]
fn actual_sqlite_full_never_acknowledges_or_leaves_partial_view_admission() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    s.connection
        .execute_batch("CREATE TABLE owned_pressure(data BLOB)")
        .unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    let size: i64 = s
        .connection
        .pragma_query_value(None, "page_size", |r| r.get(0))
        .unwrap();
    s.connection.execute_batch(&format!("CREATE TRIGGER owned_full BEFORE INSERT ON sparse_view_role BEGIN INSERT INTO owned_pressure VALUES(zeroblob({})); END",(pages+64)*size)).unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let before = rows(&s.connection);
    let err = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin([7; 32], binding(2, 17), 17)
        .unwrap_err();
    assert_eq!(
        err.downcast_ref::<rusqlite::Error>()
            .and_then(|e| e.sqlite_error_code()),
        Some(rusqlite::ErrorCode::DiskFull)
    );
    assert_eq!(rows(&s.connection), before);
    let count: i64 = s
        .connection
        .query_row("SELECT count(*) FROM owned_pressure", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    s.connection
        .pragma_update(None, "max_page_count", pages + 1024)
        .unwrap();
    s.connection
        .execute_batch("DROP TRIGGER owned_full")
        .unwrap();
    Journal::new(&mut s, Options::default())
        .unwrap()
        .begin([7; 32], binding(2, 17), 17)
        .unwrap();
}
