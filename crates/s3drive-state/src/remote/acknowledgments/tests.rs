use super::*;
use std::{
    fs,
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn target(key: &str) -> Target {
    Target::new([1; 32], &"a".repeat(64), key).unwrap()
}
fn stage(
    state: &mut State,
    key: &Target,
    parent: Option<GenerationId>,
    bytes: &[u8],
) -> GenerationId {
    let mut j = journal::Journal::new(state, journal::Options::default()).unwrap();
    let base = parent
        .map(|id| j.info(id).unwrap().base)
        .unwrap_or(Base::Missing);
    let id = j.begin(key, base, parent, bytes.len() as u64).unwrap();
    for part in bytes.chunks(journal::CHUNK_BYTES) {
        j.append(id, part).unwrap();
    }
    j.seal(id).unwrap();
    id
}
fn verify(state: &mut State, id: GenerationId) -> Info {
    let mut j = Journal::new(state, Options::default()).unwrap();
    let info = j.prepare(id).unwrap();
    j.claim(info.operation, &info.target).unwrap();
    j.verified(Verification {
        operation: info.operation,
        target: info.target.clone(),
        size: info.size,
        payload_sha256: info.payload_sha256,
        revision: Revision::new("\"verified\"", None).unwrap(),
    })
    .unwrap()
}
fn pending(state: &mut State) -> Vec<GenerationId> {
    journal::Journal::new(state, journal::Options::default())
        .unwrap()
        .pending(None, 1000)
        .unwrap()
        .iter()
        .map(|g| g.id)
        .collect()
}
fn read(state: &mut State, id: GenerationId) -> Vec<u8> {
    journal::Journal::new(state, journal::Options::default())
        .unwrap()
        .read(id, 0, 100)
        .unwrap()
}

#[test]
fn acknowledgment_settles_ancestors_and_preserves_newer_heads_snapshots_and_guards() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let mut state = State::open_managed(&path).unwrap();
    let key = target("日本語 %2F + café.txt");
    let a = stage(&mut state, &key, None, b"first");
    let b = stage(&mut state, &key, Some(a), b"");
    let remote = verify(&mut state, b);
    let c = stage(&mut state, &key, Some(b), b"newer dirty");
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    let receipt = j.acknowledge(remote.operation).unwrap();
    assert_eq!(receipt.info().generation, b);
    assert_eq!(receipt.covered_generations(), 2);
    assert_eq!(
        j.acknowledge(remote.operation).unwrap().checksum(),
        receipt.checksum()
    );
    assert_eq!(j.settlement(a).unwrap().unwrap().covering_generation, b);
    assert!(j.flush_receipt(a).unwrap().is_none());
    assert!(j.settlement(c).unwrap().is_none());
    assert_eq!(pending(&mut state), [c]);
    assert_eq!(read(&mut state, a), b"first");
    assert_eq!(read(&mut state, b), b"");
    let d = stage(&mut state, &key, Some(c), b"last");
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    let info = j.prepare(d).unwrap();
    assert_eq!(info.anchor, Some(remote.operation));
    assert_eq!(info.guard, Guard::Match("\"verified\"".into()));
    j.claim(info.operation, &key).unwrap();
    j.verified(Verification {
        operation: info.operation,
        target: key.clone(),
        size: info.size,
        payload_sha256: info.payload_sha256,
        revision: Revision::new("\"last\"", None).unwrap(),
    })
    .unwrap();
    assert_eq!(
        j.acknowledge(info.operation).unwrap().covered_generations(),
        4
    );
    assert_eq!(j.settlement(a).unwrap().unwrap().covering_generation, b);
    assert_eq!(j.settlement(c).unwrap().unwrap().covering_generation, d);
    assert!(pending(&mut state).is_empty());
    drop(state);
    let mut state = State::open_managed(&path).unwrap();
    assert!(pending(&mut state).is_empty());
    assert_eq!(read(&mut state, c), b"newer dirty");
    assert_eq!(read(&mut state, d), b"last");
    let j = Journal::new(&mut state, Options::default()).unwrap();
    assert_eq!(
        j.flush_receipt(b).unwrap().unwrap().checksum(),
        receipt.checksum()
    );
}

#[test]
fn every_nonverified_phase_refuses_acknowledgment_and_retains_pending_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    for (index, phase) in [
        Phase::Prepared,
        Phase::InFlight,
        Phase::Uncertain,
        Phase::Rejected,
        Phase::Cancelled,
    ]
    .into_iter()
    .enumerate()
    {
        let key = target(&format!("file-{index}"));
        let id = stage(&mut state, &key, None, b"preserve");
        let mut j = Journal::new(&mut state, Options::default()).unwrap();
        let info = j.prepare(id).unwrap();
        match phase {
            Phase::Prepared => {}
            Phase::Cancelled => {
                j.cancel_prepared(info.operation).unwrap();
            }
            other => {
                j.claim(info.operation, &key).unwrap();
                match other {
                    Phase::Uncertain => {
                        j.uncertain(info.operation, Reason::Timeout).unwrap();
                    }
                    Phase::Rejected => {
                        j.rejected(info.operation, 412).unwrap();
                    }
                    _ => {}
                }
            }
        }
        let error = j.acknowledge(info.operation).unwrap_err();
        assert_eq!(
            error.downcast_ref::<Failure>(),
            Some(&Failure::InvalidState)
        );
        assert!(j.flush_receipt(id).unwrap().is_none());
        assert!(j.settlement(id).unwrap().is_none());
        assert_eq!(read(&mut state, id), b"preserve");
    }
    assert_eq!(pending(&mut state).len(), 5);
}

#[test]
fn corrupt_acknowledgments_never_hide_pending_work_or_authorize_a_claim() {
    for case in 0..8 {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state");
        let mut state = State::open_managed(&path).unwrap();
        let key = target("file");
        let a = stage(&mut state, &key, None, b"a");
        let b = stage(&mut state, &key, Some(a), b"b");
        let info = verify(&mut state, b);
        Journal::new(&mut state, Options::default())
            .unwrap()
            .acknowledge(info.operation)
            .unwrap();
        let c = stage(&mut state, &key, Some(b), b"c");
        let next = Journal::new(&mut state, Options::default())
            .unwrap()
            .prepare(c)
            .unwrap();
        match case {
            0 => {
                state
                    .connection
                    .execute_batch("UPDATE remote_flushes SET receipt_hash=zeroblob(32)")
                    .unwrap();
            }
            1 => {
                state
                    .connection
                    .execute_batch("UPDATE remote_flushes SET covered_generations=1")
                    .unwrap();
            }
            2 => {
                state
                    .connection
                    .execute_batch("UPDATE write_settlements SET settlement_hash=zeroblob(32)")
                    .unwrap();
            }
            3 => {
                state
                    .connection
                    .execute_batch("UPDATE write_settlements SET manifest_hash=zeroblob(32)")
                    .unwrap();
            }
            4 => {
                state
                    .connection
                    .execute(
                        "DELETE FROM write_settlements WHERE generation_id=?1",
                        [a.value()],
                    )
                    .unwrap();
            }
            5 => {
                state
                    .connection
                    .execute(
                        "DELETE FROM write_settlements WHERE generation_id=?1",
                        [b.value()],
                    )
                    .unwrap();
            }
            6 => {
                state.connection.execute_batch("PRAGMA ignore_check_constraints=ON; UPDATE write_settlements SET settlement_hash=zeroblob(1048576)").unwrap();
            }
            _ => {
                // A correctly checksummed marker for an unrelated verified
                // target still cannot settle this generation.
                let other = stage(&mut state, &target("unrelated"), None, b"z");
                let other = verify(&mut state, other);
                let receipt = Journal::new(&mut state, Options::default())
                    .unwrap()
                    .acknowledge(other.operation)
                    .unwrap();
                let manifest = journal::Journal::new(&mut state, journal::Options::default())
                    .unwrap()
                    .info(a)
                    .unwrap()
                    .manifest_hash
                    .unwrap();
                let hash = settlement_hash(a, manifest, *receipt.checksum());
                state.connection.execute("UPDATE write_settlements SET covering_operation=?1,settlement_hash=?2 WHERE generation_id=?3",params![other.operation.0.as_slice(),hash.as_slice(),a.value()]).unwrap();
            }
        }
        assert!(
            journal::Journal::new(&mut state, journal::Options::default())
                .unwrap()
                .pending(None, 100)
                .is_err(),
            "case {case}"
        );
        assert!(
            Journal::new(&mut state, Options::default())
                .unwrap()
                .claim(next.operation, &key)
                .is_err(),
            "case {case}"
        );
        assert_eq!(read(&mut state, a), b"a");
        assert_eq!(read(&mut state, c), b"c");
        drop(state);
        assert!(State::open_managed(&path).is_err(), "case {case}");
    }
}

#[test]
fn disk_full_and_precommit_failures_roll_back_the_complete_acknowledgment() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let key = target("file");
    let mut parent = None;
    for _ in 0..160 {
        parent = Some(stage(&mut state, &key, parent, b"x"));
    }
    let info = verify(&mut state, parent.unwrap());
    let pages: i64 = state
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    state
        .connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let error = Journal::new(&mut state, Options::default())
        .unwrap()
        .acknowledge(info.operation)
        .unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<rusqlite::Error>()
            .unwrap()
            .sqlite_error_code(),
        Some(rusqlite::ErrorCode::DiskFull)
    );
    assert_eq!(pending(&mut state).len(), 160);
    state
        .connection
        .pragma_update(None, "max_page_count", pages + 100)
        .unwrap();
    for point in [AckPoint::Root, AckPoint::Settlements] {
        assert!(
            Journal::new(&mut state, Options::default())
                .unwrap()
                .acknowledge_inner(info.operation, |at| if at == point {
                    Err("injected rollback".into())
                } else {
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(pending(&mut state).len(), 160);
        assert_eq!(
            state
                .connection
                .query_row("SELECT count(*) FROM remote_flushes", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(
        Journal::new(&mut state, Options::default())
            .unwrap()
            .acknowledge(info.operation)
            .unwrap()
            .covered_generations(),
        160
    );
    assert!(pending(&mut state).is_empty());
    assert_eq!(read(&mut state, parent.unwrap()), b"x");
}

#[test]
fn migration_six_preserves_verified_intents_and_rolls_back_conflicts() {
    for conflict in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state");
        let mut state = State::open_managed(&path).unwrap();
        let id = stage(&mut state, &target("file"), None, b"old bytes");
        let info = verify(&mut state, id);
        state
            .connection
            .execute_batch(
                "DROP TABLE retained_client_identity; DROP TABLE client_identity; DROP TABLE namespace_removals; DROP TABLE namespace_intents; DROP INDEX namespace_portable_names; ALTER TABLE namespace_nodes DROP COLUMN name_key; ALTER TABLE namespace_nodes DROP COLUMN placement; DROP TABLE logical_publication_links; DROP TABLE logical_edits; DROP TABLE logical_files; DROP TABLE managed_publications; DROP TABLE namespace_uploads; DROP TABLE namespace_stores; DROP TABLE namespace_operations; DROP TABLE namespace_nodes; DROP TABLE namespace_volume; DROP TABLE write_settlements; DROP TABLE remote_flushes; PRAGMA user_version=6",
            )
            .unwrap();
        if conflict {
            state.connection.execute_batch("CREATE TABLE remote_flushes (personal TEXT); INSERT INTO remote_flushes VALUES ('preserve')").unwrap();
        }
        drop(state);
        let reopened = State::open_managed(&path);
        assert_eq!(
            reopened.is_err(),
            conflict,
            "unexpected migration open result: {}",
            reopened
                .as_ref()
                .err()
                .map(ToString::to_string)
                .unwrap_or_default()
        );
        if let Ok(mut state) = reopened {
            assert_eq!(pending(&mut state), [id]);
            assert_eq!(read(&mut state, id), b"old bytes");
            assert_eq!(
                Journal::new(&mut state, Options::default())
                    .unwrap()
                    .info(info.operation)
                    .unwrap()
                    .phase,
                Phase::Verified
            );
        } else {
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                    .unwrap(),
                6
            );
            assert_eq!(
                db.query_row("SELECT personal FROM remote_flushes", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "preserve"
            );
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM remote_intents WHERE phase='verified'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
        }
    }
}

#[test]
fn ack_crash_child() {
    let Ok(path) = std::env::var("S3DRIVE_ACK_CRASH_PATH") else {
        return;
    };
    let phase = std::env::var("S3DRIVE_ACK_CRASH_PHASE").unwrap();
    let mut state = State::open_managed(std::path::Path::new(&path)).unwrap();
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    let op = j.list(None, 100).unwrap()[0].operation;
    j.acknowledge_inner(op, |point| {
        if format!("{point:?}") == phase {
            fs::write(
                std::path::Path::new(&path).with_extension("ready"),
                b"ready",
            )
            .unwrap();
            std::thread::sleep(Duration::from_secs(30));
        }
        Ok(())
    })
    .unwrap();
}
#[test]
fn process_death_before_and_after_ack_commit_recovers_without_another_claim() {
    for point in [AckPoint::Root, AckPoint::Settlements, AckPoint::Committed] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state");
        let mut state = State::open_managed(&path).unwrap();
        let key = target("file");
        let a = stage(&mut state, &key, None, b"old");
        let b = stage(&mut state, &key, Some(a), b"new");
        let info = verify(&mut state, b);
        drop(state);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "remote::acknowledgments::tests::ack_crash_child",
                "--nocapture",
            ])
            .env("S3DRIVE_ACK_CRASH_PATH", &path)
            .env("S3DRIVE_ACK_CRASH_PHASE", format!("{point:?}"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !path.with_extension("ready").exists() {
            if let Some(status) = child.try_wait().unwrap() {
                let mut err = String::new();
                child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut err)
                    .unwrap();
                panic!("child {status}: {err}");
            }
            if Instant::now() > deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("child deadline");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        let mut state = State::open_managed(&path).unwrap();
        assert_eq!(pending(&mut state).is_empty(), point == AckPoint::Committed);
        let mut j = Journal::new(&mut state, Options::default()).unwrap();
        assert_eq!(
            j.claim(info.operation, &key)
                .err()
                .unwrap()
                .downcast_ref::<Failure>(),
            Some(&Failure::AlreadyDispatched)
        );
        assert_eq!(
            j.acknowledge(info.operation).unwrap().covered_generations(),
            2
        );
        assert!(pending(&mut state).is_empty());
        assert_eq!(read(&mut state, a), b"old");
        assert_eq!(read(&mut state, b), b"new");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn migration_reopen_survives_concurrent_process_spawning() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct Spawner(Arc<AtomicBool>, Option<std::thread::JoinHandle<()>>);
    impl Drop for Spawner {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
            self.1.take().unwrap().join().unwrap();
        }
    }
    let running = Arc::new(AtomicBool::new(true));
    let flag = running.clone();
    let _spawner = Spawner(
        running,
        Some(std::thread::spawn(move || {
            while flag.load(Ordering::SeqCst) {
                std::process::Command::new("/bin/true").status().unwrap();
            }
        })),
    );
    for _ in 0..100 {
        migration_six_preserves_verified_intents_and_rolls_back_conflicts();
    }
}
