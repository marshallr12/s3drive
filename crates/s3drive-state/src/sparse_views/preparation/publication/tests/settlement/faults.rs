use super::*;
#[test]
fn actual_sqlite_full_rolls_back_epoch_admission_without_losing_published_or_dirty_data() {
    let temp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&temp.path().join("full")).unwrap();
    s.connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA page_size=512; VACUUM; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;").unwrap();
    assert_eq!(
        s.connection
            .pragma_query_value(None, "page_size", |r| r.get::<_, i64>(0))
            .unwrap(),
        512
    );
    let f = fixture();
    let r = first(&mut s, &f);
    let r = commit(&mut s, r);
    let p = r.input.plan;
    s.connection
        .execute_batch("CREATE TABLE epoch_full_filler(data BLOB)")
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
            "INSERT INTO epoch_full_filler VALUES(?1)",
            [vec![55u8; 350]],
        ) {
            Ok(_) => (),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::DiskFull =>
            {
                break;
            }
            Err(e) => panic!("unexpected filler failure: {e}"),
        }
    }
    let source = crate::sparse_views::ledger::inspect(&s.connection).unwrap();
    let meta = ledger::inspect(&s.connection).unwrap();
    let e = j(&mut s)
        .acknowledge([40; 32], p, crate::sparse_views::Options::default())
        .unwrap_err();
    assert!(
        matches!(e.downcast_ref::<rusqlite::Error>(),Some(rusqlite::Error::SqliteFailure(e,_)) if e.code==rusqlite::ErrorCode::DiskFull),
        "{e}"
    );
    assert!(crate::sparse_views::ledger::inspect(&s.connection).unwrap() == source);
    assert!(ledger::inspect(&s.connection).unwrap() == meta);
    assert!(j(&mut s).get(p).unwrap().receipt().is_some());
    let base = f.binding.base().unwrap();
    assert!(namespace_pending(&s.connection, base.volume(), base.node()).is_err());
    s.connection
        .pragma_update(None, "max_page_count", pages + 64)
        .unwrap();
    assert!(
        j(&mut s)
            .acknowledge([40; 32], p, crate::sparse_views::Options::default())
            .unwrap()
            .acknowledgment()
            .clean_view()
            .is_some()
    );
    assert!(namespace_pending(&s.connection, base.volume(), base.node()).is_ok());
}
#[test]
fn schema_thirty_five_preserves_old_records_and_rejects_new_ack_formats_under_old_markers_without_repair()
 {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("migration");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let r = first(&mut s, &f);
    let r = commit(&mut s, r);
    let p = r.input.plan;
    let source = crate::sparse_views::ledger::inspect(&s.connection).unwrap();
    let meta = ledger::inspect(&s.connection).unwrap();
    let raw: Vec<u8> = s
        .connection
        .query_row(
            "SELECT record FROM sparse_publications WHERE plan_id=?1",
            [p.as_slice()],
            |q| q.get(0),
        )
        .unwrap();
    assert!(
        !String::from_utf8(raw.clone())
            .unwrap()
            .contains("acknowledgment")
    );
    s.connection
        .execute_batch("DROP TABLE source_read_pins; DROP TABLE source_read_pin_session; DROP TABLE sparse_scoped_mode")
        .unwrap();
    crate::test_schema::without_creation(&s.connection);
    crate::test_schema::demote(&s.connection, 34).unwrap();
    drop(s);
    let mut recovery = RecoveryStore::open(&path).unwrap();
    assert_eq!(recovery.acknowledged(p).unwrap(), None);
    drop(recovery);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
    assert!(crate::sparse_views::ledger::inspect(&s.connection).unwrap() == source);
    assert!(ledger::inspect(&s.connection).unwrap() == meta);
    let after: Vec<u8> = s
        .connection
        .query_row(
            "SELECT record FROM sparse_publications WHERE plan_id=?1",
            [p.as_slice()],
            |q| q.get(0),
        )
        .unwrap();
    assert_eq!(after, raw);
    let a = j(&mut s)
        .acknowledge([40; 32], p, crate::sparse_views::Options::default())
        .unwrap();
    crate::test_schema::without_creation(&s.connection);
    crate::test_schema::demote(&s.connection, 34).unwrap();
    drop(s);
    assert!(RecoveryStore::open(&path).is_err());
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |q| q.get::<_, i32>(0))
            .unwrap(),
        34
    );
    let raw: Vec<u8> = db
        .query_row(
            "SELECT record FROM sparse_publications WHERE plan_id=?1",
            [p.as_slice()],
            |q| q.get(0),
        )
        .unwrap();
    assert_eq!(serde_json::from_slice::<Record>(&raw).unwrap(), *a.record());
}
#[test]
fn self_consistent_missing_or_historical_ack_metadata_cannot_hide_a_rebase_or_change_its_identity()
{
    for historical in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("corruption");
        let mut s = State::open_managed(&path).unwrap();
        let f = fixture();
        let r = first(&mut s, &f);
        let r = commit(&mut s, r);
        let p = r.input.plan;
        let a = j(&mut s)
            .acknowledge([40; 32], p, crate::sparse_views::Options::default())
            .unwrap();
        let next = vj(&mut s)
            .write([41; 32], a.acknowledgment().clean_view().unwrap(), 6, &[99])
            .unwrap();
        let before = ledger::inspect(&s.connection).unwrap();
        let mut bad = before.clone();
        let r = bad.records.get_mut(&p).unwrap();
        if historical {
            let a = r.acknowledgment.as_mut().unwrap();
            a.clean_view = None;
            a.observed_head = next.id();
        } else {
            r.acknowledgment = None;
            r.format = 1;
        }
        bad.validate().unwrap();
        let tx = s
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        ledger::save(&tx, &before, &bad, false).unwrap();
        tx.commit().unwrap();
        assert!(crate::sparse_views::ledger::inspect(&s.connection).is_err());
        assert!(j(&mut s).get(p).is_err());
        drop(s);
        assert!(State::open_managed(&path).is_err());
    }
}

#[test]
fn missing_publication_table_never_forgets_current_schema_ack_identities_or_adopts_old_data() {
    for historical in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("missing metadata");
        let mut s = State::open_managed(&path).unwrap();
        let f = fixture();
        let r = first(&mut s, &f);
        let r = commit(&mut s, r);
        let p = r.input.plan;
        if historical {
            vj(&mut s).write([41; 32], r.input.view, 6, &[88]).unwrap();
        }
        j(&mut s)
            .acknowledge([40; 32], p, crate::sparse_views::Options::default())
            .unwrap();
        s.connection
            .execute_batch("DROP TABLE sparse_publications")
            .unwrap();
        assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
        assert!(
            crate::sparse_views::preparation::publication::settlement::guard_operation(
                &s.connection,
                [40; 32]
            )
            .is_err()
        );
        assert!(
            crate::sparse_views::Journal::new(&mut s, crate::sparse_views::Options::default())
                .is_err()
        );
        assert!(
            s.connection
                .query_row("SELECT count(*) FROM sparse_view_blocks", [], |q| q
                    .get::<_, i64>(0))
                .unwrap()
                > 0
        );
        drop(s);
        assert!(State::open_managed(&path).is_err());
    }
}
