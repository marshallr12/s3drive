use super::lineage::{commit, first};
use super::*;
fn vj(s: &mut State) -> crate::sparse_views::Journal<'_> {
    crate::sparse_views::Journal::new(s, crate::sparse_views::Options::default()).unwrap()
}
#[test]
fn exact_published_head_gets_new_base_without_rewriting_old_views_and_future_edits_use_it() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("epochs café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let r = first(&mut s, &f);
    let r = commit(&mut s, r);
    let p = r.input.plan;
    let old = crate::sparse_views::ledger::inspect(&s.connection).unwrap();
    assert!(j(&mut s).acknowledged(p).unwrap().is_none());
    let receipt = j(&mut s)
        .acknowledge([40; 32], p, crate::sparse_views::Options::default())
        .unwrap();
    assert_eq!(receipt.view(), r.input.view);
    let clean = receipt.acknowledgment().clean_view().unwrap();
    let view = vj(&mut s).current_head(&f.binding).unwrap();
    assert_eq!(view.id(), clean);
    assert_eq!(view.size(), 17);
    assert_eq!(
        view.binding().base().unwrap().payload_reference(),
        ns::payload_reference(&r.receipt().unwrap().node.payload).unwrap()
    );
    assert_eq!(
        view.range(0, 18).unwrap(),
        vec![crate::sparse_views::Range::Base {
            offset: 0,
            length: 17
        }]
    );
    assert!(vj(&mut s).head(&f.binding).is_err());
    let after = crate::sparse_views::ledger::inspect(&s.connection).unwrap();
    for (id, saved) in &old.records {
        assert!(after.records.get(id) == Some(saved));
    }
    assert!(old.blocks == after.blocks, "no reclamation yet");
    assert_eq!(receipt.record().input(), r.input());
    assert_eq!(receipt.record().publication(), r.publication());
    assert_eq!(receipt.record().receipt(), r.receipt());
    let next = vj(&mut s).write([41; 32], view.id(), 6, &[88]).unwrap();
    assert_eq!(next.binding(), view.binding());
    assert!(
        vj(&mut s).write([42; 32], r.input.view, 7, &[99]).is_err(),
        "stale immutable parent changed current epoch"
    );
    assert_eq!(
        j(&mut s)
            .acknowledge([40; 32], p, crate::sparse_views::Options::default())
            .unwrap(),
        receipt
    );
    assert_eq!(vj(&mut s).current_head(&f.binding).unwrap(), next);
    assert!(
        j(&mut s)
            .acknowledge([43; 32], p, crate::sparse_views::Options::default())
            .is_err()
    );
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(j(&mut s).acknowledged(p).unwrap(), Some(receipt));
    assert_eq!(vj(&mut s).current_head(&f.binding).unwrap(), next);
    assert_eq!(vj(&mut s).view(r.input.view).unwrap().binding(), &f.binding);
}
#[test]
fn historical_acknowledgment_preserves_newer_head_and_reserves_operation_identity_without_a_view() {
    let temp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&temp.path().join("history")).unwrap();
    let f = fixture();
    let r = first(&mut s, &f);
    let r = commit(&mut s, r);
    let p = r.input.plan;
    let newer = vj(&mut s).write([41; 32], r.input.view, 6, &[88]).unwrap();
    let before = crate::sparse_views::ledger::inspect(&s.connection).unwrap();
    let a = j(&mut s)
        .acknowledge([40; 32], p, crate::sparse_views::Options::default())
        .unwrap();
    assert_eq!(a.acknowledgment().clean_view(), None);
    assert_eq!(a.acknowledgment().observed_head(), newer.id());
    assert!(crate::sparse_views::ledger::inspect(&s.connection).unwrap() == before);
    assert!(vj(&mut s).begin([40; 32], f.binding.clone(), 17).is_err());
    assert!(vj(&mut s).write([40; 32], newer.id(), 1, &[7]).is_err());
    assert!(vj(&mut s).resize([40; 32], newer.id(), 0).is_err());
    let newest = vj(&mut s).resize([42; 32], newer.id(), 0).unwrap();
    assert_eq!(
        j(&mut s)
            .acknowledge([40; 32], p, crate::sparse_views::Options::default())
            .unwrap(),
        a
    );
    assert_eq!(vj(&mut s).head(&f.binding).unwrap(), newest);
    assert!(
        j(&mut s)
            .acknowledge([43; 32], p, crate::sparse_views::Options::default())
            .is_err()
    );
}
#[test]
fn unresolved_roots_colliding_operations_narrowed_admission_and_triggers_never_change_head_or_ack()
{
    let temp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&temp.path().join("refusal")).unwrap();
    let f = fixture();
    let r = first(&mut s, &f);
    let p = r.input.plan;
    assert!(
        j(&mut s)
            .acknowledge([40; 32], p, crate::sparse_views::Options::default())
            .is_err()
    );
    let r = commit(&mut s, r);
    let source = crate::sparse_views::ledger::inspect(&s.connection).unwrap();
    let meta = ledger::inspect(&s.connection).unwrap();
    for op in [[0; 32], [2; 32]] {
        assert!(
            j(&mut s)
                .acknowledge(op, p, crate::sparse_views::Options::default())
                .is_err()
        );
    }
    let options = crate::sparse_views::Options::default()
        .with_limits(crate::sparse_views::Limits {
            views: 1,
            ..crate::sparse_views::Limits::default()
        })
        .unwrap();
    assert!(j(&mut s).acknowledge([40; 32], p, options).is_err());
    s.connection.execute_batch("CREATE TRIGGER false_epoch BEFORE UPDATE ON sparse_view_files BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    assert!(
        j(&mut s)
            .acknowledge([40; 32], p, crate::sparse_views::Options::default())
            .is_err()
    );
    s.connection
        .execute_batch("DROP TRIGGER false_epoch")
        .unwrap();
    assert!(crate::sparse_views::ledger::inspect(&s.connection).unwrap() == source);
    assert!(ledger::inspect(&s.connection).unwrap() == meta);
    let a = j(&mut s)
        .acknowledge([40; 32], p, crate::sparse_views::Options::default())
        .unwrap();
    assert_eq!(j(&mut s).acknowledge([40; 32], p, options).unwrap(), a);
    assert_eq!(a.view(), r.input.view);
}
#[test]
fn recorded_ack_recovery_needs_no_source_blobs_and_never_repairs_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("recovery");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let r = first(&mut s, &f);
    let r = commit(&mut s, r);
    let p = r.input.plan;
    let a = j(&mut s)
        .acknowledge([40; 32], p, crate::sparse_views::Options::default())
        .unwrap();
    s.connection
        .execute(
            "UPDATE sparse_view_blocks SET data=zeroblob(length(data))",
            [],
        )
        .unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
    let mut recovery = RecoveryStore::open(&path).unwrap();
    assert_eq!(recovery.acknowledged(p).unwrap(), Some(a));
    drop(recovery);
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert!(
        db.query_row(
            "SELECT count(*) FROM sparse_view_blocks WHERE data=zeroblob(length(data))",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap()
            > 0
    );
}

#[test]
fn damaged_epoch_metadata_refuses_recovered_ack_but_does_not_require_source_body_for_saved_root() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("metadata");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let r = first(&mut s, &f);
    let r = commit(&mut s, r);
    let p = r.input.plan;
    let a = j(&mut s)
        .acknowledge([40; 32], p, crate::sparse_views::Options::default())
        .unwrap();
    let clean = a.acknowledgment().clean_view().unwrap();
    s.connection
        .execute(
            "UPDATE sparse_views SET state_hash=zeroblob(32) WHERE view_id=?1",
            [clean.as_slice()],
        )
        .unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
    let mut recovery = RecoveryStore::open(&path).unwrap();
    assert!(
        recovery.get(p).unwrap().receipt().is_some(),
        "historical root metadata did not need source bodies"
    );
    assert!(
        recovery.acknowledged(p).is_err(),
        "damaged rebase metadata returned a typed source acknowledgment"
    );
}

mod crash;
mod faults;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn private_owner_races_ack_and_write_without_losing_data_and_reopens_same_receipt() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        for reverse in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("racing epoch");
            let mut s = State::open_managed(&path).unwrap();
            let f = fixture();
            let r = first(&mut s, &f);
            let r = commit(&mut s, r);
            let p = r.input.plan;
            drop(s);
            let storage = crate::worker::Storage::open(
                &path,
                crate::cache::Options::default(),
                crate::journal::Options::default(),
            )
            .await
            .unwrap();
            let local = storage
                .sparse_views(crate::sparse_views::Options::default())
                .unwrap();
            let root = local
                .preparations(Options::default())
                .unwrap()
                .publications(Limits::default())
                .unwrap();
            let ack = root.acknowledge([40; 32], p, crate::sparse_views::Options::default());
            let write = local.write([41; 32], r.input.view, 6, std::sync::Arc::from(&[88][..]));
            let (ack, write) = if reverse {
                let (w, a) = tokio::join!(write, ack);
                (a, w)
            } else {
                tokio::join!(ack, write)
            };
            let a = ack.unwrap();
            let head = local.current_head(f.binding.clone()).await.unwrap();
            match write {
                Ok(v) => {
                    assert_eq!(head, v);
                    assert_eq!(a.acknowledgment().clean_view(), None);
                    assert_eq!(a.acknowledgment().observed_head(), v.id());
                }
                Err(e) => {
                    assert_eq!(
                        e.downcast_ref::<crate::sparse_views::Failure>(),
                        Some(&crate::sparse_views::Failure::Conflict)
                    );
                    assert_eq!(a.acknowledgment().clean_view(), Some(head.id()));
                }
            }
            assert_eq!(
                root.acknowledge([40; 32], p, crate::sparse_views::Options::default())
                    .await
                    .unwrap(),
                a
            );
            storage.close().await.unwrap();
            assert!(root.acknowledged(p).await.is_err());
            let recovery = RecoveryClient::open(&path).await.unwrap();
            assert_eq!(recovery.acknowledged(p).await.unwrap(), Some(a));
            recovery.close().await.unwrap();
        }
    })
    .await
    .expect("bounded epoch worker race timed out");
}

#[test]
fn scoped_ack_closes_epoch_and_retained_old_tokens_replay_without_rebasing_twice() {
    use crate::sparse_views::{Options as SourceOptions, ScopedOperation};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("scoped epochs 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let r = first(&mut s, &f);
    let r = commit(&mut s, r);
    let p = r.input.plan;
    vj(&mut s).enable_scoped().unwrap();
    let token = ScopedOperation::new(&f.binding, [71; 32]).unwrap();
    assert!(
        j(&mut s)
            .acknowledge([72; 32], p, SourceOptions::default())
            .is_err()
    );
    let a = j(&mut s)
        .acknowledge_scoped(token, p, SourceOptions::default())
        .unwrap();
    let clean = vj(&mut s).current_head(&f.binding).unwrap();
    assert_eq!(a.acknowledgment().clean_view(), Some(clean.id()));
    assert_ne!(clean.binding().epoch().unwrap(), token.epoch());
    let old_token = ScopedOperation::new(&f.binding, [73; 32]).unwrap();
    assert!(
        vj(&mut s)
            .write_scoped(old_token, clean.id(), 0, b"stale")
            .is_err()
    );
    let new_token = ScopedOperation::new(clean.binding(), [73; 32]).unwrap();
    let next = vj(&mut s)
        .write_scoped(new_token, clean.id(), 0, b"current")
        .unwrap();
    assert_eq!(
        j(&mut s)
            .acknowledge_scoped(token, p, SourceOptions::default())
            .unwrap(),
        a
    );
    assert_eq!(vj(&mut s).current_head(&f.binding).unwrap(), next);
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(
        j(&mut s)
            .acknowledge_scoped(token, p, SourceOptions::default())
            .unwrap(),
        a
    );
    assert_eq!(
        vj(&mut s)
            .write_scoped(new_token, clean.id(), 0, b"current")
            .unwrap(),
        next
    );
    assert_eq!(vj(&mut s).current_head(&f.binding).unwrap(), next);
}

mod scoped_crash;

#[test]
fn recorded_ack_with_read_pins_recovers_without_source_bodies_or_session_rotation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("pinned recovery");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let r = first(&mut s, &f);
    let r = commit(&mut s, r);
    let p = r.input.plan;
    let a = j(&mut s)
        .acknowledge([40; 32], p, crate::sparse_views::Options::default())
        .unwrap();
    vj(&mut s).pin(r.input.view).unwrap();
    let header: Vec<u8> = s
        .connection
        .query_row("SELECT state_hash FROM source_read_pin_session", [], |r| {
            r.get(0)
        })
        .unwrap();
    s.connection
        .execute(
            "UPDATE sparse_view_blocks SET data=zeroblob(length(data))",
            [],
        )
        .unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
    let mut recovery = RecoveryStore::open(&path).unwrap();
    assert_eq!(recovery.acknowledged(p).unwrap(), Some(a));
    drop(recovery);
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("SELECT state_hash FROM source_read_pin_session", [], |r| {
            r.get::<_, Vec<u8>>(0)
        })
        .unwrap(),
        header
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM source_read_pins", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(State::open_managed(&path).is_err());
}
