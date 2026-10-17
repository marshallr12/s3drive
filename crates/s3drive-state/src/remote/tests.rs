use super::*;
use crate::cache;
use std::{
    fs,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn target() -> Target {
    Target::new([1; 32], &"a".repeat(64), "日本語/a%2Fb + café.txt").unwrap()
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
fn proof(info: &Info, etag: &str) -> Verification {
    Verification {
        operation: info.operation,
        target: info.target.clone(),
        size: info.size,
        payload_sha256: info.payload_sha256,
        revision: Revision::new(etag, Some("version-1")).unwrap(),
    }
}

#[test]
fn empty_and_maximum_single_put_bodies_hash_fully_and_one_byte_more_is_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    for size in [0, MAX_BODY_BYTES, MAX_BODY_BYTES + 1] {
        let target = Target::new([1; 32], &"a".repeat(64), &format!("sparse-{size}")).unwrap();
        let mut local = journal::Journal::new(&mut state, journal::Options::default()).unwrap();
        let id = local
            .begin(&target, Base::Missing, None, size as u64)
            .unwrap();
        if size > 0 {
            local.append_zero(id, size as u64).unwrap();
        }
        local.seal(id).unwrap();
        let mut remote = Journal::new(&mut state, Options::default()).unwrap();
        if size > MAX_BODY_BYTES {
            failure(remote.prepare(id), Failure::TooLarge);
            assert!(remote.for_generation(id).unwrap().is_none());
        } else {
            let info = remote.prepare(id).unwrap();
            assert_eq!(info.size, size as u64);
            assert_eq!(
                info.payload_sha256,
                Sha256::digest(vec![0; size]).as_slice()
            );
            remote.claim(info.operation, &target).unwrap();
            remote.verified(proof(&info, "\"exact-boundary\"")).unwrap();
        }
        let local = journal::Journal::new(&mut state, journal::Options::default()).unwrap();
        assert_eq!(local.info(id).unwrap().size, size as u64);
        assert_eq!(local.head(&target).unwrap().unwrap().id, id);
        if size > 0 {
            assert_eq!(local.read(id, size as u64 - 1, 1).unwrap(), [0]);
        }
    }
}
fn failure<T>(result: Result<T>, kind: Failure) {
    let error = result.err().expect("expected failure");
    assert_eq!(error.downcast_ref::<Failure>(), Some(&kind), "{error}");
}

#[test]
fn one_permit_unknown_observation_and_verified_parent_guards_preserve_newer_dirty_heads() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let mut state = State::open_managed(&path).unwrap();
    let first = stage(&mut state, &target(), None, b"first");
    let mut r = Journal::new(&mut state, Options::default()).unwrap();
    let one = r.prepare(first).unwrap();
    assert_eq!(one.phase, Phase::Prepared);
    assert_eq!(one.guard, Guard::Missing);
    assert_eq!(one.payload_sha256, Sha256::digest(b"first").as_slice());
    assert_eq!(r.prepare(first).unwrap().operation, one.operation);
    let permit = r.claim(one.operation, &target()).unwrap();
    assert_eq!(permit.intent().generation, first);
    failure(
        r.claim(one.operation, &target()),
        Failure::AlreadyDispatched,
    );
    assert_eq!(
        r.uncertain(one.operation, Reason::Transport).unwrap().phase,
        Phase::Uncertain
    );
    let observed = r
        .observe_unmatched(one.operation, Observation::Absent)
        .unwrap();
    assert_eq!(observed.phase, Phase::Uncertain);
    assert_eq!(observed.reason, Reason::Transport);
    let second = stage(&mut state, &target(), Some(first), b"second");
    let mut r = Journal::new(&mut state, Options::default()).unwrap();
    failure(r.prepare(second), Failure::TargetBlocked);
    let original_proof = proof(&one, "\"remote-one\"");
    r.verified(original_proof.clone()).unwrap();
    let two = r.prepare(second).unwrap();
    assert_eq!(two.guard, Guard::Match("\"remote-one\"".into()));
    assert_eq!(two.anchor, Some(one.operation));
    assert_eq!(r.verified(original_proof).unwrap().phase, Phase::Verified);
    r.claim(two.operation, &target()).unwrap();
    r.verified(proof(&two, "\"remote-two\"")).unwrap();
    assert_eq!(
        r.info(one.operation).unwrap().revision.unwrap().etag(),
        "\"remote-one\""
    );
    let j = journal::Journal::new(&mut state, journal::Options::default()).unwrap();
    assert_eq!(j.head(&target()).unwrap().unwrap().id, second);
    assert_eq!(j.pending(None, 100).unwrap().len(), 2);
    assert_eq!(j.read(first, 0, 100).unwrap(), b"first");
    assert_eq!(j.read(second, 0, 100).unwrap(), b"second");
}

#[test]
fn startup_recovers_inflight_and_never_grants_a_second_permission() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let mut state = State::open_managed(&path).unwrap();
    let id = stage(&mut state, &target(), None, b"keep");
    let mut r = Journal::new(&mut state, Options::default()).unwrap();
    let info = r.prepare(id).unwrap();
    r.claim(info.operation, &target()).unwrap();
    drop(state);
    let mut state = State::open_managed(&path).unwrap();
    let mut r = Journal::new(&mut state, Options::default()).unwrap();
    let info = r.info(info.operation).unwrap();
    assert_eq!(info.phase, Phase::Uncertain);
    assert_eq!(info.reason, Reason::Interrupted);
    failure(
        r.claim(info.operation, &target()),
        Failure::AlreadyDispatched,
    );
    r.observe_unmatched(info.operation, Observation::Other)
        .unwrap();
    failure(
        r.claim(info.operation, &target()),
        Failure::AlreadyDispatched,
    );
    r.verified(proof(&info, "\"observed\"")).unwrap();
    drop(state);
    let mut state = State::open_managed(&path).unwrap();
    let r = Journal::new(&mut state, Options::default()).unwrap();
    assert_eq!(r.info(info.operation).unwrap().phase, Phase::Verified);
}

#[test]
fn stale_prepared_can_be_cancelled_but_claimed_rejected_or_uncertain_intents_cannot() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let first = stage(&mut state, &target(), None, b"one");
    let one = Journal::new(&mut state, Options::default())
        .unwrap()
        .prepare(first)
        .unwrap();
    let second = stage(&mut state, &target(), Some(first), b"two");
    let mut r = Journal::new(&mut state, Options::default()).unwrap();
    failure(r.claim(one.operation, &target()), Failure::StaleGeneration);
    r.cancel_prepared(one.operation).unwrap();
    assert_eq!(r.prepare(first).unwrap().phase, Phase::Cancelled);
    let two = r.prepare(second).unwrap();
    r.claim(two.operation, &target()).unwrap();
    failure(r.cancel_prepared(two.operation), Failure::AlreadyDispatched);
    r.rejected(two.operation, 412).unwrap();
    failure(r.cancel_prepared(two.operation), Failure::AlreadyDispatched);
    failure(
        r.claim(two.operation, &target()),
        Failure::AlreadyDispatched,
    );
    let third = stage(&mut state, &target(), Some(second), b"three");
    failure(
        Journal::new(&mut state, Options::default())
            .unwrap()
            .prepare(third),
        Failure::TargetBlocked,
    );
}

#[test]
fn wrong_binding_body_size_and_receipt_never_verify_or_rebind_an_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let id = stage(&mut state, &target(), None, b"data");
    let mut r = Journal::new(&mut state, Options::default()).unwrap();
    let info = r.prepare(id).unwrap();
    let wrong = Target::new([2; 32], target().fingerprint(), target().object_key()).unwrap();
    failure(r.claim(info.operation, &wrong), Failure::BindingMismatch);
    failure(r.verified(proof(&info, "\"tag\"")), Failure::InvalidState);
    r.claim(info.operation, &target()).unwrap();
    for bad in 0..3 {
        let mut p = proof(&info, "\"tag\"");
        match bad {
            0 => p.target = wrong.clone(),
            1 => p.size += 1,
            _ => p.payload_sha256 = [0; 32],
        };
        failure(r.verified(p), Failure::EvidenceMismatch);
        assert_eq!(r.info(info.operation).unwrap().phase, Phase::InFlight);
    }
    let p = proof(&info, "\"tag\"");
    r.verified(p.clone()).unwrap();
    r.verified(p).unwrap();
    failure(
        r.verified(proof(&info, "\"other\"")),
        Failure::EvidenceMismatch,
    );
    assert_eq!(
        r.uncertain(info.operation, Reason::Timeout).unwrap().phase,
        Phase::Verified
    );
    failure(r.rejected(info.operation, 412), Failure::InvalidState);
    assert!(Revision::new("*", None).is_err());
    assert!(Revision::new("W/\"weak\"", None).is_err());
    assert_eq!(
        Revision::new("\"tag\"", Some("null")).unwrap().version(),
        None
    );
}

#[test]
fn different_credential_generations_require_explicit_binding_resolution() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let one = stage(&mut state, &target(), None, b"old namespace");
    let mut r = Journal::new(&mut state, Options::default()).unwrap();
    let info = r.prepare(one).unwrap();
    r.claim(info.operation, &target()).unwrap();
    r.verified(proof(&info, "\"tag\"")).unwrap();
    let other = Target::new([2; 32], target().fingerprint(), target().object_key()).unwrap();
    let two = stage(&mut state, &other, None, b"new namespace");
    failure(
        Journal::new(&mut state, Options::default())
            .unwrap()
            .prepare(two),
        Failure::BindingMismatch,
    );
    assert_eq!(
        journal::Journal::new(&mut state, journal::Options::default())
            .unwrap()
            .read(one, 0, 100)
            .unwrap(),
        b"old namespace"
    );
}

#[test]
fn bounds_record_quota_and_version_only_bases_preserve_local_data() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let mut j = journal::Journal::new(&mut state, journal::Options::default()).unwrap();
    let huge = j
        .begin(&target(), Base::Missing, None, 3 * 1024_u64.pow(4))
        .unwrap();
    j.append_zero(huge, 3 * 1024_u64.pow(4)).unwrap();
    j.seal(huge).unwrap();
    failure(
        Journal::new(&mut state, Options::default())
            .unwrap()
            .prepare(huge),
        Failure::TooLarge,
    );
    let key = Target::new([1; 32], target().fingerprint(), "version-only").unwrap();
    let mut j = journal::Journal::new(&mut state, journal::Options::default()).unwrap();
    let version = j
        .begin(
            &key,
            Base::Existing {
                etag: None,
                version: Some("immutable".into()),
            },
            None,
            0,
        )
        .unwrap();
    j.seal(version).unwrap();
    failure(
        Journal::new(&mut state, Options::default())
            .unwrap()
            .prepare(version),
        Failure::InvalidInput,
    );
    let key = Target::new([1; 32], target().fingerprint(), "small").unwrap();
    let one = stage(&mut state, &key, None, b"a");
    let info = Journal::new(&mut state, Options { max_records: 1 })
        .unwrap()
        .prepare(one)
        .unwrap();
    let key = Target::new([1; 32], target().fingerprint(), "other").unwrap();
    let two = stage(&mut state, &key, None, b"b");
    failure(
        Journal::new(&mut state, Options { max_records: 1 })
            .unwrap()
            .prepare(two),
        Failure::QuotaExceeded,
    );
    assert_eq!(
        Journal::new(&mut state, Options { max_records: 1 })
            .unwrap()
            .info(info.operation)
            .unwrap()
            .size,
        1
    );
}

#[test]
fn lost_local_results_recover_identity_without_reissuing_permission() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let id = stage(&mut state, &target(), None, b"body");
    let mut r = Journal::new(&mut state, Options::default()).unwrap();
    assert!(
        r.prepare_inner(id, |point| if point == PreparePoint::Committed {
            Err("lost prepare reply".into())
        } else {
            Ok(())
        })
        .is_err()
    );
    let info = r.prepare(id).unwrap();
    assert!(
        r.claim_inner(info.operation, &target(), |point| {
            if point == ClaimPoint::Committed {
                Err("lost claim reply".into())
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    failure(
        r.claim(info.operation, &target()),
        Failure::AlreadyDispatched,
    );
    let p = proof(&info, "\"durable\"");
    assert!(
        r.verified_inner(p.clone(), |point| if point == VerifyPoint::Committed {
            Err("lost verify reply".into())
        } else {
            Ok(())
        })
        .is_err()
    );
    assert_eq!(r.verified(p).unwrap().phase, Phase::Verified);
}

#[test]
fn retained_markers_and_hashes_refuse_forgotten_or_rewound_attempts() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let id = stage(&mut state, &target(), None, b"keep");
    let mut r = Journal::new(&mut state, Options::default()).unwrap();
    let info = r.prepare(id).unwrap();
    r.claim(info.operation, &target()).unwrap();
    assert!(
        state
            .connection
            .execute("DELETE FROM remote_intents", [])
            .is_err()
    );
    state
        .connection
        .execute("UPDATE remote_intents SET phase='prepared'", [])
        .unwrap();
    failure(
        Journal::new(&mut state, Options::default())
            .unwrap()
            .claim(info.operation, &target()),
        Failure::Corrupt,
    );
    state
        .connection
        .execute("UPDATE remote_intents SET phase='in-flight'", [])
        .unwrap();
    assert_eq!(
        Journal::new(&mut state, Options::default())
            .unwrap()
            .info(info.operation)
            .unwrap()
            .phase,
        Phase::InFlight
    );
    state
        .connection
        .execute("DELETE FROM retained_remote_intents", [])
        .unwrap();
    failure(
        Journal::new(&mut state, Options::default())
            .unwrap()
            .info(info.operation),
        Failure::Corrupt,
    );
    state
        .connection
        .execute(
            "INSERT INTO retained_remote_intents VALUES (?1)",
            [info.operation.as_bytes().as_slice()],
        )
        .unwrap();
    state
        .connection
        .execute("PRAGMA ignore_check_constraints=ON", [])
        .unwrap();
    state
        .connection
        .execute(
            "UPDATE remote_intents SET object_key=?1",
            ["x".repeat(32768)],
        )
        .unwrap();
    failure(
        Journal::new(&mut state, Options::default())
            .unwrap()
            .info(info.operation),
        Failure::Corrupt,
    );
}

#[test]
fn sqlite_full_and_injected_claim_failure_roll_back_without_permissions() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let mut j = journal::Journal::new(&mut state, journal::Options::default()).unwrap();
    let tag = format!("\"{}\"", "a".repeat(8190));
    let id = j
        .begin(
            &target(),
            Base::Existing {
                etag: Some(tag),
                version: None,
            },
            None,
            1,
        )
        .unwrap();
    j.append(id, b"x").unwrap();
    j.seal(id).unwrap();
    state
        .connection
        .execute_batch("CREATE TABLE disk_pressure (payload BLOB)")
        .unwrap();
    let pages: i64 = state
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    state
        .connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    // Sealing can leave reusable pages. Consume those before testing a real
    // SQLITE_FULL during intent insertion, rather than assuming no freelist.
    loop {
        match state
            .connection
            .execute("INSERT INTO disk_pressure VALUES (zeroblob(4096))", [])
        {
            Ok(_) => (),
            Err(error) => {
                assert_eq!(
                    error.sqlite_error_code(),
                    Some(rusqlite::ErrorCode::DiskFull)
                );
                break;
            }
        }
    }
    let error = Journal::new(&mut state, Options::default())
        .unwrap()
        .prepare(id)
        .unwrap_err();
    assert!(cache::is_disk_full(error.as_ref()));
    assert!(
        Journal::new(&mut state, Options::default())
            .unwrap()
            .for_generation(id)
            .unwrap()
            .is_none()
    );
    state
        .connection
        .pragma_update(None, "max_page_count", pages + 100)
        .unwrap();
    let mut r = Journal::new(&mut state, Options::default()).unwrap();
    let info = r.prepare(id).unwrap();
    assert!(
        r.claim_inner(info.operation, &target(), |point| {
            if point == ClaimPoint::Updated {
                Err("injected before commit".into())
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    assert_eq!(r.info(info.operation).unwrap().phase, Phase::Prepared);
    r.claim(info.operation, &target()).unwrap();
    assert_eq!(
        journal::Journal::new(&mut state, journal::Options::default())
            .unwrap()
            .read(id, 0, 1)
            .unwrap(),
        b"x"
    );
}

#[test]
fn migration_five_preserves_manifests_and_conflicting_schema_rolls_back() {
    for conflict in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state");
        let mut state = State::open_managed(&path).unwrap();
        let id = stage(&mut state, &target(), None, b"old");
        state.connection.execute_batch("DROP TABLE logical_publication_links; DROP TABLE logical_edits; DROP TABLE logical_files; DROP TABLE managed_publications; DROP TABLE namespace_uploads; DROP TABLE namespace_stores; DROP TABLE namespace_operations; DROP TABLE namespace_nodes; DROP TABLE namespace_volume; DROP TABLE write_settlements; DROP TABLE remote_flushes; DROP TABLE retained_remote_intents; DROP TABLE remote_intents; PRAGMA user_version=5").unwrap();
        if conflict {
            state.connection.execute_batch("CREATE TABLE remote_intents (personal TEXT); INSERT INTO remote_intents VALUES ('preserve')").unwrap();
        }
        drop(state);
        let reopened = State::open_managed(&path);
        assert_eq!(reopened.is_err(), conflict);
        if let Ok(mut state) = reopened {
            assert_eq!(state.info().unwrap().schema_version, crate::SCHEMA_VERSION);
            assert_eq!(
                journal::Journal::new(&mut state, journal::Options::default())
                    .unwrap()
                    .read(id, 0, 100)
                    .unwrap(),
                b"old"
            );
        } else {
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
                    .unwrap(),
                5
            );
            assert_eq!(
                db.query_row("SELECT personal FROM remote_intents", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "preserve"
            );
        }
    }
}

#[test]
fn crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_TEST_REMOTE_DIRECTORY") else {
        return;
    };
    let ready = std::env::var_os("S3DRIVE_TEST_REMOTE_READY").unwrap();
    let at = std::env::var("S3DRIVE_TEST_REMOTE_POINT").unwrap();
    let mut state = State::open_managed(Path::new(&path)).unwrap();
    let id = stage(&mut state, &target(), None, b"payload");
    let signal = |point: String| -> Result<()> {
        if point == at {
            let temp = std::path::PathBuf::from(&ready).with_extension("tmp");
            fs::write(&temp, id.value().to_string())?;
            fs::rename(temp, &ready)?;
            let _ = std::io::stdin().read(&mut [0]);
        }
        Ok(())
    };
    let mut r = Journal::new(&mut state, Options::default()).unwrap();
    if at.starts_with("Prepare") {
        r.prepare_inner(id, |point| signal(format!("Prepare{point:?}")))
            .unwrap();
    } else {
        let info = r.prepare(id).unwrap();
        if at.starts_with("Claim") {
            r.claim_inner(info.operation, &target(), |point| {
                signal(format!("Claim{point:?}"))
            })
            .unwrap();
        } else {
            r.claim(info.operation, &target()).unwrap();
            r.verified_inner(proof(&info, "\"verified\""), |point| {
                signal(format!("Verify{point:?}"))
            })
            .unwrap();
        }
    }
}

#[test]
fn process_death_preserves_identity_and_never_reissues_committed_claims() {
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for point in [
        "PrepareStored",
        "PrepareCommitted",
        "ClaimRetained",
        "ClaimUpdated",
        "ClaimCommitted",
        "VerifyUpdated",
        "VerifyCommitted",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state");
        let ready = temp.path().join("ready");
        let mut child = Child(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "remote::tests::crash_child", "--nocapture"])
                .env("S3DRIVE_TEST_REMOTE_DIRECTORY", &path)
                .env("S3DRIVE_TEST_REMOTE_READY", &ready)
                .env("S3DRIVE_TEST_REMOTE_POINT", point)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        while !ready.exists() {
            assert!(Instant::now() < deadline, "timeout at {point}");
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "child exited at {point}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let id = GenerationId::from_database(fs::read_to_string(&ready).unwrap().parse().unwrap())
            .unwrap();
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let mut state = State::open_managed(&path).unwrap();
        let mut r = Journal::new(&mut state, Options::default()).unwrap();
        if point == "PrepareStored" {
            assert!(r.for_generation(id).unwrap().is_none());
            r.prepare(id).unwrap();
        }
        let info = r.for_generation(id).unwrap().unwrap();
        if point == "VerifyCommitted" {
            assert_eq!(info.phase, Phase::Verified);
            failure(
                r.claim(info.operation, &target()),
                Failure::AlreadyDispatched,
            );
        } else if point == "ClaimCommitted" || point == "VerifyUpdated" {
            assert_eq!(info.phase, Phase::Uncertain);
            failure(
                r.claim(info.operation, &target()),
                Failure::AlreadyDispatched,
            );
            r.verified(proof(&info, "\"verified\"")).unwrap();
        } else {
            assert_eq!(info.phase, Phase::Prepared);
            r.claim(info.operation, &target()).unwrap();
            failure(
                r.claim(info.operation, &target()),
                Failure::AlreadyDispatched,
            );
        }
        assert_eq!(
            journal::Journal::new(&mut state, journal::Options::default())
                .unwrap()
                .read(id, 0, 100)
                .unwrap(),
            b"payload"
        );
    }
}
