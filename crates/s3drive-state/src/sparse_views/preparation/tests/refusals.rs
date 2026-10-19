use super::*;
#[test]
fn malformed_bindings_paths_coverage_tags_and_local_bytes_never_advance_or_repair() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("state")).unwrap();
    let b = base(129 * C, BTreeMap::from([(0, pattern(0, C))]));
    let initial = view(&mut s, &b, 129 * C);
    let v = super::super::super::Journal::new(&mut s, super::super::super::Options::default())
        .unwrap()
        .write([2; 32], initial.id(), 5, b"dirty")
        .unwrap();
    let before = ledger::inspect(&s.connection).unwrap();
    for kind in 0..5 {
        let mut r = request(&v, &b, 9);
        match kind {
            0 => r.generation = [7; 16],
            1 => r.base_page[10] ^= 1,
            2 => r.base_root[12] ^= 1,
            3 => r.operation = [0; 32],
            _ => r.base_root.push(0),
        }
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .begin(r)
                .is_err()
        );
        assert!(ledger::inspect(&s.connection).unwrap() == before);
    }
    let info = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin(request(&v, &b, 9))
        .unwrap();
    let plan = info.id().unwrap();
    let before = ledger::inspect(&s.connection).unwrap();
    let good = proof(&b, 0, None);
    let mut variants = vec![good.clone()];
    let mut raw = good.clone();
    raw[8] ^= 1;
    variants.push(raw);
    let mut raw = good.clone();
    raw.push(0);
    variants.push(raw);
    let mut raw = good.clone();
    raw.truncate(49);
    raw[48] = 0;
    variants.push(raw);
    for raw in variants {
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .append(plan, info.cursor(), Candidate::InheritedChunk(raw))
                .is_err()
        );
        assert!(ledger::inspect(&s.connection).unwrap() == before);
    }
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .append(
                plan,
                info.cursor(),
                Candidate::InheritedPage(proof(&b, 0, Some(b.page.level())))
            )
            .is_err()
    );
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .append(plan, info.cursor(), Candidate::Zero { length: C })
            .is_err()
    );
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .append(plan, info.cursor(), Candidate::Bytes(pattern(0, C)))
            .is_err()
    );
    assert!(ledger::inspect(&s.connection).unwrap() == before);
    let mut bytes = pattern(0, C);
    bytes[5..10].copy_from_slice(b"dirty");
    let ready = Journal::new(&mut s, Options::default())
        .unwrap()
        .append(plan, info.cursor(), Candidate::Bytes(bytes))
        .unwrap();
    assert_eq!(ready.cursor().offset, C);
}
#[test]
fn actual_sqlite_full_rolls_back_new_blobs_cursor_and_objects_and_preserves_dirty_bytes() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("state")).unwrap();
    let b = base(C, BTreeMap::from([(0, pattern(0, C))]));
    let initial = view(&mut s, &b, C);
    let v = super::super::super::Journal::new(&mut s, super::super::super::Options::default())
        .unwrap()
        .write([2; 32], initial.id(), 0, b"old dirty")
        .unwrap();
    let info = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin(request(&v, &b, 9))
        .unwrap();
    let plan = info.id().unwrap();
    let before = ledger::inspect(&s.connection).unwrap();
    let count: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", count)
        .unwrap();
    let mut bytes = pattern(0, C);
    bytes[..9].copy_from_slice(b"old dirty");
    let error = Journal::new(&mut s, Options::default())
        .unwrap()
        .append(plan, info.cursor(), Candidate::Bytes(bytes.clone()))
        .unwrap_err();
    assert!(
        matches!(error.downcast_ref::<rusqlite::Error>(),Some(rusqlite::Error::SqliteFailure(e,_)) if e.code==rusqlite::ErrorCode::DiskFull)
    );
    assert!(ledger::inspect(&s.connection).unwrap() == before);
    s.connection
        .pragma_update(None, "max_page_count", count + 4096)
        .unwrap();
    let ready = Journal::new(&mut s, Options::default())
        .unwrap()
        .append(plan, info.cursor(), Candidate::Bytes(bytes))
        .unwrap();
    assert!(finish(&mut s, ready).root().unwrap().is_some());
}
#[test]
fn attached_triggers_corrupt_rows_and_oversized_media_are_refused_without_any_repair() {
    for case in 0..5 {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("state");
        let mut s = State::open_managed(&path).unwrap();
        let b = base(C, BTreeMap::from([(0, pattern(0, C))]));
        let v = view(&mut s, &b, C);
        let info = Journal::new(&mut s, Options::default())
            .unwrap()
            .begin(request(&v, &b, 9))
            .unwrap();
        let plan = info.id().unwrap();
        let ready = Journal::new(&mut s, Options::default())
            .unwrap()
            .append(plan, info.cursor(), Candidate::Bytes(pattern(0, C)))
            .unwrap();
        match case {
            0=>s.connection.execute_batch("CREATE TRIGGER foreign_write AFTER INSERT ON sparse_preparation_objects BEGIN DELETE FROM staged_chunks; SELECT RAISE(IGNORE); END").unwrap(),
            1=>{s.connection.execute("UPDATE sparse_preparation_blobs SET data=zeroblob(length(data)) WHERE length(data)=1048576",[]).unwrap();},
            2=>{s.connection.execute_batch("PRAGMA ignore_check_constraints=ON; UPDATE sparse_preparation_blobs SET data=zeroblob(2097152) WHERE length(data)=1048576").unwrap();},
            3=>{let mut object=ledger::inspect(&s.connection).unwrap().objects[&(plan,-1,0)].object.clone();object.level=127;let raw=encode(&object).unwrap();s.connection.execute("UPDATE sparse_preparation_objects SET record=?1",[raw]).unwrap();},
            _=>{s.connection.execute("DELETE FROM sparse_preparation_blobs WHERE length(data)=1048576",[]).unwrap();},
        }
        let before: Vec<(Vec<u8>, i64)> = s
            .connection
            .prepare("SELECT blob,length(data) FROM sparse_preparation_blobs ORDER BY blob")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .step(plan, ready.cursor())
                .is_err()
        );
        let after: Vec<(Vec<u8>, i64)> = s
            .connection
            .prepare("SELECT blob,length(data) FROM sparse_preparation_blobs ORDER BY blob")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(before, after);
        drop(s);
        assert!(State::open_managed(&path).is_err());
    }
}
#[test]
fn additive_schema_thirty_two_preserves_old_dirty_blobs_and_collision_rolls_back_every_new_table() {
    for collision in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("state");
        let mut s = State::open_managed(&path).unwrap();
        let b = base(0, BTreeMap::new());
        let initial = view(&mut s, &b, 0);
        let v = super::super::super::Journal::new(&mut s, super::super::super::Options::default())
            .unwrap()
            .write([2; 32], initial.id(), 0, "café retained bytes".as_bytes())
            .unwrap();
        let before = super::super::super::ledger::inspect(&s.connection).unwrap();
        s.connection
            .execute_batch(&crate::test_schema::before_preparation(
                "PRAGMA user_version=31",
            ))
            .unwrap();
        if collision {
            s.connection.execute_batch("CREATE TABLE sparse_preparation_blobs(keep TEXT); INSERT INTO sparse_preparation_blobs VALUES('untouched')").unwrap();
        }
        drop(s);
        if collision {
            assert!(State::open_managed(&path).is_err());
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                    .unwrap(),
                31
            );
            assert_eq!(
                db.query_row::<String, _, _>(
                    "SELECT keep FROM sparse_preparation_blobs",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
                "untouched"
            );
            assert!(db.prepare("SELECT * FROM sparse_preparations").is_err());
            assert!(
                db.prepare("SELECT * FROM sparse_preparation_objects")
                    .is_err()
            );
            assert!(super::super::super::ledger::inspect(&db).unwrap() == before);
        } else {
            let mut s = State::open_managed(&path).unwrap();
            assert_eq!(s.info().unwrap().schema_version, 32);
            assert!(super::super::super::ledger::inspect(&s.connection).unwrap() == before);
            let original =
                super::super::super::Journal::new(&mut s, super::super::super::Options::default())
                    .unwrap()
                    .view(v.id())
                    .unwrap();
            assert_eq!(original, v);
            assert_eq!(
                s.connection
                    .query_row::<Vec<u8>, _, _>("SELECT data FROM sparse_view_blocks", [], |r| r
                        .get(0))
                    .unwrap(),
                "café retained bytes".as_bytes()
            );
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn private_owner_reopens_exact_preparation_after_close_and_newer_head_without_another_database()
 {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("worker café");
    let b = base(0, BTreeMap::new());
    let storage = crate::worker::Storage::open(
        &path,
        crate::cache::Options::default(),
        crate::journal::Options::default(),
    )
    .await
    .unwrap();
    let local = storage
        .sparse_views(super::super::super::Options::default())
        .unwrap();
    let v = local.begin([1; 32], binding(0), C).await.unwrap();
    let plans = local.preparations(Options::default()).unwrap();
    let info = plans.begin(request(&v, &b, 9)).await.unwrap();
    let plan = info.id().unwrap();
    let ready = plans
        .append(plan, info.cursor(), Candidate::Zero { length: C })
        .await
        .unwrap();
    let newer = local.resize([2; 32], v.id(), 0).await.unwrap();
    storage.close().await.unwrap();
    assert!(plans.info(plan).await.is_err());
    let storage = crate::worker::Storage::open(
        &path,
        crate::cache::Options::default(),
        crate::journal::Options::default(),
    )
    .await
    .unwrap();
    let local = storage
        .sparse_views(super::super::super::Options::default())
        .unwrap();
    let plans = local.preparations(Options::default()).unwrap();
    assert_eq!(plans.info(plan).await.unwrap().cursor(), ready.cursor());
    let done = plans.step(plan, ready.cursor()).await.unwrap();
    assert!(done.root().unwrap().is_some());
    assert_eq!(local.head(v.binding().clone()).await.unwrap(), newer);
    storage.close().await.unwrap();
}
#[test]
fn orphan_preparation_rows_cannot_be_adopted_by_authority_or_legacy_publication_roles() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("state")).unwrap();
    let data = b"orphan bytes";
    s.connection
        .execute(
            "INSERT INTO sparse_preparation_blobs VALUES(?1,?2)",
            params![digest(data).as_slice(), data.as_slice()],
        )
        .unwrap();
    assert!(s.namespace_initialize().is_err());
    assert_eq!(
        s.connection
            .query_row::<u32, _, _>("SELECT count(*) FROM namespace_volume", [], |r| r.get(0))
            .unwrap(),
        0
    );
    assert_eq!(
        s.connection
            .query_row::<Vec<u8>, _, _>("SELECT data FROM sparse_preparation_blobs", [], |r| r
                .get(0))
            .unwrap(),
        data
    );
    assert!(require_legacy(&s.connection).is_err());
}
