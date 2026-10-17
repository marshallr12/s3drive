use super::*;
fn start(f: &mut Fixture, n: u64) {
    Journal::new(&mut f.client)
        .unwrap()
        .start_session(&key(n), &f.volume)
        .unwrap();
}
fn make(f: &mut Fixture, n: u64, doc: bool, activate: bool) -> OpenInput {
    let admission = doc.then(|| f.admission());
    let mut i = f.open(n, admission.clone());
    i.session = key(2000);
    i.controlled = true;
    i.session_bound = true;
    Journal::new(&mut f.client)
        .unwrap()
        .prepare(i.clone())
        .unwrap();
    if activate {
        if let Some(a) = admission {
            f.apply(&a);
        }
        Journal::new(&mut f.client)
            .unwrap()
            .activate(&i.id, &key(n + 10000))
            .unwrap();
    }
    i
}
fn cancel(f: &mut Fixture) -> Input {
    let owned = removals::owned(&f.client.connection, &f.volume, &f.node.id)
        .unwrap()
        .unwrap();
    f.input(Mutation::CancelRemoval {
        node: f.node.id.clone(),
        fence: owned.fence,
    })
}
#[test]
fn only_replacing_exclusive_state_owner_can_make_a_registered_session_obsolete() {
    let mut f = Fixture::new();
    start(&mut f, 2000);
    let i = make(&mut f, 1000, false, true);
    let mut j = Journal::new(&mut f.client).unwrap();
    assert!(!j.obsolete_session(&key(2000), &f.volume).unwrap());
    assert!(j.start_session(&key(2001), &f.volume).is_err());
    assert!(
        j.prepare_recovery(&key(2000), &f.volume, &i.node, None)
            .is_err()
    );
    assert!(State::open_managed(&f.tmp.path().join("client")).is_err());
    f.reopen();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .start_session(&key(2000), &f.volume)
            .is_err()
    );
    start(&mut f, 2001);
    let j = Journal::new(&mut f.client).unwrap();
    assert!(j.obsolete_session(&key(2000), &f.volume).unwrap());
    assert!(j.current_session(&key(2000), &f.volume).is_err());
    assert!(j.current_session(&key(2001), &key(999)).is_err());
}
#[test]
fn exact_old_link_replay_is_bookkeeping_while_a_later_effect_is_unknown() {
    let mut f = Fixture::new();
    start(&mut f, 2000);
    let i = make(&mut f, 1000, true, true);
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3000), Callback::Cleanup { flags: 1 })
        .unwrap();
    let owned = removals::owned(&f.client.connection, &f.volume, &i.node)
        .unwrap()
        .unwrap();
    let promote = f.input(Mutation::TransitionRemoval {
        node: namespace::NodeGuard {
            id: i.node.clone(),
            revision: f.node.revision,
        },
        parent: namespace::NodeGuard {
            id: ROOT.into(),
            revision: 0,
        },
        fence: owned.fence.clone(),
        expected: RemovalMode::Deferred,
        mode: RemovalMode::Pending,
    });
    Journal::new(&mut f.client)
        .unwrap()
        .link(&i.id, &key(3001), &key(3000), promote.clone())
        .unwrap();
    f.apply(&promote);
    Journal::new(&mut f.client)
        .unwrap()
        .acknowledge(&i.id, &key(3002), &promote.request.operation)
        .unwrap();
    let commit = f.input(Mutation::CommitRemoval {
        node: namespace::NodeGuard {
            id: i.node.clone(),
            revision: f.node.revision,
        },
        parent: namespace::NodeGuard {
            id: ROOT.into(),
            revision: 0,
        },
        fence: owned.fence,
    });
    Journal::new(&mut f.client)
        .unwrap()
        .link(&i.id, &key(3003), &key(3000), commit.clone())
        .unwrap();
    let mut j = Journal::new(&mut f.client).unwrap();
    let before = j.get(&i.id).unwrap();
    assert_eq!(
        j.link(&i.id, &key(3001), &key(3000), promote.clone())
            .unwrap(),
        before
    );
    assert!(j.link(&i.id, &key(3004), &key(3000), promote).is_err());
    assert_eq!(j.get(&i.id).unwrap().pending(), vec![&commit]);
}
#[test]
fn ignored_registration_and_changed_runtime_binding_cannot_authorize_a_session() {
    let mut f = Fixture::new();
    f.client.connection.execute_batch("CREATE TRIGGER ignore_session BEFORE INSERT ON windows_mount_sessions BEGIN SELECT RAISE(IGNORE);END").unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .start_session(&key(2000), &f.volume)
            .is_err()
    );
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .current_session(&key(2000), &f.volume)
            .is_err()
    );
    f.client
        .connection
        .execute_batch("DROP TRIGGER ignore_session")
        .unwrap();
    start(&mut f, 2000);
    let i = make(&mut f, 1000, false, true);
    f.client
        .connection
        .execute("UPDATE windows_mount_sessions SET runtime_id=?", [ROOT])
        .unwrap();
    assert!(Journal::new(&mut f.client).unwrap().get(&i.id).is_err());
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .obsolete_session(&i.session, &f.volume)
            .is_err()
    );
}
#[test]
fn retained_session_without_any_open_still_forbids_replacing_a_lost_client_identity() {
    let mut f = Fixture::new();
    start(&mut f, 2000);
    f.client
        .connection
        .execute_batch("DELETE FROM client_identity;DELETE FROM retained_client_identity")
        .unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .start_session(&key(2001), &f.volume)
            .is_err()
    );
    assert!(identity::read(&f.client.connection).unwrap().is_none());
    assert_eq!(
        f.client
            .connection
            .query_row::<i64, _, _>("SELECT count(*) FROM windows_mount_sessions", [], |r| r
                .get(0))
            .unwrap(),
        1
    );
}
#[test]
fn abandonment_preserves_missing_cleanup_close_and_all_original_event_bytes() {
    let mut f = Fixture::new();
    start(&mut f, 2000);
    let i = make(&mut f, 1000, false, true);
    let original = Journal::new(&mut f.client).unwrap().get(&i.id).unwrap();
    f.reopen();
    start(&mut f, 2001);
    let mut j = Journal::new(&mut f.client).unwrap();
    let mut next = i.clone();
    next.id = key(1001);
    next.session = key(2001);
    assert!(j.prepare(next.clone()).is_err());
    let r = j
        .prepare_recovery(&key(2001), &f.volume, &i.node, None)
        .unwrap();
    assert!(!r.finished);
    assert!(
        j.callback(&i.id, &key(3000), Callback::Cleanup { flags: 1 })
            .is_err()
    );
    assert!(!j.recovered(&i.id).unwrap());
    j.finish_recovery(&key(2001), &r.input.id).unwrap();
    j.retire(&i.id).unwrap();
    assert_eq!(j.get(&i.id).unwrap(), original);
    assert!(original.cleanup().is_none() && original.close().is_none());
    j.prepare(next).unwrap();
    f.reopen();
    start(&mut f, 2002);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .recovered(&i.id)
            .unwrap()
    );
    assert_eq!(
        Journal::new(&mut f.client).unwrap().get(&i.id).unwrap(),
        original
    );
}
#[test]
fn cancellation_proof_is_exact_and_remains_historical_after_a_successor_epoch() {
    let mut f = Fixture::new();
    start(&mut f, 2000);
    let i = make(&mut f, 1000, true, true);
    f.reopen();
    start(&mut f, 2001);
    let close = cancel(&mut f);
    let r = Journal::new(&mut f.client)
        .unwrap()
        .prepare_recovery(&key(2001), &f.volume, &i.node, Some(close.clone()))
        .unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .finish_recovery(&key(2001), &r.input.id)
            .is_err()
    );
    f.apply(&close);
    Journal::new(&mut f.client)
        .unwrap()
        .finish_recovery(&key(2001), &r.input.id)
        .unwrap();
    Journal::new(&mut f.client).unwrap().retire(&i.id).unwrap();
    let new = f.admission();
    f.apply(&new);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .recovered(&i.id)
            .unwrap()
    );
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .get(&i.id)
            .unwrap()
            .cleanup()
            .is_none()
    );
    f.client
        .connection
        .execute(
            "UPDATE namespace_intents SET state_hash=zeroblob(32) WHERE operation_id=?",
            [close.request.operation],
        )
        .unwrap();
    assert!(Journal::new(&mut f.client).unwrap().get(&i.id).is_err());
}
#[test]
fn successor_fence_and_unresolved_callback_work_cannot_be_abandoned_by_rebasing() {
    let mut f = Fixture::new();
    start(&mut f, 2000);
    let i = make(&mut f, 1000, true, true);
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3000), Callback::SetDelete { delete: false })
        .unwrap();
    f.reopen();
    start(&mut f, 2001);
    let close = cancel(&mut f);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .prepare_recovery(&key(2001), &f.volume, &i.node, Some(close.clone()))
            .is_err()
    );
    let mut j = Journal::new(&mut f.client).unwrap();
    j.reply(&i.id, &key(3001), &key(3000), None).unwrap();
    j.complete(&i.id, &key(3002), &key(3000)).unwrap();
    let r = j
        .prepare_recovery(&key(2001), &f.volume, &i.node, Some(close.clone()))
        .unwrap();
    f.apply(&close);
    let new = f.admission();
    f.apply(&new);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .finish_recovery(&key(2001), &r.input.id)
            .is_err()
    );
    assert!(
        !Journal::new(&mut f.client)
            .unwrap()
            .recovered(&i.id)
            .unwrap()
    );
    assert_eq!(
        removals::owned(&f.client.connection, &f.volume, &i.node)
            .unwrap()
            .unwrap()
            .fence
            .intent,
        new.request.operation
    );
}
#[test]
fn sqlite_full_abandonment_proof_preserves_frozen_request_and_can_retry_after_reopen() {
    let mut f = Fixture::new();
    start(&mut f, 2000);
    let i = make(&mut f, 1000, true, true);
    f.reopen();
    start(&mut f, 2001);
    let close = cancel(&mut f);
    let r = Journal::new(&mut f.client)
        .unwrap()
        .prepare_recovery(&key(2001), &f.volume, &i.node, Some(close.clone()))
        .unwrap();
    f.apply(&close);
    f.client
        .connection
        .execute_batch("CREATE TABLE session_full(data BLOB);PRAGMA wal_checkpoint(TRUNCATE)")
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
    f.client.connection.execute_batch("CREATE TRIGGER full_session BEFORE UPDATE ON windows_session_recoveries BEGIN INSERT INTO session_full VALUES(zeroblob(1048576));END").unwrap();
    let e = Journal::new(&mut f.client)
        .unwrap()
        .finish_recovery(&key(2001), &r.input.id)
        .err()
        .unwrap();
    assert!(
        matches!(e.downcast_ref::<rusqlite::Error>(),Some(rusqlite::Error::SqliteFailure(e,_)) if e.code==rusqlite::ErrorCode::DiskFull)
    );
    assert!(
        !Journal::new(&mut f.client)
            .unwrap()
            .recovered(&i.id)
            .unwrap()
    );
    f.client
        .connection
        .pragma_update(None, "max_page_count", pages + 1024)
        .unwrap();
    f.client
        .connection
        .execute_batch("DROP TRIGGER full_session")
        .unwrap();
    f.reopen();
    start(&mut f, 2002);
    assert_eq!(
        Journal::new(&mut f.client)
            .unwrap()
            .pending_recovery(&f.volume, &i.node)
            .unwrap()
            .unwrap()
            .input,
        r.input
    );
    Journal::new(&mut f.client)
        .unwrap()
        .finish_recovery(&key(2002), &r.input.id)
        .unwrap();
    Journal::new(&mut f.client).unwrap().retire(&i.id).unwrap();
}
#[test]
fn ignored_session_recovery_and_index_writes_roll_back_without_false_abandonment() {
    for target in ["windows_session_recoveries", "recovered_windows_opens"] {
        let mut f = Fixture::new();
        start(&mut f, 2000);
        let i = make(&mut f, 1000, false, true);
        f.reopen();
        start(&mut f, 2001);
        f.client.connection.execute_batch(&format!("CREATE TRIGGER fail_session BEFORE INSERT ON {target} BEGIN SELECT RAISE(IGNORE);END")).unwrap();
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .prepare_recovery(&key(2001), &f.volume, &i.node, None)
                .is_err()
        );
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .pending_recovery(&f.volume, &i.node)
                .unwrap()
                .is_none()
        );
        f.client
            .connection
            .execute_batch("DROP TRIGGER fail_session")
            .unwrap();
        let r = Journal::new(&mut f.client)
            .unwrap()
            .prepare_recovery(&key(2001), &f.volume, &i.node, None)
            .unwrap();
        f.client.connection.execute_batch("CREATE TRIGGER fail_session BEFORE UPDATE ON windows_session_recoveries BEGIN SELECT RAISE(IGNORE);END").unwrap();
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .finish_recovery(&key(2001), &r.input.id)
                .is_err()
        );
        assert!(
            !Journal::new(&mut f.client)
                .unwrap()
                .recovered(&i.id)
                .unwrap()
        );
        f.client
            .connection
            .execute_batch("DROP TRIGGER fail_session;DELETE FROM recovered_windows_opens")
            .unwrap();
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .pending_recovery(&f.volume, &i.node)
                .is_err()
        );
        assert!(Journal::new(&mut f.client).unwrap().records().is_err());
    }
}
#[test]
fn failed_and_unknown_admission_are_distinct_and_neither_fabricates_activation() {
    let mut f = Fixture::new();
    start(&mut f, 2000);
    let a = f.admission();
    let mut i = f.open(1000, Some(a.clone()));
    i.controlled = true;
    i.session_bound = true;
    Journal::new(&mut f.client)
        .unwrap()
        .prepare(i.clone())
        .unwrap();
    f.authority
        .namespace_apply(&Apply {
            volume: f.volume.clone(),
            operation: key(9000),
            expected_sequence: f.authority.namespace_volume().unwrap().sequence,
            mutation: Mutation::Move {
                node: i.node.clone(),
                parent: ROOT.into(),
                name: "changed before admission".into(),
            },
        })
        .unwrap();
    f.reopen();
    start(&mut f, 2001);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .prepare_recovery(&key(2001), &f.volume, &i.node, None)
            .is_err()
    );
    super::super::super::Journal::new(&mut f.client)
        .unwrap()
        .begin(a.clone())
        .unwrap();
    let outcome = f.authority.namespace_apply(&a.request);
    assert!(outcome.is_err());
    super::super::super::Journal::new(&mut f.client)
        .unwrap()
        .acknowledge(&a.request.operation, outcome)
        .unwrap();
    let r = Journal::new(&mut f.client)
        .unwrap()
        .prepare_recovery(&key(2001), &f.volume, &i.node, None)
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .finish_recovery(&key(2001), &r.input.id)
        .unwrap();
    Journal::new(&mut f.client).unwrap().retire(&i.id).unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .get(&i.id)
            .unwrap()
            .events
            .is_empty()
    );
}
#[test]
fn schema_nineteen_migration_preserves_hashes_and_session_table_conflict_is_non_destructive() {
    let mut f = Fixture::new();
    let i = f.active();
    let old = load(&f.client.connection, &i.id).unwrap().unwrap().0;
    f.client.connection.execute_batch("DROP TABLE recovered_windows_opens;DROP TABLE windows_session_recoveries;DROP TABLE windows_mount_sessions;PRAGMA user_version=19").unwrap();
    f.reopen();
    assert_eq!(f.client.info().unwrap().schema_version, 20);
    assert!(load(&f.client.connection, &i.id).unwrap().unwrap().0 == old);
    assert!(!serde_json::to_string(&i).unwrap().contains("session_bound"));
    f.client.connection.execute_batch("DROP TABLE recovered_windows_opens;DROP TABLE windows_session_recoveries;DROP TABLE windows_mount_sessions;CREATE TABLE windows_mount_sessions(keep TEXT);INSERT INTO windows_mount_sessions VALUES('untouched');PRAGMA user_version=19").unwrap();
    let path = f.tmp.path().join("client");
    drop(f.client);
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value::<i32, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        19
    );
    assert_eq!(
        db.query_row::<String, _, _>("SELECT keep FROM windows_mount_sessions", [], |r| r.get(0))
            .unwrap(),
        "untouched"
    );
}
#[test]
fn obsolete_full_open_quota_can_be_retained_without_synthesizing_closing_callbacks() {
    let mut f = Fixture::new();
    start(&mut f, 2000);
    for n in 0..OPEN_LIMIT {
        make(&mut f, 10000 + n as u64, false, true);
    }
    let mut extra = f.open(99999, None);
    extra.session_bound = true;
    extra.controlled = true;
    assert!(Journal::new(&mut f.client).unwrap().prepare(extra).is_err());
    f.reopen();
    start(&mut f, 2001);
    let r = Journal::new(&mut f.client)
        .unwrap()
        .prepare_recovery(&key(2001), &f.volume, &f.node.id, None)
        .unwrap();
    assert_eq!(r.input.opens.len(), OPEN_LIMIT);
    Journal::new(&mut f.client)
        .unwrap()
        .finish_recovery(&key(2001), &r.input.id)
        .unwrap();
    for o in &r.input.opens {
        Journal::new(&mut f.client).unwrap().retire(&o.id).unwrap();
    }
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .records()
            .unwrap()
            .is_empty()
    );
}
