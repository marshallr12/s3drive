use super::*;
fn active(f: &mut Fixture, doc: bool) -> OpenInput {
    let admission = doc.then(|| f.admission());
    let mut input = f.open(1000, admission.clone());
    input.controlled = true;
    Journal::new(&mut f.client)
        .unwrap()
        .prepare(input.clone())
        .unwrap();
    if let Some(a) = admission {
        f.apply(&a);
    }
    Journal::new(&mut f.client)
        .unwrap()
        .activate(&input.id, &key(3000))
        .unwrap();
    input
}
fn change(f: &mut Fixture, mode: RemovalMode) -> Input {
    let o = removals::owned(&f.client.connection, &f.volume, &f.node.id)
        .unwrap()
        .unwrap();
    f.input(Mutation::TransitionRemoval {
        node: namespace::NodeGuard {
            id: f.node.id.clone(),
            revision: f.node.revision,
        },
        parent: namespace::NodeGuard {
            id: ROOT.into(),
            revision: 0,
        },
        fence: o.fence,
        expected: o.mode,
        mode,
    })
}
fn apply_link(f: &mut Fixture, i: &OpenInput, cb: u64, link: u64, ack: u64, input: &Input) {
    Journal::new(&mut f.client)
        .unwrap()
        .link(&i.id, &key(link), &key(cb), input.clone())
        .unwrap();
    f.apply(input);
    Journal::new(&mut f.client)
        .unwrap()
        .acknowledge(&i.id, &key(ack), &input.request.operation)
        .unwrap();
}
#[test]
fn accepted_true_false_responses_require_exact_pending_or_deferred_proof() {
    let mut f = Fixture::new();
    let i = active(&mut f, true);
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3100), Callback::SetDelete { delete: true })
        .unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .reply(&i.id, &key(3400), &key(3100), None)
            .is_err()
    );
    let t = change(&mut f, RemovalMode::Pending);
    apply_link(&mut f, &i, 3100, 3200, 3300, &t);
    let r = Journal::new(&mut f.client)
        .unwrap()
        .reply(&i.id, &key(3400), &key(3100), None)
        .unwrap();
    assert_eq!(r.reply(&key(3100)), Some(None));
    Journal::new(&mut f.client)
        .unwrap()
        .complete(&i.id, &key(3500), &key(3100))
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3101), Callback::SetDelete { delete: false })
        .unwrap();
    let t = change(&mut f, RemovalMode::Deferred);
    apply_link(&mut f, &i, 3101, 3201, 3301, &t);
    Journal::new(&mut f.client)
        .unwrap()
        .reply(&i.id, &key(3401), &key(3101), None)
        .unwrap();
    let r = Journal::new(&mut f.client)
        .unwrap()
        .complete(&i.id, &key(3501), &key(3101))
        .unwrap();
    f.reopen();
    assert_eq!(Journal::new(&mut f.client).unwrap().get(&i.id).unwrap(), r);
}
#[test]
fn rejected_unknown_transition_cannot_complete_until_same_fence_mode_is_restored() {
    let mut f = Fixture::new();
    let i = active(&mut f, true);
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3100), Callback::SetDelete { delete: true })
        .unwrap();
    let t = change(&mut f, RemovalMode::Pending);
    Journal::new(&mut f.client)
        .unwrap()
        .link(&i.id, &key(3200), &key(3100), t.clone())
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .reply(
            &i.id,
            &key(3400),
            &key(3100),
            Some(namespace::Error::Unavailable),
        )
        .unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .complete(&i.id, &key(3500), &key(3100))
            .is_err()
    );
    f.apply(&t);
    Journal::new(&mut f.client)
        .unwrap()
        .acknowledge(&i.id, &key(3300), &t.request.operation)
        .unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .complete(&i.id, &key(3500), &key(3100))
            .is_err()
    );
    let restore = change(&mut f, RemovalMode::Deferred);
    apply_link(&mut f, &i, 3100, 3201, 3301, &restore);
    let r = Journal::new(&mut f.client)
        .unwrap()
        .complete(&i.id, &key(3500), &key(3100))
        .unwrap();
    assert!(r.completed(&key(3100)));
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .reply(&i.id, &key(3400), &key(3100), None)
            .is_err()
    );
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .reply(
                &i.id,
                &key(3400),
                &key(3100),
                Some(namespace::Error::NotEmpty)
            )
            .is_err()
    );
    f.reopen();
    assert_eq!(Journal::new(&mut f.client).unwrap().get(&i.id).unwrap(), r);
}
#[test]
fn a_new_epoch_is_not_a_compensating_capability_for_an_old_rejected_callback() {
    let mut f = Fixture::new();
    let i = active(&mut f, true);
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3100), Callback::SetDelete { delete: true })
        .unwrap();
    let t = change(&mut f, RemovalMode::Pending);
    apply_link(&mut f, &i, 3100, 3200, 3300, &t);
    Journal::new(&mut f.client)
        .unwrap()
        .reply(
            &i.id,
            &key(3400),
            &key(3100),
            Some(namespace::Error::Unavailable),
        )
        .unwrap();
    let old = removals::owned(&f.client.connection, &f.volume, &f.node.id)
        .unwrap()
        .unwrap();
    let cancel = f.input(Mutation::CancelRemoval {
        node: f.node.id.clone(),
        fence: old.fence,
    });
    f.apply(&cancel);
    let admission = f.admission();
    f.apply(&admission);
    let p = change(&mut f, RemovalMode::Pending);
    f.apply(&p);
    let restore = change(&mut f, RemovalMode::Deferred);
    apply_link(&mut f, &i, 3100, 3201, 3301, &restore);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .complete(&i.id, &key(3500), &key(3100))
            .is_err()
    );
}
#[test]
fn accepted_reply_and_completion_sql_failure_do_not_change_namespace_or_fabricate_progress() {
    for action in ["reply", "completed"] {
        let mut f = Fixture::new();
        let i = active(&mut f, true);
        Journal::new(&mut f.client)
            .unwrap()
            .callback(&i.id, &key(3100), Callback::SetDelete { delete: true })
            .unwrap();
        let t = change(&mut f, RemovalMode::Pending);
        apply_link(&mut f, &i, 3100, 3200, 3300, &t);
        if action == "completed" {
            Journal::new(&mut f.client)
                .unwrap()
                .reply(&i.id, &key(3400), &key(3100), None)
                .unwrap();
        }
        let before = Journal::new(&mut f.client).unwrap().get(&i.id).unwrap();
        f.client.connection.execute_batch("CREATE TRIGGER fail BEFORE UPDATE ON windows_open_journal BEGIN SELECT RAISE(IGNORE); END").unwrap();
        let result = if action == "reply" {
            Journal::new(&mut f.client)
                .unwrap()
                .reply(&i.id, &key(3400), &key(3100), None)
        } else {
            Journal::new(&mut f.client)
                .unwrap()
                .complete(&i.id, &key(3500), &key(3100))
        };
        assert!(result.is_err());
        assert_eq!(
            Journal::new(&mut f.client).unwrap().get(&i.id).unwrap(),
            before
        );
        assert_eq!(
            removals::owned(&f.client.connection, &f.volume, &f.node.id)
                .unwrap()
                .unwrap()
                .mode,
            RemovalMode::Pending
        );
        f.client
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        Journal::new(&mut f.client)
            .unwrap()
            .reply(&i.id, &key(3400), &key(3100), None)
            .unwrap();
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .complete(&i.id, &key(3500), &key(3100))
                .unwrap()
                .completed(&key(3100))
        );
    }
}
#[test]
fn full_disk_reply_retains_unknown_response_and_exact_mode_receipt() {
    let mut f = Fixture::new();
    let i = active(&mut f, true);
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3100), Callback::SetDelete { delete: true })
        .unwrap();
    let t = change(&mut f, RemovalMode::Pending);
    apply_link(&mut f, &i, 3100, 3200, 3300, &t);
    let old = Journal::new(&mut f.client).unwrap().get(&i.id).unwrap();
    f.client
        .connection
        .execute_batch("CREATE TABLE controller_full(data BLOB);PRAGMA wal_checkpoint(TRUNCATE)")
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
    f.client.connection.execute_batch("CREATE TRIGGER fail BEFORE INSERT ON windows_open_events BEGIN INSERT INTO controller_full VALUES(zeroblob(1048576)); END").unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .reply(&i.id, &key(3400), &key(3100), None)
            .is_err()
    );
    assert_eq!(
        Journal::new(&mut f.client).unwrap().get(&i.id).unwrap(),
        old
    );
    f.client
        .connection
        .pragma_update(None, "max_page_count", pages + 1024)
        .unwrap();
    f.client
        .connection
        .execute_batch("DROP TRIGGER fail")
        .unwrap();
    f.reopen();
    Journal::new(&mut f.client)
        .unwrap()
        .reply(&i.id, &key(3400), &key(3100), None)
        .unwrap();
}
fn fill_completed_noops(f: &mut Fixture, open: &str, triples: usize) {
    let (mut row, mut record) = load(&f.client.connection, open).unwrap().unwrap();
    let tx = f.client.connection.transaction().unwrap();
    for n in 0..triples {
        let cb = key(100_000 + n as u64 * 3);
        let events = [
            Event {
                id: cb.clone(),
                action: Action::Callback(Callback::SetDelete { delete: false }),
            },
            Event {
                id: key(100_001 + n as u64 * 3),
                action: Action::Reply {
                    callback: cb.clone(),
                    error: None,
                    proof: None,
                },
            },
            Event {
                id: key(100_002 + n as u64 * 3),
                action: Action::Completed { callback: cb },
            },
        ];
        for e in events {
            let previous = row.head.clone();
            let h = event_hash(open, row.count + 1, &e, &previous).unwrap();
            reduce(&tx, &mut record, &e).unwrap();
            row.count += 1;
            row.head = Some(h.clone());
            tx.execute(
                "INSERT INTO windows_open_events VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    e.id,
                    open,
                    row.count as i64,
                    serde_json::to_vec(&e.action).unwrap(),
                    previous,
                    h
                ],
            )
            .unwrap();
        }
    }
    tx.execute("UPDATE windows_open_journal SET event_count=?1,head_hash=?2,state_hash=?3 WHERE open_id=?4",params![row.count as i64,row.head,checksum(&row).unwrap(),open]).unwrap();
    tx.commit().unwrap();
}
#[test]
fn controlled_quota_keeps_response_compensation_cleanup_and_final_release_capacity() {
    let mut f = Fixture::new();
    let i = active(&mut f, false);
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3100), Callback::SetDelete { delete: true })
        .unwrap();
    let owner = identity::read(&f.client.connection).unwrap().unwrap();
    let arm = f.input(Mutation::ArmRemoval {
        node: namespace::NodeGuard {
            id: f.node.id.clone(),
            revision: f.node.revision,
        },
        parent: namespace::NodeGuard {
            id: ROOT.into(),
            revision: 0,
        },
        owner,
    });
    Journal::new(&mut f.client)
        .unwrap()
        .link(&i.id, &key(3200), &key(3100), arm.clone())
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .reply(
            &i.id,
            &key(3400),
            &key(3100),
            Some(namespace::Error::Unavailable),
        )
        .unwrap();
    fill_completed_noops(&mut f, &i.id, 1359);
    capacity(&f.client.connection, &all(&f.client.connection).unwrap()).unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .callback(&i.id, &key(3101), Callback::SetDelete { delete: false })
            .is_err()
    );
    f.apply(&arm);
    Journal::new(&mut f.client)
        .unwrap()
        .acknowledge(&i.id, &key(3300), &arm.request.operation)
        .unwrap();
    let restore = change(&mut f, RemovalMode::Deferred);
    apply_link(&mut f, &i, 3100, 3201, 3301, &restore);
    Journal::new(&mut f.client)
        .unwrap()
        .complete(&i.id, &key(3500), &key(3100))
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3600), Callback::Cleanup { flags: 0 })
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .complete(&i.id, &key(3601), &key(3600))
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3700), Callback::Close)
        .unwrap();
    let owned = removals::owned(&f.client.connection, &f.volume, &f.node.id)
        .unwrap()
        .unwrap();
    let cancel = f.input(Mutation::CancelRemoval {
        node: f.node.id.clone(),
        fence: owned.fence,
    });
    apply_link(&mut f, &i, 3700, 3701, 3702, &cancel);
    let r = Journal::new(&mut f.client)
        .unwrap()
        .complete(&i.id, &key(3703), &key(3700))
        .unwrap();
    assert!(r.pending().is_empty());
    capacity(&f.client.connection, &all(&f.client.connection).unwrap()).unwrap();
}
#[test]
fn schema_seventeen_evidence_hashes_are_preserved_without_opting_into_controller_policy() {
    let mut f = Fixture::new();
    let i = f.active();
    let old = Journal::new(&mut f.client).unwrap().get(&i.id).unwrap();
    let raw = f
        .client
        .connection
        .query_row::<Vec<u8>, _, _>(
            "SELECT state_hash FROM windows_open_journal WHERE open_id=?",
            [&i.id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        !String::from_utf8(serde_json::to_vec(&i).unwrap())
            .unwrap()
            .contains("controlled")
    );
    f.client
        .connection
        .execute_batch(&crate::test_schema::legacy_sql("DROP TABLE chunk_source_settlements; DROP TABLE chunk_source_role; DROP TABLE chunk_publications; DROP TABLE chunk_client_role; DROP TABLE chunk_objects; DROP TABLE chunk_plans; DROP TABLE multipart_parts; DROP TABLE multipart_plans; DROP TABLE recovered_windows_opens; DROP TABLE windows_session_recoveries; DROP TABLE windows_mount_sessions; DROP TABLE retired_windows_opens"))
        .unwrap();
    crate::test_schema::demote(&f.client.connection, 17).unwrap();
    f.reopen();
    assert_eq!(
        f.client.info().unwrap().schema_version,
        crate::SCHEMA_VERSION
    );
    assert_eq!(
        Journal::new(&mut f.client).unwrap().get(&i.id).unwrap(),
        old
    );
    assert_eq!(
        f.client
            .connection
            .query_row::<Vec<u8>, _, _>(
                "SELECT state_hash FROM windows_open_journal WHERE open_id=?",
                [&i.id],
                |r| r.get(0)
            )
            .unwrap(),
        raw
    );
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .reply(&i.id, &key(3400), &key(3100), None)
            .is_err()
    );
}
#[test]
fn close_completion_requires_cleanup_evidence_and_disposition_completion_requires_a_reply() {
    let mut f = Fixture::new();
    let i = active(&mut f, false);
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3100), Callback::SetDelete { delete: false })
        .unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .complete(&i.id, &key(3500), &key(3100))
            .is_err()
    );
    Journal::new(&mut f.client)
        .unwrap()
        .reply(&i.id, &key(3400), &key(3100), None)
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .complete(&i.id, &key(3500), &key(3100))
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3700), Callback::Close)
        .unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .complete(&i.id, &key(3703), &key(3700))
            .is_err()
    );
}
