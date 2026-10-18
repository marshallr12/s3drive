use super::*;
use crate::{cache, worker::Storage};
use std::{
    fs,
    io::Read,
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

fn target() -> Target {
    Target::new([8; 32], &"a".repeat(64), "multipart/日本語 + café.bin").unwrap()
}
fn limits() -> Limits {
    Limits {
        max_object_bytes: 128 * 1024 * 1024,
        min_nonfinal_part_bytes: 1,
        max_part_bytes: 8 * 1024 * 1024,
        max_parts: 10_000,
    }
}
fn stage(s: &mut State, parent: Option<GenerationId>, bytes: &[u8]) -> GenerationId {
    let mut j = journal::Journal::new(s, journal::Options::default()).unwrap();
    let id = j
        .begin(
            &target(),
            journal::Base::Missing,
            parent,
            bytes.len() as u64,
        )
        .unwrap();
    for piece in bytes.chunks(journal::CHUNK_BYTES) {
        j.append(id, piece).unwrap();
    }
    j.seal(id).unwrap();
    id
}
fn freeze(s: &mut State, id: Id, generation: GenerationId, preferred: u64) -> Info {
    Journal::new(s, Options::default())
        .unwrap()
        .freeze(id, generation, preferred, limits())
        .unwrap()
}
fn hash(s: &mut State, input: Info) -> HashWork {
    let mut work = HashWork::new(input);
    while work.offset < work.input.size {
        work = Journal::new(s, Options::default())
            .unwrap()
            .step(work)
            .unwrap();
    }
    work
}
fn finish(s: &mut State, work: HashWork) -> Info {
    Journal::new(s, Options::default())
        .unwrap()
        .finish(work, |_| Ok(()))
        .unwrap()
}
fn verified(s: &mut State, generation: GenerationId, tag: &str) -> remote::Info {
    let mut j = remote::Journal::new(s, remote::Options::default()).unwrap();
    let i = j.prepare(generation).unwrap();
    j.claim(i.operation, &i.target).unwrap();
    j.verified(remote::Verification {
        operation: i.operation,
        target: i.target.clone(),
        payload_sha256: i.payload_sha256,
        size: i.size,
        revision: remote::Revision::new(tag, None).unwrap(),
    })
    .unwrap()
}
fn assert_parts(info: &Info, bytes: &[u8]) {
    assert_eq!(info.phase(), Phase::Ready);
    assert_eq!(
        info.payload_sha256(),
        Some(&<[u8; 32]>::from(Sha256::digest(bytes)))
    );
    assert_eq!(info.parts.len(), usize::from(info.geometry.part_count()));
    for p in info.parts() {
        assert_eq!(
            p.sha256,
            <[u8; 32]>::from(Sha256::digest(
                &bytes[p.part.offset as usize..(p.part.offset + p.part.length) as usize]
            ))
        );
    }
}

#[test]
fn frozen_identity_and_non_aligned_parts_survive_reopen_without_target_permission() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    let bytes: Vec<u8> = (0..3 * journal::CHUNK_BYTES + 37)
        .map(|i| (i ^ (i >> 15)) as u8)
        .collect();
    let generation = stage(&mut s, None, &bytes);
    let id = Id::new([1; 32]);
    let preferred = journal::CHUNK_BYTES as u64 / 3 + 17;
    let input = freeze(&mut s, id, generation, preferred);
    assert_eq!(input.phase(), Phase::Frozen);
    assert!(input.parts().is_empty());
    assert!(input.payload_sha256().is_none());
    let ih = *input.input_hash();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    let input = freeze(&mut s, id, generation, preferred);
    assert_eq!(*input.input_hash(), ih);
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .freeze(id, generation, preferred + 1, limits())
            .is_err()
    );
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .freeze(Id::new([2; 32]), generation, preferred, limits())
            .is_err()
    );
    let work = hash(&mut s, input);
    let ready = finish(&mut s, work);
    assert_parts(&ready, &bytes);
    // This artifact has not created an old single-PUT intent or retained send claim.
    let (intents, claims): (i64,i64) = s.connection.query_row("SELECT (SELECT count(*) FROM remote_intents),(SELECT count(*) FROM retained_remote_intents)", [], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!((intents, claims), (0, 0));
    let ph = *ready.plan_hash().unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    let again = freeze(&mut s, id, generation, preferred);
    assert_eq!(again.phase(), Phase::Ready);
    let work = hash(&mut s, again);
    let again = finish(&mut s, work);
    assert_parts(&again, &bytes);
    assert_eq!(*again.plan_hash().unwrap(), ph);
}

#[test]
fn original_anchor_is_not_reselected_when_a_newer_head_and_receipt_appear() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let parent = stage(&mut s, None, b"parent");
    let anchor = verified(&mut s, parent, "\"parent\"");
    let child = stage(&mut s, Some(parent), b"child bytes");
    let id = Id::new([3; 32]);
    let input = freeze(&mut s, id, child, 3);
    assert_eq!(input.guard(), &remote::Guard::Match("\"parent\"".into()));
    assert_eq!(input.anchor(), Some(anchor.operation.as_bytes()));
    let ih = *input.input_hash();
    let receipt = *input.anchor_receipt().unwrap();
    let newer = stage(&mut s, Some(child), b"newer");
    verified(&mut s, newer, "\"newer\"");
    let same = freeze(&mut s, id, child, 3);
    assert_eq!(*same.input_hash(), ih);
    let work = hash(&mut s, same);
    let ready = finish(&mut s, work);
    assert_eq!(ready.guard(), &remote::Guard::Match("\"parent\"".into()));
    assert_eq!(ready.anchor_receipt(), Some(&receipt));
    assert_parts(&ready, b"child bytes");
    assert_eq!(
        journal::Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .head(&target())
            .unwrap()
            .unwrap()
            .id,
        newer
    );
    // A new key cannot silently rebase that old generation after the head moved.
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .freeze(Id::new([4; 32]), child, 3, limits())
            .is_err()
    );
}

#[test]
fn inherited_chunks_sparse_holes_and_small_final_part_hash_exactly() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let hole = journal::CHUNK_BYTES as u64 + 13;
    let mut j = journal::Journal::new(&mut s, journal::Options::default()).unwrap();
    let parent = j
        .begin(&target(), journal::Base::Missing, None, hole + 6)
        .unwrap();
    j.append(parent, b"abc").unwrap();
    j.append_zero(parent, hole).unwrap();
    j.append(parent, b"xyz").unwrap();
    let r = j.seal(parent).unwrap();
    let child = j
        .begin(&target(), journal::Base::Missing, Some(parent), hole + 7)
        .unwrap();
    let mut offset = 0;
    while offset < hole + 6 {
        offset += j
            .copy_next_at(
                child,
                offset,
                journal::CopyRange {
                    generation: parent,
                    manifest_hash: r.manifest_hash,
                    offset,
                    length: hole + 6 - offset,
                },
            )
            .unwrap();
    }
    j.append(child, b"!").unwrap();
    j.seal(child).unwrap();
    let mut bytes = b"abc".to_vec();
    bytes.resize((hole + 3) as usize, 0);
    bytes.extend_from_slice(b"xyz!");
    let input = freeze(&mut s, Id::new([5; 32]), child, 333_337);
    let work = hash(&mut s, input);
    let ready = finish(&mut s, work);
    assert_parts(&ready, &bytes);
}

#[test]
fn quotas_are_reserved_before_hashing_and_changed_binding_or_corruption_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let first = stage(&mut s, None, b"abc");
    let options = Options {
        max_plans: 1,
        max_reserved_parts: 3,
        max_scan_extents: 1,
    };
    let i = Journal::new(&mut s, options)
        .unwrap()
        .freeze(Id::new([6; 32]), first, 1, limits())
        .unwrap();
    let child = stage(&mut s, Some(first), b"def");
    assert!(
        Journal::new(&mut s, options)
            .unwrap()
            .freeze(Id::new([7; 32]), child, 1, limits())
            .is_err()
    );
    let before = s.connection.total_changes();
    assert_eq!(
        Journal::new(&mut s, options)
            .unwrap()
            .info(i.id())
            .unwrap()
            .phase(),
        Phase::Frozen
    );
    assert_eq!(s.connection.total_changes(), before);
    s.connection
        .execute_batch(
            "BEGIN; UPDATE multipart_plans SET input=x'7b226e65766572206563686f206d65223a317d'",
        )
        .unwrap();
    let error = Journal::new(&mut s, options)
        .unwrap()
        .info(i.id())
        .unwrap_err()
        .to_string();
    assert!(!error.contains("never echo me"));
    s.connection.execute_batch("ROLLBACK").unwrap();
    let work = hash(&mut s, i);
    let ready = finish(&mut s, work);
    s.connection
        .execute_batch("BEGIN; UPDATE multipart_parts SET sha256=zeroblob(32) WHERE part_number=1")
        .unwrap();
    assert!(
        Journal::new(&mut s, options)
            .unwrap()
            .info(ready.id())
            .is_err()
    );
    s.connection.execute_batch("ROLLBACK").unwrap();
    assert_eq!(validate_all(&s.connection).unwrap(), (1, 3));
}

#[test]
fn boundary_failures_leave_only_frozen_or_complete_ready_records() {
    for fail in [
        Point::Frozen,
        Point::FreezeCommitted,
        Point::Parts,
        Point::Ready,
        Point::ReadyCommitted,
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut s = State::open_managed(&path).unwrap();
        let generation = stage(&mut s, None, b"abcdefg");
        let id = Id::new([9; 32]);
        let mut hook = |p| -> Result<()> {
            if p == fail {
                Err("injected boundary".into())
            } else {
                Ok(())
            }
        };
        let input = Journal::new(&mut s, Options::default())
            .unwrap()
            .freeze_inner(id, generation, 2, limits(), &mut hook);
        if [Point::Frozen, Point::FreezeCommitted].contains(&fail) {
            assert!(input.is_err());
        } else {
            let work = hash(&mut s, input.unwrap());
            assert!(
                Journal::new(&mut s, Options::default())
                    .unwrap()
                    .finish(work, &mut hook)
                    .is_err()
            );
        }
        drop(s);
        let mut s = State::open_managed(&path).unwrap();
        let input = freeze(&mut s, id, generation, 2);
        assert_eq!(
            input.phase(),
            if fail == Point::ReadyCommitted {
                Phase::Ready
            } else {
                Phase::Frozen
            }
        );
        let count: i64 = s
            .connection
            .query_row("SELECT count(*) FROM multipart_parts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, if fail == Point::ReadyCommitted { 4 } else { 0 });
        let work = hash(&mut s, input);
        assert_parts(&finish(&mut s, work), b"abcdefg");
    }
}

#[test]
fn real_sqlite_full_rolls_back_part_rows_and_preserves_the_original_input() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let bytes = vec![42; 4096];
    let g = stage(&mut s, None, &bytes);
    let input = freeze(&mut s, Id::new([10; 32]), g, 1);
    let ih = *input.input_hash();
    let work = hash(&mut s, input);
    s.connection
        .execute_batch("CREATE TABLE disk_pressure (payload BLOB)")
        .unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    loop {
        match s
            .connection
            .execute("INSERT INTO disk_pressure VALUES (zeroblob(4096))", [])
        {
            Ok(_) => (),
            Err(e) => {
                assert_eq!(e.sqlite_error_code(), Some(rusqlite::ErrorCode::DiskFull));
                break;
            }
        }
    }
    let error = Journal::new(&mut s, Options::default())
        .unwrap()
        .finish(work, |_| Ok(()))
        .unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<rusqlite::Error>()
            .unwrap()
            .sqlite_error_code(),
        Some(rusqlite::ErrorCode::DiskFull)
    );
    let i = Journal::new(&mut s, Options::default())
        .unwrap()
        .info(Id::new([10; 32]))
        .unwrap();
    assert_eq!(i.phase(), Phase::Frozen);
    assert_eq!(*i.input_hash(), ih);
    assert!(i.parts().is_empty());
    s.connection
        .execute_batch("DROP TABLE disk_pressure")
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", 1_000_000)
        .unwrap();
    let work = hash(&mut s, i);
    assert_parts(&finish(&mut s, work), &bytes);
}

#[test]
fn corrupt_unread_bytes_fail_and_ready_metadata_does_not_claim_atomic_media_freshness() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let mut bytes = vec![1; journal::CHUNK_BYTES];
    bytes.extend(vec![2; journal::CHUNK_BYTES]);
    let g = stage(&mut s, None, &bytes);
    let i = freeze(&mut s, Id::new([11; 32]), g, 700_001);
    let work = Journal::new(&mut s, Options::default())
        .unwrap()
        .step(HashWork::new(i.clone()))
        .unwrap();
    s.connection.execute_batch("BEGIN").unwrap();
    s.connection
        .execute(
            "UPDATE staged_chunks SET data=?1 WHERE chunk_hash=?2",
            params![
                vec![3_u8; journal::CHUNK_BYTES],
                Sha256::digest(vec![2; journal::CHUNK_BYTES]).as_slice()
            ],
        )
        .unwrap();
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .step(work)
            .is_err()
    );
    s.connection.execute_batch("ROLLBACK").unwrap();
    let work = hash(&mut s, i);
    // Fault outside public sealed-generation mutation APIs, after all verified reads.
    s.connection
        .execute(
            "UPDATE staged_chunks SET data=?1 WHERE chunk_hash=?2",
            params![
                vec![3_u8; journal::CHUNK_BYTES],
                Sha256::digest(vec![1; journal::CHUNK_BYTES]).as_slice()
            ],
        )
        .unwrap();
    let ready = finish(&mut s, work);
    assert_parts(&ready, &bytes);
    assert!(
        journal::Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .read(g, 0, 1)
            .is_err()
    );
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .step(HashWork::new(ready))
            .is_err()
    );
}

#[tokio::test]
async fn async_planner_hashes_above_single_put_bound_yields_and_recovers_ready() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let storage = Storage::open(
        &path,
        cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap();
    let staging = storage.staging();
    let size = 16 * journal::CHUNK_BYTES + 7;
    let g = staging
        .begin_operation([1; 32], target(), journal::Base::Missing, None, size as u64)
        .await
        .unwrap();
    let piece: Arc<[u8]> = vec![31; journal::CHUNK_BYTES].into();
    for n in 0..16 {
        staging
            .append_at(g, (n * journal::CHUNK_BYTES) as u64, piece.clone())
            .await
            .unwrap();
    }
    staging
        .append_at(
            g,
            (16 * journal::CHUNK_BYTES) as u64,
            Arc::from(&b"tail!!!"[..]),
        )
        .await
        .unwrap();
    staging.seal(g).await.unwrap();
    let planner = storage.multipart(Options::default()).unwrap();
    let id = Id::new([12; 32]);
    let preferred = 3 * journal::CHUNK_BYTES as u64 + 11;
    let mut observations = 0;
    let ready = {
        let prepare = planner.prepare(id, g, preferred, limits());
        tokio::pin!(prepare);
        loop {
            tokio::select! { r=&mut prepare=>break r.unwrap(),r=staging.info(g)=>{ assert_eq!(r.unwrap().id,g);observations+=1;tokio::task::yield_now().await; } }
        }
    };
    assert!(observations > 1);
    assert_eq!(ready.phase(), Phase::Ready);
    let mut h = Sha256::new();
    for _ in 0..16 {
        h.update(&piece);
    }
    h.update(b"tail!!!");
    assert_eq!(
        ready.payload_sha256(),
        Some(&<[u8; 32]>::from(h.finalize()))
    );
    assert!(storage.uploads().prepare(g).await.is_err()); // Existing8MiB path unchanged.
    let digest = *ready.plan_hash().unwrap();
    storage.close().await.unwrap();
    let storage = Storage::open(
        &path,
        cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        storage
            .multipart(Options::default())
            .unwrap()
            .info(id)
            .await
            .unwrap()
            .plan_hash(),
        Some(&digest)
    );
    storage.close().await.unwrap();
}

#[test]
fn crash_child() {
    let Ok(path) = std::env::var("S3DRIVE_MULTIPART_TEST_DIRECTORY") else {
        return;
    };
    let ready = std::env::var("S3DRIVE_MULTIPART_TEST_READY").unwrap();
    let stop = std::env::var("S3DRIVE_MULTIPART_TEST_POINT").unwrap();
    let mut s = State::open_managed(std::path::Path::new(&path)).unwrap();
    let g = stage(&mut s, None, b"abcdefg");
    let hook = |p: Point| -> Result<()> {
        if format!("{p:?}") == stop {
            fs::write(&ready, g.value().to_string()).unwrap();
            let mut byte = [0];
            std::io::stdin().read_exact(&mut byte).unwrap();
            panic!("child unexpectedly resumed");
        }
        Ok(())
    };
    let i = Journal::new(&mut s, Options::default())
        .unwrap()
        .freeze_inner(Id::new([9; 32]), g, 2, limits(), hook)
        .unwrap();
    let work = hash(&mut s, i);
    Journal::new(&mut s, Options::default())
        .unwrap()
        .finish(work, hook)
        .unwrap();
    panic!("stop point not reached");
}
#[test]
fn process_death_preserves_original_input_and_all_or_nothing_ready_parts() {
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for point in [
        "Frozen",
        "FreezeCommitted",
        "Parts",
        "Ready",
        "ReadyCommitted",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let signal = tmp.path().join("ready");
        let mut child = Child(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "multipart::tests::crash_child", "--nocapture"])
                .env("S3DRIVE_MULTIPART_TEST_DIRECTORY", &path)
                .env("S3DRIVE_MULTIPART_TEST_READY", &signal)
                .env("S3DRIVE_MULTIPART_TEST_POINT", point)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        while !signal.exists() {
            assert!(Instant::now() < deadline, "child timeout at {point}");
            assert!(child.0.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(10));
        }
        let g = GenerationId::from_database(fs::read_to_string(&signal).unwrap().parse().unwrap())
            .unwrap();
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let mut s = State::open_managed(&path).unwrap();
        let input = freeze(&mut s, Id::new([9; 32]), g, 2);
        assert_eq!(
            input.phase(),
            if point == "ReadyCommitted" {
                Phase::Ready
            } else {
                Phase::Frozen
            }
        );
        let work = hash(&mut s, input);
        assert_parts(&finish(&mut s, work), b"abcdefg");
    }
}

#[test]
fn schema_twenty_three_migration_keeps_exact_legacy_hashes_and_conflicts_roll_back() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    let g = stage(&mut s, None, b"legacy bytes");
    verified(&mut s, g, "\"legacy\"");
    fn rows(db: &Connection) -> Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        db.prepare(
            "SELECT operation_id,intent_hash,state_hash FROM remote_intents ORDER BY generation_id",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
    }
    let original = rows(&s.connection);
    s.connection
        .execute_batch(
            "DROP TABLE chunk_publications; DROP TABLE chunk_client_role; DROP TABLE chunk_objects; DROP TABLE chunk_plans; DROP TABLE multipart_parts; DROP TABLE multipart_plans; PRAGMA user_version=23",
        )
        .unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
    assert_eq!(rows(&s.connection), original);
    let child = stage(&mut s, Some(g), b"new bytes");
    let input = freeze(&mut s, Id::new([13; 32]), child, 3);
    assert_eq!(input.guard(), &remote::Guard::Match("\"legacy\"".into()));
    // A preexisting conflicting table is not adopted with IF NOT EXISTS.
    s.connection.execute_batch("DROP TABLE chunk_publications; DROP TABLE chunk_client_role; DROP TABLE chunk_objects; DROP TABLE chunk_plans; DROP TABLE multipart_parts; DROP TABLE multipart_plans; CREATE TABLE multipart_plans(keep TEXT); INSERT INTO multipart_plans VALUES('untouched'); PRAGMA user_version=23").unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value::<i32, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        23
    );
    assert_eq!(
        db.query_row::<String, _, _>("SELECT keep FROM multipart_plans", [], |r| r.get(0))
            .unwrap(),
        "untouched"
    );
    assert_eq!(rows(&db), original);
    assert_eq!(
        db.query_row::<i64, _, _>(
            "SELECT count(*) FROM sqlite_schema WHERE name='multipart_parts'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn every_local_planning_table_is_refused_by_authority_initialization_and_snapshots() {
    for table in ["multipart_plans", "multipart_parts"] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut s = State::open_managed(&path).unwrap();
        // Deliberately orphan rows bypassing FKs test role checks themselves.
        s.connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        let sql = if table == "multipart_plans" {
            "INSERT INTO multipart_plans VALUES(zeroblob(32),1,x'7b7d',zeroblob(32),1,'frozen',NULL,NULL)"
        } else {
            "INSERT INTO multipart_parts VALUES(zeroblob(32),1,0,1,zeroblob(32))"
        };
        s.connection.execute_batch(sql).unwrap();
        assert!(s.namespace_initialize().is_err());
        s.connection
            .execute(&format!("DELETE FROM {table}"), [])
            .unwrap();
        let v = s.namespace_initialize().unwrap();
        s.connection.execute_batch(sql).unwrap();
        drop(s);
        let output = tmp.path().join("snapshot");
        assert!(
            crate::authority_snapshot::create(
                &path,
                &v.id,
                &v.name_policy,
                &output,
                &crate::authority_snapshot::Limits::default()
            )
            .is_err()
        );
        assert!(!output.exists());
        assert!(State::open_managed(&path).is_err());
    }
}

#[test]
fn shared_guard_extraction_keeps_legacy_error_precedence() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let parent = stage(&mut s, None, b"parent");
    let original = remote::Journal::new(&mut s, remote::Options::default())
        .unwrap()
        .prepare(parent)
        .unwrap();
    let foreign = Target::new([7; 32], target().fingerprint(), target().object_key()).unwrap();
    let mut j = journal::Journal::new(&mut s, journal::Options::default()).unwrap();
    let foreign_g = j.begin(&foreign, journal::Base::Missing, None, 1).unwrap();
    j.append(foreign_g, b"x").unwrap();
    j.seal(foreign_g).unwrap();
    let e = remote::Journal::new(&mut s, remote::Options { max_records: 1 })
        .unwrap()
        .prepare(foreign_g)
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<remote::Failure>(),
        Some(&remote::Failure::TargetBlocked)
    );
    remote::Journal::new(&mut s, remote::Options::default())
        .unwrap()
        .cancel_prepared(original.operation)
        .unwrap();
    let e = remote::Journal::new(&mut s, remote::Options { max_records: 1 })
        .unwrap()
        .prepare(foreign_g)
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<remote::Failure>(),
        Some(&remote::Failure::BindingMismatch)
    );
    let child = stage(&mut s, Some(parent), b"child");
    // The remote catalog quota precedes the separate staged-head binding check.
    let e = remote::Journal::new(&mut s, remote::Options { max_records: 1 })
        .unwrap()
        .prepare(child)
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<remote::Failure>(),
        Some(&remote::Failure::QuotaExceeded)
    );
    let same = remote::Journal::new(&mut s, remote::Options { max_records: 1 })
        .unwrap()
        .prepare(parent)
        .unwrap();
    assert_eq!(same.operation, original.operation);
    assert_eq!(same.phase, remote::Phase::Cancelled);
}

#[test]
fn reserved_parts_and_extent_work_are_independent_admission_limits() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let g = stage(&mut s, None, b"abc");
    let options = Options {
        max_plans: 10,
        max_reserved_parts: 2,
        max_scan_extents: 10,
    };
    assert!(
        Journal::new(&mut s, options)
            .unwrap()
            .freeze(Id::new([14; 32]), g, 1, limits())
            .is_err()
    );
    assert_eq!(validate_all(&s.connection).unwrap(), (0, 0));
    let options = Options {
        max_reserved_parts: 3,
        ..options
    };
    Journal::new(&mut s, options)
        .unwrap()
        .freeze(Id::new([14; 32]), g, 1, limits())
        .unwrap();
    let child = stage(&mut s, Some(g), b"x");
    assert!(
        Journal::new(&mut s, options)
            .unwrap()
            .freeze(Id::new([15; 32]), child, 1, limits())
            .is_err()
    );
    assert_eq!(validate_all(&s.connection).unwrap(), (1, 3));
    let mut j = journal::Journal::new(&mut s, journal::Options::default()).unwrap();
    let next = j
        .begin(&target(), journal::Base::Missing, Some(child), 2)
        .unwrap();
    j.append(next, b"x").unwrap();
    j.append(next, b"y").unwrap();
    j.seal(next).unwrap();
    let options = Options {
        max_plans: 10,
        max_reserved_parts: 100,
        max_scan_extents: 1,
    };
    assert!(
        Journal::new(&mut s, options)
            .unwrap()
            .freeze(Id::new([16; 32]), next, 2, limits())
            .is_err()
    );
    assert_eq!(validate_all(&s.connection).unwrap(), (1, 3));
}

#[test]
fn malformed_large_digest_and_identifier_cells_are_refused_before_materializing_them() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let g = stage(&mut s, None, b"abc");
    let id = Id::new([17; 32]);
    let input = freeze(&mut s, id, g, 1);
    let work = hash(&mut s, input);
    finish(&mut s, work);
    s.connection.execute_batch("PRAGMA ignore_check_constraints=ON; PRAGMA foreign_keys=OFF; BEGIN; UPDATE multipart_parts SET sha256=zeroblob(2097152) WHERE part_number=1").unwrap();
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .info(id)
            .is_err()
    );
    s.connection
        .execute_batch("ROLLBACK; BEGIN; UPDATE multipart_plans SET plan_id=zeroblob(2097152)")
        .unwrap();
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .for_generation(g)
            .is_err()
    );
    assert!(validate_all(&s.connection).is_err());
    // Remove orphan parts so catalog enumeration itself must reject the large ID.
    s.connection
        .execute("DELETE FROM multipart_parts", [])
        .unwrap();
    assert!(validate_all(&s.connection).is_err());
    s.connection
        .execute_batch("ROLLBACK; PRAGMA foreign_keys=ON; PRAGMA ignore_check_constraints=OFF")
        .unwrap();
    assert_eq!(validate_all(&s.connection).unwrap(), (1, 3));
}

#[test]
fn ignored_or_deleted_sql_effects_cannot_commit_partial_ready_or_a_false_frozen_input() {
    for trigger in [
        "CREATE TRIGGER sabotage BEFORE INSERT ON multipart_plans BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER sabotage BEFORE INSERT ON multipart_parts WHEN NEW.part_number=2 BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER sabotage AFTER INSERT ON multipart_parts WHEN NEW.part_number=2 BEGIN DELETE FROM multipart_parts WHERE part_number=1; END",
        "CREATE TRIGGER sabotage BEFORE UPDATE OF phase ON multipart_plans BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER sabotage AFTER UPDATE OF phase ON multipart_plans BEGIN DELETE FROM multipart_parts WHERE part_number=1; END",
        "CREATE TRIGGER sabotage AFTER UPDATE OF phase ON multipart_plans BEGIN UPDATE multipart_plans SET input=x'7b7d'; END",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let g = stage(&mut s, None, b"abcdefg");
        let id = Id::new([18; 32]);
        let freeze_sabotage = trigger.contains("ON multipart_plans BEGIN SELECT RAISE(IGNORE)")
            && trigger.contains("BEFORE INSERT");
        if freeze_sabotage {
            s.connection.execute_batch(trigger).unwrap();
            assert!(
                Journal::new(&mut s, Options::default())
                    .unwrap()
                    .freeze(id, g, 2, limits())
                    .is_err()
            );
            assert_eq!(validate_all(&s.connection).unwrap(), (0, 0));
        } else {
            let input = freeze(&mut s, id, g, 2);
            let ih = *input.input_hash();
            let work = hash(&mut s, input);
            s.connection.execute_batch(trigger).unwrap();
            assert!(
                Journal::new(&mut s, Options::default())
                    .unwrap()
                    .finish(work, |_| Ok(()))
                    .is_err()
            );
            let input = Journal::new(&mut s, Options::default())
                .unwrap()
                .info(id)
                .unwrap();
            assert_eq!(input.phase(), Phase::Frozen);
            assert_eq!(*input.input_hash(), ih);
            assert!(input.parts().is_empty());
        }
        s.connection.execute_batch("DROP TRIGGER sabotage").unwrap();
        let input = freeze(&mut s, id, g, 2);
        let work = hash(&mut s, input);
        assert_parts(&finish(&mut s, work), b"abcdefg");
    }
}
