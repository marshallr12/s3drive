use super::*;
use crate::chunk_publication::sources::{self, Point};

fn committed(f: &mut Fixture) -> Record {
    let p = f.prepare();
    let a = f
        .authority
        .namespace_apply(p.publication().unwrap())
        .unwrap();
    let id = f.input.id;
    f.journal().published(id, a).unwrap()
}
fn settle(f: &mut Fixture) -> sources::SourceReceipt {
    let id = f.input.id;
    sources::Journal::new(&mut f.client)
        .unwrap()
        .settle_source(id)
        .unwrap()
}
fn status(f: &mut Fixture) -> sources::SourceStatus {
    let id = f.input.id;
    sources::Journal::new(&mut f.client)
        .unwrap()
        .source_status(id)
        .unwrap()
}
fn rows(f: &Fixture, table: &str) -> u32 {
    assert!(["chunk_source_role", "chunk_source_settlements"].contains(&table));
    f.client
        .connection
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[test]
fn exact_source_receipt_survives_restart_without_generic_flush_or_role_release() {
    let mut f = Fixture::new(false);
    let r = committed(&mut f);
    assert!(status(&mut f).settlement().is_none());
    let receipt = settle(&mut f);
    assert_eq!(receipt.plan(), &f.input.id);
    assert_eq!(receipt.snapshot(), f.snapshot);
    assert_eq!(receipt.target(), &f.input.local_target().unwrap());
    assert_eq!(receipt.authority_receipt(), r.receipt().unwrap());
    assert_eq!(Some(receipt.root_generation()), r.root_generation());
    assert_ne!(receipt.snapshot(), receipt.root_generation());
    assert_eq!(receipt.manifest(), &r.input.snapshot_manifest);
    f.source_pending();
    assert_eq!(rows(&f, "chunk_source_settlements"), 1);
    f.reopen();
    assert_eq!(settle(&mut f), receipt);
    let s = status(&mut f);
    assert_eq!(s.settlement(), Some(&receipt));
    assert_eq!(s.publication(), &r);
    assert_eq!(s.current_head(), f.snapshot);
    assert!(crate::logical::Journal::new(&mut f.client, journal::Options::default()).is_err());
    assert!(crate::namespace_client::Journal::new(&mut f.client).is_err());
    f.source_pending();
}
#[test]
fn historical_commit_preserves_newer_remote_root_local_head_and_building_draft() {
    for settle_early in [false, true] {
        let mut f = Fixture::new(false);
        let first = committed(&mut f);
        let original = f.input.clone();
        let early = settle_early.then(|| settle(&mut f));
        let second = next_plan(&mut f, 9, &first.receipt().unwrap().node);
        f.input = second.clone();
        let seq = f.authority.namespace_volume().unwrap().sequence;
        let store = f.store.clone();
        let r = f
            .journal()
            .begin(
                second.id,
                first.receipt().unwrap().node.clone(),
                store,
                seq,
                uuid(10),
            )
            .unwrap();
        let a = f.authority.namespace_apply(r.input.reserve()).unwrap();
        f.journal().reserved(second.id, a).unwrap();
        f.complete();
        let seq = f.authority.namespace_volume().unwrap().sequence;
        let p = f.journal().prepare_publish(second.id, seq).unwrap();
        let newer = f
            .authority
            .namespace_apply(p.publication().unwrap())
            .unwrap();
        let r = f.journal().published(second.id, newer.clone()).unwrap();
        let head = r.input.snapshot;
        assert!(status(&mut f).settlement().is_none());
        let mut j = journal::Journal::new(&mut f.client, journal::Options::default()).unwrap();
        let draft = j
            .begin(
                &second.local_target().unwrap(),
                Base::Missing,
                Some(head),
                3,
            )
            .unwrap();
        j.append_at(draft, 0, b"new").unwrap();
        if !settle_early {
            let newer_receipt = settle(&mut f);
            assert_eq!(newer_receipt.snapshot(), head);
        }
        f.input = original;
        f.reopen();
        if !settle_early {
            assert!(status(&mut f).settlement().is_none());
        }
        let receipt = settle(&mut f);
        if let Some(early) = early {
            assert_eq!(receipt, early);
        }
        assert_eq!(receipt.authority_receipt(), first.receipt().unwrap());
        assert_eq!(status(&mut f).current_head(), head);
        assert!(newer.sequence > receipt.authority_receipt().sequence);
        assert_eq!(
            journal::load(&f.client.connection, draft).unwrap().phase,
            journal::Phase::Building
        );
        f.source_pending();
        f.input = second;
        assert_eq!(status(&mut f).settlement().is_none(), settle_early);
        assert_eq!(
            rows(&f, "chunk_source_settlements"),
            if settle_early { 1 } else { 2 }
        );
    }
}
#[test]
fn unprepared_unknown_rejected_and_missing_publications_cannot_settle_source() {
    for stage in 0..4 {
        let mut f = Fixture::new(false);
        let id = f.input.id;
        let r = if stage == 0 {
            f.begin()
        } else if stage == 1 {
            f.reserve()
        } else {
            f.prepare()
        };
        if stage == 3 {
            let request = r.publication().unwrap().clone();
            f.journal()
                .reject(id, request, namespace::Error::Conflict)
                .unwrap();
        }
        let saved = f.journal().get(id).unwrap();
        assert!(
            sources::Journal::new(&mut f.client)
                .unwrap()
                .settle_source(id)
                .is_err()
        );
        assert!(status(&mut f).settlement().is_none());
        assert_eq!(status(&mut f).publication(), &saved);
        assert!(
            sources::Journal::new(&mut f.client)
                .unwrap()
                .source_status([0; 32])
                .is_err()
        );
        assert_eq!(rows(&f, "chunk_source_role"), 0);
        assert_eq!(rows(&f, "chunk_source_settlements"), 0);
        f.source_pending();
    }
}
#[test]
fn generic_physical_intent_on_source_is_refused_without_mixing_receipts() {
    let mut f = Fixture::new(false);
    committed(&mut f);
    remote::Journal::new(&mut f.client, remote::Options::default())
        .unwrap()
        .prepare(f.snapshot)
        .unwrap();
    let id = f.input.id;
    let e = sources::Journal::new(&mut f.client)
        .unwrap()
        .settle_source(id)
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<sources::Failure>(),
        Some(&sources::Failure::MixedSemantics)
    );
    assert!(
        sources::Journal::new(&mut f.client)
            .unwrap()
            .source_status(id)
            .is_err()
    );
    assert_eq!(rows(&f, "chunk_source_settlements"), 0);
}
#[test]
fn metadata_only_settlement_does_not_claim_current_payload_integrity() {
    let mut f = Fixture::new(false);
    committed(&mut f);
    f.client
        .connection
        .execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])
        .unwrap();
    assert!(
        journal::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .read(f.snapshot, 0, 64)
            .is_err()
    );
    f.reopen();
    let r = settle(&mut f);
    assert_eq!(status(&mut f).settlement(), Some(&r));
    f.source_pending();
}
#[test]
fn source_rows_role_hashes_and_frozen_proofs_are_checked_on_reopen() {
    for effect in [
        "DELETE FROM chunk_source_settlements",
        "DELETE FROM chunk_source_role",
        "UPDATE chunk_source_role SET state_hash=zeroblob(32)",
        "UPDATE chunk_source_settlements SET state_hash=zeroblob(32)",
        "UPDATE chunk_source_settlements SET record=zeroblob(length(record))",
        "UPDATE chunk_plans SET manifest_hash=zeroblob(32)",
        "DELETE FROM write_settlements; DELETE FROM remote_flushes",
    ] {
        let mut f = Fixture::new(false);
        committed(&mut f);
        settle(&mut f);
        f.client.connection.execute_batch(effect).unwrap();
        assert!(sources::Journal::new(&mut f.client).is_err(), "{effect}");
        let tmp = State::open_managed(&f._tmp.path().join("swap")).unwrap();
        drop(std::mem::replace(&mut f.client, tmp));
        assert!(State::open_managed(&f.path).is_err(), "{effect}");
    }
}
#[test]
fn final_postwrite_validation_refuses_ignored_deleted_rewritten_and_foreign_effects() {
    for (table, effect) in [
        ("chunk_source_role", "SELECT RAISE(IGNORE)"),
        ("chunk_source_role", "DELETE FROM chunk_source_role"),
        (
            "chunk_source_role",
            "UPDATE chunk_source_role SET settled=settled+1",
        ),
        ("chunk_source_settlements", "SELECT RAISE(IGNORE)"),
        (
            "chunk_source_settlements",
            "DELETE FROM chunk_source_settlements",
        ),
        (
            "chunk_source_settlements",
            "UPDATE chunk_source_settlements SET state_hash=zeroblob(32)",
        ),
        (
            "chunk_source_settlements",
            "UPDATE chunk_source_settlements SET record=zeroblob(length(record))",
        ),
        (
            "chunk_source_settlements",
            "UPDATE chunk_plans SET manifest_hash=zeroblob(32)",
        ),
        (
            "chunk_source_settlements",
            "DELETE FROM write_settlements; DELETE FROM remote_flushes",
        ),
        (
            "chunk_source_settlements",
            "INSERT INTO retained_client_identity VALUES(1,zeroblob(32))",
        ),
    ] {
        let mut f = Fixture::new(false);
        let r = committed(&mut f);
        let timing = if effect == "SELECT RAISE(IGNORE)" {
            "BEFORE"
        } else {
            "AFTER"
        };
        f.client
            .connection
            .execute_batch(&format!(
                "CREATE TRIGGER damage {timing} INSERT ON {table} BEGIN {effect}; END"
            ))
            .unwrap();
        let id = f.input.id;
        assert!(
            sources::Journal::new(&mut f.client)
                .unwrap()
                .settle_source(id)
                .is_err(),
            "{table}: {effect}"
        );
        assert_eq!(rows(&f, "chunk_source_role"), 0, "{effect}");
        assert_eq!(rows(&f, "chunk_source_settlements"), 0, "{effect}");
        assert_eq!(f.journal().get(id).unwrap(), r);
        f.client
            .connection
            .execute_batch("DROP TRIGGER damage")
            .unwrap();
        settle(&mut f);
        f.source_pending();
    }
}
#[test]
fn grandchild_receipt_damage_blocks_source_settlement_with_direct_root_proof_intact() {
    let mut f = Fixture::new(true);
    let p = f.prepare();
    let a = f
        .authority
        .namespace_apply(p.publication().unwrap())
        .unwrap();
    let id = f.input.id;
    f.journal().published(id, a).unwrap();
    let child = chunked::Journal::new(&mut f.client, chunked::Options::default())
        .unwrap()
        .materialize(id, -1, 128, journal::Options::default())
        .unwrap()
        .unwrap()
        .generation()
        .unwrap();
    f.client
        .connection
        .execute(
            "DELETE FROM write_settlements WHERE generation_id=?1",
            [child.value()],
        )
        .unwrap();
    f.client
        .connection
        .execute(
            "DELETE FROM remote_flushes WHERE generation_id=?1",
            [child.value()],
        )
        .unwrap();
    assert!(
        chunked::Journal::new(&mut f.client, chunked::Options::default())
            .unwrap()
            .require_dependencies(id, 1, 0)
            .is_ok()
    );
    assert!(sources::Journal::new(&mut f.client).is_err());
    assert_eq!(rows(&f, "chunk_source_settlements"), 0);
}
#[test]
fn actual_sqlite_full_rolls_back_complete_source_acknowledgment() {
    let mut f = Fixture::new(false);
    let r = committed(&mut f);
    let id = f.input.id;
    f.client.connection.execute_batch("CREATE TABLE pressure(data BLOB); CREATE TRIGGER pressure_source BEFORE INSERT ON chunk_source_settlements BEGIN INSERT INTO pressure VALUES(zeroblob(65536)); END").unwrap();
    let pages: i64 = f
        .client
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    f.client
        .connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    loop {
        match f
            .client
            .connection
            .execute("INSERT INTO pressure VALUES(zeroblob(4096))", [])
        {
            Ok(_) => (),
            Err(e) => {
                assert_eq!(e.sqlite_error_code(), Some(rusqlite::ErrorCode::DiskFull));
                break;
            }
        }
    }
    let e = sources::Journal::new(&mut f.client)
        .unwrap()
        .settle_source(id)
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<rusqlite::Error>()
            .and_then(|e| e.sqlite_error_code()),
        Some(rusqlite::ErrorCode::DiskFull)
    );
    assert_eq!(rows(&f, "chunk_source_role"), 0);
    assert_eq!(rows(&f, "chunk_source_settlements"), 0);
    assert_eq!(f.journal().get(id).unwrap(), r);
    f.client
        .connection
        .pragma_update(None, "max_page_count", 1000000)
        .unwrap();
    f.client
        .connection
        .execute_batch("DROP TRIGGER pressure_source")
        .unwrap();
    settle(&mut f);
    f.source_pending();
}
#[test]
fn source_schema_upgrade_preserves_history_and_conflicting_shape_rolls_back() {
    for conflict in [false, true] {
        let mut f = Fixture::new(false);
        let original = committed(&mut f);
        let id = f.input.id;
        f.client.connection.execute_batch("DROP TABLE chunk_source_settlements; DROP TABLE chunk_source_role; PRAGMA user_version=27").unwrap();
        if conflict {
            f.client.connection.execute_batch("CREATE TABLE chunk_source_settlements(keep TEXT); INSERT INTO chunk_source_settlements VALUES('preserve')").unwrap();
        }
        let tmp = State::open_managed(&f._tmp.path().join("swap")).unwrap();
        drop(std::mem::replace(&mut f.client, tmp));
        let reopened = State::open_managed(&f.path);
        if conflict {
            assert!(reopened.is_err());
            let db = Connection::open(f.path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
                    .unwrap(),
                27
            );
            assert_eq!(
                db.query_row("SELECT keep FROM chunk_source_settlements", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "preserve"
            );
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name='chunk_source_role'",
                    [],
                    |r| r.get::<_, i32>(0)
                )
                .unwrap(),
                0
            );
        } else {
            f.client = reopened.unwrap();
            assert_eq!(f.journal().get(id).unwrap(), original);
            settle(&mut f);
            f.source_pending();
        }
    }
}
#[tokio::test]
async fn same_owner_concurrent_receipts_and_lost_reply_recover_identically() {
    for lost_reply in [false, true] {
        let mut f = Fixture::new(false);
        let original = committed(&mut f);
        let id = f.input.id;
        if lost_reply {
            let e = sources::Journal::new(&mut f.client)
                .unwrap()
                .settle_inner(id, |p| {
                    if p == Point::Committed {
                        Err("lost source reply".into())
                    } else {
                        Ok(())
                    }
                });
            assert!(e.is_err());
        } else {
            assert!(status(&mut f).settlement().is_none());
        }
        drop(f.client);
        let storage = crate::worker::Storage::open(
            &f.path,
            crate::cache::Options::default(),
            journal::Options::default(),
        )
        .await
        .unwrap();
        let c = storage.chunk_publications().sources();
        let (a, b) = tokio::join!(c.settle_source(id), c.settle_source(id));
        let a = a.unwrap();
        assert_eq!(b.unwrap(), a);
        assert_eq!(a.authority_receipt(), original.receipt().unwrap());
        assert_eq!(c.source_status(id).await.unwrap().settlement(), Some(&a));
        storage.close().await.unwrap();
        drop(c);
        drop(storage);
        f.client = State::open_managed(&f.path).unwrap();
        assert_eq!(settle(&mut f), a);
        assert_eq!(rows(&f, "chunk_source_settlements"), 1);
        f.source_pending();
    }
}

#[test]
fn source_crash_child() {
    let Ok(path) = std::env::var("S3DRIVE_SOURCE_CRASH_PATH") else {
        return;
    };
    let point = std::env::var("S3DRIVE_SOURCE_CRASH_POINT").unwrap();
    let path = std::path::Path::new(&path);
    let mut state = State::open_managed(path).unwrap();
    sources::Journal::new(&mut state)
        .unwrap()
        .settle_inner([7; 32], |p| {
            if format!("{p:?}") == point {
                std::fs::write(path.with_extension("source-ready"), b"ready").unwrap();
                std::thread::sleep(std::time::Duration::from_secs(30));
            }
            Ok(())
        })
        .unwrap();
}
#[test]
fn oversized_transitive_and_source_cells_are_retained_and_refused() {
    for effect in [
        "UPDATE staged_generations SET phase=CAST(zeroblob(2097152) AS TEXT)",
        "UPDATE staged_extents SET chunk_hash=zeroblob(2097152) WHERE chunk_hash IS NOT NULL",
        "UPDATE chunk_source_settlements SET record=zeroblob(2097152)",
        "UPDATE chunk_source_settlements SET state_hash=zeroblob(2097152)",
        "UPDATE chunk_source_role SET state_hash=zeroblob(2097152)",
        "UPDATE chunk_source_settlements SET plan_id=zeroblob(2097152)",
    ] {
        let mut f = Fixture::new(false);
        committed(&mut f);
        settle(&mut f);
        f.client
            .connection
            .execute_batch("PRAGMA foreign_keys=OFF; PRAGMA ignore_check_constraints=ON")
            .unwrap();
        f.client.connection.execute_batch(effect).unwrap();
        assert!(sources::Journal::new(&mut f.client).is_err(), "{effect}");
        assert_eq!(rows(&f, "chunk_source_settlements"), 1);
    }
}
#[test]
fn orphan_source_state_cannot_release_legacy_role_exclusion() {
    for keep in ["chunk_source_role", "chunk_source_settlements"] {
        let mut f = Fixture::new(false);
        committed(&mut f);
        settle(&mut f);
        f.client.connection.execute_batch("PRAGMA foreign_keys=OFF; DELETE FROM chunk_publications; DELETE FROM chunk_client_role").unwrap();
        let remove = if keep == "chunk_source_role" {
            "chunk_source_settlements"
        } else {
            "chunk_source_role"
        };
        f.client
            .connection
            .execute(&format!("DELETE FROM {remove}"), [])
            .unwrap();
        assert!(sources::Journal::new(&mut f.client).is_err());
        assert!(crate::namespace_client::Journal::new(&mut f.client).is_err());
        assert!(crate::logical::Journal::new(&mut f.client, journal::Options::default()).is_err());
    }
}
#[test]
fn killed_owner_rolls_back_partial_rows_and_recovers_exact_committed_receipt() {
    use std::{
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };
    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for point in [Point::Role, Point::Row, Point::Committed] {
        let mut f = Fixture::new(false);
        let committed = committed(&mut f);
        let swap = State::open_managed(&f._tmp.path().join("swap")).unwrap();
        drop(std::mem::replace(&mut f.client, swap));
        let mut child = OwnedChild(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "chunk_publication::tests::source_settlement::source_crash_child",
                    "--nocapture",
                ])
                .env("S3DRIVE_SOURCE_CRASH_PATH", &f.path)
                .env("S3DRIVE_SOURCE_CRASH_POINT", format!("{point:?}"))
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let until = Instant::now() + Duration::from_secs(15);
        while !f.path.with_extension("source-ready").exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "source child exited before checkpoint"
            );
            assert!(Instant::now() < until, "source child checkpoint deadline");
            std::thread::sleep(Duration::from_millis(10));
        }
        child.0.kill().unwrap();
        assert!(
            !child.0.wait().unwrap().success(),
            "child must be killed at checkpoint"
        );
        drop(child);
        f.client = State::open_managed(&f.path).unwrap();
        assert_eq!(
            status(&mut f).settlement().is_some(),
            point == Point::Committed
        );
        assert_eq!(
            rows(&f, "chunk_source_role"),
            u32::from(point == Point::Committed)
        );
        let receipt = settle(&mut f);
        assert_eq!(receipt.authority_receipt(), committed.receipt().unwrap());
        f.reopen();
        assert_eq!(settle(&mut f), receipt);
        f.source_pending();
    }
}
fn extend_empty_chain(f: &mut Fixture, count: usize) -> GenerationId {
    let target = f.input.local_target().unwrap();
    let tx = f.client.connection.transaction().unwrap();
    let mut parent = journal::current_head(&tx, &target).unwrap().unwrap();
    for _ in 0..count {
        tx.execute("INSERT INTO staged_generations(source_id,object_key,parent_id,base_absent,size_bytes,filled_bytes,extent_count) VALUES(?1,?2,?3,1,0,0,0)",params![target.source().as_slice(),target.object_key(),parent.value()]).unwrap();
        let id = GenerationId::from_database(tx.last_insert_rowid()).unwrap();
        tx.execute(
            "INSERT INTO write_journal VALUES(?1,'pending')",
            [id.value()],
        )
        .unwrap();
        let g = journal::load(&tx, id).unwrap();
        let digest = journal::validate_manifest(&tx, &g, false).unwrap();
        tx.execute(
            "UPDATE staged_generations SET phase='sealed',manifest_hash=?2 WHERE generation_id=?1",
            params![id.value(), digest.as_slice()],
        )
        .unwrap();
        parent = id;
    }
    tx.execute(
        "UPDATE staged_heads SET generation_id=?3 WHERE source_id=?1 AND object_key=?2",
        params![
            target.source().as_slice(),
            target.object_key(),
            parent.value()
        ],
    )
    .unwrap();
    tx.commit().unwrap();
    parent
}
#[test]
fn lineage_limit_checks_ancestry_and_extent_budget_without_erasing_historical_receipt() {
    let mut f = Fixture::new(false);
    committed(&mut f);
    let receipt = settle(&mut f);
    // Legitimate metadata and hashes, assembled in one transaction to avoid
    // thousands of unrelated fsyncs. This is a work-bound control, no benchmark.
    let head = extend_empty_chain(&mut f, sources::MAX_LINEAGE - 1);
    assert_eq!(status(&mut f).current_head(), head);
    assert_eq!(settle(&mut f), receipt);
    let head = extend_empty_chain(&mut f, 1);
    let id = f.input.id;
    let e = sources::Journal::new(&mut f.client)
        .unwrap()
        .settle_source(id)
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<sources::Failure>(),
        Some(&sources::Failure::Limit)
    );
    assert_eq!(rows(&f, "chunk_source_settlements"), 1);
    // A corrupt oversized count must be refused before fetching its extents.
    f.client
        .connection
        .execute(
            "UPDATE staged_generations SET extent_count=?2 WHERE generation_id=?1",
            params![
                head.value(),
                i64::try_from(sources::MAX_LINEAGE_EXTENTS + 1).unwrap()
            ],
        )
        .unwrap();
    let e = sources::Journal::new(&mut f.client)
        .unwrap()
        .source_status(id)
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<sources::Failure>(),
        Some(&sources::Failure::Limit)
    );
    assert_eq!(rows(&f, "chunk_source_settlements"), 1);
    f.source_pending();
}
