use super::*;
use std::{
    fs,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn target() -> Target {
    Target::new([1; 32], &"a".repeat(64), "dir/日本語/a%2Fb + café.txt").unwrap()
}
fn opts(bytes: u64) -> Options {
    Options {
        max_bytes: bytes,
        max_generations: 100,
        max_extents_per_generation: 100,
    }
}

#[test]
fn operation_keys_recover_progress_and_receipts_without_rewinding_heads() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let mut state = State::open_managed(&path).unwrap();
    let mut j = Journal::new(&mut state, opts(100)).unwrap();
    let key = [9; 32];
    let id = j
        .begin_operation(&key, &target(), Base::Missing, None, 3)
        .unwrap();
    j.append_at(id, 0, b"abc").unwrap();
    assert_eq!(
        j.begin_operation(&key, &target(), Base::Missing, None, 3)
            .unwrap(),
        id
    );
    assert!(
        j.begin_operation(&key, &target(), Base::Missing, None, 4)
            .is_err()
    );
    let receipt = j.seal(id).unwrap();
    let child = j.begin(&target(), Base::Missing, Some(id), 0).unwrap();
    j.seal(child).unwrap();
    drop(state);
    let mut state = State::open_managed(&path).unwrap();
    let mut j = Journal::new(&mut state, opts(1)).unwrap();
    assert_eq!(
        j.begin_operation(&key, &target(), Base::Missing, None, 3)
            .unwrap(),
        id
    );
    assert_eq!(j.seal(id).unwrap(), receipt);
    assert_eq!(j.head(&target()).unwrap().unwrap().id, child);
    assert_eq!(j.read(id, 0, 3).unwrap(), b"abc");
    // A damaged mapping must invalidate the bound manifest, not associate the
    // old sealed bytes with a different intent or fall back to an old digest.
    state
        .connection
        .execute(
            "UPDATE staged_operations SET operation_key=?1",
            [[8; 32].as_slice()],
        )
        .unwrap();
    assert!(
        Journal::new(&mut state, opts(100))
            .unwrap()
            .read(id, 0, 3)
            .is_err()
    );
}

#[test]
fn begin_operation_failure_rolls_back_mapping_and_generation_and_abort_removes_only_draft_key() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    state.connection.execute_batch("CREATE TRIGGER reject_intent BEFORE INSERT ON staged_operations BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(
        Journal::new(&mut state, opts(100))
            .unwrap()
            .begin_operation(&[1; 32], &target(), Base::Missing, None, 0)
            .is_err()
    );
    let count: i64 = state
        .connection
        .query_row("SELECT count(*) FROM staged_generations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    state
        .connection
        .execute_batch("DROP TRIGGER reject_intent")
        .unwrap();
    let mut j = Journal::new(&mut state, opts(100)).unwrap();
    let id = j
        .begin_operation(&[1; 32], &target(), Base::Missing, None, 0)
        .unwrap();
    j.abort(id).unwrap();
    let next = j
        .begin_operation(&[1; 32], &target(), Base::Missing, None, 0)
        .unwrap();
    assert!(next > id);
    j.seal(next).unwrap();
    assert!(j.abort(next).is_err());
}

#[test]
fn copying_parent_pieces_preserves_sparse_holes_and_idempotent_partial_chunks() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let mut j = Journal::new(&mut state, opts(100)).unwrap();
    let hole = 3 * 1024_u64.pow(4);
    let parent = j
        .begin_operation(&[1; 32], &target(), Base::Missing, None, hole + 6)
        .unwrap();
    j.append(parent, b"abc").unwrap();
    j.append_zero(parent, hole).unwrap();
    j.append(parent, b"xyz").unwrap();
    let receipt = j.seal(parent).unwrap();
    let child = j
        .begin_operation(&[2; 32], &target(), Base::Missing, Some(parent), hole + 4)
        .unwrap();
    let mut range = CopyRange {
        generation: parent,
        manifest_hash: receipt.manifest_hash,
        offset: 1,
        length: hole + 4,
    };
    assert_eq!(j.copy_next_at(child, 0, range).unwrap(), 2);
    assert_eq!(j.copy_next_at(child, 0, range).unwrap(), 2);
    range.offset = 3;
    range.length = hole + 2;
    assert_eq!(j.copy_next_at(child, 2, range).unwrap(), hole);
    range.offset = hole + 3;
    range.length = 2;
    assert_eq!(j.copy_next_at(child, hole + 2, range).unwrap(), 2);
    j.seal(child).unwrap();
    assert_eq!(j.read(child, 0, 4).unwrap(), b"bc\0\0");
    assert_eq!(j.read(child, hole + 1, 10).unwrap(), b"\0xy");
    let draft = j.begin(&target(), Base::Missing, Some(child), 1).unwrap();
    range.generation = child;
    range.manifest_hash = [0; 32];
    range.offset = 0;
    range.length = 1;
    assert!(j.copy_next_at(draft, 0, range).is_err());
    assert_eq!(j.info(draft).unwrap().filled, 0);
}

#[test]
fn schema_four_migration_preserves_legacy_manifests_and_rolls_back_conflicting_operations() {
    for conflict in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state");
        let mut state = State::open_managed(&path).unwrap();
        let mut j = Journal::new(&mut state, opts(100)).unwrap();
        let id = j.begin(&target(), Base::Missing, None, 3).unwrap();
        j.append(id, b"old").unwrap();
        let receipt = j.seal(id).unwrap();
        state
            .connection
            .execute_batch("DROP TABLE write_settlements; DROP TABLE remote_flushes; DROP TABLE retained_remote_intents; DROP TABLE remote_intents; DROP TABLE staged_operations; PRAGMA user_version=4;")
            .unwrap();
        if conflict {
            state.connection.execute_batch("CREATE TABLE staged_operations (personal TEXT); INSERT INTO staged_operations VALUES ('preserve');").unwrap();
        }
        drop(state);
        let result = State::open_managed(&path);
        assert_eq!(result.is_err(), conflict);
        if let Ok(mut state) = result {
            assert_eq!(state.info().unwrap().schema_version, crate::SCHEMA_VERSION);
            let mut j = Journal::new(&mut state, opts(100)).unwrap();
            assert_eq!(j.seal(id).unwrap(), receipt);
            assert_eq!(j.read(id, 0, 3).unwrap(), b"old");
        } else {
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
                    .unwrap(),
                4
            );
            assert_eq!(
                db.query_row("SELECT personal FROM staged_operations", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "preserve"
            );
            assert_eq!(
                db.query_row("SELECT count(*) FROM staged_generations", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
    }
}

#[test]
fn empty_files_limits_and_failed_appends_keep_precise_builder_progress() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    assert!(
        Journal::new(
            &mut state,
            Options {
                max_bytes: 0,
                ..Options::default()
            }
        )
        .is_err()
    );
    let mut j = Journal::new(
        &mut state,
        Options {
            max_bytes: 10,
            max_generations: 2,
            max_extents_per_generation: 1,
        },
    )
    .unwrap();
    let empty = stage(&mut j, &target(), None, b"");
    assert!(j.read(empty, 0, 1).unwrap().is_empty());
    let draft = j.begin(&target(), Base::Missing, Some(empty), 3).unwrap();
    assert!(j.append(draft, b"").is_err());
    assert!(j.append_zero(draft, 0).is_err());
    assert!(j.append_zero(draft, 4).is_err());
    assert_eq!(j.info(draft).unwrap().filled, 0);
    j.append(draft, b"a").unwrap();
    let error = j.append_zero(draft, 2).unwrap_err();
    assert_eq!(
        error.downcast_ref::<Failure>(),
        Some(&Failure::QuotaExceeded)
    );
    assert_eq!(j.info(draft).unwrap().filled, 1);
    assert_eq!(j.read_draft(draft, 0, 10).unwrap(), b"a");
    assert!(j.begin(&target(), Base::Missing, Some(empty), 0).is_err());
    j.abort(draft).unwrap();
    let later = j.begin(&target(), Base::Missing, Some(empty), 0).unwrap();
    assert!(later > draft, "aborted generation ID was reused");
}

#[cfg(target_os = "linux")]
#[test]
fn tmpfs_cannot_acknowledge_durable_staging() {
    let shared = fs::canonicalize("/dev/shm")
        .expect("Linux staging acceptance needs an existing tmpfs fixture");
    let temp = tempfile::tempdir_in(shared).unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let mut j = Journal::new(&mut state, opts(30)).unwrap();
    let error = j.begin(&target(), Base::Missing, None, 0).unwrap_err();
    assert!(error.to_string().contains("tmpfs"));
    assert!(j.pending(None, 10).unwrap().is_empty());
    assert!(j.drafts(None, 10).unwrap().is_empty());
}

#[test]
fn lost_append_acknowledgment_can_be_replayed_without_duplicate_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let mut j = Journal::new(&mut state, opts(30)).unwrap();
    let id = j.begin(&target(), Base::Missing, None, 6).unwrap();
    assert!(
        j.append_inner(id, Content::Bytes(b"abc"), Some(0), |at| {
            if at == AppendPoint::Committed {
                Err("lost append acknowledgment".into())
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    j.append_at(id, 0, b"abc").unwrap();
    assert_eq!(j.info(id).unwrap().filled, 3);
    assert!(j.append_at(id, 0, b"xyz").is_err());
    assert!(j.append_at(id, 4, b"x").is_err());
    j.append_zero_at(id, 3, 3).unwrap();
    j.append_zero_at(id, 3, 3).unwrap();
    assert_eq!(j.info(id).unwrap().filled, 6);
    j.seal(id).unwrap();
    assert_eq!(j.read(id, 0, 20).unwrap(), b"abc\0\0\0");
}

#[tokio::test]
async fn clean_cache_eviction_cannot_remove_pending_staged_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let id = {
        let mut state = State::open_managed(&path).unwrap();
        let mut j = Journal::new(&mut state, opts(30)).unwrap();
        stage(&mut j, &target(), None, b"dirty")
    };
    let cache = crate::cache::Cache::open(
        &path,
        crate::cache::Options {
            max_bytes: 3,
            max_file_bytes: 3,
            max_entries: 1,
        },
    )
    .await
    .unwrap();
    for name in ["first", "second"] {
        let key =
            crate::cache::Key::new([1; 32], &"a".repeat(64), name, Some("etag"), None, 3).unwrap();
        drop(
            cache
                .put(key, std::sync::Arc::from(b"abc".as_slice()))
                .await
                .unwrap(),
        );
    }
    cache.close().await.unwrap();
    let mut state = State::open_managed(&path).unwrap();
    let j = Journal::new(&mut state, opts(30)).unwrap();
    assert_eq!(j.read(id, 0, 20).unwrap(), b"dirty");
    assert_eq!(j.pending(None, 10).unwrap().len(), 1);
}
fn stage(
    j: &mut Journal<'_>,
    target: &Target,
    parent: Option<GenerationId>,
    bytes: &[u8],
) -> GenerationId {
    let id = j
        .begin(target, Base::Missing, parent, bytes.len() as u64)
        .unwrap();
    for chunk in bytes.chunks(CHUNK_BYTES) {
        j.append(id, chunk).unwrap();
    }
    j.seal(id).unwrap();
    id
}

#[test]
fn sparse_generations_restart_and_seal_replay_do_not_rewind_heads() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let t = target();
    let (old, new, receipt) = {
        let mut state = State::open_managed(&path).unwrap();
        let mut j = Journal::new(&mut state, opts(50)).unwrap();
        let size = 3 * 1024u64.pow(4);
        let old = j.begin(&t, Base::Missing, None, size).unwrap();
        j.append(old, b"abc").unwrap();
        j.append_zero(old, size - 6).unwrap();
        j.append(old, b"xyz").unwrap();
        let receipt = j.seal(old).unwrap();
        assert_eq!(j.read(old, 0, 6).unwrap(), b"abc\0\0\0");
        assert_eq!(j.read(old, size - 5, 10).unwrap(), b"\0\0xyz");
        assert!(j.read(old, size, 20).unwrap().is_empty());
        assert!(j.read(old, 0, MAX_READ_BYTES + 1).is_err());
        let new = stage(&mut j, &t, Some(old), b"replacement");
        assert_eq!(j.seal(old).unwrap(), receipt);
        assert_eq!(j.head(&t).unwrap().unwrap().id, new);
        assert!(j.append(old, b"bad").is_err());
        assert!(j.abort(old).is_err());
        assert_eq!(j.pending(None, 1).unwrap()[0].id, old);
        assert_eq!(j.pending(Some(old), 1).unwrap()[0].id, new);
        (old, new, receipt)
    };
    let mut state = State::open_managed(&path).unwrap();
    let j = Journal::new(&mut state, opts(1)).unwrap();
    // A smaller configured quota never deletes acknowledged data or blocks inspection.
    assert_eq!(
        j.info(old).unwrap().manifest_hash,
        Some(receipt.manifest_hash)
    );
    assert_eq!(j.head(&t).unwrap().unwrap().id, new);
    assert_eq!(j.read(new, 0, 100).unwrap(), b"replacement");
    assert_eq!(j.read(old, 0, 4).unwrap(), b"abc\0");
    assert!(!format!("{:?}", t).contains("café"));
}

#[test]
fn source_generations_and_exact_unicode_keys_have_separate_local_heads() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let targets = [
        Target::new([1; 32], &"a".repeat(64), "a%2Fb + café").unwrap(),
        Target::new([1; 32], &"a".repeat(64), "a%2Fb + cafe\u{301}").unwrap(),
        Target::new([1; 32], &"a".repeat(64), "a/b + café").unwrap(),
        Target::new([2; 32], &"a".repeat(64), "a%2Fb + café").unwrap(),
    ];
    let mut state = State::open_managed(&path).unwrap();
    let mut j = Journal::new(&mut state, opts(30)).unwrap();
    for (i, target) in targets.iter().enumerate() {
        stage(&mut j, target, None, &[i as u8]);
    }
    let wrong = Target::new([1; 32], &"b".repeat(64), "a%2Fb + café").unwrap();
    assert!(j.begin(&wrong, Base::Missing, None, 0).is_err());
    assert!(j.head(&wrong).is_err());
    drop(state);
    let mut state = State::open_managed(&path).unwrap();
    let j = Journal::new(&mut state, opts(30)).unwrap();
    for (i, target) in targets.iter().enumerate() {
        let head = j.head(target).unwrap().unwrap();
        assert_eq!(&head.target, target);
        assert_eq!(j.read(head.id, 0, 10).unwrap(), [i as u8]);
    }
}

#[test]
fn competing_builders_preserve_conflict_bytes_and_enforce_parent_identity() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let t = target();
    let mut j = Journal::new(&mut state, opts(30)).unwrap();
    let first = j.begin(&t, Base::Missing, None, 3).unwrap();
    let other = j.begin(&t, Base::Missing, None, 3).unwrap();
    j.append(first, b"one").unwrap();
    j.append(other, b"two").unwrap();
    j.seal(other).unwrap();
    assert!(j.seal(first).is_err());
    assert_eq!(j.read_draft(first, 0, 10).unwrap(), b"one");
    assert_eq!(j.head(&t).unwrap().unwrap().id, other);
    assert_eq!(j.pending(None, 100).unwrap().len(), 1);
    let base = Base::Existing {
        etag: Some("new-remote-base".into()),
        version: None,
    };
    assert!(j.begin(&t, base, Some(other), 1).is_err());
    let different = Target::new([1; 32], &"a".repeat(64), "different").unwrap();
    assert!(j.begin(&different, Base::Missing, Some(other), 1).is_err());
    j.abort(first).unwrap();
    assert!(j.info(first).is_err());
    assert_eq!(j.read(other, 0, 10).unwrap(), b"two");
}

#[test]
fn quotas_deduplication_and_abort_never_reclaim_published_chunks() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let t = target();
    let mut j = Journal::new(&mut state, opts(6)).unwrap();
    let first = stage(&mut j, &t, None, b"abc");
    let second = j.begin(&t, Base::Missing, Some(first), 6).unwrap();
    j.append(second, b"abc").unwrap();
    j.append(second, b"def").unwrap();
    j.seal(second).unwrap();
    assert!(
        j.state
            .connection
            .execute("DELETE FROM staged_chunks", [])
            .is_err(),
        "retention foreign keys did not protect published payloads"
    );
    let failed = j.begin(&t, Base::Missing, Some(second), 3).unwrap();
    assert!(j.append(failed, b"ghi").is_err());
    assert_eq!(j.info(failed).unwrap().filled, 0);
    j.abort(failed).unwrap();
    let shared = j.begin(&t, Base::Missing, Some(second), 3).unwrap();
    j.append(shared, b"abc").unwrap();
    j.abort(shared).unwrap();
    assert_eq!(j.read(first, 0, 20).unwrap(), b"abc");
    assert_eq!(j.read(second, 0, 20).unwrap(), b"abcdef");
    let count: i64 = j
        .state
        .connection
        .query_row("SELECT count(*) FROM staged_chunks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
    // Two unpublished builders can share one new chunk. Aborting either must
    // retain the other's data; aborting both reclaims that unpublished payload.
    let mut j = Journal::new(j.state, opts(20)).unwrap();
    let a = j.begin(&t, Base::Missing, Some(second), 3).unwrap();
    let b = j.begin(&t, Base::Missing, Some(second), 3).unwrap();
    j.append(a, b"ghi").unwrap();
    j.append(b, b"ghi").unwrap();
    j.abort(a).unwrap();
    assert_eq!(j.read_draft(b, 0, 10).unwrap(), b"ghi");
    j.abort(b).unwrap();
    let count: i64 = j
        .state
        .connection
        .query_row("SELECT count(*) FROM staged_chunks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn corruption_is_reported_without_reset_or_cleanup_of_dirty_bytes() {
    for fault in [
        "UPDATE staged_chunks SET data=x'78797a'",
        "DELETE FROM staged_extents",
        "UPDATE staged_generations SET base_absent=0,base_etag='tampered'",
        "DELETE FROM write_journal",
        "PRAGMA ignore_check_constraints=ON; UPDATE staged_generations SET object_key=printf('%.*c',2048,'x'); PRAGMA ignore_check_constraints=OFF;",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut state = State::open_managed(&temp.path().join("state")).unwrap();
        let mut j = Journal::new(&mut state, opts(20)).unwrap();
        let id = stage(&mut j, &target(), None, b"abc");
        j.state.connection.execute_batch(fault).unwrap();
        assert!(j.read(id, 0, 10).is_err(), "{fault}");
        assert!(j.abort(id).is_err());
        let count: i64 = j
            .state
            .connection
            .query_row("SELECT count(*) FROM staged_chunks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}

#[test]
fn missing_or_rewound_head_is_not_silently_adopted() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let t = target();
    let mut j = Journal::new(&mut state, opts(30)).unwrap();
    let old = stage(&mut j, &t, None, b"old");
    let new = stage(&mut j, &t, Some(old), b"new");
    j.state
        .connection
        .execute("UPDATE staged_heads SET generation_id=?1", [old.0])
        .unwrap();
    assert!(j.head(&t).is_err());
    assert!(j.begin(&t, Base::Missing, Some(old), 3).is_err());
    assert_eq!(j.read(new, 0, 10).unwrap(), b"new");
    j.state
        .connection
        .execute("DELETE FROM staged_heads", [])
        .unwrap();
    assert!(j.head(&t).is_err());
}

#[test]
fn incomplete_or_failed_local_publication_cannot_become_a_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let t = target();
    let mut j = Journal::new(&mut state, opts(30)).unwrap();
    let id = j.begin(&t, Base::Missing, None, 6).unwrap();
    j.append(id, b"abc").unwrap();
    assert!(j.seal(id).is_err());
    assert!(j.head(&t).unwrap().is_none());
    assert!(j.pending(None, 10).unwrap().is_empty());
    j.append(id, b"def").unwrap();
    for point in [
        SealPoint::Validated,
        SealPoint::Retained,
        SealPoint::Journaled,
        SealPoint::HeadUpdated,
    ] {
        assert!(
            j.seal_inner(id, |at| if at == point {
                Err("injected publication failure".into())
            } else {
                Ok(())
            })
            .is_err()
        );
        assert_eq!(j.info(id).unwrap().phase, Phase::Building);
        assert!(j.head(&t).unwrap().is_none());
        assert!(j.pending(None, 10).unwrap().is_empty());
    }
    // Lost acknowledgment after a committed seal: replay must identify it.
    assert!(
        j.seal_inner(id, |at| if at == SealPoint::Committed {
            Err("lost receipt".into())
        } else {
            Ok(())
        })
        .is_err()
    );
    assert_eq!(j.seal(id).unwrap().generation, id);
    assert_eq!(j.read(id, 0, 10).unwrap(), b"abcdef");
}

#[test]
fn sqlite_full_rolls_back_append_and_preserves_earlier_acknowledged_data() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let t = target();
    let mut j = Journal::new(&mut state, opts(2 * CHUNK_BYTES as u64)).unwrap();
    let before = stage(&mut j, &t, None, b"keep");
    let next = j
        .begin(&t, Base::Missing, Some(before), CHUNK_BYTES as u64)
        .unwrap();
    let pages: i64 = j
        .state
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    j.state
        .connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let error = j.append(next, &vec![7; CHUNK_BYTES]).unwrap_err();
    assert!(crate::cache::is_disk_full(error.as_ref()));
    assert_eq!(j.info(next).unwrap().filled, 0);
    assert_eq!(j.read(before, 0, 10).unwrap(), b"keep");
    assert_eq!(j.head(&t).unwrap().unwrap().id, before);
    assert!(j.seal(next).is_err());
}

#[test]
fn memory_base_revision_changes_do_not_publish_mixed_bytes() {
    struct MemoryBase {
        revision: u8,
        bytes: Vec<u8>,
    }
    impl MemoryBase {
        fn read(&self, expected: u8, offset: usize) -> Result<Vec<u8>> {
            if self.revision != expected {
                return Err("base revision changed".into());
            }
            Ok(self.bytes[offset..(offset + 3).min(self.bytes.len())].to_vec())
        }
    }
    let mut base = MemoryBase {
        revision: 1,
        bytes: b"original".to_vec(),
    };
    let temp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&temp.path().join("state")).unwrap();
    let mut j = Journal::new(&mut state, opts(30)).unwrap();
    let id = j
        .begin(
            &target(),
            Base::Existing {
                etag: Some("revision-1".into()),
                version: None,
            },
            None,
            base.bytes.len() as u64,
        )
        .unwrap();
    j.append(id, &base.read(1, 0).unwrap()).unwrap();
    base.revision = 2;
    base.bytes = b"replaced".to_vec();
    assert!(base.read(1, 3).is_err());
    assert!(j.seal(id).is_err());
    assert!(j.head(&target()).unwrap().is_none());
    assert_eq!(j.read_draft(id, 0, 10).unwrap(), b"ori");
}

#[test]
fn migration_three_preserves_clean_data_and_rolls_back_on_conflicting_tables() {
    for conflict in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state");
        let mut state = State::open_managed(&path).unwrap();
        state.register_source(&[1; 32], &"a".repeat(64)).unwrap();
        state
            .connection
            .execute(
                "INSERT INTO clean_blobs VALUES (?1,?2,3,?3,?4,1)",
                params![
                    [9u8; 32].as_slice(),
                    [1u8; 32].as_slice(),
                    Sha256::digest(b"abc").as_slice(),
                    b"abc".as_slice()
                ],
            )
            .unwrap();
        state.connection.execute_batch("DROP TABLE write_settlements; DROP TABLE remote_flushes; DROP TABLE retained_remote_intents; DROP TABLE remote_intents; DROP TABLE staged_operations; DROP TABLE write_journal; DROP TABLE staged_heads; DROP TABLE staged_extents; DROP TABLE retained_staged_chunks; DROP TABLE staged_chunks; DROP TABLE staged_generations; PRAGMA user_version=3;").unwrap();
        if conflict {
            state.connection.execute_batch("CREATE TABLE staged_chunks(personal TEXT); INSERT INTO staged_chunks VALUES ('keep');").unwrap();
        }
        drop(state);
        let result = State::open_managed(&path);
        assert_eq!(result.is_err(), conflict);
        drop(result);
        let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
        let version: i32 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, if conflict { 3 } else { crate::SCHEMA_VERSION });
        let count: i64 = db
            .query_row("SELECT count(*) FROM source_bindings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let clean: Vec<u8> = db
            .query_row("SELECT data FROM clean_blobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(clean, b"abc");
        if conflict {
            assert_eq!(
                db.query_row("SELECT personal FROM staged_chunks", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "keep"
            );
            assert!(db.prepare("SELECT * FROM staged_generations").is_err());
        }
    }
}

#[test]
fn crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_TEST_STAGE_DIRECTORY") else {
        return;
    };
    let ready = std::env::var_os("S3DRIVE_TEST_STAGE_READY").unwrap();
    let point = std::env::var("S3DRIVE_TEST_STAGE_POINT").unwrap();
    let mut state = State::open_managed(Path::new(&path)).unwrap();
    let mut j = Journal::new(&mut state, opts(100)).unwrap();
    let t = target();
    let old = stage(&mut j, &t, None, b"before");
    let id = j
        .begin_operation(&[7; 32], &t, Base::Missing, Some(old), 5)
        .unwrap();
    let signal = |name: String| -> Result<()> {
        if name == point {
            let marker = std::path::PathBuf::from(&ready).with_extension("tmp");
            fs::write(&marker, id.0.to_string())?;
            fs::rename(marker, &ready)?;
            let _ = std::io::stdin().read(&mut [0u8]);
        }
        Ok(())
    };
    if point.starts_with("Append") {
        j.append_inner(id, Content::Bytes(b"after"), None, |at| {
            signal(format!("Append{at:?}"))
        })
        .unwrap();
    } else {
        j.append(id, b"after").unwrap();
        j.seal_inner(id, |at| signal(format!("Seal{at:?}")))
            .unwrap();
    }
}

#[test]
fn process_death_at_local_transaction_boundaries_preserves_receipts() {
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for point in [
        "AppendChunkStored",
        "AppendExtentStored",
        "AppendCursorAdvanced",
        "AppendCommitted",
        "SealValidated",
        "SealRetained",
        "SealJournaled",
        "SealHeadUpdated",
        "SealCommitted",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state");
        let ready = temp.path().join("ready");
        let mut child = ChildGuard(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "journal::tests::crash_child", "--nocapture"])
                .env("S3DRIVE_TEST_STAGE_DIRECTORY", &path)
                .env("S3DRIVE_TEST_STAGE_READY", &ready)
                .env("S3DRIVE_TEST_STAGE_POINT", point)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        while !ready.exists() {
            assert!(Instant::now() < deadline, "child timeout at {point}");
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "child exited at {point}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let id = GenerationId(fs::read_to_string(&ready).unwrap().parse().unwrap());
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let mut state = State::open_managed(&path).unwrap();
        let mut j = Journal::new(&mut state, opts(100)).unwrap();
        assert_eq!(
            j.begin_operation(&[7; 32], &target(), Base::Missing, Some(GenerationId(1)), 5)
                .unwrap(),
            id
        );
        let committed = point == "SealCommitted";
        let progress = if point.starts_with("Seal") || point == "AppendCommitted" {
            5
        } else {
            0
        };
        assert_eq!(j.info(id).unwrap().filled, progress, "{point}");
        assert_eq!(
            j.info(id).unwrap().phase,
            if committed {
                Phase::Sealed
            } else {
                Phase::Building
            }
        );
        assert_eq!(j.read(GenerationId(1), 0, 20).unwrap(), b"before");
        if committed {
            assert_eq!(j.head(&target()).unwrap().unwrap().id, id);
            assert_eq!(j.read(id, 0, 10).unwrap(), b"after");
            j.seal(id).unwrap();
        } else {
            assert_eq!(j.head(&target()).unwrap().unwrap().id, GenerationId(1));
            assert_eq!(j.pending(None, 10).unwrap().len(), 1);
            if progress == 5 {
                assert_eq!(j.read_draft(id, 0, 10).unwrap(), b"after");
            } else {
                j.append(id, b"after").unwrap();
            }
            j.seal(id).unwrap();
        }
    }
}
