use super::*;
use namespace::{RemovalClaim, RemovalMode};
fn fixture(source_deferred: bool, mode: RemovalMode) -> (Fixture, Node, Node, Input) {
    let mut f = Fixture::new();
    let source = f.create("source 日本語 🦀");
    let target = f.create("target café 🦀");
    f.bind(&source);
    f.bind(&target);
    let source = if source_deferred {
        let i = deferred::prepare(&mut f, &source);
        apply(&mut f, &i).node
    } else {
        source
    };
    let i = if mode == RemovalMode::Deferred {
        deferred::prepare(&mut f, &target)
    } else {
        arm(&mut f, &target)
    };
    let target = apply(&mut f, &i).node;
    let old = owned(&mut f, &target).unwrap();
    let claim = Some(Box::new(RemovalClaim {
        fence: old.fence,
        previous: old.event,
        mode,
    }));
    let parent = NodeGuard {
        id: ROOT.into(),
        revision: 0,
    };
    let mutation = if source_deferred {
        let old = owned(&mut f, &source).unwrap();
        Mutation::MoveDeferredEntry {
            node: guard(&source),
            source_parent: parent.clone(),
            parent,
            name: target.name.clone(),
            destination: Some(guard(&target)),
            fence: Box::new(old.fence),
            previous: old.event.into(),
            destination_removal: claim,
        }
    } else {
        Mutation::MoveEntry {
            node: guard(&source),
            source_parent: parent.clone(),
            parent,
            name: target.name.clone(),
            destination: Some(guard(&target)),
            destination_removal: claim,
        }
    };
    let input = f.input(mutation, &[source.clone(), target.clone()]);
    (f, source, target, input)
}
fn paired_fixture() -> (Fixture, Node, Input) {
    let mut f = Fixture::new();
    let source = f.create("paired source");
    let request = f.request(Mutation::RegisterStore {
        route: "c".repeat(64),
        root: "other/".into(),
    });
    let receipt = f.authority.namespace_apply(&request).unwrap();
    let Some(Artifact::Store(other)) = receipt.artifact else {
        unreachable!()
    };
    let request = f.request(Mutation::CreateEntry {
        parent: ROOT.into(),
        parent_revision: 0,
        store: Some(other.id.clone()),
        name: "paired target".into(),
        node_kind: Kind::EmptyFile,
    });
    let target = f.authority.namespace_apply(&request).unwrap().node;
    let mut origin = deferred::prepare(&mut f, &target);
    origin.store = other;
    origin.source = [2; 32];
    origin.fingerprint = "d".repeat(64);
    let target = apply(&mut f, &origin).node;
    let old = owned(&mut f, &target).unwrap();
    let parent = NodeGuard {
        id: ROOT.into(),
        revision: 0,
    };
    let mut input = f.input(
        Mutation::MoveEntry {
            node: guard(&source),
            source_parent: parent.clone(),
            parent,
            name: target.name.clone(),
            destination: Some(guard(&target)),
            destination_removal: Some(Box::new(RemovalClaim {
                fence: old.fence,
                previous: old.event,
                mode: old.mode,
            })),
        },
        &[source, target.clone()],
    );
    input.destination_control = Some(Box::new(origin.control()));
    (f, target, input)
}
#[test]
fn paired_replacement_requires_original_target_store_source_and_credential_control() {
    for case in 0..5 {
        let (mut f, target, mut input) = paired_fixture();
        if case == 4 {
            let result = apply(&mut f, &input);
            assert!(!result.node.deleted);
            assert!(owned(&mut f, &target).is_none());
            f.reopen_client();
            f.ack(&input, result).unwrap();
            assert!(owned(&mut f, &target).is_none());
            continue;
        }
        match case {
            0 => input.destination_control = None,
            1 => input.destination_control.as_mut().unwrap().fingerprint = "e".repeat(64),
            2 => input.destination_control.as_mut().unwrap().source = [3; 32],
            _ => input.destination_control.as_mut().unwrap().store.root = "forged/".into(),
        }
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .begin(input.clone())
                .is_err()
        );
        assert!(
            load(&f.client.connection, &input.request.operation)
                .unwrap()
                .is_none()
        );
        assert!(owned(&mut f, &target).is_some());
    }
}
#[test]
fn owned_replacement_recovers_both_modes_and_sources_and_retains_dirty_target_and_late_receipts() {
    for source_deferred in [false, true] {
        for mode in [RemovalMode::Deferred, RemovalMode::Pending] {
            let (mut f, source, target, i) = fixture(source_deferred, mode);
            let dirty = f.stage(&target, 500, b"retained target\0\xff");
            let old = owned(&mut f, &target).unwrap();
            Journal::new(&mut f.client)
                .unwrap()
                .begin(i.clone())
                .unwrap();
            let receipt = f.authority.namespace_apply(&i.request).unwrap();
            for n in [&source, &target] {
                assert!(
                    crate::namespace_client::pending_for_node(
                        &f.client.connection,
                        &f.volume,
                        &n.id
                    )
                    .unwrap()
                );
            }
            assert_eq!(owned(&mut f, &target).unwrap(), old);
            let cancel = close(&mut f, &target, &old.fence, false);
            assert!(Journal::new(&mut f.client).unwrap().begin(cancel).is_err());
            let mut forged = receipt.clone();
            match forged.artifact.as_mut().unwrap() {
                Artifact::Namespace { displaced: Some(n) }
                | Artifact::RemovalMove {
                    displaced: Some(n), ..
                } => n.name = "forged".into(),
                _ => unreachable!(),
            }
            assert!(f.ack(&i, forged).is_err());
            f.reopen_client();
            f.ack(&i, receipt.clone()).unwrap();
            assert!(owned(&mut f, &target).is_none());
            assert_eq!(f.file(&source).base, receipt.node);
            let detached = f.file(&target);
            assert!(detached.base.deleted);
            assert_eq!(detached.head, dirty.generation);
            assert_eq!(detached.binding.node.id, target.id);
            if source_deferred {
                let old = owned(&mut f, &source).unwrap();
                assert_eq!(old.event, i.request.operation);
                let cancel = close(&mut f, &receipt.node, &old.fence, false);
                let kept = apply(&mut f, &cancel);
                let origin = deferred::prepare(&mut f, &kept.node);
                apply(&mut f, &origin);
                let successor = owned(&mut f, &source).unwrap();
                f.ack(&i, receipt).unwrap();
                assert_eq!(owned(&mut f, &source).unwrap(), successor);
            }
            assert!(owned(&mut f, &target).is_none());
        }
    }
}
#[test]
fn owned_replacement_admission_requires_exact_target_identity_mode_event_and_storage_binding() {
    for case in 0..6 {
        let (mut f, _, target, mut i) = fixture(false, RemovalMode::Deferred);
        let Mutation::MoveEntry {
            destination_removal: Some(claim),
            ..
        } = &mut i.request.mutation
        else {
            unreachable!()
        };
        match case {
            0 => claim.fence.owner = id(999),
            1 => claim.fence.intent = id(999),
            2 => claim.previous = id(999),
            3 => claim.mode = RemovalMode::Pending,
            4 => i.fingerprint = "c".repeat(64),
            _ => i.entries[1].name = "forged".into(),
        }
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .begin(i.clone())
                .is_err()
        );
        assert!(
            load(&f.client.connection, &i.request.operation)
                .unwrap()
                .is_none()
        );
        assert!(owned(&mut f, &target).is_some());
    }
}
#[test]
fn owned_replacement_acknowledgment_failures_preserve_both_bases_and_target_epoch_until_exact_retry()
 {
    for case in 0..4 {
        let (mut f, source, target, i) = fixture(true, RemovalMode::Pending);
        Journal::new(&mut f.client)
            .unwrap()
            .begin(i.clone())
            .unwrap();
        let receipt = f.authority.namespace_apply(&i.request).unwrap();
        let old = owned(&mut f, &target).unwrap();
        let pages: i64 = f
            .client
            .connection
            .pragma_query_value(None, "page_count", |r| r.get(0))
            .unwrap();
        if case == 3 {
            f.client
                .connection
                .execute_batch("CREATE TABLE full(data BLOB); PRAGMA wal_checkpoint(TRUNCATE)")
                .unwrap();
            let now: i64 = f
                .client
                .connection
                .pragma_query_value(None, "page_count", |r| r.get(0))
                .unwrap();
            f.client
                .connection
                .pragma_update(None, "max_page_count", now)
                .unwrap();
            f.client.connection.execute_batch("CREATE TRIGGER fail BEFORE UPDATE ON namespace_intents BEGIN INSERT INTO full VALUES(zeroblob(1048576)); END").unwrap();
        } else {
            let when = match case {
                0 => "BEFORE UPDATE ON namespace_intents".into(),
                1 => format!(
                    "BEFORE UPDATE ON logical_files WHEN NEW.node='{}'",
                    source.id
                ),
                _ => format!(
                    "BEFORE UPDATE ON logical_files WHEN NEW.node='{}'",
                    target.id
                ),
            };
            f.client
                .connection
                .execute_batch(&format!(
                    "CREATE TRIGGER fail {when} BEGIN SELECT RAISE(IGNORE); END"
                ))
                .unwrap();
        }
        assert!(f.ack(&i, receipt.clone()).is_err());
        assert_eq!(f.file(&source).base, source);
        assert_eq!(f.file(&target).base, target);
        assert_eq!(owned(&mut f, &target).unwrap(), old);
        f.client
            .connection
            .pragma_update(None, "max_page_count", pages + 1024)
            .unwrap();
        f.client
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        f.reopen_client();
        f.ack(&i, receipt).unwrap();
        assert!(owned(&mut f, &target).is_none());
    }
}
#[test]
fn owned_replacement_schema21_upgrade_keeps_legacy_request_bytes_and_hashes() {
    let (mut f, source, target, mut i) = fixture(true, RemovalMode::Deferred);
    let Mutation::MoveDeferredEntry {
        destination,
        destination_removal,
        name,
        ..
    } = &mut i.request.mutation
    else {
        unreachable!()
    };
    *destination = None;
    *destination_removal = None;
    *name = "legacy move".into();
    i.entries.truncate(1);
    let encoded = serde_json::to_vec(&i).unwrap();
    assert!(
        !String::from_utf8(encoded.clone())
            .unwrap()
            .contains("destination_removal")
    );
    let receipt = apply(&mut f, &i);
    let before = load(&f.client.connection, &i.request.operation)
        .unwrap()
        .unwrap();
    let bytes = hash(&before).unwrap();
    f.client
        .connection
        .pragma_update(None, "user_version", 21)
        .unwrap();
    f.reopen_client();
    assert_eq!(
        f.client.info().unwrap().schema_version,
        crate::SCHEMA_VERSION
    );
    let after = load(&f.client.connection, &i.request.operation)
        .unwrap()
        .unwrap();
    assert_eq!(serde_json::to_vec(&after.input).unwrap(), encoded);
    assert_eq!(hash(&after).unwrap(), bytes);
    assert_eq!(f.file(&source).base, receipt.node);
    assert!(owned(&mut f, &target).is_some());
}

#[test]
fn owned_replacement_preserves_both_client_closing_budgets_at_full_capacity() {
    let (mut f, source, target, i) = fixture(true, RemovalMode::Deferred);
    let template = Input {
        request: f.request(Mutation::CreateEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            store: Some(f.store.id.clone()),
            name: "quota".into(),
            node_kind: Kind::EmptyFile,
        }),
        entries: vec![],
        ..i.clone()
    };
    let count: i64 = f
        .client
        .connection
        .query_row("SELECT count(*) FROM namespace_intents", [], |r| r.get(0))
        .unwrap();
    let tx = f.client.connection.transaction().unwrap();
    for op in 0..LIMIT - count - 4 {
        let mut input = template.clone();
        input.request.operation = id(10000 + op as u64);
        let record = Record {
            input,
            outcome: None,
        };
        tx.execute(
            "INSERT INTO namespace_intents VALUES(?1,?2,NULL,?3)",
            params![
                record.input.request.operation,
                serde_json::to_vec(&record.input).unwrap(),
                hash(&record).unwrap()
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .begin(i.clone())
            .is_err()
    );
    assert!(owned(&mut f, &source).is_some());
    assert!(owned(&mut f, &target).is_some());
    f.client
        .connection
        .execute(
            "DELETE FROM namespace_intents WHERE operation_id=?",
            [id(10000)],
        )
        .unwrap();
    apply(&mut f, &i);
    assert!(owned(&mut f, &target).is_none());
}
