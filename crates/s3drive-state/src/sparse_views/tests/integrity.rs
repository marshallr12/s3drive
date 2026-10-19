use super::*;
fn saved_change(s: &State, record: &Record) {
    s.connection
        .execute(
            "UPDATE sparse_views SET record=?1,state_hash=?2 WHERE view_id=?3",
            params![
                encode(record).unwrap(),
                hash(b"s3drive-sparse-view-v1\0", record)
                    .unwrap()
                    .as_slice(),
                record.id.as_slice()
            ],
        )
        .unwrap();
    s.connection
        .execute(
            "UPDATE sparse_view_operations SET request=?1,state_hash=?2 WHERE operation=?3",
            params![
                encode(&record.request).unwrap(),
                hash(
                    b"s3drive-sparse-request-v1\0",
                    &(record.operation, &record.request)
                )
                .unwrap()
                .as_slice(),
                record.operation.as_slice()
            ],
        )
        .unwrap();
}
#[test]
fn corrupt_patches_self_consistent_forks_cycles_and_unread_bytes_are_refused_without_repair() {
    for case in 0..11 {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut s = State::open_managed(&path).unwrap();
        let mut j = Journal::new(&mut s, Options::default()).unwrap();
        let root = j.begin([7; 32], binding(2, 0), 0).unwrap();
        let child = j.write([8; 32], root.id(), 0, b"abcdef").unwrap();
        s.connection
            .execute_batch("PRAGMA foreign_keys=OFF; PRAGMA ignore_check_constraints=ON")
            .unwrap();
        match case {
            0 => {
                s.connection
                    .execute(
                        "UPDATE sparse_view_blocks SET data=?1",
                        [b"abcdeg".as_slice()],
                    )
                    .unwrap();
            }
            1 => {
                s.connection
                    .execute_batch("DELETE FROM sparse_view_blocks")
                    .unwrap();
            }
            2 => {
                s.connection
                    .execute_batch("UPDATE sparse_view_blocks SET data=zeroblob(1048577)")
                    .unwrap();
            }
            3 => {
                s.connection
                    .execute(
                        "UPDATE sparse_view_files SET head=?1",
                        [root.id().as_slice()],
                    )
                    .unwrap();
            }
            4 => {
                s.connection
                    .execute(
                        "INSERT INTO sparse_view_blocks VALUES(?1,?2)",
                        params![digest(b"orphan").as_slice(), b"orphan".as_slice()],
                    )
                    .unwrap();
            }
            5 => {
                let mut r = child.0.clone();
                r.spans = vec![model::Span::Zero { length: 6 }];
                saved_change(&s, &r);
            }
            6 => {
                let mut r = child.0.clone();
                r.parent = Some(r.id);
                if let Some(Change::Write { parent, .. }) = &mut r.request.edit {
                    *parent = r.id;
                }
                saved_change(&s, &r);
            }
            7 => {
                let r = Record::child([9; 32], child.0.request.clone(), &root.0).unwrap();
                let saved = Saved::new(r.clone()).unwrap();
                s.connection
                    .execute(
                        "INSERT INTO sparse_views VALUES(?1,?2,?3,?4)",
                        params![
                            r.id.as_slice(),
                            saved.target.as_slice(),
                            &saved.raw,
                            hash(b"s3drive-sparse-view-v1\0", &r).unwrap().as_slice()
                        ],
                    )
                    .unwrap();
                s.connection
                    .execute(
                        "INSERT INTO sparse_view_operations VALUES(?1,?2,?3,?4)",
                        params![
                            r.operation.as_slice(),
                            r.id.as_slice(),
                            saved.request,
                            hash(b"s3drive-sparse-request-v1\0", &(r.operation, &r.request))
                                .unwrap()
                                .as_slice()
                        ],
                    )
                    .unwrap();
            }
            8 => {
                s.connection
                    .execute_batch("UPDATE sparse_view_blocks SET block=zeroblob(33)")
                    .unwrap();
            }
            9 => {
                s.connection.execute_batch("ALTER TABLE sparse_view_blocks RENAME TO owned_old_blocks; CREATE TABLE sparse_view_blocks(block,data); INSERT INTO sparse_view_blocks SELECT block,CAST(data AS TEXT) FROM owned_old_blocks; DROP TABLE owned_old_blocks").unwrap();
            }
            10 => {
                let mut r = child.0.clone();
                if let Some(Change::Write { blocks, .. }) = &mut r.request.edit {
                    blocks[0].length = 5;
                }
                saved_change(&s, &r);
            }
            _ => unreachable!(),
        }
        let before = rows(&s.connection);
        assert!(
            Journal::new(&mut s, Options::default()).is_err(),
            "case{case}"
        );
        assert_eq!(rows(&s.connection), before, "repair case{case}");
        drop(s);
        assert!(State::open_managed(&path).is_err(), "reopen case{case}");
        let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
        assert_eq!(rows(&db), before, "reopen repair case{case}");
    }
}
#[test]
fn every_patch_transaction_effect_is_checked_and_sql_trigger_failures_roll_back_exact_old_bytes() {
    for case in 0..8 {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let root = Journal::new(&mut s, Options::default())
            .unwrap()
            .begin([7; 32], binding(2, 0), 0)
            .unwrap();
        let old = Journal::new(&mut s, Options::default())
            .unwrap()
            .write([8; 32], root.id(), 0, b"old")
            .unwrap();
        // Literal old BLOB comparison must catch even a semantically equivalent
        // trigger rewrite that leaves all canonical hashes valid.
        s.connection
            .execute_batch("UPDATE sparse_views SET record=CAST(x'20'||record||x'0a' AS BLOB)")
            .unwrap();
        let sql=match case {
            0=>"CREATE TRIGGER owned_fault BEFORE INSERT ON sparse_view_blocks BEGIN SELECT RAISE(IGNORE); END".into(),
            1=>"CREATE TRIGGER owned_fault BEFORE INSERT ON sparse_views BEGIN SELECT RAISE(IGNORE); END".into(),
            2=>"CREATE TRIGGER owned_fault BEFORE INSERT ON sparse_view_operations BEGIN SELECT RAISE(IGNORE); END".into(),
            3=>"CREATE TRIGGER owned_fault BEFORE UPDATE ON sparse_view_files BEGIN SELECT RAISE(IGNORE); END".into(),
            4=>"CREATE TRIGGER owned_fault AFTER INSERT ON sparse_view_blocks BEGIN UPDATE sparse_view_blocks SET data=x'00'; END".into(),
            5=>"CREATE TRIGGER owned_fault AFTER INSERT ON sparse_view_operations BEGIN UPDATE sparse_views SET record=CAST(trim(CAST(record AS TEXT),char(10)||' ') AS BLOB); END".into(),
            6=>"CREATE TRIGGER owned_fault AFTER UPDATE ON sparse_view_files BEGIN DELETE FROM sparse_view_role; END".into(),
            7=>format!("CREATE TRIGGER owned_fault AFTER INSERT ON sparse_view_operations BEGIN INSERT INTO sparse_view_blocks VALUES(x'{}',x'6f727068616e'); END",digest(b"orphan").iter().map(|b|format!("{b:02x}")).collect::<String>()),
            _=>unreachable!(),
        };
        let before = rows(&s.connection);
        s.connection.execute_batch(&sql).unwrap();
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .write([9; 32], old.id(), 0, b"new")
                .is_err(),
            "case{case}"
        );
        assert_eq!(rows(&s.connection), before, "case{case}");
        s.connection
            .execute_batch("DROP TRIGGER owned_fault")
            .unwrap();
        let mut j = Journal::new(&mut s, Options::default()).unwrap();
        let new = j.write([9; 32], old.id(), 0, b"new").unwrap();
        assert_eq!(
            j.read(new.id(), 0, 3).unwrap(),
            vec![Piece::Local(b"new".to_vec())]
        );
        assert_eq!(
            j.read(old.id(), 0, 3).unwrap(),
            vec![Piece::Local(b"old".to_vec())]
        );
    }
}
#[test]
fn actual_sqlite_full_after_head_update_keeps_old_head_receipts_and_all_patch_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let root = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin([7; 32], binding(2, 0), 0)
        .unwrap();
    let old = Journal::new(&mut s, Options::default())
        .unwrap()
        .write([8; 32], root.id(), 0, b"old")
        .unwrap();
    s.connection
        .execute_batch("CREATE TABLE owned_patch_pressure(data BLOB)")
        .unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    let size: i64 = s
        .connection
        .pragma_query_value(None, "page_size", |r| r.get(0))
        .unwrap();
    s.connection.execute_batch(&format!("CREATE TRIGGER owned_patch_full AFTER UPDATE ON sparse_view_files BEGIN INSERT INTO owned_patch_pressure VALUES(zeroblob({})); END",(pages+64)*size)).unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let before = rows(&s.connection);
    let error = Journal::new(&mut s, Options::default())
        .unwrap()
        .write([9; 32], old.id(), 0, b"new")
        .unwrap_err();
    assert!(
        crate::cache::is_disk_full(error.as_ref()),
        "expected real SQLITE_FULL: {error}"
    );
    assert_eq!(rows(&s.connection), before);
    let count: u32 = s
        .connection
        .query_row("SELECT count(*) FROM owned_patch_pressure", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
    let mut j = Journal::new(&mut s, Options::default()).unwrap();
    assert_eq!(j.head(root.binding()).unwrap(), old);
    assert_eq!(
        j.read(old.id(), 0, 3).unwrap(),
        vec![Piece::Local(b"old".to_vec())]
    );
    s.connection
        .pragma_update(None, "max_page_count", pages + 1024)
        .unwrap();
    s.connection
        .execute_batch("DROP TRIGGER owned_patch_full")
        .unwrap();
    let mut j = Journal::new(&mut s, Options::default()).unwrap();
    let new = j.write([9; 32], old.id(), 0, b"new").unwrap();
    assert_eq!(j.head(root.binding()).unwrap(), new);
    assert_eq!(j.write([8; 32], root.id(), 0, b"old").unwrap(), old);
}
#[test]
fn patch_table_alone_cannot_be_adopted_by_legacy_or_authority_roles() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    s.connection
        .execute(
            "INSERT INTO sparse_view_blocks VALUES(?1,?2)",
            params![digest(b"orphan").as_slice(), b"orphan".as_slice()],
        )
        .unwrap();
    let before = rows(&s.connection);
    assert!(crate::chunk_publication::Journal::new(&mut s).is_err());
    assert!(crate::logical::Journal::new(&mut s, crate::journal::Options::default()).is_err());
    assert!(s.namespace_initialize().is_err());
    assert!(Journal::new(&mut s, Options::default()).is_err());
    assert_eq!(rows(&s.connection), before);
    let path = tmp.path().join("authority");
    let mut a = State::open_managed(&path).unwrap();
    let volume = a.namespace_initialize().unwrap();
    drop(a);
    assert!(State::inspect_existing_authority(&path, &volume.id, &volume.name_policy).is_ok());
    let a = State::open_managed(&path).unwrap();
    a.connection
        .execute(
            "INSERT INTO sparse_view_blocks VALUES(?1,?2)",
            params![digest(b"orphan").as_slice(), b"orphan".as_slice()],
        )
        .unwrap();
    drop(a);
    assert!(State::inspect_existing_authority(&path, &volume.id, &volume.name_policy).is_err());
}
