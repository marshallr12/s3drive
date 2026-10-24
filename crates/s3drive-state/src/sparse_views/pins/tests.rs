use super::*;
fn open(path: &std::path::Path) -> (State, View) {
    let mut s = State::open_managed(path).unwrap();
    let binding = Binding::new(
        s3drive_core::manifest::base::Descriptor::from_hex(
            &"01".repeat(16),
            &"02".repeat(16),
            &"03".repeat(16),
            &"4".repeat(64),
        )
        .unwrap(),
        [5; 16],
        [6; 32],
        "a".repeat(64),
        "managed/".into(),
        0,
    )
    .unwrap();
    let v = j(&mut s).begin([7; 32], binding, 0).unwrap();
    let v = j(&mut s)
        .write([8; 32], v.id(), 0, "café 🦀".as_bytes())
        .unwrap();
    (s, v)
}
fn j(s: &mut State) -> Journal<'_> {
    Journal::new(s, Options::default()).unwrap()
}
fn rows(s: &State) -> Vec<Vec<String>> {
    ["SELECT quote(view_id)||quote(record)||quote(state_hash) FROM sparse_views ORDER BY view_id",
     "SELECT quote(operation)||quote(view_id)||quote(request)||quote(state_hash) FROM sparse_view_operations ORDER BY operation",
     "SELECT quote(target)||quote(binding)||quote(head)||quote(state_hash) FROM sparse_view_files ORDER BY target",
     "SELECT quote(block)||quote(data) FROM sparse_view_blocks ORDER BY block"]
    .iter().map(|q|s.connection.prepare(q).unwrap().query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<Vec<String>>>().unwrap()).collect()
}
#[test]
fn explicit_pins_keep_exact_reads_close_idempotently_and_never_reuse_released_ids() {
    let temp = tempfile::tempdir().unwrap();
    let (mut s, v) = open(&temp.path().join("pins café 🦀"));
    let before = rows(&s);
    let a = j(&mut s).pin(v.id()).unwrap();
    let b = j(&mut s).pin(v.id()).unwrap();
    assert_ne!(a.id(), b.id());
    let data = vec![Piece::Local("café 🦀".as_bytes().to_vec())];
    assert_eq!(j(&mut s).read_pinned(&a, 0, 32).unwrap(), data);
    assert!(j(&mut s).release_pin(&a).unwrap());
    assert!(!j(&mut s).release_pin(&a).unwrap());
    assert!(j(&mut s).read_pinned(&a, 0, 32).is_err());
    let c = j(&mut s).pin(v.id()).unwrap();
    assert_ne!(a.id(), c.id());
    assert!(!j(&mut s).release_pin(&a).unwrap());
    assert_eq!(j(&mut s).read_pinned(&b, 0, 32).unwrap(), data);
    assert_eq!(j(&mut s).read_pinned(&c, 0, 32).unwrap(), data);
    let l = load(&s.connection).unwrap();
    assert_eq!(l.role.unwrap().issued, 3);
    assert_eq!(l.pins.len(), 2);
    assert_eq!(rows(&s), before);
}
#[test]
fn replacement_owner_retires_only_old_read_pins_and_late_release_cannot_free_new_sequence_one() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("restart");
    let (mut s, v) = open(&path);
    let old = j(&mut s).pin(v.id()).unwrap();
    let before = rows(&s);
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(load(&s.connection).unwrap().pins.len(), 1);
    assert!(j(&mut s).read_pinned(&old, 0, 32).is_err());
    assert!(j(&mut s).release_pin(&old).is_err());
    let new = j(&mut s).pin(v.id()).unwrap();
    assert_eq!(old.sequence, 1);
    assert_eq!(new.sequence, 1);
    assert_ne!(old.id(), new.id());
    assert!(j(&mut s).release_pin(&old).is_err());
    assert_eq!(
        j(&mut s).read_pinned(&new, 0, 32).unwrap(),
        vec![Piece::Local("café 🦀".as_bytes().to_vec())]
    );
    assert_eq!(load(&s.connection).unwrap().pins.len(), 1);
    assert_eq!(rows(&s), before);
}
#[test]
fn active_quota_is_independent_of_lifetime_open_count_without_tombstone_growth() {
    let temp = tempfile::tempdir().unwrap();
    let (mut s, v) = open(&temp.path().join("quota"));
    for _ in 0..(2 * MAX_ACTIVE + 1) {
        let p = j(&mut s).pin(v.id()).unwrap();
        assert!(j(&mut s).release_pin(&p).unwrap());
    }
    let mut live = Vec::new();
    for _ in 0..MAX_ACTIVE {
        live.push(j(&mut s).pin(v.id()).unwrap());
    }
    let before = load(&s.connection).unwrap();
    let source = rows(&s);
    let error = j(&mut s).pin(v.id()).unwrap_err();
    assert_eq!(error.downcast_ref::<Failure>(), Some(&Failure::Quota));
    assert!(load(&s.connection).unwrap() == before);
    assert_eq!(rows(&s), source);
    assert!(j(&mut s).release_pin(&live[0]).unwrap());
    let p = j(&mut s).pin(v.id()).unwrap();
    assert_ne!(p.id(), live[0].id());
    assert!(!j(&mut s).release_pin(&live[0]).unwrap());
    assert!(j(&mut s).read_pinned(&p, 0, 32).is_ok());
    let l = load(&s.connection).unwrap();
    assert_eq!(l.pins.len(), MAX_ACTIVE);
    assert_eq!(l.role.unwrap().issued, (3 * MAX_ACTIVE + 2) as u64);
}
#[test]
fn live_owner_rejects_a_self_consistent_rewind_to_an_earlier_session_projection() {
    let temp = tempfile::tempdir().unwrap();
    let (mut s, v) = open(&temp.path().join("rewind"));
    let first = j(&mut s).pin(v.id()).unwrap();
    let old = load(&s.connection).unwrap();
    let second = j(&mut s).pin(v.id()).unwrap();
    let current = load(&s.connection).unwrap();
    s.connection
        .execute(
            "DELETE FROM source_read_pins WHERE pin_id=?1",
            [second.id.as_slice()],
        )
        .unwrap();
    update_role(
        &s.connection,
        current.role.as_ref(),
        old.role.as_ref().unwrap(),
    )
    .unwrap();
    assert!(
        load(&s.connection).unwrap() == old,
        "the attack must be self-consistent on disk"
    );
    assert!(Journal::new(&mut s, Options::default()).is_err());
    assert!(check(&s, &first).is_err());
}
#[test]
fn source_mutation_trigger_cannot_silently_remove_the_complete_pin_projection() {
    let temp = tempfile::tempdir().unwrap();
    let (mut s, v) = open(&temp.path().join("trigger"));
    let p = j(&mut s).pin(v.id()).unwrap();
    let before = rows(&s);
    let pins = load(&s.connection).unwrap();
    s.connection.execute_batch("CREATE TRIGGER erase_pins AFTER UPDATE ON sparse_view_files BEGIN DELETE FROM source_read_pins; DELETE FROM source_read_pin_session; END;").unwrap();
    assert!(j(&mut s).write([9; 32], v.id(), 0, b"bad").is_err());
    assert_eq!(rows(&s), before);
    assert!(load(&s.connection).unwrap() == pins);
    assert!(j(&mut s).read_pinned(&p, 0, 32).is_ok());
}
#[test]
fn attached_trigger_preflight_refuses_pin_mutations_without_session_or_source_changes() {
    let temp = tempfile::tempdir().unwrap();
    let (mut s, v) = open(&temp.path().join("ignored"));
    let before = rows(&s);
    s.connection.execute_batch("CREATE TRIGGER ignore_pin BEFORE INSERT ON source_read_pins BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    assert!(j(&mut s).pin(v.id()).is_err());
    assert!(load(&s.connection).unwrap() == Ledger::default());
    assert_eq!(rows(&s), before);
    assert!(s.source_pin_witness.is_none());
}
#[test]
fn schema36_migration_preserves_literal_source_bytes_and_current_missing_ddl_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("migration");
    let (s, v) = open(&path);
    let before = rows(&s);
    s.connection.execute_batch("DROP TABLE sparse_retention_anchors; DROP TABLE sparse_retention_role; DROP TABLE sparse_retry_mode; DROP TABLE chunk_mount_role; DROP TABLE manifest_creations; DROP TABLE manifest_creation_role; DROP TABLE source_read_pins; DROP TABLE source_read_pin_session; PRAGMA user_version=36").unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
    assert_eq!(rows(&s), before);
    let p = j(&mut s).pin(v.id()).unwrap();
    assert!(j(&mut s).read_pinned(&p, 0, 32).is_ok());
    s.connection
        .execute_batch("DROP TABLE source_read_pins")
        .unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
}
#[test]
fn pin_role_is_isolated_from_legacy_publication_and_authority_adoption() {
    let temp = tempfile::tempdir().unwrap();
    let (mut s, v) = open(&temp.path().join("roles"));
    let p = j(&mut s).pin(v.id()).unwrap();
    j(&mut s).release_pin(&p).unwrap();
    let tx = s
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("DELETE FROM sparse_view_operations; DELETE FROM sparse_views; DELETE FROM sparse_view_files; DELETE FROM sparse_view_blocks; DELETE FROM sparse_view_role;").unwrap();
    tx.commit().unwrap();
    assert!(crate::chunk_publication::Journal::new(&mut s).is_err());
    assert!(s.namespace_initialize().is_err());
    assert!(crate::logical::Journal::new(&mut s, crate::journal::Options::default()).is_err());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_owner_manual_pin_api_keeps_one_connection_and_old_tokens_close_after_restart() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("worker");
        let (s, v) = open(&path);
        drop(s);
        let storage = crate::worker::Storage::open(
            &path,
            crate::cache::Options::default(),
            crate::journal::Options::default(),
        )
        .await
        .unwrap();
        let client = storage.sparse_views(Options::default()).unwrap();
        let a = client.pin(v.id()).await.unwrap();
        let b = client.clone().pin(v.id()).await.unwrap();
        assert_ne!(a.id(), b.id());
        assert!(client.read_pinned(a.clone(), 0, 32).await.is_ok());
        assert!(State::open_managed(&path).is_err());
        assert!(client.release_pin(a.clone()).await.unwrap());
        assert!(!client.release_pin(a.clone()).await.unwrap());
        assert!(client.read_pinned(a, 0, 32).await.is_err());
        storage.close().await.unwrap();
        let mut s = State::open_managed(&path).unwrap();
        let fresh = j(&mut s).pin(v.id()).unwrap();
        assert!(j(&mut s).release_pin(&b).is_err());
        assert!(j(&mut s).read_pinned(&fresh, 0, 32).is_ok());
    })
    .await
    .expect("manual source pin worker timed out");
}

#[test]
fn issued_counter_exhaustion_refuses_instead_of_reusing_old_pin_ids() {
    let temp = tempfile::tempdir().unwrap();
    let (mut s, v) = open(&temp.path().join("overflow"));
    let p = j(&mut s).pin(v.id()).unwrap();
    let previous = load(&s.connection).unwrap().role.unwrap();
    let role = Role {
        issued: i64::MAX as u64,
        ..previous.clone()
    };
    update_role(&s.connection, Some(&previous), &role).unwrap();
    s.source_pin_witness = Some(role.sha().unwrap());
    let before = load(&s.connection).unwrap();
    let e = j(&mut s).pin(v.id()).unwrap_err();
    assert_eq!(e.downcast_ref::<Failure>(), Some(&Failure::Quota));
    assert!(load(&s.connection).unwrap() == before);
    assert!(j(&mut s).read_pinned(&p, 0, 32).is_ok());
}
#[test]
fn real_sqlite_full_pin_allocation_preserves_counter_projection_and_source_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let (mut s, v) = open(&temp.path().join("full"));
    s.connection
        .execute_batch(
            "PRAGMA journal_mode=DELETE; PRAGMA page_size=512; VACUUM; PRAGMA journal_mode=WAL;",
        )
        .unwrap();
    let source = rows(&s);
    let original: i64 = s
        .connection
        .pragma_query_value(None, "max_page_count", |r| r.get(0))
        .unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let mut admitted = Vec::new();
    let mut full = false;
    for _ in 0..MAX_ACTIVE {
        let before = load(&s.connection).unwrap();
        match j(&mut s).pin(v.id()) {
            Ok(p) => admitted.push(p),
            Err(e) => {
                assert!(
                    crate::cache::is_disk_full(e.as_ref()),
                    "expected real SQLITE_FULL: {e}"
                );
                assert!(load(&s.connection).unwrap() == before);
                full = true;
                break;
            }
        }
    }
    assert!(full, "fixture never reached SQLITE_FULL");
    assert_eq!(rows(&s), source);
    for p in &admitted {
        assert!(j(&mut s).read_pinned(p, 0, 32).is_ok());
    }
    s.connection
        .pragma_update(None, "max_page_count", original)
        .unwrap();
    let next = j(&mut s).pin(v.id()).unwrap();
    assert_eq!(next.sequence, admitted.len() as u64 + 1);
    assert!(j(&mut s).read_pinned(&next, 0, 32).is_ok());
    assert_eq!(rows(&s), source);
}
mod crash;

#[test]
fn native_retry_validation_work_isolation_never_covers_later_pin_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let (mut state, view) = open(&temp.path().join("isolated pin validation"));
    let pin = acquire(&mut state, view.id()).unwrap();
    let before = inspect(&state.connection).unwrap();
    {
        let tx = state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let checks = isolation_count();
        let snapshot = ledger::inspect_isolated(&tx).unwrap();
        assert_eq!(isolation_count() - checks, 1);
        assert!(snapshot == before);
        tx.execute(
            "UPDATE source_read_pins SET state_hash=zeroblob(32) WHERE pin_id=?",
            [pin.id().as_slice()],
        )
        .unwrap();
        let checks = isolation_count();
        assert!(
            inspect(&tx).is_err(),
            "pre-write isolation cannot cover a changed pin"
        );
        assert_eq!(isolation_count() - checks, 1);
        tx.rollback().unwrap();
    }
    assert!(inspect(&state.connection).unwrap() == before);
    assert!(release(&mut state, &pin).unwrap());
}

#[test]
fn native_retry_validation_work_guard_prestate_and_pin_faults_roll_back_together() {
    for point in [
        Boundary::SessionCleared,
        Boundary::PinStored,
        Boundary::RoleStored,
        Boundary::Validated,
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("guard rollback");
        let (mut state, view) = open(&path);
        acquire(&mut state, view.id()).unwrap();
        drop(state);
        let mut state = State::open_managed(&path).unwrap();
        let before = super::super::ledger::inspect(&state.connection).unwrap();
        let witness = state.source_pin_witness;
        let mut reached = false;
        assert!(
            acquire_view_inner(&mut state, view.id(), |at| {
                if at == point {
                    reached = true;
                    return Err("guard cut".into());
                }
                Ok(())
            })
            .is_err()
        );
        assert!(reached);
        assert!(state.connection.is_autocommit());
        assert!(super::super::ledger::inspect(&state.connection).unwrap() == before);
        assert_eq!(state.source_pin_witness, witness);
        let (selected, pin) = acquire_view_inner(&mut state, view.id(), |_| Ok(())).unwrap();
        assert_eq!(selected, view);
        assert!(release(&mut state, &pin).unwrap());
    }
}

#[tokio::test]
async fn native_retry_validation_work_fused_reads_reject_changed_pins_and_late_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("fused reads");
    let (state, view) = open(&path);
    drop(state);
    let storage = crate::worker::Storage::open(
        &path,
        crate::cache::Options::default(),
        crate::journal::Options::default(),
    )
    .await
    .unwrap();
    let client = storage.sparse_views(Options::default()).unwrap();
    let pin = client.pin(view.id()).await.unwrap();
    let expected = vec![Piece::Local("café 🦀".as_bytes().to_vec())];
    assert_eq!(
        client.read_pinned(pin.clone(), 0, 32).await.unwrap(),
        expected
    );
    for field in 0..3 {
        let mut forged = pin.clone();
        match field {
            0 => forged.id = [231; 32],
            1 => forged.owner = "e".repeat(32),
            _ => forged.view = [232; 32],
        }
        assert!(client.read_pinned(forged, 0, 32).await.is_err());
    }
    let guard = client.guard(view.id()).await.unwrap();
    let cloned = guard.clone();
    let (a, b) = tokio::join!(guard.read(0, 32), cloned.read(0, 32));
    assert_eq!(a.unwrap(), expected);
    assert_eq!(b.unwrap(), expected);
    let saved = storage
        .cache()
        .owner
        .call(|store| {
            let data: Vec<u8> = store.state.connection.query_row(
                "SELECT data FROM sparse_view_blocks",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(
                store.state.connection.execute(
                    "UPDATE sparse_view_blocks SET data=zeroblob(length(data))",
                    []
                )?,
                1
            );
            Ok(data)
        })
        .await
        .unwrap();
    assert!(client.read_pinned(pin.clone(), 0, 32).await.is_err());
    assert!(guard.read(0, 32).await.is_err());
    storage
        .cache()
        .owner
        .call(move |store| {
            assert_eq!(
                store
                    .state
                    .connection
                    .execute("UPDATE sparse_view_blocks SET data=?1", [saved])?,
                1
            );
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(guard.read(0, 32).await.unwrap(), expected);
    drop(guard);
    drop(cloned);
    assert!(client.release_pin(pin.clone()).await.unwrap());
    assert!(client.read_pinned(pin, 0, 32).await.is_err());
    storage.close().await.unwrap();
}
