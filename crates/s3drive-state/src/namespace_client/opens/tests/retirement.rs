use super::*;
fn make(f: &mut Fixture, n: u64) -> OpenInput {
    let mut i = f.open(n, None);
    i.controlled = true;
    let mut j = Journal::new(&mut f.client).unwrap();
    j.prepare(i.clone()).unwrap();
    j.activate(&i.id, &key(n + 1)).unwrap();
    i
}
fn finish(f: &mut Fixture, i: &OpenInput, n: u64) -> OpenRecord {
    let mut j = Journal::new(&mut f.client).unwrap();
    j.callback(&i.id, &key(n), Callback::Cleanup { flags: 0 })
        .unwrap();
    j.complete(&i.id, &key(n + 1), &key(n)).unwrap();
    j.callback(&i.id, &key(n + 2), Callback::Close).unwrap();
    j.complete(&i.id, &key(n + 3), &key(n + 2)).unwrap()
}
#[test]
fn retirement_preserves_complete_proofs_exact_replay_and_permanent_identity() {
    let mut f = Fixture::new();
    let i = make(&mut f, 1000);
    let closed = finish(&mut f, &i, 4000);
    let old = load(&f.client.connection, &i.id).unwrap().unwrap().0;
    Journal::new(&mut f.client).unwrap().retire(&i.id).unwrap();
    f.reopen();
    let mut j = Journal::new(&mut f.client).unwrap();
    assert!(j.records().unwrap().is_empty());
    assert_eq!(j.get(&i.id).unwrap(), closed);
    j.retire(&i.id).unwrap();
    assert_eq!(
        j.callback(&i.id, &key(4002), Callback::Close).unwrap(),
        closed
    );
    assert!(j.callback(&i.id, &key(5000), Callback::Close).is_err());
    assert!(j.prepare(i.clone()).is_err());
    assert!(load(&f.client.connection, &i.id).unwrap().unwrap().0 == old);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .abandoned(&key(9999))
            .unwrap()
            .is_empty()
    );
}
#[test]
fn retired_opens_free_live_open_and_event_budgets_with_a_long_lived_reader() {
    let mut f = Fixture::new();
    let reader = make(&mut f, 1000);
    for n in 0..300 {
        let base = 10000 + n * 100;
        let i = make(&mut f, base);
        for k in 0..3 {
            let cb = base + 10 + k * 3;
            let mut j = Journal::new(&mut f.client).unwrap();
            j.callback(&i.id, &key(cb), Callback::SetDelete { delete: false })
                .unwrap();
            j.reply(&i.id, &key(cb + 1), &key(cb), None).unwrap();
            j.complete(&i.id, &key(cb + 2), &key(cb)).unwrap();
        }
        finish(&mut f, &i, base + 80);
        Journal::new(&mut f.client).unwrap().retire(&i.id).unwrap();
    }
    f.reopen();
    let records = Journal::new(&mut f.client).unwrap().records().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].input, reader);
    let count: i64 = f
        .client
        .connection
        .query_row("SELECT count(*) FROM windows_open_events", [], |r| r.get(0))
        .unwrap();
    assert!(count > EVENT_LIMIT as i64);
    assert_eq!(
        f.client
            .connection
            .query_row::<i64, _, _>("SELECT count(*) FROM retired_windows_opens", [], |r| r
                .get(0))
            .unwrap(),
        300
    );
}
#[test]
fn uncertain_or_evidence_only_records_cannot_be_retired_or_fabricate_session_cleanup() {
    let mut f = Fixture::new();
    let i = make(&mut f, 1000);
    let mut j = Journal::new(&mut f.client).unwrap();
    assert!(j.retire(&i.id).is_err());
    j.callback(&i.id, &key(3000), Callback::SetDelete { delete: false })
        .unwrap();
    finish(&mut f, &i, 4000);
    let mut j = Journal::new(&mut f.client).unwrap();
    assert!(j.retire(&i.id).is_err());
    assert_eq!(j.records().unwrap().len(), 1);
    j.reply(&i.id, &key(3001), &key(3000), None).unwrap();
    j.complete(&i.id, &key(3002), &key(3000)).unwrap();
    j.retire(&i.id).unwrap();
    let legacy = f.open(6000, None);
    let mut j = Journal::new(&mut f.client).unwrap();
    j.prepare(legacy.clone()).unwrap();
    j.activate(&legacy.id, &key(6500)).unwrap();
    let mut j = Journal::new(&mut f.client).unwrap();
    j.callback(&legacy.id, &key(6000), Callback::Cleanup { flags: 0 })
        .unwrap();
    j.callback(&legacy.id, &key(6001), Callback::Close).unwrap();
    assert!(j.retire(&legacy.id).is_err());
    let open = make(&mut f, 7000);
    let mut j = Journal::new(&mut f.client).unwrap();
    assert_eq!(j.abandoned(&key(9999)).unwrap()[0].input, open);
    assert!(j.retire(&open.id).is_err());
    assert!(j.get(&open.id).unwrap().cleanup().is_none());
}
#[test]
fn ignored_retirement_and_changed_seals_never_free_unproven_capacity() {
    let mut f = Fixture::new();
    let i = make(&mut f, 1000);
    let closed = finish(&mut f, &i, 4000);
    f.client.connection.execute_batch("CREATE TRIGGER ignore_retire BEFORE INSERT ON retired_windows_opens BEGIN SELECT RAISE(IGNORE); END").unwrap();
    assert!(Journal::new(&mut f.client).unwrap().retire(&i.id).is_err());
    assert_eq!(
        Journal::new(&mut f.client).unwrap().records().unwrap(),
        vec![closed.clone()]
    );
    f.client
        .connection
        .execute_batch("DROP TRIGGER ignore_retire")
        .unwrap();
    Journal::new(&mut f.client).unwrap().retire(&i.id).unwrap();
    f.client
        .connection
        .execute(
            "UPDATE retired_windows_opens SET state_hash=zeroblob(32) WHERE open_id=?",
            [&i.id],
        )
        .unwrap();
    let j = Journal::new(&mut f.client).unwrap();
    assert!(j.records().is_err());
    assert!(j.get(&i.id).is_err());
}
#[test]
fn sqlite_full_retirement_rolls_back_without_losing_close_or_admitting_reuse() {
    let mut f = Fixture::new();
    let i = make(&mut f, 1000);
    let closed = finish(&mut f, &i, 4000);
    f.client
        .connection
        .execute_batch("CREATE TABLE retirement_full(data BLOB); PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let pages: i64 = f
        .client
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    f.client
        .connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    f.client.connection.execute_batch("CREATE TRIGGER fill_retirement BEFORE INSERT ON retired_windows_opens BEGIN INSERT INTO retirement_full VALUES(zeroblob(1048576)); END").unwrap();
    let error = Journal::new(&mut f.client)
        .unwrap()
        .retire(&i.id)
        .unwrap_err();
    assert!(
        matches!(error.downcast_ref::<rusqlite::Error>(), Some(rusqlite::Error::SqliteFailure(e,_)) if e.code==rusqlite::ErrorCode::DiskFull)
    );
    assert_eq!(
        Journal::new(&mut f.client).unwrap().get(&i.id).unwrap(),
        closed
    );
    assert_eq!(
        Journal::new(&mut f.client)
            .unwrap()
            .records()
            .unwrap()
            .len(),
        1
    );
    f.client
        .connection
        .pragma_update(None, "max_page_count", pages + 1024)
        .unwrap();
    f.client
        .connection
        .execute_batch("DROP TRIGGER fill_retirement")
        .unwrap();
    f.reopen();
    Journal::new(&mut f.client).unwrap().retire(&i.id).unwrap();
    assert!(Journal::new(&mut f.client).unwrap().prepare(i).is_err());
}
#[test]
fn retired_proofs_are_still_validated_and_missing_anchors_are_not_hidden() {
    for mutation in [
        "DELETE FROM retained_windows_opens",
        "DELETE FROM windows_open_events WHERE ordinal=2",
        "UPDATE windows_open_events SET action=x'7b7d' WHERE ordinal=2",
    ] {
        let mut f = Fixture::new();
        let i = make(&mut f, 1000);
        finish(&mut f, &i, 4000);
        Journal::new(&mut f.client).unwrap().retire(&i.id).unwrap();
        f.client.connection.execute_batch(mutation).unwrap();
        assert!(Journal::new(&mut f.client).unwrap().get(&i.id).is_err());
        assert!(Journal::new(&mut f.client).unwrap().prepare(i).is_err());
    }
}
#[test]
fn copied_live_row_hash_is_not_a_retirement_proof() {
    let mut f = Fixture::new();
    let i = make(&mut f, 1000);
    f.client.connection.execute("INSERT INTO retired_windows_opens SELECT open_id,state_hash FROM windows_open_journal WHERE open_id=?", [&i.id]).unwrap();
    let mut j = Journal::new(&mut f.client).unwrap();
    assert!(j.records().is_err());
    assert!(j.get(&i.id).is_err());
    assert!(j.retire(&i.id).is_err());
}
#[test]
fn schema_eighteen_migration_preserves_closed_history_and_refuses_conflicting_table() {
    let mut f = Fixture::new();
    let i = make(&mut f, 1000);
    let closed = finish(&mut f, &i, 4000);
    let old = load(&f.client.connection, &i.id).unwrap().unwrap().0;
    f.client
        .connection
        .execute_batch("DROP TABLE chunk_publications; DROP TABLE chunk_client_role; DROP TABLE chunk_objects; DROP TABLE chunk_plans; DROP TABLE multipart_parts; DROP TABLE multipart_plans; DROP TABLE recovered_windows_opens; DROP TABLE windows_session_recoveries; DROP TABLE windows_mount_sessions; DROP TABLE retired_windows_opens; PRAGMA user_version=18")
        .unwrap();
    f.reopen();
    assert_eq!(
        f.client.info().unwrap().schema_version,
        crate::SCHEMA_VERSION
    );
    assert!(load(&f.client.connection, &i.id).unwrap().unwrap().0 == old);
    assert_eq!(
        Journal::new(&mut f.client).unwrap().get(&i.id).unwrap(),
        closed
    );
    f.client.connection.execute_batch("DROP TABLE chunk_publications; DROP TABLE chunk_client_role; DROP TABLE chunk_objects; DROP TABLE chunk_plans; DROP TABLE multipart_parts; DROP TABLE multipart_plans; DROP TABLE recovered_windows_opens; DROP TABLE windows_session_recoveries; DROP TABLE windows_mount_sessions; DROP TABLE retired_windows_opens; CREATE TABLE retired_windows_opens(keep TEXT); INSERT INTO retired_windows_opens VALUES('untouched'); PRAGMA user_version=18").unwrap();
    let path = f.tmp.path().join("client");
    drop(f.client);
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.query_row::<String, _, _>("SELECT keep FROM retired_windows_opens", [], |r| r.get(0))
            .unwrap(),
        "untouched"
    );
    assert_eq!(
        db.pragma_query_value::<i32, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        18
    );
}
