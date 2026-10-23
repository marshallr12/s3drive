use super::*;
use crate::sparse_views::{self as views, preparation::retry};
use std::path::Path;
fn vj(s: &mut State) -> views::Journal<'_> {
    views::Journal::new(s, views::Options::default()).unwrap()
}
fn setup(s: &mut State) -> Fixture {
    let f = fixture();
    vj(s).begin([1; 32], f.binding.clone(), 17).unwrap();
    pj(s).enable_retry_safety().unwrap();
    f
}
fn cycle(
    s: &mut State,
    f: &mut Fixture,
    bytes: Vec<u8>,
) -> (
    Info,
    retry::Operation,
    retry::Checkpoint,
    Vec<super::super::super::Transfer>,
) {
    let old = vj(s).current_head(&f.binding).unwrap();
    let source = views::ScopedOperation::new(old.binding(), [2; 32]).unwrap();
    let v = vj(s).write_scoped(source, old.id(), 0, &bytes).unwrap();
    let token = retry::Operation::new(v.binding(), [9; 32]).unwrap();
    let i = pj(s)
        .begin_scoped(
            token,
            Request {
                operation: token.id().unwrap(),
                view: v.id(),
                generation: token.generation().unwrap(),
                base_root: f.base.root.encode().to_vec(),
                base_page: f.base.page.encode(),
            },
        )
        .unwrap();
    let i = pj(s)
        .append(i.id().unwrap(), i.cursor(), Candidate::Bytes(bytes.clone()))
        .unwrap();
    let i = super::super::super::tests::fixture::finish(s, i);
    let plan = i.id().unwrap();
    j(s).check(plan, 0, None).unwrap();
    physical(s, plan);
    let transfers = pj(s).transfers(plan, None, 128).unwrap();
    let r = j(s)
        .begin(
            plan,
            f.node.clone(),
            f.store.clone(),
            f.node.revision,
            token.publish_operation().unwrap(),
        )
        .unwrap();
    let r = lineage::commit(s, r);
    j(s).acknowledge_scoped(
        views::ScopedOperation::new(v.binding(), [40; 32]).unwrap(),
        plan,
        views::Options::default(),
    )
    .unwrap();
    let cp = pj(s).checkpoint(plan).unwrap();
    let root = i.root().unwrap().unwrap();
    let page_bytes = crate::journal::Journal::new(s, crate::journal::Options::default())
        .unwrap()
        .read(r.physical.generation, 0, r.physical.size as usize)
        .unwrap();
    let page = s3drive_core::manifest::Page::decode(&page_bytes).unwrap();
    f.base = Base {
        root,
        page: page.clone(),
        pages: BTreeMap::from([((0, 0), page)]),
    };
    f.node = r.receipt().unwrap().node.clone();
    f.binding = vj(s).current_head(&f.binding).unwrap().binding().clone();
    f.bytes = bytes;
    (i, token, cp, transfers)
}
fn count(s: &State, table: &str) -> i64 {
    s.connection
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn disk(path: &Path) -> (u64, u64) {
    (
        std::fs::metadata(path.join("metadata.sqlite3"))
            .unwrap()
            .len(),
        std::fs::metadata(path.join("metadata.sqlite3-wal"))
            .map(|m| m.len())
            .unwrap_or(0),
    )
}
#[test]
fn retention_repeated_cycles_exceed_history_limits_with_bounded_database_wal_and_staging() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("retention café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let mut f = setup(&mut s);
    let mut peak = (0, 0);
    let mut late = (0, 0);
    for n in 0..300u64 {
        let mut bytes = vec![(n % 251) as u8; 256 * 1024];
        bytes[..8].copy_from_slice(&n.to_be_bytes());
        let (_, _, cp, _) = cycle(&mut s, &mut f, bytes);
        assert!(cp.unblocked());
        assert_eq!(
            j(&mut s)
                .retire_checkpoint(&cp, views::Options::default())
                .unwrap(),
            cp.anchor()
        );
        assert_eq!(
            j(&mut s)
                .retire_checkpoint(&cp, views::Options::default())
                .unwrap(),
            cp.anchor()
        );
        assert_eq!(count(&s, "sparse_views"), 1);
        assert_eq!(count(&s, "sparse_preparations"), 0);
        assert_eq!(count(&s, "sparse_publications"), 0);
        assert_eq!(count(&s, "staged_generations"), 1);
        assert_eq!(count(&s, "sparse_retention_anchors"), 1);
        assert_eq!(count(&s, "sparse_view_blocks"), 0);
        assert_eq!(count(&s, "sparse_preparation_blobs"), 0);
        let staging: i64 = s
            .connection
            .query_row(
                "SELECT coalesce(sum(length(data)),0) FROM staged_chunks",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(staging < 4096, "only one small root page may remain");
        let d = disk(&path);
        peak = (peak.0.max(d.0), peak.1.max(d.1));
        if n >= 150 {
            late = (late.0.max(d.0), late.1.max(d.1));
        }
        assert!(
            d.0 <= 4 * 1024 * 1024 && d.1 <= 8 * 1024 * 1024,
            "disk bound exceeded at {n}: {d:?}"
        );
        if n % 31 == 30 {
            drop(s);
            s = State::open_managed(&path).unwrap();
            assert_eq!(
                vj(&mut s).current_head(&f.binding).unwrap().size(),
                256 * 1024
            );
        }
    }
    println!(
        "RETENTION_BOUNDS cycles=300 db_peak={} wal_peak={} late_db_peak={} late_wal_peak={} retained_generations=1 retained_views=1",
        peak.0, peak.1, late.0, late.1
    );
}
#[test]
fn retention_closed_source_plan_root_and_physical_retries_never_become_fresh_operations() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let mut f = setup(&mut s);
    let old_root = f.base.root.encode().to_vec();
    let old_page = f.base.page.encode();
    let (i, t, cp, transfers) = cycle(&mut s, &mut f, vec![255; 17]);
    j(&mut s)
        .retire_checkpoint(&cp, views::Options::default())
        .unwrap();
    let before = super::retry_safety::migration_rows(&s);
    assert!(
        pj(&mut s)
            .begin_scoped(
                t,
                Request {
                    operation: t.id().unwrap(),
                    view: i.view().id(),
                    generation: t.generation().unwrap(),
                    base_root: old_root,
                    base_page: old_page
                }
            )
            .is_err()
    );
    assert!(pj(&mut s).info(i.id().unwrap()).is_err());
    assert!(j(&mut s).get(i.id().unwrap()).is_err());
    assert!(vj(&mut s).view(i.view().id()).is_err());
    let data = transfers.iter().find(|t| t.position().level < 0).unwrap();
    assert!(
        crate::journal::Journal::new(&mut s, crate::journal::Options::default())
            .unwrap()
            .begin_operation(
                &data.operation,
                data.target(),
                crate::journal::Base::Missing,
                None,
                data.size()
            )
            .is_err()
    );
    assert!(
        crate::remote::Journal::new(&mut s, crate::remote::Options::default())
            .unwrap()
            .prepare(data.generation().unwrap())
            .is_err()
    );
    assert_eq!(super::retry_safety::migration_rows(&s), before);
    let (_, _, next, _) = cycle(&mut s, &mut f, vec![128; 17]);
    j(&mut s)
        .retire_checkpoint(&next, views::Options::default())
        .unwrap();
    assert!(
        j(&mut s)
            .retire_checkpoint(&cp, views::Options::default())
            .is_err()
    );
}
#[test]
fn retention_reader_dirty_head_and_unknown_plan_refusals_preserve_all_rows() {
    for case in 0..3 {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let mut f = setup(&mut s);
        let (i, _, cp, _) = cycle(&mut s, &mut f, vec![255; 17]);
        let pin = if case == 0 {
            Some(vj(&mut s).pin(i.view().id()).unwrap())
        } else {
            None
        };
        if case == 1 {
            let head = vj(&mut s).current_head(&f.binding).unwrap();
            vj(&mut s)
                .write_scoped(
                    views::ScopedOperation::new(head.binding(), [81; 32]).unwrap(),
                    head.id(),
                    0,
                    &[8],
                )
                .unwrap();
        }
        if case == 2 {
            let head = vj(&mut s).current_head(&f.binding).unwrap();
            let token = retry::Operation::new(head.binding(), [82; 32]).unwrap();
            pj(&mut s)
                .begin_scoped(
                    token,
                    Request {
                        operation: token.id().unwrap(),
                        view: head.id(),
                        generation: token.generation().unwrap(),
                        base_root: f.base.root.encode().to_vec(),
                        base_page: f.base.page.encode(),
                    },
                )
                .unwrap();
        }
        let cp = if case == 2 {
            pj(&mut s).checkpoint(i.id().unwrap()).unwrap()
        } else {
            cp
        };
        let before = super::retry_safety::migration_rows(&s);
        assert!(
            j(&mut s)
                .retire_checkpoint(&cp, views::Options::default())
                .is_err()
        );
        assert_eq!(super::retry_safety::migration_rows(&s), before);
        if let Some(pin) = pin {
            assert!(vj(&mut s).release_pin(&pin).unwrap());
        }
    }
}

#[test]
fn retention_sqlite_full_rolls_back_anchor_and_deletions_atomically() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("full café")).unwrap();
    s.connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA page_size=512; VACUUM; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;").unwrap();
    let mut f = setup(&mut s);
    let (_, _, cp, _) = cycle(&mut s, &mut f, vec![251; 17]);
    s.connection
        .execute_batch("CREATE TABLE retention_pressure(data BLOB)")
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
            .execute("INSERT INTO retention_pressure VALUES(zeroblob(350))", [])
        {
            Ok(_) => (),
            Err(e) if e.sqlite_error_code() == Some(rusqlite::ErrorCode::DiskFull) => break,
            Err(e) => panic!("{e}"),
        }
    }
    let before = super::retry_safety::migration_rows(&s);
    let e = j(&mut s)
        .retire_checkpoint(&cp, views::Options::default())
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<rusqlite::Error>()
            .and_then(|e| e.sqlite_error_code()),
        Some(rusqlite::ErrorCode::DiskFull)
    );
    assert_eq!(super::retry_safety::migration_rows(&s), before);
    assert_eq!(count(&s, "sparse_retention_anchors"), 0);
    s.connection
        .pragma_update(None, "max_page_count", pages + 128)
        .unwrap();
    j(&mut s)
        .retire_checkpoint(&cp, views::Options::default())
        .unwrap();
    assert_eq!(count(&s, "sparse_views"), 1);
}
#[test]
fn retention_crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_RETENTION_CRASH_DIR") else {
        return;
    };
    let mut s = State::open_managed(std::path::Path::new(&path)).unwrap();
    let plan: Vec<u8> = s
        .connection
        .query_row("SELECT plan_id FROM sparse_preparations", [], |r| r.get(0))
        .unwrap();
    let cp = pj(&mut s).checkpoint(plan.try_into().unwrap()).unwrap();
    j(&mut s)
        .retire_checkpoint(&cp, views::Options::default())
        .unwrap();
    panic!("did not stop at retirement checkpoint");
}
#[test]
fn retention_killed_owners_preserve_exact_history_or_complete_anchor_without_partial_deletion() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    for point in [
        "Eligible",
        "AnchorStored",
        "SourceStored",
        "PhysicalStored",
        "Validated",
        "Committed",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("retire crash café");
        let ready = tmp.path().join("ready");
        let mut s = State::open_managed(&path).unwrap();
        let mut f = setup(&mut s);
        let (_, _, cp, _) = cycle(&mut s, &mut f, vec![251; 17]);
        let before = super::retry_safety::migration_rows(&s);
        drop(s);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sparse_views::preparation::publication::tests::retention::retention_crash_child",
                "--nocapture",
            ])
            .env("S3DRIVE_RETENTION_CRASH_DIR", &path)
            .env("S3DRIVE_MIXED_CRASH_KIND", "retire")
            .env("S3DRIVE_MIXED_CRASH_POINT", point)
            .env("S3DRIVE_MIXED_CRASH_READY", &ready)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        while !ready.exists() {
            if let Some(status) = child.try_wait().unwrap() {
                use std::io::Read;
                let mut err = String::new();
                child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut err)
                    .unwrap();
                panic!("{point}: {status}: {err}");
            }
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("retirement checkpoint timed out {point}");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(State::open_managed(&path).is_err());
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        let mut s = State::open_managed(&path).unwrap();
        if point != "Committed" {
            assert_eq!(super::retry_safety::migration_rows(&s), before);
            assert_eq!(count(&s, "sparse_retention_anchors"), 0);
        } else {
            assert_eq!(count(&s, "sparse_views"), 1);
            assert_eq!(count(&s, "staged_generations"), 1);
        }
        assert_eq!(
            j(&mut s)
                .retire_checkpoint(&cp, views::Options::default())
                .unwrap(),
            cp.anchor()
        );
        let (_, _, next, _) = cycle(&mut s, &mut f, vec![129; 17]);
        j(&mut s)
            .retire_checkpoint(&next, views::Options::default())
            .unwrap();
    }
}
fn rewrite_anchor(s: &mut State, mut value: serde_json::Value) {
    // Canonical reserialization and both public hashes are updated. This is a
    // self-consistent forged row, not merely a stale checksum negative control.
    let owner: Vec<u8> = s
        .connection
        .query_row("SELECT owner FROM sparse_retention_role", [], |r| r.get(0))
        .unwrap();
    let owner: Id = owner.try_into().unwrap();
    let a: super::super::retention::Anchor = serde_json::from_value(value.take()).unwrap();
    let target = a.view().request.binding.target().unwrap();
    s.connection
        .execute(
            "UPDATE sparse_retention_anchors SET record=?1,state_hash=?2",
            params![
                serde_json::to_vec(&a).unwrap(),
                hash(b"s3drive-retention-anchor-v1\0", &a)
                    .unwrap()
                    .as_slice()
            ],
        )
        .unwrap();
    let anchors = BTreeMap::from([(target, a)]);
    let sha = hash(
        b"s3drive-retention-role-v1\0",
        &(owner, anchors.iter().collect::<Vec<_>>()),
    )
    .unwrap();
    s.connection
        .execute(
            "UPDATE sparse_retention_role SET state_hash=?1",
            [sha.as_slice()],
        )
        .unwrap();
}
#[test]
fn retention_missing_corrupt_self_consistent_and_foreign_owner_anchors_fail_without_repair() {
    for case in 0..5 {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let mut f = setup(&mut s);
        let (_, _, cp, _) = cycle(&mut s, &mut f, vec![255; 17]);
        j(&mut s)
            .retire_checkpoint(&cp, views::Options::default())
            .unwrap();
        match case {
            0 => {
                s.connection
                    .execute("DELETE FROM sparse_retention_anchors", [])
                    .unwrap();
            }
            1 => {
                s.connection
                    .execute("UPDATE remote_flushes SET receipt_hash=zeroblob(32)", [])
                    .unwrap();
            }
            2 => {
                let raw: Vec<u8> = s
                    .connection
                    .query_row("SELECT record FROM sparse_retention_anchors", [], |r| {
                        r.get(0)
                    })
                    .unwrap();
                let mut value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
                value["view"]["spans"] = serde_json::json!([{"kind":"Zero","length":17}]);
                rewrite_anchor(&mut s, value);
            }
            3 => {
                let mut foreign = State::open_managed(&tmp.path().join("foreign")).unwrap();
                let mut f = setup(&mut foreign);
                let (_, _, cp, _) = cycle(&mut foreign, &mut f, vec![255; 17]);
                j(&mut foreign)
                    .retire_checkpoint(&cp, views::Options::default())
                    .unwrap();
                let raw: Vec<u8> = foreign
                    .connection
                    .query_row("SELECT record FROM sparse_retention_anchors", [], |r| {
                        r.get(0)
                    })
                    .unwrap();
                rewrite_anchor(&mut s, serde_json::from_slice(&raw).unwrap());
            }
            _ => {
                s.connection
                    .execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])
                    .unwrap();
            }
        }
        let before = super::retry_safety::migration_rows(&s);
        assert!(
            views::Journal::new(&mut s, views::Options::default())
                .and_then(|mut j| j.heads())
                .is_err()
        );
        assert_eq!(super::retry_safety::migration_rows(&s), before);
    }
}

#[test]
fn retention_actual_old_binary_refuses_reclaimed_store() {
    let Some(binary) = std::env::var_os("S3DRIVE_RETENTION_OLD_BINARY") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("old binary retired café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let mut f = setup(&mut s);
    let (_, _, cp, _) = cycle(&mut s, &mut f, vec![255; 17]);
    j(&mut s)
        .retire_checkpoint(&cp, views::Options::default())
        .unwrap();
    drop(s);
    fn files(path: &Path) -> BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(path)
            .unwrap()
            .filter_map(|p| {
                let p = p.unwrap();
                let name = p.file_name().to_string_lossy().into_owned();
                name.starts_with("metadata.sqlite3")
                    .then(|| (name, std::fs::read(p.path()).unwrap()))
            })
            .collect()
    }
    let before = files(&path);
    let output = std::process::Command::new(binary)
        .arg("init-state")
        .arg("--directory")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "old executable opened a reclaimed store"
    );
    assert_eq!(files(&path), before);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(
        vj(&mut s).current_head(&f.binding).unwrap().id(),
        cp.anchor()
    );
    println!("RETENTION_OLD_BINARY_REFUSAL unchanged_bytes=true current_reopen=true");
}
#[test]
fn retention_empty_clean_anchor_remains_exact_across_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("empty café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let f = setup(&mut s);
    let head = vj(&mut s).current_head(&f.binding).unwrap();
    let v = vj(&mut s)
        .resize_scoped(
            views::ScopedOperation::new(head.binding(), [2; 32]).unwrap(),
            head.id(),
            0,
        )
        .unwrap();
    let token = retry::Operation::new(v.binding(), [9; 32]).unwrap();
    let i = pj(&mut s)
        .begin_scoped(
            token,
            Request {
                operation: token.id().unwrap(),
                view: v.id(),
                generation: token.generation().unwrap(),
                base_root: f.base.root.encode().to_vec(),
                base_page: f.base.page.encode(),
            },
        )
        .unwrap();
    let i = super::super::super::tests::fixture::finish(&mut s, i);
    let p = i.id().unwrap();
    physical(&mut s, p);
    let r = j(&mut s)
        .begin(
            p,
            f.node.clone(),
            f.store.clone(),
            5,
            token.publish_operation().unwrap(),
        )
        .unwrap();
    lineage::commit(&mut s, r);
    j(&mut s)
        .acknowledge_scoped(
            views::ScopedOperation::new(v.binding(), [40; 32]).unwrap(),
            p,
            views::Options::default(),
        )
        .unwrap();
    let cp = pj(&mut s).checkpoint(p).unwrap();
    j(&mut s)
        .retire_checkpoint(&cp, views::Options::default())
        .unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    let view = vj(&mut s).current_head(&f.binding).unwrap();
    assert_eq!(view.id(), cp.anchor());
    assert_eq!(view.size(), 0);
    assert!(view.range(0, 0).unwrap().is_empty());
    assert_eq!(count(&s, "staged_generations"), 1);
}

#[test]
fn retention_migrated_legacy_history_is_retired_only_after_new_scoped_clean_publication() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("legacy café")).unwrap();
    let mut f = fixture();
    let legacy = super::retry_safety::legacy_complete(&mut s, &f);
    let p = legacy.id().unwrap();
    let r = j(&mut s).get(p).unwrap();
    let root = legacy.root().unwrap().unwrap();
    let bytes = crate::journal::Journal::new(&mut s, crate::journal::Options::default())
        .unwrap()
        .read(r.physical.generation, 0, r.physical.size as usize)
        .unwrap();
    let page = s3drive_core::manifest::Page::decode(&bytes).unwrap();
    f.base = Base {
        root,
        page: page.clone(),
        pages: BTreeMap::from([((0, 0), page)]),
    };
    f.node = r.receipt().unwrap().node.clone();
    f.binding = vj(&mut s)
        .current_head(&f.binding)
        .unwrap()
        .binding()
        .clone();
    pj(&mut s).migrate_retry_safety().unwrap();
    assert!(pj(&mut s).checkpoint(p).is_err());
    let (_, _, cp, _) = cycle(&mut s, &mut f, vec![127; 17]);
    j(&mut s)
        .retire_checkpoint(&cp, views::Options::default())
        .unwrap();
    assert_eq!(count(&s, "sparse_preparations"), 0);
    assert_eq!(count(&s, "staged_generations"), 1);
    assert!(pj(&mut s).operation(legacy.operation()).unwrap().is_none());
    assert_eq!(
        vj(&mut s).current_head(&f.binding).unwrap().id(),
        cp.anchor()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retention_physical_clone_blocks_old_anchor_replacement_and_releases_without_queue() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("retention pinned root café");
        let mut s = State::open_managed(&path).unwrap();
        let mut f = setup(&mut s);
        let (_, _, cp, _) = cycle(&mut s, &mut f, vec![255; 17]);
        j(&mut s)
            .retire_checkpoint(&cp, views::Options::default())
            .unwrap();
        let generation: i64 = s
            .connection
            .query_row(
                "SELECT generation_id FROM sparse_retention_anchors",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let generation = GenerationId::try_from(generation).unwrap();
        drop(s);
        let storage = crate::worker::Storage::open(
            &path,
            crate::cache::Options::default(),
            crate::journal::Options::default(),
        )
        .await
        .unwrap();
        let plans = storage
            .sparse_views(views::Options::default())
            .unwrap()
            .preparations(Options::default())
            .unwrap();
        let roots = plans.publications(Limits::default()).unwrap();
        let reader = storage.staging().snapshot(generation).await.unwrap();
        let clone = reader.clone();
        let expected = reader.read(0, 1024).await.unwrap();
        drop(reader);
        let cp = plans
            .owner
            .call(move |store| {
                let (_, _, cp, _) = cycle(&mut store.state, &mut f, vec![127; 17]);
                Ok(cp)
            })
            .await
            .unwrap();
        assert_eq!(cp.protected_generations(), &[generation]);
        assert!(!cp.unblocked());
        let (read, retired) = tokio::join!(
            clone.read(0, 1024),
            roots.retire_checkpoint(cp.clone(), views::Options::default())
        );
        assert_eq!(read.unwrap(), expected);
        assert!(retired.is_err());
        drop(clone);
        let fresh = plans.checkpoint(cp.plan()).await.unwrap();
        assert!(fresh.unblocked());
        roots
            .retire_checkpoint(fresh, views::Options::default())
            .await
            .unwrap();
        storage.close().await.unwrap();
        let s = State::open_managed(&path).unwrap();
        assert_eq!(count(&s, "sparse_views"), 1);
        assert_eq!(count(&s, "staged_generations"), 1);
    })
    .await
    .expect("physical reader retirement test timed out");
}

#[test]
fn retention_other_target_inherited_witness_pages_and_pending_work_remain_intact() {
    use s3drive_core::manifest::{Entry, Page, Reference, objects::RootDescriptor};
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let mut f = setup(&mut s);
    let (_, _, cp, _) = cycle(&mut s, &mut f, vec![255; 17]);
    let c = s3drive_core::manifest::CHUNK_BYTES;
    let leaf = Page::new(
        [1; 16],
        [22; 16],
        0,
        0,
        vec![Entry::Hole { length: 128 * c }],
    )
    .unwrap();
    let tail = Page::new(
        [1; 16],
        [22; 16],
        128 * c,
        0,
        vec![Entry::Hole { length: c }],
    )
    .unwrap();
    let reference = |slot, p: &Page| {
        Reference::new(
            [5; 16],
            [7; 16],
            super::super::super::model::ordinal(0, slot),
            p.digest(),
        )
        .unwrap()
    };
    let page = Page::new(
        [1; 16],
        [22; 16],
        0,
        1,
        vec![
            Entry::Child {
                length: 128 * c,
                object: reference(0, &leaf),
            },
            Entry::Child {
                length: c,
                object: reference(1, &tail),
            },
        ],
    )
    .unwrap();
    let root = RootDescriptor::new(
        [1; 16],
        [22; 16],
        129 * c,
        Reference::new(
            [5; 16],
            [7; 16],
            super::super::super::model::ordinal(1, 0),
            page.digest(),
        )
        .unwrap(),
    )
    .unwrap();
    let b = Base {
        root,
        page: page.clone(),
        pages: BTreeMap::from([((0, 0), leaf), ((0, 1), tail), ((1, 0), page)]),
    };
    let binding = Binding::new(
        Descriptor::from_hex(
            &"01".repeat(16),
            &"16".repeat(16),
            &"03".repeat(16),
            &"4".repeat(64),
        )
        .unwrap(),
        [5; 16],
        [6; 32],
        "a".repeat(64),
        "managed/".into(),
        129 * c,
    )
    .unwrap();
    let v = vj(&mut s)
        .begin_scoped(
            views::ScopedOperation::new(&binding, [71; 32]).unwrap(),
            binding,
            129 * c,
        )
        .unwrap();
    let token = retry::Operation::new(v.binding(), [72; 32]).unwrap();
    let i = pj(&mut s)
        .begin_scoped(
            token,
            Request {
                operation: token.id().unwrap(),
                generation: token.generation().unwrap(),
                view: v.id(),
                base_root: b.root.encode().to_vec(),
                base_page: b.page.encode(),
            },
        )
        .unwrap();
    let i = pj(&mut s)
        .append(
            i.id().unwrap(),
            i.cursor(),
            Candidate::InheritedChunk(super::super::super::tests::fixture::proof(&b, 0, None)),
        )
        .unwrap();
    let before = super::super::super::ledger::inspect(&s.connection).unwrap();
    let proof_blobs = before
        .objects
        .values()
        .filter(|o| o.object.inherited())
        .flat_map(|o| o.object.blobs())
        .collect::<Vec<_>>();
    assert!(!proof_blobs.is_empty());
    let cp = pj(&mut s).checkpoint(cp.plan()).unwrap();
    j(&mut s)
        .retire_checkpoint(&cp, views::Options::default())
        .unwrap();
    let after = super::super::super::ledger::inspect(&s.connection).unwrap();
    assert_eq!(after.info(i.id().unwrap()).unwrap().cursor(), i.cursor());
    for id in proof_blobs {
        assert_eq!(after.blobs.get(&id), before.blobs.get(&id));
    }
    assert_eq!(after.plans.len(), 1);
    assert_eq!(after.objects.len(), 1);
    assert_eq!(vj(&mut s).view(v.id()).unwrap(), v);
}

#[test]
fn retention_self_consistent_forged_plan_cannot_relabel_original_physical_receipt() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let mut f = setup(&mut s);
    let (_, _, cp, _) = cycle(&mut s, &mut f, vec![255; 17]);
    j(&mut s)
        .retire_checkpoint(&cp, views::Options::default())
        .unwrap();
    let raw: Vec<u8> = s
        .connection
        .query_row("SELECT record FROM sparse_retention_anchors", [], |r| {
            r.get(0)
        })
        .unwrap();
    let mut a: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    let mut input: super::super::super::model::Input =
        serde_json::from_value(a["input"].clone()).unwrap();
    input.retry = None;
    input.operation = [217; 32];
    let mut publication: Record = serde_json::from_value(a["publication"].clone()).unwrap();
    publication.input.operation = input.operation;
    publication.input.plan = input.id().unwrap();
    let mut parent = a["published"].clone();
    parent["spans"] = serde_json::json!([]);
    let parent: views::model::Record = serde_json::from_value(parent).unwrap();
    let view = views::model::Record::child(
        publication.acknowledgment().unwrap().operation(),
        super::super::settlement::request(&publication).unwrap(),
        &parent,
    )
    .unwrap();
    let mut ack = serde_json::to_value(publication.acknowledgment().unwrap()).unwrap();
    ack["clean_view"] = serde_json::to_value(view.id).unwrap();
    publication.acknowledgment = Some(serde_json::from_value(ack).unwrap());
    publication.validate().unwrap(); // internally coherent copied publication
    a["input"] = serde_json::to_value(input).unwrap();
    a["publication"] = serde_json::to_value(publication).unwrap();
    a["view"] = serde_json::to_value(&view).unwrap();
    s.connection
        .execute_batch("BEGIN IMMEDIATE; PRAGMA defer_foreign_keys=ON")
        .unwrap();
    s.connection
        .execute(
            "UPDATE sparse_views SET view_id=?1,record=?2,state_hash=?3",
            params![
                view.id.as_slice(),
                serde_json::to_vec(&view).unwrap(),
                views::hash(b"s3drive-sparse-view-v1\0", &view)
                    .unwrap()
                    .as_slice()
            ],
        )
        .unwrap();
    s.connection
        .execute(
            "UPDATE sparse_view_operations SET view_id=?1,request=?2,state_hash=?3",
            params![
                view.id.as_slice(),
                serde_json::to_vec(&view.request).unwrap(),
                views::hash(
                    b"s3drive-sparse-request-v1\0",
                    &(view.operation, &view.request)
                )
                .unwrap()
                .as_slice()
            ],
        )
        .unwrap();
    s.connection
        .execute("UPDATE sparse_view_files SET head=?1", [view.id.as_slice()])
        .unwrap();
    s.connection
        .execute(
            "UPDATE sparse_retention_anchors SET view_id=?1",
            [view.id.as_slice()],
        )
        .unwrap();
    rewrite_anchor(&mut s, a);
    s.connection.execute_batch("COMMIT").unwrap();
    let before = super::retry_safety::migration_rows(&s);
    assert!(
        views::Journal::new(&mut s, views::Options::default())
            .and_then(|mut j| j.heads())
            .is_err()
    );
    assert_eq!(super::retry_safety::migration_rows(&s), before);
}
