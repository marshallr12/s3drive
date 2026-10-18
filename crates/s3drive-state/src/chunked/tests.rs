use super::*;
use crate::{cache, worker::Storage};
use std::{
    fs,
    process::{Command, Stdio},
    time::Duration,
};

fn input(size: u64) -> Input {
    Input {
        id: [7; 32],
        volume: [1; 16],
        node: [2; 16],
        store: [3; 16],
        generation: [4; 16],
        root: "managed/".into(),
        source: [8; 32],
        fingerprint: "a".repeat(64),
        parent: None,
        size,
        expected_content: Some([9; 32]),
    }
}
fn freeze(s: &mut State, i: Input) -> Info {
    Journal::new(s, Options::default())
        .unwrap()
        .freeze(i)
        .unwrap()
}
fn stage(s: &mut State, i: &Input, bytes: &[u8]) -> GenerationId {
    let mut j = journal::Journal::new(s, journal::Options::default()).unwrap();
    let g = j
        .begin_operation(
            &i.staging_operation().unwrap(),
            &i.local_target().unwrap(),
            journal::Base::Missing,
            i.parent,
            i.size,
        )
        .unwrap();
    for chunk in bytes.chunks(manifest::CHUNK_BYTES as usize) {
        j.append(g, chunk).unwrap();
    }
    j.seal(g).unwrap();
    g
}
fn attach(s: &mut State, i: &Input, g: GenerationId) -> Info {
    Journal::new(s, Options::default())
        .unwrap()
        .attach(i.id, g)
        .unwrap()
}
fn prepare(s: &mut State, id: [u8; 32]) -> Info {
    let mut j = Journal::new(s, Options::default()).unwrap();
    let mut i = j.info(id).unwrap();
    for _ in 0..1000 {
        if i.root.is_some() {
            return i;
        }
        i = j.step(id, i.cursor).unwrap();
    }
    panic!("preparation failed to terminate")
}
fn row_count(s: &State) -> u64 {
    s.connection
        .query_row("SELECT count(*) FROM chunk_objects", [], |r| {
            read_number(r, 0)
        })
        .unwrap()
}
fn bytes(s: &mut State, id: [u8; 32], level: i8, slot: u64) -> PreparedObject {
    Journal::new(s, Options::default())
        .unwrap()
        .bytes(id, level, slot)
        .unwrap()
        .unwrap()
}

#[test]
fn sparse_multilevel_above_legacy_limit_matches_independent_pages_and_reads() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    let tail = "hello\0café 🦀".as_bytes();
    let i = input(128 * manifest::CHUNK_BYTES + tail.len() as u64);
    let frozen = freeze(&mut s, i.clone());
    let actual: String = i
        .input_hash()
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(actual, include_str!("fixtures/input.sha256").trim());
    assert!(frozen.generation.is_none());
    assert_eq!(row_count(&s), 0);
    let g = {
        let mut j = journal::Journal::new(&mut s, journal::Options::default()).unwrap();
        let g = j
            .begin_operation(
                &i.staging_operation().unwrap(),
                &i.local_target().unwrap(),
                journal::Base::Missing,
                None,
                i.size,
            )
            .unwrap();
        j.append(g, &vec![b'a'; manifest::CHUNK_BYTES as usize])
            .unwrap();
        j.append_zero(g, 127 * manifest::CHUNK_BYTES).unwrap();
        j.append(g, tail).unwrap();
        j.seal(g).unwrap();
        g
    };
    attach(&mut s, &i, g);
    let ready = prepare(&mut s, i.id);
    assert_eq!(row_count(&s), 132);
    let root_bytes = bytes(&mut s, i.id, 1, 0);
    let left = bytes(&mut s, i.id, 0, 0);
    let right = bytes(&mut s, i.id, 0, 1);
    assert_eq!(root_bytes.bytes, include_bytes!("fixtures/root.bin"));
    assert_eq!(left.bytes, include_bytes!("fixtures/leaf0.bin"));
    assert_eq!(right.bytes, include_bytes!("fixtures/leaf1.bin"));
    let root = ready.root().unwrap();
    let verified = root.verify_page(&root_bytes.bytes).unwrap();
    let leaf = verified.verify_child(0, &left.bytes).unwrap();
    leaf.verify_data(0, &bytes(&mut s, i.id, -1, 0).bytes)
        .unwrap();
    assert!(
        matches!(leaf.page().lookup(manifest::CHUNK_BYTES).unwrap().entry,Entry::Hole{length} if *length==127*manifest::CHUNK_BYTES)
    );
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .bytes(i.id, -1, 30)
            .unwrap()
            .is_none()
    );
    let end = verified.verify_child(1, &right.bytes).unwrap();
    end.verify_data(0, tail).unwrap();
    assert_eq!(bytes(&mut s, i.id, -1, 128).bytes, tail);
    assert_eq!(root.length(), i.size);
    assert_eq!(root.page(), &root_bytes.reference);
    assert_ne!(left.reference.ordinal(), root.page().ordinal());
    let preserved = ready.clone();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .info(i.id)
            .unwrap(),
        preserved
    );
}
#[test]
fn empty_and_partial_chunks_build_without_fabricating_data_objects() {
    for data in [
        Vec::new(),
        vec![0; 31],
        "日本語 + binary\0".as_bytes().to_vec(),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let i = input(data.len() as u64);
        freeze(&mut s, i.clone());
        let g = stage(&mut s, &i, &data);
        attach(&mut s, &i, g);
        let done = prepare(&mut s, i.id);
        let page = bytes(&mut s, i.id, 0, 0);
        let verified = done.root().unwrap().verify_page(&page.bytes).unwrap();
        assert_eq!(verified.page().length(), data.len() as u64);
        if !data.is_empty() {
            let data_object = Journal::new(&mut s, Options::default())
                .unwrap()
                .bytes(i.id, -1, 0)
                .unwrap();
            assert_eq!(data_object.is_none(), data.iter().all(|b| *b == 0));
            if let Some(o) = data_object {
                verified.verify_data(0, &o.bytes).unwrap();
            }
        }
    }
}
#[test]
fn original_ids_guards_routes_and_parent_bindings_cannot_be_replaced() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let i = input(3);
    let f = freeze(&mut s, i.clone());
    for changed in [
        Input {
            expected_content: None,
            ..i.clone()
        },
        Input {
            size: 4,
            ..i.clone()
        },
        Input {
            root: "elsewhere/".into(),
            ..i.clone()
        },
        Input {
            node: [5; 16],
            ..i.clone()
        },
        Input {
            generation: [5; 16],
            ..i.clone()
        },
        Input {
            source: [6; 32],
            ..i.clone()
        },
    ] {
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .freeze(changed)
                .is_err()
        );
        assert_eq!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .info(i.id)
                .unwrap(),
            f
        );
    }
    let mut same_scope = i.clone();
    same_scope.id = [6; 32];
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .freeze(same_scope)
            .is_err()
    );
    let wrong = Input {
        generation: [5; 16],
        id: [5; 32],
        ..i.clone()
    };
    freeze(&mut s, wrong.clone());
    let g = stage(&mut s, &wrong, b"abc");
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .attach(i.id, g)
            .is_err()
    );
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .info(i.id)
            .unwrap()
            .generation
            .is_none()
    );
}
#[test]
fn cursor_replay_reopen_and_exact_operation_mapping_preserve_progress() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    let i = input(manifest::CHUNK_BYTES + 3);
    freeze(&mut s, i.clone());
    let data = vec![42; i.size as usize];
    let g = stage(&mut s, &i, &data);
    let old = attach(&mut s, &i, g);
    let next = Journal::new(&mut s, Options::default())
        .unwrap()
        .step(i.id, old.cursor)
        .unwrap();
    assert_eq!(row_count(&s), 1);
    assert_eq!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .step(i.id, old.cursor)
            .unwrap(),
        next
    );
    assert_eq!(row_count(&s), 1);
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .info(i.id)
            .unwrap(),
        next
    );
    let done = prepare(&mut s, i.id);
    assert!(done.root.is_some());
    s.connection
        .execute(
            "DELETE FROM staged_operations WHERE generation_id=?1",
            [g.value()],
        )
        .unwrap();
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .info(i.id)
            .is_err()
    );
}
#[test]
fn corrupted_or_missing_bytes_never_become_holes_or_verified_prepared_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let i = input(4);
    freeze(&mut s, i.clone());
    let g = stage(&mut s, &i, b"data");
    attach(&mut s, &i, g);
    prepare(&mut s, i.id);
    s.connection
        .execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])
        .unwrap();
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .bytes(i.id, -1, 0)
            .is_err()
    );
    s.connection
        .execute_batch("PRAGMA foreign_keys=OFF")
        .unwrap();
    s.connection
        .execute("DELETE FROM staged_chunks", [])
        .unwrap();
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .bytes(i.id, -1, 0)
            .is_err()
    );
}
#[test]
fn missing_and_changed_prepared_records_fail_ready_and_reopen_validation() {
    for sql in [
        "UPDATE chunk_objects SET sha256=zeroblob(32)",
        "UPDATE chunk_objects SET ordinal=zeroblob(8) WHERE level=0",
        "UPDATE chunk_plans SET scope_hash=zeroblob(32)",
        "DELETE FROM chunk_objects WHERE level=-1",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut s = State::open_managed(&path).unwrap();
        let i = input(3);
        freeze(&mut s, i.clone());
        let g = stage(&mut s, &i, b"abc");
        attach(&mut s, &i, g);
        prepare(&mut s, i.id);
        s.connection.execute_batch(sql).unwrap();
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .info(i.id)
                .is_err(),
            "{sql}"
        );
        drop(s);
        assert!(State::open_managed(&path).is_err(), "{sql}");
    }
}
#[test]
fn payload_and_metadata_quotas_precede_work_and_keep_frozen_inputs() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let i = input(manifest::CHUNK_BYTES + 1);
    let options = Options {
        max_objects: 2,
        ..Options::default()
    };
    assert!(
        Journal::new(&mut s, options)
            .unwrap()
            .freeze(i.clone())
            .is_err()
    );
    assert_eq!(row_count(&s), 0);
    freeze(&mut s, i.clone());
    let g = stage(&mut s, &i, &vec![1; i.size as usize]);
    let limited = Options {
        max_scan_extents: 1,
        ..Options::default()
    };
    assert!(
        Journal::new(&mut s, limited)
            .unwrap()
            .attach(i.id, g)
            .is_err()
    );
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .info(i.id)
            .unwrap()
            .generation
            .is_none()
    );
    let invalid = Input {
        size: MAX_CHUNKS * manifest::CHUNK_BYTES + 1,
        ..i
    };
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .freeze(invalid)
            .is_err()
    );
}
#[test]
fn ignored_deleted_and_aborted_sql_effects_roll_back_entire_step() {
    for trigger in [
        "CREATE TRIGGER fail BEFORE INSERT ON chunk_objects BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER fail AFTER INSERT ON chunk_objects BEGIN DELETE FROM chunk_objects; END",
        "CREATE TRIGGER fail BEFORE UPDATE ON chunk_plans BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER fail AFTER UPDATE ON chunk_plans BEGIN DELETE FROM chunk_objects; END",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let i = input(3);
        freeze(&mut s, i.clone());
        let g = stage(&mut s, &i, b"abc");
        let before = attach(&mut s, &i, g);
        s.connection.execute_batch(trigger).unwrap();
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .step(i.id, before.cursor)
                .is_err(),
            "{trigger}"
        );
        assert_eq!(row_count(&s), 0);
        assert_eq!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .info(i.id)
                .unwrap(),
            before
        );
        s.connection.execute_batch("DROP TRIGGER fail").unwrap();
        assert!(prepare(&mut s, i.id).root.is_some());
    }
}
#[test]
fn real_sqlite_full_preserves_identity_and_cursor_and_allows_exact_resume() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let i = input(3);
    freeze(&mut s, i.clone());
    let g = stage(&mut s, &i, b"abc");
    let before = attach(&mut s, &i, g);
    s.connection
        .execute_batch("CREATE TABLE pressure(data BLOB)")
        .unwrap();
    s.connection.execute_batch("CREATE TRIGGER fail BEFORE INSERT ON chunk_objects BEGIN INSERT INTO pressure VALUES(zeroblob(65536)); END").unwrap();
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
            .execute("INSERT INTO pressure VALUES (zeroblob(4096))", [])
        {
            Ok(_) => (),
            Err(e) => {
                assert_eq!(e.sqlite_error_code(), Some(rusqlite::ErrorCode::DiskFull));
                break;
            }
        }
    }
    // Fill the remaining object B-tree page too, so a step must grow SQLite.

    let e = Journal::new(&mut s, Options::default())
        .unwrap()
        .step(i.id, before.cursor)
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<rusqlite::Error>()
            .and_then(|e| e.sqlite_error_code()),
        Some(rusqlite::ErrorCode::DiskFull)
    );
    assert_eq!(row_count(&s), 0);
    assert_eq!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .info(i.id)
            .unwrap(),
        before
    );
    s.connection
        .pragma_update(None, "max_page_count", 1000000)
        .unwrap();
    s.connection.execute_batch("DROP TRIGGER fail").unwrap();
    prepare(&mut s, i.id);
}
#[test]
fn schema_twenty_four_upgrade_keeps_legacy_state_and_collision_rolls_back() {
    for conflict in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut s = State::open_managed(&path).unwrap();
        let i = input(3);
        let g = stage(&mut s, &i, b"old");
        let before = journal::load(&s.connection, g).unwrap().manifest_hash;
        s.connection
            .execute_batch(
                "DROP TABLE chunk_source_settlements; DROP TABLE chunk_source_role; DROP TABLE chunk_publications; DROP TABLE chunk_client_role; DROP TABLE chunk_objects; DROP TABLE chunk_plans; PRAGMA user_version=24",
            )
            .unwrap();
        if conflict {
            s.connection.execute_batch("CREATE TABLE chunk_plans(keep TEXT);INSERT INTO chunk_plans VALUES('untouched')").unwrap();
        }
        drop(s);
        if conflict {
            assert!(State::open_managed(&path).is_err());
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value::<i32, _>(None, "user_version", |r| r.get(0))
                    .unwrap(),
                24
            );
            assert_eq!(
                db.query_row::<String, _, _>("SELECT keep FROM chunk_plans", [], |r| r.get(0))
                    .unwrap(),
                "untouched"
            );
        } else {
            let s = State::open_managed(&path).unwrap();
            assert_eq!(
                journal::load(&s.connection, g).unwrap().manifest_hash,
                before
            );
            assert_eq!(row_count(&s), 0);
        }
    }
}
#[tokio::test]
async fn async_owner_yields_prepares_and_reopens_the_same_frozen_plan() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let storage = Storage::open(
        &path,
        cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap();
    let plan = storage.chunked(Options::default()).unwrap();
    let i = input(9 * manifest::CHUNK_BYTES + 7);
    plan.freeze(i.clone()).await.unwrap();
    let staging = storage.staging();
    let g = staging
        .begin_operation(
            i.staging_operation().unwrap(),
            i.local_target().unwrap(),
            journal::Base::Missing,
            None,
            i.size,
        )
        .await
        .unwrap();
    for (index, piece) in vec![42; i.size as usize]
        .chunks(manifest::CHUNK_BYTES as usize)
        .enumerate()
    {
        staging
            .append_at(
                g,
                index as u64 * manifest::CHUNK_BYTES,
                std::sync::Arc::from(piece),
            )
            .await
            .unwrap();
    }
    staging.seal(g).await.unwrap();
    plan.attach(i.id, g).await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(20), plan.prepare(i.id))
        .await
        .unwrap()
        .unwrap();
    assert!(result.root().is_some());
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
            .chunked(Options::default())
            .unwrap()
            .info(i.id)
            .await
            .unwrap(),
        result
    );
    storage.close().await.unwrap();
}
#[test]
fn crash_child() {
    let Ok(path) = std::env::var("S3DRIVE_CHUNK_CRASH_DIR") else {
        return;
    };
    let mut s = State::open_managed(std::path::Path::new(&path)).unwrap();
    let i = input(manifest::CHUNK_BYTES + 1);
    freeze(&mut s, i.clone());
    let g = stage(&mut s, &i, &vec![42; i.size as usize]);
    let start = attach(&mut s, &i, g);
    Journal::new(&mut s, Options::default())
        .unwrap()
        .step(i.id, start.cursor)
        .unwrap();
    fs::write(
        std::path::Path::new(&path).with_extension("chunk-point"),
        b"committed",
    )
    .unwrap();
    std::thread::sleep(Duration::from_secs(60));
}
#[test]
fn process_death_recovers_committed_chunk_cursor_without_new_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "chunked::tests::crash_child", "--nocapture"])
        .env("S3DRIVE_CHUNK_CRASH_DIR", &path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while !path.with_extension("chunk-point").exists() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("child did not reach committed point");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let mut s = State::open_managed(&path).unwrap();
    let id = [7; 32];
    let before = Journal::new(&mut s, Options::default())
        .unwrap()
        .info(id)
        .unwrap();
    assert_eq!(before.cursor.next_chunk, 1);
    assert_eq!(row_count(&s), 1);
    let done = prepare(&mut s, id);
    assert_eq!(done.input, before.input);
    assert_eq!(done.generation, before.generation);
}

#[test]
fn every_chunk_table_is_refused_by_authority_roles_and_snapshot_preflight() {
    for (table, sql) in [
        (
            "chunk_source_role",
            "INSERT INTO chunk_source_role VALUES(1,1,zeroblob(32))",
        ),
        (
            "chunk_source_settlements",
            "INSERT INTO chunk_source_settlements VALUES(zeroblob(32),1,x'7b7d',zeroblob(32))",
        ),
        (
            "chunk_plans",
            "INSERT INTO chunk_plans VALUES(zeroblob(32),zeroblob(32),x'7b7d',zeroblob(32),1,NULL,NULL,0,-1,0,NULL,zeroblob(32))",
        ),
        (
            "chunk_objects",
            "INSERT INTO chunk_objects VALUES(zeroblob(32),-1,0,zeroblob(8),0,1,0,zeroblob(32),NULL,zeroblob(32))",
        ),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut s = State::open_managed(&path).unwrap();
        s.connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
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
fn malformed_large_cells_and_negative_cursors_fail_without_materializing_them() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let i = input(3);
    freeze(&mut s, i.clone());
    let g = stage(&mut s, &i, b"abc");
    attach(&mut s, &i, g);
    prepare(&mut s, i.id);
    s.connection
        .execute_batch("PRAGMA foreign_keys=OFF; PRAGMA ignore_check_constraints=ON")
        .unwrap();
    for mutation in [
        "UPDATE chunk_plans SET input=zeroblob(2097152)",
        "UPDATE chunk_plans SET input_hash=zeroblob(2097152)",
        "UPDATE chunk_plans SET scope_hash=zeroblob(2097152)",
        "UPDATE chunk_plans SET state_hash=zeroblob(2097152)",
        "UPDATE chunk_plans SET root=zeroblob(2097152)",
        "UPDATE chunk_plans SET next_chunk=-1",
        "UPDATE chunk_objects SET ordinal=zeroblob(2097152)",
        "UPDATE chunk_objects SET sha256=zeroblob(2097152)",
        "UPDATE chunk_objects SET record_hash=zeroblob(2097152)",
        "UPDATE chunk_objects SET body=zeroblob(2097152) WHERE level=0",
    ] {
        s.connection.execute_batch("BEGIN").unwrap();
        s.connection.execute_batch(mutation).unwrap();
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .info(i.id)
                .is_err(),
            "{mutation}"
        );
        s.connection.execute_batch("ROLLBACK").unwrap();
    }
    s.connection
        .execute_batch(
            "BEGIN;DELETE FROM chunk_objects;UPDATE chunk_plans SET plan_id=zeroblob(2097152)",
        )
        .unwrap();
    assert!(validate_all(&s.connection).is_err());
    s.connection
        .execute_batch("ROLLBACK;PRAGMA foreign_keys=ON;PRAGMA ignore_check_constraints=OFF")
        .unwrap();
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .info(i.id)
            .unwrap()
            .root
            .is_some()
    );
}

#[test]
fn recomputed_hash_cannot_legalize_a_finished_cursor_without_its_root() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let i = input(3);
    freeze(&mut s, i.clone());
    let g = stage(&mut s, &i, b"abc");
    attach(&mut s, &i, g);
    let mut finished = prepare(&mut s, i.id);
    finished.root = None;
    store(&s.connection, &finished).unwrap();
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .info(i.id)
            .is_err()
    );
}

#[test]
fn child_edits_keep_old_root_and_bytes_while_freezing_a_new_guard_and_object_uuid() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let old = input(2 * manifest::CHUNK_BYTES + 9);
    let original = vec![17; old.size as usize];
    freeze(&mut s, old.clone());
    let first = stage(&mut s, &old, &original);
    attach(&mut s, &old, first);
    let old_ready = prepare(&mut s, old.id);
    let mut edited = original.clone();
    edited[manifest::CHUNK_BYTES as usize + 3] = 99;
    edited.resize(edited.len() + 31, 0);
    let next = Input {
        id: [10; 32],
        generation: [5; 16],
        parent: Some(first),
        size: edited.len() as u64,
        expected_content: Some([11; 32]),
        ..old.clone()
    };
    freeze(&mut s, next.clone());
    let second = stage(&mut s, &next, &edited);
    attach(&mut s, &next, second);
    let done = prepare(&mut s, next.id);
    assert_eq!(done.input.expected_content, Some([11; 32]));
    assert_eq!(done.input.parent, Some(first));
    assert_ne!(done.root, old_ready.root);
    assert_eq!(
        bytes(&mut s, old.id, -1, 1).bytes,
        &original[manifest::CHUNK_BYTES as usize..2 * manifest::CHUNK_BYTES as usize]
    );
    assert_eq!(
        bytes(&mut s, next.id, -1, 1).bytes,
        &edited[manifest::CHUNK_BYTES as usize..2 * manifest::CHUNK_BYTES as usize]
    );
    let old_first = bytes(&mut s, old.id, -1, 0);
    let new_first = bytes(&mut s, next.id, -1, 0);
    assert_eq!(old_first.reference.sha256(), new_first.reference.sha256());
    assert_ne!(
        old_first.reference.generation(),
        new_first.reference.generation()
    );
    assert_eq!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .info(old.id)
            .unwrap(),
        old_ready
    );
}
#[test]
fn missing_dependency_refuses_root_completion_and_keeps_snapshot_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let i = input(3);
    freeze(&mut s, i.clone());
    let g = stage(&mut s, &i, b"abc");
    let attached = attach(&mut s, &i, g);
    let hashed = Journal::new(&mut s, Options::default())
        .unwrap()
        .step(i.id, attached.cursor)
        .unwrap();
    assert!(hashed.root.is_none());
    s.connection
        .execute("DELETE FROM chunk_objects", [])
        .unwrap();
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .step(i.id, hashed.cursor)
            .is_err()
    );
    assert!(
        s.connection
            .query_row::<Option<Vec<u8>>, _, _>("SELECT root FROM chunk_plans", [], |r| r.get(0))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        journal::read_data(&s.connection, g, 0, 3, false).unwrap(),
        b"abc"
    );
}
