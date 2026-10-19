use super::*;
#[test]
fn quotas_triggers_wrong_identities_and_invalid_receipts_never_acknowledge_or_repair() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("faults");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let info = prepare(&mut s, &f, false);
    let p = info.id().unwrap();
    let source = super::super::super::ledger::inspect(&s.connection).unwrap();
    let limits = Limits {
        plans: 1,
        checks: 1,
        metadata_bytes: 1,
    };
    assert!(
        Journal::new(&mut s, limits)
            .unwrap()
            .check(p, 0, Some(&proof(&f)))
            .is_err()
    );
    assert!(ledger::inspect(&s.connection).unwrap().empty());
    s.connection.execute_batch("CREATE TRIGGER corrupt_semantics AFTER INSERT ON sparse_publication_checks BEGIN UPDATE sparse_view_blocks SET data=zeroblob(length(data)); END;").unwrap();
    assert!(j(&mut s).check(p, 0, Some(&proof(&f))).is_err());
    s.connection
        .execute_batch("DROP TRIGGER corrupt_semantics")
        .unwrap();
    assert!(ledger::inspect(&s.connection).unwrap().empty());
    assert!(super::super::super::ledger::inspect(&s.connection).unwrap() == source);
    j(&mut s).check(p, 0, Some(&proof(&f))).unwrap();
    Journal::new(&mut s, limits)
        .unwrap()
        .check(p, 0, Some(&proof(&f)))
        .unwrap();
    physical(&mut s, p);
    let mut wrong = f.store.clone();
    wrong.route = "f".repeat(64);
    assert!(
        j(&mut s)
            .begin(p, f.node.clone(), wrong, 5, hex(&[10; 16]))
            .is_err()
    );
    assert!(
        j(&mut s)
            .begin(p, f.node.clone(), f.store.clone(), 5, ns::ROOT.into())
            .is_err()
    );
    let mut wrong = f.node.clone();
    wrong.id = hex(&[11; 16]);
    assert!(
        j(&mut s)
            .begin(p, wrong, f.store.clone(), 5, hex(&[10; 16]))
            .is_err()
    );
    let r = j(&mut s)
        .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
        .unwrap();
    let before = ledger::inspect(&s.connection).unwrap();
    let mut wrong = reserved(&r);
    wrong.sequence = 5;
    assert!(j(&mut s).reserved(p, wrong).is_err());
    assert!(ledger::inspect(&s.connection).unwrap() == before);
    s.connection.execute_batch("CREATE TRIGGER skip_root BEFORE UPDATE ON sparse_publications BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    assert!(j(&mut s).reserved(p, reserved(&r)).is_err());
    s.connection
        .execute_batch("DROP TRIGGER skip_root")
        .unwrap();
    assert!(ledger::inspect(&s.connection).unwrap() == before);
    let r = Journal::new(&mut s, limits)
        .unwrap()
        .reserved(p, reserved(&r))
        .unwrap();
    assert!(r.reservation().is_some());
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(j(&mut s).get(p).unwrap(), r);
    assert!(super::super::super::ledger::inspect(&s.connection).unwrap() == source);
}
#[test]
fn actual_sqlite_full_during_semantic_admission_rolls_back_and_retries_same_plan() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("full")).unwrap();
    let f = fixture();
    let first = prepare(&mut s, &f, false);
    let mut plans = vec![first.id().unwrap()];
    let mut bytes = f.bytes.clone();
    bytes[5] = 255;
    for op in 10..25 {
        let i = pj(&mut s)
            .begin(Request {
                operation: [op; 32],
                view: first.view().id(),
                generation: [op; 16],
                base_root: f.base.root.encode().to_vec(),
                base_page: f.base.page.encode(),
            })
            .unwrap();
        let i = pj(&mut s)
            .append(i.id().unwrap(), i.cursor(), Candidate::Bytes(bytes.clone()))
            .unwrap();
        plans.push(
            super::super::super::tests::fixture::finish(&mut s, i)
                .id()
                .unwrap(),
        );
    }
    let source = super::super::super::ledger::inspect(&s.connection).unwrap();
    s.connection
        .execute_batch("CREATE TABLE semantic_full_filler(data BLOB)")
        .unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    loop {
        match s.connection.execute(
            "INSERT INTO semantic_full_filler VALUES(?1)",
            [vec![99u8; 3500]],
        ) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::DiskFull =>
            {
                break;
            }
            Err(e) => panic!("unexpected filler error: {e}"),
        }
    }
    let mut failed = None;
    for p in plans {
        let before = ledger::inspect(&s.connection).unwrap();
        match j(&mut s).check(p, 0, Some(&proof(&f))) {
            Ok(()) => {}
            Err(e) => {
                assert!(
                    matches!(e.downcast_ref::<rusqlite::Error>(),Some(rusqlite::Error::SqliteFailure(v,_)) if v.code==rusqlite::ErrorCode::DiskFull),
                    "{e}"
                );
                assert!(ledger::inspect(&s.connection).unwrap() == before);
                failed = Some(p);
                break;
            }
        }
    }
    let p =
        failed.expect("full SQLite must refuse at least one check before allocating all16 rows");
    assert!(super::super::super::ledger::inspect(&s.connection).unwrap() == source);
    s.connection
        .pragma_update(None, "max_page_count", pages + 1024)
        .unwrap();
    j(&mut s).check(p, 0, Some(&proof(&f))).unwrap();
    assert_eq!(pj(&mut s).info(p).unwrap().view(), first.view());
    assert!(super::super::super::ledger::inspect(&s.connection).unwrap() == source);
}
#[tokio::test]
async fn metadata_only_worker_refuses_fabricated_replies_and_releases_exclusive_ownership() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("worker");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let info = prepare(&mut s, &f, false);
    let p = info.id().unwrap();
    j(&mut s).check(p, 0, Some(&proof(&f))).unwrap();
    physical(&mut s, p);
    let r = j(&mut s)
        .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
        .unwrap();
    let r = j(&mut s).reserved(p, reserved(&r)).unwrap();
    let r = j(&mut s)
        .prepare_publish(p, r.reservation().unwrap().sequence)
        .unwrap();
    assert!(RecoveryClient::open(&path).await.is_err());
    drop(s);
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    db.execute(
        "UPDATE sparse_preparation_blobs SET data=zeroblob(length(data))",
        [],
    )
    .unwrap();
    drop(db);
    let recovery = RecoveryClient::open(&path).await.unwrap();
    assert!(RecoveryStore::open(&path).is_err());
    assert_eq!(recovery.records().await.unwrap(), vec![r.clone()]);
    let mut wrong = published(&r);
    wrong.sequence = 6;
    assert!(recovery.published(p, wrong).await.is_err());
    assert_eq!(recovery.get(p).await.unwrap(), r);
    recovery.close().await.unwrap();
    assert!(recovery.get(p).await.is_err());
    let mut owner = RecoveryStore::open(&path).unwrap();
    assert_eq!(owner.get(p).unwrap(), r);
}
#[test]
fn recovery_never_creates_or_migrates_and_orphan_metadata_cannot_adopt_another_role() {
    let t = tempfile::tempdir().unwrap();
    let missing = t.path().join("missing");
    assert!(RecoveryStore::open(&missing).is_err());
    assert!(!missing.exists());
    let path = t.path().join("old");
    let s = State::open_managed(&path).unwrap();
    s.connection
        .execute_batch(&crate::test_schema::before_publication(
            "PRAGMA user_version=32",
        ))
        .unwrap();
    drop(s);
    assert!(RecoveryStore::open(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        32
    );
    assert!(db.prepare("SELECT * FROM sparse_publication_role").is_err());
    drop(db);
    let mut s = State::open_managed(&path).unwrap();
    s.connection
        .execute_batch("INSERT INTO sparse_publication_role VALUES(1,0,0,zeroblob(32))")
        .unwrap();
    assert!(crate::chunk_publication::Journal::new(&mut s).is_err());
    assert!(s.namespace_initialize().is_err());
    assert!(RecoveryStore::open(&path).is_err());
}

#[test]
fn self_consistent_substituted_check_metadata_cannot_report_semantic_completion() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("substitution")).unwrap();
    let f = fixture();
    let info = prepare(&mut s, &f, false);
    let p = info.id().unwrap();
    j(&mut s).check(p, 0, Some(&proof(&f))).unwrap();
    let mut meta = ledger::inspect(&s.connection).unwrap();
    let c = meta.checks.get_mut(&(p, 0)).unwrap();
    c.candidate = [0; 32];
    let raw = serde_json::to_vec(c).unwrap();
    let row_hash = hash(b"s3drive-mixed-root-check-v1\0", c).unwrap();
    s.connection
        .execute(
            "UPDATE sparse_publication_checks SET record=?1,state_hash=?2",
            params![raw, row_hash.as_slice()],
        )
        .unwrap();
    let role = hash(
        b"s3drive-mixed-root-role-v1\0",
        &(
            &meta.records.iter().collect::<Vec<_>>(),
            &meta
                .checks
                .iter()
                .map(|(k, v)| (k.0, k.1, v))
                .collect::<Vec<_>>(),
        ),
    )
    .unwrap();
    s.connection
        .execute(
            "UPDATE sparse_publication_role SET state_hash=?1",
            [role.as_slice()],
        )
        .unwrap();
    assert!(
        ledger::inspect(&s.connection).is_ok(),
        "fixture must pass metadata-only hashes"
    );
    assert!(j(&mut s).unchecked(p, None, 128).is_err());
    assert!(
        j(&mut s)
            .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
            .is_err()
    );
    assert_eq!(
        s.connection
            .query_row("SELECT record FROM sparse_publication_checks", [], |r| r
                .get::<_, Vec<u8>>(0))
            .unwrap(),
        raw
    );
}
