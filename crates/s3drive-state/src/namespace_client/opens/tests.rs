use super::*;
fn key(n: u64) -> String {
    format!("{n:032x}")
}
struct Fixture {
    tmp: tempfile::TempDir,
    client: State,
    authority: State,
    volume: String,
    store: Store,
    node: namespace::Node,
    op: u64,
}
impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let mut authority = State::open_managed(&tmp.path().join("authority")).unwrap();
        let volume = authority
            .namespace_initialize_with_policy(s3drive_core::names::PORTABLE)
            .unwrap()
            .id;
        let r = authority
            .namespace_apply(&Apply {
                volume: volume.clone(),
                operation: key(200),
                expected_sequence: 0,
                mutation: Mutation::RegisterStore {
                    route: "a".repeat(64),
                    root: "internal/".into(),
                },
            })
            .unwrap();
        let Some(namespace::Artifact::Store(store)) = r.artifact else {
            panic!()
        };
        let node = authority
            .namespace_apply(&Apply {
                volume: volume.clone(),
                operation: key(10),
                expected_sequence: r.sequence,
                mutation: Mutation::CreateEntry {
                    parent: ROOT.into(),
                    parent_revision: 0,
                    store: Some(store.id.clone()),
                    name: "open café 日本語 🦀".into(),
                    node_kind: namespace::Kind::EmptyFile,
                },
            })
            .unwrap()
            .node;
        let client = State::open_managed(&tmp.path().join("client")).unwrap();
        Self {
            tmp,
            client,
            authority,
            volume,
            store,
            node,
            op: 100,
        }
    }
    fn admission(&mut self) -> Input {
        let owner = super::super::Journal::new(&mut self.client)
            .unwrap()
            .client_identity()
            .unwrap();
        self.input(Mutation::PrepareRemoval {
            node: namespace::NodeGuard {
                id: self.node.id.clone(),
                revision: self.node.revision,
            },
            parent: namespace::NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            owner,
        })
    }
    fn input(&mut self, m: Mutation) -> Input {
        self.op += 1;
        Input {
            request: Apply {
                volume: self.volume.clone(),
                operation: key(self.op),
                expected_sequence: self.authority.namespace_volume().unwrap().sequence,
                mutation: m,
            },
            store: self.store.clone(),
            source: [1; 32],
            fingerprint: "b".repeat(64),
            entries: vec![Entry::from_node(&self.node).unwrap()],
        }
    }
    fn open(&self, n: u64, admission: Option<Input>) -> OpenInput {
        OpenInput {
            id: key(n),
            session: key(2000),
            volume: self.volume.clone(),
            node: self.node.id.clone(),
            store: self.store.clone(),
            source: [1; 32],
            fingerprint: "b".repeat(64),
            delete_on_close: admission.is_some(),
            admission: admission.map(Box::new),
        }
    }
    fn apply(&mut self, i: &Input) -> namespace::Receipt {
        super::super::Journal::new(&mut self.client)
            .unwrap()
            .begin(i.clone())
            .unwrap();
        let r = self.authority.namespace_apply(&i.request).unwrap();
        super::super::Journal::new(&mut self.client)
            .unwrap()
            .acknowledge(&i.request.operation, Ok(r.clone()))
            .unwrap();
        self.node = r.node.clone();
        r
    }
    fn reopen(&mut self) {
        let p = State::open_managed(&self.tmp.path().join("placeholder")).unwrap();
        drop(std::mem::replace(&mut self.client, p));
        self.client = State::open_managed(&self.tmp.path().join("client")).unwrap();
    }
    fn active(&mut self) -> OpenInput {
        let i = self.open(1000, None);
        Journal::new(&mut self.client)
            .unwrap()
            .prepare(i.clone())
            .unwrap();
        Journal::new(&mut self.client)
            .unwrap()
            .activate(&i.id, &key(3000))
            .unwrap();
        i
    }
    fn transition(&mut self) -> Input {
        let o = removals::owned(&self.client.connection, &self.volume, &self.node.id)
            .unwrap()
            .unwrap();
        self.input(Mutation::TransitionRemoval {
            node: namespace::NodeGuard {
                id: self.node.id.clone(),
                revision: self.node.revision,
            },
            parent: namespace::NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            fence: o.fence,
            expected: o.mode,
            mode: RemovalMode::Pending,
        })
    }
}
#[test]
fn prepared_open_freezes_exact_admission_before_rpc_and_activates_only_after_same_origin_ack() {
    let mut f = Fixture::new();
    let admission = f.admission();
    let i = f.open(1000, Some(admission.clone()));
    let prepared = Journal::new(&mut f.client)
        .unwrap()
        .prepare(i.clone())
        .unwrap();
    assert!(!prepared.active());
    assert!(
        super::super::load(&f.client.connection, &admission.request.operation)
            .unwrap()
            .is_none()
    );
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .activate(&i.id, &key(3000))
            .is_err()
    );
    f.reopen();
    assert_eq!(
        Journal::new(&mut f.client)
            .unwrap()
            .prepare(i.clone())
            .unwrap(),
        prepared
    );
    f.apply(&admission);
    let r = Journal::new(&mut f.client)
        .unwrap()
        .activate(&i.id, &key(3000))
        .unwrap();
    assert!(r.active());
    assert_eq!(
        Journal::new(&mut f.client)
            .unwrap()
            .activate(&i.id, &key(3000))
            .unwrap(),
        r
    );
    let mut changed = i.clone();
    changed.session = key(2001);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .prepare(changed)
            .is_err()
    );
    let o = removals::owned(&f.client.connection, &f.volume, &i.node)
        .unwrap()
        .unwrap();
    let cancel = f.input(Mutation::CancelRemoval {
        node: i.node.clone(),
        fence: o.fence,
    });
    f.apply(&cancel);
    let next = f.admission();
    f.apply(&next);
    let mut old = i.clone();
    old.id = key(1001);
    Journal::new(&mut f.client)
        .unwrap()
        .prepare(old.clone())
        .unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .activate(&old.id, &key(3001))
            .is_err()
    );
}
#[test]
fn cleanup_close_and_multiple_file_objects_are_facts_not_ored_disposition() {
    let mut f = Fixture::new();
    let a = f.active();
    let mut b = f.open(1001, None);
    b.delete_on_close = false;
    Journal::new(&mut f.client)
        .unwrap()
        .prepare(b.clone())
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .activate(&b.id, &key(3001))
        .unwrap();
    let a1 = Journal::new(&mut f.client)
        .unwrap()
        .callback(&a.id, &key(3100), Callback::SetDelete { delete: true })
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&b.id, &key(3101), Callback::SetDelete { delete: false })
        .unwrap();
    assert!(
        removals::owned(&f.client.connection, &f.volume, &a.node)
            .unwrap()
            .is_none()
    );
    let c = Journal::new(&mut f.client)
        .unwrap()
        .callback(&a.id, &key(3200), Callback::Cleanup { flags: 0x81 })
        .unwrap();
    assert!(c.cleanup().is_some());
    assert!(c.close().is_none());
    assert_eq!(a1.events.len() + 1, c.events.len());
    assert_eq!(
        Journal::new(&mut f.client)
            .unwrap()
            .callback(&a.id, &key(3200), Callback::Cleanup { flags: 0x81 })
            .unwrap(),
        c
    );
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .callback(&a.id, &key(3200), Callback::Cleanup { flags: 1 })
            .is_err()
    );
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&a.id, &key(3300), Callback::Close)
        .unwrap();
    f.reopen();
    let old = Journal::new(&mut f.client)
        .unwrap()
        .abandoned(&key(2001))
        .unwrap();
    assert_eq!(old.len(), 1);
    assert_eq!(old[0].input, b);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .callback(&a.id, &key(3400), Callback::SetDelete { delete: true })
            .is_err()
    );
}
#[test]
fn exact_link_and_namespace_ack_survive_close_and_restart_without_inventing_outcomes() {
    let mut f = Fixture::new();
    let admission = f.admission();
    let i = f.open(1000, Some(admission.clone()));
    Journal::new(&mut f.client)
        .unwrap()
        .prepare(i.clone())
        .unwrap();
    f.apply(&admission);
    Journal::new(&mut f.client)
        .unwrap()
        .activate(&i.id, &key(3000))
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3100), Callback::Cleanup { flags: 1 })
        .unwrap();
    let change = f.transition();
    Journal::new(&mut f.client)
        .unwrap()
        .link(&i.id, &key(3200), &key(3100), change.clone())
        .unwrap();
    assert!(
        super::super::load(&f.client.connection, &change.request.operation)
            .unwrap()
            .is_none()
    );
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .acknowledge(&i.id, &key(3400), &change.request.operation)
            .is_err()
    );
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3300), Callback::Close)
        .unwrap();
    f.reopen();
    let r = Journal::new(&mut f.client).unwrap().get(&i.id).unwrap();
    assert!(r.close().is_some());
    assert_eq!(r.pending(), vec![&change]);
    f.apply(&change);
    let r = Journal::new(&mut f.client)
        .unwrap()
        .acknowledge(&i.id, &key(3400), &change.request.operation)
        .unwrap();
    assert!(r.pending().is_empty());
    assert_eq!(
        Journal::new(&mut f.client)
            .unwrap()
            .acknowledge(&i.id, &key(3400), &change.request.operation)
            .unwrap(),
        r
    );
    f.reopen();
    assert_eq!(Journal::new(&mut f.client).unwrap().get(&i.id).unwrap(), r);
}
#[test]
fn ignored_preparation_or_event_writes_leave_no_half_open_or_false_callback() {
    for table in ["windows_open_journal", "retained_windows_opens"] {
        let mut f = Fixture::new();
        let i = f.open(1000, None);
        f.client
            .connection
            .execute_batch(&format!(
                "CREATE TRIGGER fail BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE); END"
            ))
            .unwrap();
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .prepare(i.clone())
                .is_err()
        );
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .records()
                .unwrap()
                .is_empty()
        );
    }
    for (table, action) in [
        ("windows_open_events", "INSERT"),
        ("windows_open_journal", "UPDATE"),
    ] {
        let mut f = Fixture::new();
        let i = f.active();
        let old = Journal::new(&mut f.client).unwrap().get(&i.id).unwrap();
        f.client
            .connection
            .execute_batch(&format!(
                "CREATE TRIGGER fail BEFORE {action} ON {table} BEGIN SELECT RAISE(IGNORE); END"
            ))
            .unwrap();
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .callback(&i.id, &key(3100), Callback::Cleanup { flags: 1 })
                .is_err()
        );
        assert_eq!(
            Journal::new(&mut f.client).unwrap().get(&i.id).unwrap(),
            old
        );
    }
}
#[test]
fn actual_sqlite_full_preserves_cleanup_and_exact_event_retry() {
    let mut f = Fixture::new();
    let i = f.active();
    let old = Journal::new(&mut f.client).unwrap().get(&i.id).unwrap();
    f.client
        .connection
        .execute_batch("CREATE TABLE open_full(data BLOB); PRAGMA wal_checkpoint(TRUNCATE)")
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
    f.client.connection.execute_batch("CREATE TRIGGER fail BEFORE INSERT ON windows_open_events BEGIN INSERT INTO open_full VALUES(zeroblob(1048576)); END").unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .callback(&i.id, &key(3100), Callback::Cleanup { flags: 1 })
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
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .callback(&i.id, &key(3100), Callback::Cleanup { flags: 1 })
            .unwrap()
            .cleanup()
            .is_some()
    );
}
#[test]
fn missing_rows_chain_events_or_identity_cannot_be_recreated_or_adopted() {
    for case in 0..5 {
        let mut f = Fixture::new();
        let i = f.active();
        Journal::new(&mut f.client)
            .unwrap()
            .callback(&i.id, &key(3100), Callback::Cleanup { flags: 1 })
            .unwrap();
        match case {
            0 => {
                f.client
                    .connection
                    .execute_batch("DELETE FROM windows_open_events")
                    .unwrap();
            }
            1 => {
                f.client
                    .connection
                    .execute_batch("PRAGMA foreign_keys=OFF;DELETE FROM windows_open_journal")
                    .unwrap();
            }
            2 => {
                f.client
                    .connection
                    .execute_batch("UPDATE windows_open_events SET action=x'7b7d'")
                    .unwrap();
            }
            3 => {
                f.client
                    .connection
                    .execute_batch(
                        "DELETE FROM client_identity;DELETE FROM retained_client_identity",
                    )
                    .unwrap();
            }
            _ => {
                f.client.connection.execute_batch("UPDATE windows_open_journal SET owner_id='aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'").unwrap();
            }
        }
        assert!(Journal::new(&mut f.client).unwrap().records().is_err());
        assert!(Journal::new(&mut f.client).unwrap().prepare(i).is_err());
    }
}
#[test]
fn missing_cleanup_is_reported_without_fabrication_and_late_evidence_is_retained() {
    let mut f = Fixture::new();
    let i = f.active();
    let r = Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3100), Callback::Close)
        .unwrap();
    assert!(r.cleanup().is_none());
    assert!(r.close().is_some());
    f.reopen();
    let r = Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3200), Callback::Cleanup { flags: 1 })
        .unwrap();
    assert!(r.close().is_some());
    assert!(r.cleanup().is_some());
    assert!(matches!(
        r.events[1].action,
        Action::Callback(Callback::Close)
    ));
}
#[test]
fn malformed_effect_bindings_and_foreign_owners_do_not_enter_the_callback_chain() {
    let mut f = Fixture::new();
    let admission = f.admission();
    let i = f.open(1000, Some(admission.clone()));
    Journal::new(&mut f.client)
        .unwrap()
        .prepare(i.clone())
        .unwrap();
    f.apply(&admission);
    Journal::new(&mut f.client)
        .unwrap()
        .activate(&i.id, &key(3000))
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3100), Callback::SetDelete { delete: true })
        .unwrap();
    let input = f.transition();
    let before = Journal::new(&mut f.client).unwrap().get(&i.id).unwrap();
    for case in 0..5 {
        let mut bad = input.clone();
        match case {
            0 => bad.request.volume = key(999),
            1 => bad.fingerprint = "c".repeat(64),
            2 => bad.source = [2; 32],
            3 => {
                let Mutation::TransitionRemoval { fence, .. } = &mut bad.request.mutation else {
                    unreachable!()
                };
                fence.owner = key(999);
            }
            _ => bad.store.route = "d".repeat(64),
        }
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .link(&i.id, &key(3200), &key(3100), bad)
                .is_err()
        );
        assert_eq!(
            Journal::new(&mut f.client).unwrap().get(&i.id).unwrap(),
            before
        );
    }
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .link(&i.id, &key(3200), &key(999), input)
            .is_err()
    );
}
fn fill_events(f: &mut Fixture, key_id: &str, count: usize) {
    let (mut row, mut record) = load(&f.client.connection, key_id).unwrap().unwrap();
    let tx = f.client.connection.transaction().unwrap();
    while record.events.len() < count {
        let event = Event {
            id: key(100_000 + record.events.len() as u64),
            action: Action::Callback(Callback::SetDelete { delete: true }),
        };
        let previous = row.head.clone();
        let hash = event_hash(key_id, row.count + 1, &event, &previous).unwrap();
        reduce(&tx, &mut record, &event).unwrap();
        row.count += 1;
        row.head = Some(hash.clone());
        tx.execute(
            "INSERT INTO windows_open_events VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                event.id,
                key_id,
                row.count as i64,
                serde_json::to_vec(&event.action).unwrap(),
                previous,
                hash
            ],
        )
        .unwrap();
    }
    tx.execute("UPDATE windows_open_journal SET event_count=?1,head_hash=?2,state_hash=?3 WHERE open_id=?4",params![row.count as i64,row.head,checksum(&row).unwrap(),key_id]).unwrap();
    tx.commit().unwrap();
}
#[test]
fn ordinary_event_quota_cannot_consume_cleanup_close_or_linked_ack_slots() {
    let mut f = Fixture::new();
    let admission = f.admission();
    let i = f.open(1000, Some(admission.clone()));
    Journal::new(&mut f.client)
        .unwrap()
        .prepare(i.clone())
        .unwrap();
    f.apply(&admission);
    Journal::new(&mut f.client)
        .unwrap()
        .activate(&i.id, &key(3000))
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3100), Callback::SetDelete { delete: true })
        .unwrap();
    let change = f.transition();
    Journal::new(&mut f.client)
        .unwrap()
        .link(&i.id, &key(3200), &key(3100), change.clone())
        .unwrap();
    fill_events(&mut f, &i.id, EVENT_LIMIT - 3);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .callback(&i.id, &key(3400), Callback::SetDelete { delete: false })
            .is_err()
    );
    f.apply(&change);
    Journal::new(&mut f.client)
        .unwrap()
        .acknowledge(&i.id, &key(3500), &change.request.operation)
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3600), Callback::Cleanup { flags: 1 })
        .unwrap();
    let r = Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3700), Callback::Close)
        .unwrap();
    assert_eq!(r.events.len(), EVENT_LIMIT);
    assert!(r.pending().is_empty());
    f.reopen();
    assert_eq!(Journal::new(&mut f.client).unwrap().get(&i.id).unwrap(), r);
}
#[test]
fn schema_sixteen_migration_preserves_original_namespace_receipt_identity_and_source() {
    let mut f = Fixture::new();
    let admission = f.admission();
    f.apply(&admission);
    let old = super::super::Journal::new(&mut f.client)
        .unwrap()
        .get(&admission.request.operation)
        .unwrap();
    let owner = identity::read(&f.client.connection).unwrap();
    f.client.connection.execute_batch("DROP TABLE windows_open_events;DROP TABLE retained_windows_opens;DROP TABLE windows_open_journal;PRAGMA user_version=16").unwrap();
    f.reopen();
    assert_eq!(f.client.info().unwrap().schema_version, 17);
    assert_eq!(
        super::super::Journal::new(&mut f.client)
            .unwrap()
            .get(&admission.request.operation)
            .unwrap(),
        old
    );
    assert_eq!(identity::read(&f.client.connection).unwrap(), owner);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .records()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn conflicting_schema_seventeen_migration_is_non_destructive() {
    let mut f = Fixture::new();
    f.client.connection.execute_batch("DROP TABLE windows_open_events;DROP TABLE retained_windows_opens;DROP TABLE windows_open_journal;CREATE TABLE windows_open_journal(keep TEXT);INSERT INTO windows_open_journal VALUES('untouched');PRAGMA user_version=16").unwrap();
    let p = State::open_managed(&f.tmp.path().join("placeholder")).unwrap();
    drop(std::mem::replace(&mut f.client, p));
    assert!(State::open_managed(&f.tmp.path().join("client")).is_err());
    let db = Connection::open(f.tmp.path().join("client/metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        16
    );
    assert_eq!(
        db.query_row::<String, _, _>("SELECT keep FROM windows_open_journal", [], |r| r.get(0))
            .unwrap(),
        "untouched"
    );
}
#[test]
fn settled_effect_requires_unchanged_namespace_proof_on_every_reopen() {
    let mut f = Fixture::new();
    let admission = f.admission();
    let i = f.open(1000, Some(admission.clone()));
    Journal::new(&mut f.client)
        .unwrap()
        .prepare(i.clone())
        .unwrap();
    f.apply(&admission);
    Journal::new(&mut f.client)
        .unwrap()
        .activate(&i.id, &key(3000))
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&i.id, &key(3100), Callback::Cleanup { flags: 1 })
        .unwrap();
    let change = f.transition();
    Journal::new(&mut f.client)
        .unwrap()
        .link(&i.id, &key(3200), &key(3100), change.clone())
        .unwrap();
    f.apply(&change);
    Journal::new(&mut f.client)
        .unwrap()
        .acknowledge(&i.id, &key(3300), &change.request.operation)
        .unwrap();
    f.client
        .connection
        .execute(
            "UPDATE namespace_intents SET state_hash=zeroblob(32) WHERE operation_id=?",
            [&change.request.operation],
        )
        .unwrap();
    assert!(Journal::new(&mut f.client).unwrap().get(&i.id).is_err());
}
#[test]
fn independent_file_objects_share_one_unresolved_node_wide_effect_barrier() {
    let mut f = Fixture::new();
    let admission = f.admission();
    let a = f.open(1000, Some(admission.clone()));
    Journal::new(&mut f.client)
        .unwrap()
        .prepare(a.clone())
        .unwrap();
    f.apply(&admission);
    Journal::new(&mut f.client)
        .unwrap()
        .activate(&a.id, &key(3000))
        .unwrap();
    let b = f.open(1001, None);
    Journal::new(&mut f.client)
        .unwrap()
        .prepare(b.clone())
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .activate(&b.id, &key(3001))
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&a.id, &key(3100), Callback::SetDelete { delete: true })
        .unwrap();
    let change = f.transition();
    Journal::new(&mut f.client)
        .unwrap()
        .link(&a.id, &key(3200), &key(3100), change.clone())
        .unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .callback(&b.id, &key(3101), Callback::SetDelete { delete: false })
        .unwrap();
    let old = removals::owned(&f.client.connection, &f.volume, &f.node.id)
        .unwrap()
        .unwrap();
    let cancel = f.input(Mutation::CancelRemoval {
        node: f.node.id.clone(),
        fence: old.fence,
    });
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .link(&b.id, &key(3201), &key(3101), cancel)
            .is_err()
    );
    f.apply(&change);
    Journal::new(&mut f.client)
        .unwrap()
        .acknowledge(&a.id, &key(3300), &change.request.operation)
        .unwrap();
    let old = removals::owned(&f.client.connection, &f.volume, &f.node.id)
        .unwrap()
        .unwrap();
    let cancel = f.input(Mutation::CancelRemoval {
        node: f.node.id.clone(),
        fence: old.fence,
    });
    Journal::new(&mut f.client)
        .unwrap()
        .link(&b.id, &key(3201), &key(3101), cancel.clone())
        .unwrap();
    f.apply(&cancel);
    Journal::new(&mut f.client)
        .unwrap()
        .acknowledge(&b.id, &key(3301), &cancel.request.operation)
        .unwrap();
}
#[test]
fn authority_state_or_invalid_open_binding_cannot_mint_file_object_ownership() {
    let mut f = Fixture::new();
    assert!(Journal::new(&mut f.authority).is_err());
    let template = f.open(1000, None);
    for case in 0..4 {
        let mut i = template.clone();
        match case {
            0 => i.session = ROOT.into(),
            1 => i.node = ROOT.into(),
            2 => i.fingerprint = "invalid".into(),
            _ => i.delete_on_close = true,
        }
        assert!(Journal::new(&mut f.client).unwrap().prepare(i).is_err());
    }
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .records()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn activation_retains_exact_deferred_proof_and_refuses_a_damaged_namespace_anchor() {
    let mut f = Fixture::new();
    let admission = f.admission();
    let i = f.open(1000, Some(admission.clone()));
    Journal::new(&mut f.client)
        .unwrap()
        .prepare(i.clone())
        .unwrap();
    f.apply(&admission);
    let active = Journal::new(&mut f.client)
        .unwrap()
        .activate(&i.id, &key(3000))
        .unwrap();
    let Action::Activate { proof: Some(proof) } = &active.events[0].action else {
        panic!()
    };
    assert_eq!(proof.input.as_ref(), &admission);
    let transition = f.transition();
    f.apply(&transition);
    assert_eq!(
        Journal::new(&mut f.client)
            .unwrap()
            .activate(&i.id, &key(3000))
            .unwrap(),
        active
    );
    f.client
        .connection
        .execute(
            "UPDATE namespace_intents SET state_hash=zeroblob(32) WHERE operation_id=?",
            [&admission.request.operation],
        )
        .unwrap();
    assert!(Journal::new(&mut f.client).unwrap().get(&i.id).is_err());
}
