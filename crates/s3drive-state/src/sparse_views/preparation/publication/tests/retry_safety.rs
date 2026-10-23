use super::*;
use crate::sparse_views::{self as views, preparation::retry};
fn vj(s: &mut State) -> views::Journal<'_> {
    views::Journal::new(s, views::Options::default()).unwrap()
}
fn start(s: &mut State, f: &Fixture) -> (views::View, retry::Operation) {
    let v = vj(s).begin([1; 32], f.binding.clone(), 17).unwrap();
    let v = vj(s).write([2; 32], v.id(), 5, &[255]).unwrap();
    pj(s).enable_retry_safety().unwrap();
    (v, retry::Operation::new(&f.binding, [9; 32]).unwrap())
}
fn request(f: &Fixture, v: &views::View, t: retry::Operation) -> Request {
    Request {
        operation: t.id().unwrap(),
        view: v.id(),
        generation: t.generation().unwrap(),
        base_root: f.base.root.encode().to_vec(),
        base_page: f.base.page.encode(),
    }
}
fn prepared(s: &mut State, f: &Fixture, v: &views::View, t: retry::Operation) -> Info {
    let i = pj(s).begin_scoped(t, request(f, v, t)).unwrap();
    let mut bytes = f.bytes.clone();
    bytes[5] = 255;
    let i = pj(s)
        .append(i.id().unwrap(), i.cursor(), Candidate::Bytes(bytes))
        .unwrap();
    super::super::super::tests::fixture::finish(s, i)
}
fn complete(s: &mut State, f: &Fixture, i: &Info, t: retry::Operation) -> Record {
    let p = i.id().unwrap();
    j(s).check(p, 0, Some(&proof(f))).unwrap();
    physical(s, p);
    let r = j(s)
        .begin(
            p,
            f.node.clone(),
            f.store.clone(),
            5,
            t.publish_operation().unwrap(),
        )
        .unwrap();
    let r = lineage::commit(s, r);
    j(s).acknowledge_scoped(
        views::ScopedOperation::new(&f.binding, [40; 32]).unwrap(),
        p,
        views::Options::default(),
    )
    .unwrap();
    r
}
#[test]
fn retry_safety_exact_replay_and_clean_checkpoint_survive_restart_without_reclaiming_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("retry café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let (v, t) = start(&mut s, &f);
    let i = prepared(&mut s, &f, &v, t);
    let p = i.id().unwrap();
    complete(&mut s, &f, &i, t);
    let cp = pj(&mut s).checkpoint(p).unwrap();
    assert!(cp.unblocked());
    assert_eq!(cp.terminal_plans(), &[p]);
    let before = views::preparation::ledger::inspect(&s.connection).unwrap();
    assert_eq!(
        pj(&mut s)
            .begin_scoped(t, request(&f, &v, t))
            .unwrap()
            .id()
            .unwrap(),
        p
    );
    assert!(pj(&mut s).begin(request(&f, &v, t)).is_err());
    let other = retry::Operation::new(&f.binding, [11; 32]).unwrap();
    assert!(
        pj(&mut s)
            .begin_scoped(other, request(&f, &v, other))
            .is_err()
    );
    assert!(views::preparation::ledger::inspect(&s.connection).unwrap() == before);
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    pj(&mut s).verify_checkpoint(&cp).unwrap();
    assert!(views::preparation::ledger::inspect(&s.connection).unwrap() == before);
}
#[test]
fn retry_safety_identity_conflicts_and_raw_physical_ids_fail_before_admission() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let f = fixture();
    let (v, t) = start(&mut s, &f);
    let mut q = request(&f, &v, t);
    q.generation = [19; 16];
    assert!(pj(&mut s).begin_scoped(t, q).is_err());
    let mut q = request(&f, &v, t);
    q.operation = [19; 32];
    assert!(pj(&mut s).begin_scoped(t, q).is_err());
    let i = prepared(&mut s, &f, &v, t);
    let p = i.id().unwrap();
    let transfer = pj(&mut s).transfers(p, None, 1).unwrap().remove(0);
    let mut stage =
        crate::journal::Journal::new(&mut s, crate::journal::Options::default()).unwrap();
    assert!(
        stage
            .begin_operation(
                &[93; 32],
                transfer.target(),
                crate::journal::Base::Missing,
                None,
                transfer.size()
            )
            .is_err()
    );
    assert!(
        stage
            .begin(
                transfer.target(),
                crate::journal::Base::Missing,
                None,
                transfer.size()
            )
            .is_err()
    );
    physical(&mut s, p);
    j(&mut s).check(p, 0, Some(&proof(&f))).unwrap();
    assert!(
        j(&mut s)
            .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
            .is_err()
    );
    j(&mut s)
        .begin(
            p,
            f.node.clone(),
            f.store.clone(),
            5,
            t.publish_operation().unwrap(),
        )
        .unwrap();
}
#[test]
fn retry_safety_live_reader_pin_invalidates_old_proof_and_release_does_not_collect() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let f = fixture();
    let (v, t) = start(&mut s, &f);
    let i = prepared(&mut s, &f, &v, t);
    complete(&mut s, &f, &i, t);
    let p = i.id().unwrap();
    let cp = pj(&mut s).checkpoint(p).unwrap();
    let pin = vj(&mut s).pin(v.id()).unwrap();
    assert!(pj(&mut s).verify_checkpoint(&cp).is_err());
    let pinned = pj(&mut s).checkpoint(p).unwrap();
    assert!(!pinned.unblocked());
    assert_eq!(pinned.protected_views(), &[v.id()]);
    let before = views::preparation::ledger::inspect(&s.connection).unwrap();
    assert!(vj(&mut s).release_pin(&pin).unwrap());
    assert!(pj(&mut s).verify_checkpoint(&pinned).is_err());
    assert!(pj(&mut s).checkpoint(p).unwrap().unblocked());
    let after = views::preparation::ledger::inspect(&s.connection).unwrap();
    assert!(
        before.views.records == after.views.records
            && before.views.blocks == after.views.blocks
            && before.plans == after.plans
    );
}
#[test]
fn retry_safety_new_dirty_head_and_historical_ack_cannot_be_clean_anchors() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let f = fixture();
    let (v, t) = start(&mut s, &f);
    let i = prepared(&mut s, &f, &v, t);
    complete(&mut s, &f, &i, t);
    let p = i.id().unwrap();
    let cp = pj(&mut s).checkpoint(p).unwrap();
    let clean = vj(&mut s).current_head(&f.binding).unwrap();
    let dirty = vj(&mut s)
        .write_scoped(
            views::ScopedOperation::new(clean.binding(), [66; 32]).unwrap(),
            clean.id(),
            0,
            &[77],
        )
        .unwrap();
    assert!(pj(&mut s).checkpoint(p).is_err());
    assert!(pj(&mut s).verify_checkpoint(&cp).is_err());
    assert_eq!(vj(&mut s).current_head(&f.binding).unwrap(), dirty);
}
#[test]
fn retry_safety_unknown_plan_and_claimed_upload_remain_blockers_after_other_plan_completes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let f = fixture();
    let (v, t) = start(&mut s, &f);
    let unknown = retry::Operation::new(&f.binding, [17; 32]).unwrap();
    let u = prepared(&mut s, &f, &v, unknown);
    let up = u.id().unwrap();
    let tr = pj(&mut s).transfers(up, None, 1).unwrap().remove(0);
    let tr = pj(&mut s)
        .materialize(up, tr.position(), crate::journal::Options::default())
        .unwrap()
        .unwrap();
    let mut remote =
        crate::remote::Journal::new(&mut s, crate::remote::Options::default()).unwrap();
    let ri = remote.prepare(tr.generation().unwrap()).unwrap();
    let _permit = remote.claim(ri.operation, &ri.target).unwrap();
    let i = prepared(&mut s, &f, &v, t);
    complete(&mut s, &f, &i, t);
    let cp = pj(&mut s).checkpoint(i.id().unwrap()).unwrap();
    assert!(!cp.unblocked());
    assert_eq!(cp.blocked_plans(), &[up]);
    assert!(pj(&mut s).tree_receipt(up).is_err());
    let before = views::preparation::ledger::inspect(&s.connection).unwrap();
    let unmaterialized = pj(&mut s)
        .transfers(up, Some(tr.position()), 1)
        .unwrap()
        .remove(0);
    assert!(
        pj(&mut s)
            .materialize(
                up,
                unmaterialized.position(),
                crate::journal::Options::default()
            )
            .is_err()
    );
    assert!(views::preparation::ledger::inspect(&s.connection).unwrap() == before);
}
#[test]
fn retry_safety_corruption_and_missing_ancestors_fail_closed_without_repair() {
    for damage in 0..4 {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let f = fixture();
        let (v, t) = start(&mut s, &f);
        let i = prepared(&mut s, &f, &v, t);
        complete(&mut s, &f, &i, t);
        let cp = pj(&mut s).checkpoint(i.id().unwrap()).unwrap();
        match damage {
            0 => {
                s.connection
                    .execute("UPDATE sparse_retry_mode SET state_hash=zeroblob(32)", [])
                    .unwrap();
            }
            1 => {
                s.connection
                    .execute(
                        "UPDATE sparse_views SET state_hash=zeroblob(32) WHERE view_id=?1",
                        [cp.anchor().as_slice()],
                    )
                    .unwrap();
            }
            2 => {
                let mut forged = views::ledger::inspect(&s.connection)
                    .unwrap()
                    .record(cp.anchor())
                    .unwrap()
                    .clone();
                forged.spans = vec![views::model::Span::Zero {
                    length: forged.request.size,
                }];
                let raw = views::encode(&forged).unwrap();
                let sha = views::hash(b"s3drive-sparse-view-v1\0", &forged).unwrap();
                s.connection
                    .execute(
                        "UPDATE sparse_views SET record=?1,state_hash=?2 WHERE view_id=?3",
                        params![raw, sha.as_slice(), cp.anchor().as_slice()],
                    )
                    .unwrap();
            }
            _ => {
                s.connection
                    .execute_batch("PRAGMA foreign_keys=OFF")
                    .unwrap();
                s.connection
                    .execute(
                        "DELETE FROM sparse_views WHERE view_id=?1",
                        [v.id().as_slice()],
                    )
                    .unwrap();
                s.connection
                    .execute_batch("PRAGMA foreign_keys=ON")
                    .unwrap();
            }
        }
        assert!(pj(&mut s).verify_checkpoint(&cp).is_err());
        assert!(pj(&mut s).checkpoint(i.id().unwrap()).is_err());
    }
}

#[test]
fn retry_safety_sqlite_full_rolls_back_exact_preparation_and_keeps_same_retry() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    s.connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA page_size=512; VACUUM; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;").unwrap();
    let f = fixture();
    let (v, t) = start(&mut s, &f);
    s.connection
        .execute_batch("CREATE TABLE retry_pressure(data BLOB)")
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
            .execute("INSERT INTO retry_pressure VALUES(zeroblob(350))", [])
        {
            Ok(_) => (),
            Err(e) if e.sqlite_error_code() == Some(rusqlite::ErrorCode::DiskFull) => break,
            Err(e) => panic!("{e}"),
        }
    }
    let before = views::preparation::ledger::inspect(&s.connection).unwrap();
    let e = pj(&mut s).begin_scoped(t, request(&f, &v, t)).unwrap_err();
    assert_eq!(
        e.downcast_ref::<rusqlite::Error>()
            .and_then(|e| e.sqlite_error_code()),
        Some(rusqlite::ErrorCode::DiskFull)
    );
    assert!(views::preparation::ledger::inspect(&s.connection).unwrap() == before);
    s.connection
        .pragma_update(None, "max_page_count", pages + 64)
        .unwrap();
    let i = pj(&mut s).begin_scoped(t, request(&f, &v, t)).unwrap();
    assert_eq!(i.operation(), t.id().unwrap());
    assert_eq!(i.view(), &v);
}

#[test]
fn retry_safety_crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_RETRY_CRASH_DIR") else {
        return;
    };
    let mut s = State::open_managed(std::path::Path::new(&path)).unwrap();
    let f = fixture();
    let (v, t) = start(&mut s, &f);
    pj(&mut s).begin_scoped(t, request(&f, &v, t)).unwrap();
    panic!("did not stop at requested checkpoint");
}

#[test]
fn retry_safety_killed_owners_preserve_atomic_mode_and_exact_preparation_identity() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    for (kind, points) in [
        ("retry_enable", vec!["Stored", "Validated", "Committed"]),
        (
            "begin",
            vec!["BlobStored", "PlanStored", "Validated", "Committed"],
        ),
    ] {
        for point in points {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("crash café");
            let ready = tmp.path().join("ready");
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "sparse_views::preparation::publication::tests::retry_safety::retry_safety_crash_child", "--nocapture"])
                .env("S3DRIVE_RETRY_CRASH_DIR", &path).env("S3DRIVE_MIXED_CRASH_KIND", kind).env("S3DRIVE_MIXED_CRASH_POINT", point).env("S3DRIVE_MIXED_CRASH_READY", &ready)
                .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
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
                    panic!("{kind}/{point}: {status}: {err}");
                }
                if start.elapsed() > Duration::from_secs(30) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("checkpoint timed out {kind}/{point}");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(State::open_managed(&path).is_err());
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            let mut s = State::open_managed(&path).unwrap();
            let f = fixture();
            let enabled = retry::enabled(&s.connection).unwrap();
            assert_eq!(enabled, kind == "begin" || point == "Committed");
            assert_eq!(views::scoped::enabled(&s.connection).unwrap(), enabled);
            let v = vj(&mut s).current_head(&f.binding).unwrap();
            let t = retry::Operation::new(&f.binding, [9; 32]).unwrap();
            let known = pj(&mut s).operation(t.id().unwrap()).unwrap();
            assert_eq!(known.is_some(), kind == "begin" && point == "Committed");
            pj(&mut s).enable_retry_safety().unwrap();
            let i = pj(&mut s).begin_scoped(t, request(&f, &v, t)).unwrap();
            assert_eq!(i.operation(), t.id().unwrap());
            if let Some(old) = known {
                assert_eq!(i.id().unwrap(), old.id().unwrap());
            }
        }
    }
}

#[test]
fn retry_safety_activation_refuses_populated_legacy_owner_without_changes_or_stranding_default_flow()
 {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let f = fixture();
    let i = prepare(&mut s, &f, false);
    let p = i.id().unwrap();
    let before = views::preparation::ledger::inspect(&s.connection).unwrap();
    assert!(pj(&mut s).enable_retry_safety().is_err());
    assert!(!retry::enabled(&s.connection).unwrap());
    assert!(!views::scoped::enabled(&s.connection).unwrap());
    assert!(views::preparation::ledger::inspect(&s.connection).unwrap() == before);
    physical(&mut s, p);
    j(&mut s).check(p, 0, Some(&proof(&f))).unwrap();
    let r = j(&mut s)
        .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
        .unwrap();
    lineage::commit(&mut s, r);
    assert!(
        j(&mut s)
            .acknowledge([40; 32], p, views::Options::default())
            .unwrap()
            .acknowledgment()
            .clean_view()
            .is_some()
    );
    assert!(pj(&mut s).enable_retry_safety().is_err());
    assert!(!views::scoped::enabled(&s.connection).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_safety_concurrent_cloned_readers_keep_history_protected() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("readers café 🦀");
        let mut s = State::open_managed(&path).unwrap();
        let f = fixture();
        let (v, t) = start(&mut s, &f);
        let i = prepared(&mut s, &f, &v, t);
        complete(&mut s, &f, &i, t);
        let p = i.id().unwrap();
        drop(s);
        let storage = crate::worker::Storage::open(
            &path,
            crate::cache::Options::default(),
            crate::journal::Options::default(),
        )
        .await
        .unwrap();
        let client = storage.sparse_views(views::Options::default()).unwrap();
        let plans = client.preparations(Options::default()).unwrap();
        let guard = client.guard(v.id()).await.unwrap();
        let clone = guard.clone();
        let (read, cp) = tokio::join!(clone.read(5, 1), plans.checkpoint(p));
        assert_eq!(read.unwrap(), vec![views::Piece::Local(vec![255])]);
        assert!(!cp.unwrap().unblocked());
        drop(guard);
        let (read, cp) = tokio::join!(clone.read(5, 1), plans.checkpoint(p));
        assert_eq!(read.unwrap(), vec![views::Piece::Local(vec![255])]);
        assert_eq!(cp.unwrap().protected_views(), &[v.id()]);
        drop(clone);
        storage.close().await.unwrap();
        let mut s = State::open_managed(&path).unwrap();
        assert!(pj(&mut s).checkpoint(p).unwrap().unblocked());
        assert_eq!(vj(&mut s).view(v.id()).unwrap(), v);
    })
    .await
    .expect("bounded concurrent reader checkpoint test timed out");
}

fn legacy_complete(s: &mut State, f: &Fixture) -> Info {
    let i = prepare(s, f, false);
    let p = i.id().unwrap();
    physical(s, p);
    j(s).check(p, 0, Some(&proof(f))).unwrap();
    let r = j(s)
        .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
        .unwrap();
    lineage::commit(s, r);
    j(s).acknowledge([40; 32], p, views::Options::default())
        .unwrap();
    i
}
fn legacy_request(f: &Fixture, i: &Info) -> Request {
    Request {
        operation: i.operation(),
        view: i.view().id(),
        generation: *i.generation(),
        base_root: f.base.root.encode().to_vec(),
        base_page: f.base.page.encode(),
    }
}
// Compare every original SQL value, not just selected ledger counts. Mode rows
// are the only allowed delta. Schema and sqlite_sequence are included.
fn migration_rows(s: &State) -> Vec<String> {
    let mut out = s
        .connection
        .prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")
        .unwrap()
        .query_map([], |r| {
            Ok(format!(
                "schema: {:?}",
                (
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?
                )
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let mut names = s.connection.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT IN ('sparse_scoped_mode','sparse_retry_mode') ORDER BY name").unwrap();
    let names = names
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    for name in names {
        let mut q = s
            .connection
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let n = q.column_count();
        let mut values = q
            .query_map([], |r| {
                (0..n)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .map(|r| format!("{name}: {:?}", r.unwrap()))
            .collect::<Vec<_>>();
        values.sort();
        out.extend(values);
    }
    out
}
#[test]
fn retry_safety_populated_migration_preserves_all_rows_and_exact_legacy_retries() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("migration café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let i = legacy_complete(&mut s, &f);
    let p = i.id().unwrap();
    let pin = vj(&mut s).pin(i.view().id()).unwrap();
    let before = migration_rows(&s);
    pj(&mut s).migrate_retry_safety().unwrap();
    assert!(retry::enabled(&s.connection).unwrap());
    assert_eq!(migration_rows(&s), before);
    assert_eq!(
        pj(&mut s)
            .begin(legacy_request(&f, &i))
            .unwrap()
            .id()
            .unwrap(),
        p
    );
    assert!(
        j(&mut s)
            .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
            .unwrap()
            .receipt()
            .is_some()
    );
    let mut changed = legacy_request(&f, &i);
    changed.generation = [31; 16];
    assert!(pj(&mut s).begin(changed).is_err());
    let mut unknown = legacy_request(&f, &i);
    unknown.operation = [33; 32];
    assert!(pj(&mut s).begin(unknown).is_err());
    assert_eq!(vj(&mut s).view(i.view().id()).unwrap(), *i.view());
    assert_eq!(migration_rows(&s), before);
    // A retained legacy operation is never relabeled as a scoped operation.
    assert!(pj(&mut s).checkpoint(p).is_err());
    assert!(vj(&mut s).release_pin(&pin).unwrap());
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert!(retry::enabled(&s.connection).unwrap());
    pj(&mut s).migrate_retry_safety().unwrap();
    assert_eq!(
        pj(&mut s)
            .begin(legacy_request(&f, &i))
            .unwrap()
            .id()
            .unwrap(),
        p
    );
    let clean = vj(&mut s).current_head(&f.binding).unwrap();
    let token = views::ScopedOperation::new(clean.binding(), [71; 32]).unwrap();
    assert!(vj(&mut s).write([72; 32], clean.id(), 0, &[5]).is_err());
    let dirty = vj(&mut s).write_scoped(token, clean.id(), 0, &[5]).unwrap();
    assert_ne!(dirty.id(), clean.id());
    let stale = retry::Operation::new(i.view().binding(), [81; 32]).unwrap();
    let mut stale_request = legacy_request(&f, &i);
    stale_request.operation = stale.id().unwrap();
    stale_request.generation = stale.generation().unwrap();
    assert!(pj(&mut s).begin_scoped(stale, stale_request).is_err());
    let snapshot = views::preparation::ledger::inspect(&s.connection).unwrap();
    let root = i.root().unwrap().unwrap();
    let views::preparation::model::Kind::Page { blob } = snapshot.object(p, 0, 0).unwrap().kind
    else {
        panic!("expected leaf root");
    };
    let token = retry::Operation::new(dirty.binding(), [82; 32]).unwrap();
    let next = pj(&mut s)
        .begin_scoped(
            token,
            Request {
                operation: token.id().unwrap(),
                view: dirty.id(),
                generation: token.generation().unwrap(),
                base_root: root.encode().to_vec(),
                base_page: snapshot.blobs[&blob].clone(),
            },
        )
        .unwrap();
    assert_eq!(next.view(), &dirty);
}
#[test]
fn retry_safety_populated_migration_refuses_dirty_incomplete_and_unrelated_physical_history() {
    for case in 0..4 {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let f = fixture();
        if case == 0 || case == 3 {
            let i = prepare(&mut s, &f, false);
            if case == 3 {
                let p = i.id().unwrap();
                let t = pj(&mut s).transfers(p, None, 1).unwrap().remove(0);
                let t = pj(&mut s)
                    .materialize(p, t.position(), crate::journal::Options::default())
                    .unwrap()
                    .unwrap();
                let mut remote =
                    crate::remote::Journal::new(&mut s, crate::remote::Options::default()).unwrap();
                let intent = remote.prepare(t.generation().unwrap()).unwrap();
                let _permit = remote.claim(intent.operation, &intent.target).unwrap();
            }
        } else {
            legacy_complete(&mut s, &f);
            if case == 1 {
                let clean = vj(&mut s).current_head(&f.binding).unwrap();
                vj(&mut s).write([72; 32], clean.id(), 0, &[5]).unwrap();
            } else {
                let target =
                    crate::journal::Target::new([6; 32], &"a".repeat(64), "unrelated").unwrap();
                crate::journal::Journal::new(&mut s, crate::journal::Options::default())
                    .unwrap()
                    .begin_operation(&[73; 32], &target, crate::journal::Base::Missing, None, 1)
                    .unwrap();
            }
        }
        let before = migration_rows(&s);
        assert!(pj(&mut s).migrate_retry_safety().is_err());
        assert!(!retry::enabled(&s.connection).unwrap());
        assert!(!views::scoped::enabled(&s.connection).unwrap());
        assert_eq!(migration_rows(&s), before);
    }
}
#[test]
fn retry_safety_populated_migration_rejects_corrupt_or_missing_terminal_proof() {
    for damage in 0..3 {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let f = fixture();
        legacy_complete(&mut s, &f);
        match damage {
            0 => {
                s.connection
                    .execute("UPDATE remote_flushes SET receipt_hash=zeroblob(32)", [])
                    .unwrap();
            }
            1 => {
                s.connection
                    .execute("UPDATE sparse_publications SET state_hash=zeroblob(32)", [])
                    .unwrap();
            }
            _ => {
                s.connection.execute_batch("PRAGMA foreign_keys=OFF; DELETE FROM remote_flushes; PRAGMA foreign_keys=ON;").unwrap();
            }
        }
        let before = migration_rows(&s);
        assert!(pj(&mut s).migrate_retry_safety().is_err());
        assert!(!retry::enabled(&s.connection).unwrap());
        assert!(!views::scoped::enabled(&s.connection).unwrap());
        assert_eq!(migration_rows(&s), before);
    }
}
#[test]
fn retry_safety_migration_crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_MIGRATION_CRASH_DIR") else {
        return;
    };
    let mut s = State::open_managed(std::path::Path::new(&path)).unwrap();
    pj(&mut s).migrate_retry_safety().unwrap();
    panic!("did not stop at migration checkpoint");
}
#[test]
fn retry_safety_populated_migration_killed_owners_preserve_atomic_modes_and_all_history() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    for point in [
        "Eligible",
        "SourceStored",
        "Stored",
        "Validated",
        "Committed",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("migration crash café");
        let ready = tmp.path().join("ready");
        let mut s = State::open_managed(&path).unwrap();
        let f = fixture();
        let i = legacy_complete(&mut s, &f);
        let before = migration_rows(&s);
        drop(s);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "sparse_views::preparation::publication::tests::retry_safety::retry_safety_migration_crash_child", "--nocapture"])
            .env("S3DRIVE_MIGRATION_CRASH_DIR", &path).env("S3DRIVE_MIXED_CRASH_KIND", "retry_migrate").env("S3DRIVE_MIXED_CRASH_POINT", point).env("S3DRIVE_MIXED_CRASH_READY", &ready)
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
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
                panic!("checkpoint timed out {point}");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(State::open_managed(&path).is_err());
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        let mut s = State::open_managed(&path).unwrap();
        assert_eq!(retry::enabled(&s.connection).unwrap(), point == "Committed");
        assert_eq!(
            views::scoped::enabled(&s.connection).unwrap(),
            point == "Committed"
        );
        assert_eq!(migration_rows(&s), before);
        pj(&mut s).migrate_retry_safety().unwrap();
        assert_eq!(
            pj(&mut s)
                .begin(legacy_request(&f, &i))
                .unwrap()
                .id()
                .unwrap(),
            i.id().unwrap()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_safety_populated_migration_preserves_concurrent_cloned_readers() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("migration reader café 🦀");
        let mut s = State::open_managed(&path).unwrap();
        let f = fixture();
        let i = legacy_complete(&mut s, &f);
        drop(s);
        let storage = crate::worker::Storage::open(
            &path,
            crate::cache::Options::default(),
            crate::journal::Options::default(),
        )
        .await
        .unwrap();
        let client = storage.sparse_views(views::Options::default()).unwrap();
        let plans = client.preparations(Options::default()).unwrap();
        let guard = client.guard(i.view().id()).await.unwrap();
        let clone = guard.clone();
        let (read, migrated) = tokio::join!(clone.read(5, 1), plans.migrate_retry_safety());
        assert_eq!(read.unwrap(), vec![views::Piece::Local(vec![255])]);
        migrated.unwrap();
        drop(guard);
        assert_eq!(
            clone.read(5, 1).await.unwrap(),
            vec![views::Piece::Local(vec![255])]
        );
        drop(clone);
        storage.close().await.unwrap();
        let mut s = State::open_managed(&path).unwrap();
        assert_eq!(vj(&mut s).view(i.view().id()).unwrap(), *i.view());
    })
    .await
    .expect("bounded migration reader test timed out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_safety_physical_readers_block_checkpoint_until_last_clone_drops() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("physical reader café 🦀");
        let mut s = State::open_managed(&path).unwrap();
        let f = fixture();
        let (v, t) = start(&mut s, &f);
        let i = prepared(&mut s, &f, &v, t);
        complete(&mut s, &f, &i, t);
        let p = i.id().unwrap();
        let g = pj(&mut s)
            .transfers(p, None, 1)
            .unwrap()
            .remove(0)
            .generation()
            .unwrap();
        let before = migration_rows(&s);
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
        let cp = plans.checkpoint(p).await.unwrap();
        assert!(cp.unblocked());
        let reader = storage.staging().snapshot(g).await.unwrap();
        let clone = reader.clone();
        assert!(plans.verify_checkpoint(cp).await.is_err());
        let cp = plans.checkpoint(p).await.unwrap();
        assert_eq!(cp.protected_generations(), &[g]);
        assert!(cp.protected_views().is_empty());
        assert!(!cp.unblocked());
        let expected = reader.read(0, 17).await.unwrap();
        drop(reader);
        let (read, check) = tokio::join!(clone.read(0, 17), plans.checkpoint(p));
        assert_eq!(read.unwrap(), expected);
        assert_eq!(check.unwrap().protected_generations(), &[g]);
        drop(clone);
        assert!(plans.checkpoint(p).await.unwrap().unblocked());
        assert!(plans.verify_checkpoint(cp).await.is_err());
        storage.close().await.unwrap();
        let s = State::open_managed(&path).unwrap();
        assert_eq!(migration_rows(&s), before);
    })
    .await
    .expect("bounded physical reader checkpoint test timed out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_safety_physical_readers_close_fences_old_owner_before_replacement() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("physical replacement café");
        let mut s = State::open_managed(&path).unwrap();
        let f = fixture();
        let (v, t) = start(&mut s, &f);
        let i = prepared(&mut s, &f, &v, t);
        complete(&mut s, &f, &i, t);
        let p = i.id().unwrap();
        let g = pj(&mut s)
            .transfers(p, None, 1)
            .unwrap()
            .remove(0)
            .generation()
            .unwrap();
        drop(s);
        let storage = crate::worker::Storage::open(
            &path,
            crate::cache::Options::default(),
            crate::journal::Options::default(),
        )
        .await
        .unwrap();
        let old = storage.staging().snapshot(g).await.unwrap();
        let clone = old.clone();
        let expected = clone.read(0, 17).await.unwrap();
        assert!(State::open_managed(&path).is_err());
        storage.close().await.unwrap();
        let mut replacement = State::open_managed(&path).unwrap();
        assert!(pj(&mut replacement).checkpoint(p).unwrap().unblocked());
        for reader in [&old, &clone] {
            let e = reader.read(0, 17).await.unwrap_err();
            assert_eq!(
                e.downcast_ref::<crate::worker::Failure>(),
                Some(&crate::worker::Failure::Closed)
            );
        }
        let physical =
            crate::journal::Journal::new(&mut replacement, crate::journal::Options::default())
                .unwrap();
        assert_eq!(physical.read(g, 0, 17).unwrap(), expected);
    })
    .await
    .expect("bounded replacement reader test timed out");
}
