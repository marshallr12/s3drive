use super::*;
use s3drive_core::manifest::{Entry, Page, Reference, base::Descriptor, objects::RootDescriptor};
fn binding(generation: u8, size: u64) -> Binding {
    Binding::new(
        Descriptor::from_hex(
            &"01".repeat(16),
            &"02".repeat(16),
            &format!("{generation:02x}").repeat(16),
            &format!("{generation:02x}").repeat(32),
        )
        .unwrap(),
        [5; 16],
        [6; 32],
        "a".repeat(64),
        "managed/".into(),
        size,
    )
    .unwrap()
}
fn j(s: &mut State) -> Journal<'_> {
    Journal::new(s, Options::default()).unwrap()
}
fn token(v: &View, n: u8) -> ScopedOperation {
    ScopedOperation::new(v.binding(), [n; 32]).unwrap()
}
fn initial(s: &mut State) -> View {
    let mut j = j(s);
    j.enable_scoped().unwrap();
    let b = binding(7, 17);
    j.begin_scoped(ScopedOperation::new(&b, [1; 32]).unwrap(), b, 17)
        .unwrap()
}
fn adopted(result: Adoption) -> View {
    match result {
        Adoption::Adopted(v) => v,
        _ => panic!("no admission"),
    }
}
fn fail<T: std::fmt::Debug>(result: Result<T>, expected: Failure) {
    assert_eq!(
        result.unwrap_err().downcast_ref::<Failure>(),
        Some(&expected)
    );
}
fn rows(s: &State) -> Vec<Vec<String>> {
    ["SELECT quote(target)||quote(binding)||quote(head)||quote(state_hash) FROM sparse_view_files ORDER BY target",
    "SELECT quote(view_id)||quote(record)||quote(state_hash) FROM sparse_views ORDER BY view_id",
    "SELECT quote(operation)||quote(view_id)||quote(request)||quote(state_hash) FROM sparse_view_operations ORDER BY operation",
    "SELECT quote(block)||quote(data) FROM sparse_view_blocks ORDER BY block"]
    .iter().map(|q|s.connection.prepare(q).unwrap().query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<Vec<String>>>().unwrap()).collect()
}
fn prepare(s: &mut State, v: &View, n: u8) {
    let page = Page::new([1; 16], [2; 16], 0, 0, vec![Entry::Hole { length: 17 }]).unwrap();
    let root = RootDescriptor::new(
        [1; 16],
        [2; 16],
        17,
        Reference::new([5; 16], [7; 16], 0, page.digest()).unwrap(),
    )
    .unwrap();
    preparation::Journal::new(s, preparation::Options::default())
        .unwrap()
        .begin(preparation::Request {
            operation: [n; 32],
            view: v.id(),
            generation: [n; 16],
            base_root: root.encode().to_vec(),
            base_page: page.encode(),
        })
        .unwrap();
}
#[test]
fn larger_smaller_empty_epochs_keep_old_pinned_reads_and_exact_recovery_after_restart() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("café 日本語 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let first = initial(&mut s);
    let pin = j(&mut s).pin(first.id()).unwrap();
    let mut old = first.clone();
    for (n, size) in [(8, 130_000_007), (9, 3), (10, 0), (11, 17)] {
        let op = token(&old, n);
        let b = binding(n, size);
        assert_eq!(j(&mut s).recover_adoption(op, old.id()).unwrap(), None);
        let v = adopted(
            j(&mut s)
                .adopt_clean_intent(op, old.id(), b.clone())
                .unwrap(),
        );
        assert_eq!(v.size(), size);
        assert_eq!(
            v.range(0, 32).unwrap(),
            if size == 0 {
                vec![]
            } else {
                vec![Range::Base {
                    offset: 0,
                    length: size.min(32),
                }]
            }
        );
        assert_eq!(
            j(&mut s).read_pinned(&pin, 0, 32).unwrap(),
            vec![Piece::Base {
                offset: 0,
                length: 17
            }]
        );
        assert_eq!(
            j(&mut s).recover_adoption(op, old.id()).unwrap(),
            Some(v.clone())
        );
        assert_eq!(
            j(&mut s).adopt_clean_intent(op, old.id(), b).unwrap(),
            Adoption::Adopted(v.clone())
        );
        old = v;
    }
    let prior = old.clone();
    let op = token(&prior, 12);
    let b = binding(12, 4);
    let v = adopted(
        j(&mut s)
            .adopt_clean_intent(op, prior.id(), b.clone())
            .unwrap(),
    );
    let dirty = j(&mut s)
        .write_scoped(token(&v, 13), v.id(), 1, b"new")
        .unwrap();
    let before = rows(&s);
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    let options = Options::default()
        .with_limits(Limits {
            views: 1,
            operations: 1,
            extents: 1,
            metadata_bytes: 1,
            dirty_bytes: 1,
        })
        .unwrap();
    let mut narrow = Journal::new(&mut s, options).unwrap();
    assert_eq!(
        narrow.recover_adoption(op, prior.id()).unwrap(),
        Some(v.clone())
    );
    assert_eq!(
        narrow.adopt_clean_intent(op, prior.id(), b).unwrap(),
        Adoption::Adopted(v)
    );
    assert_eq!(narrow.current_head(prior.binding()).unwrap(), dirty);
    assert_eq!(rows(&s), before);
}
#[test]
fn unchanged_polling_admits_no_rows_or_operation_and_dirty_noops_never_adopt() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let v = initial(&mut s);
    let before = rows(&s);
    let op = token(&v, 8);
    for _ in 0..32 {
        assert_eq!(
            j(&mut s)
                .adopt_clean_intent(op, v.id(), v.binding().clone())
                .unwrap(),
            Adoption::Unchanged(v.clone())
        );
    }
    assert_eq!(j(&mut s).recover_adoption(op, v.id()).unwrap(), None);
    assert_eq!(rows(&s), before);
    let next = adopted(
        j(&mut s)
            .adopt_clean_intent(op, v.id(), binding(8, 17))
            .unwrap(),
    );
    let dirty = j(&mut s)
        .resize_scoped(token(&next, 9), next.id(), 17)
        .unwrap();
    let before = rows(&s);
    fail(
        j(&mut s).adopt_clean_intent(token(&dirty, 10), dirty.id(), binding(10, 17)),
        Failure::Conflict,
    );
    assert_eq!(rows(&s), before);
}
#[test]
fn conflicting_scope_tokens_parent_and_rebound_nonce_never_change_the_head() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let v = initial(&mut s);
    let before = rows(&s);
    let mut cases = Vec::new();
    let mut b = binding(8, 17);
    b.source = [9; 32];
    cases.push(b);
    let mut b = binding(8, 17);
    b.store = [9; 16];
    cases.push(b);
    let mut b = binding(8, 17);
    b.root = "other/".into();
    cases.push(b);
    let mut b = binding(8, 17);
    b.fingerprint = "b".repeat(64);
    cases.push(b);
    let mut b = binding(8, 17);
    b.base = Descriptor::from_hex(
        &"01".repeat(16),
        &"04".repeat(16),
        &"08".repeat(16),
        &"08".repeat(32),
    )
    .unwrap()
    .encode()
    .to_vec();
    cases.push(b);
    for b in cases {
        fail(
            j(&mut s).adopt_clean_intent(token(&v, 8), v.id(), b),
            Failure::Conflict,
        );
    }
    let wrong = ScopedOperation::new(&binding(8, 17), [8; 32]).unwrap();
    fail(
        j(&mut s).adopt_clean_intent(wrong, v.id(), binding(8, 17)),
        Failure::EpochClosed,
    );
    assert_eq!(rows(&s), before);
    let op = token(&v, 8);
    let next = adopted(
        j(&mut s)
            .adopt_clean_intent(op, v.id(), binding(8, 17))
            .unwrap(),
    );
    fail(
        j(&mut s).adopt_clean_intent(op, v.id(), binding(9, 17)),
        Failure::Conflict,
    );
    fail(
        j(&mut s).adopt_clean_intent(token(&v, 9), v.id(), binding(9, 17)),
        Failure::Conflict,
    );
    fail(
        j(&mut s).recover_adoption(op, next.id()),
        Failure::EpochClosed,
    );
    fail(
        j(&mut s).write_scoped(token(&v, 10), next.id(), 0, b"old-token"),
        Failure::EpochClosed,
    );
    assert_eq!(j(&mut s).current_head(v.binding()).unwrap(), next);
}
#[test]
fn pending_preparation_is_file_scoped_and_historical_work_blocks_later_fresh_adoption() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let v = initial(&mut s);
    let next = adopted(
        j(&mut s)
            .adopt_clean_intent(token(&v, 8), v.id(), binding(8, 17))
            .unwrap(),
    );
    prepare(&mut s, &v, 30);
    let before = rows(&s);
    fail(
        j(&mut s).adopt_clean_intent(token(&next, 9), next.id(), binding(9, 17)),
        Failure::Conflict,
    );
    assert_eq!(
        j(&mut s).recover_adoption(token(&v, 8), v.id()).unwrap(),
        Some(next.clone())
    );
    assert_eq!(rows(&s), before);
    let mut other = binding(7, 17);
    other.base = Descriptor::from_hex(
        &"01".repeat(16),
        &"04".repeat(16),
        &"07".repeat(16),
        &"07".repeat(32),
    )
    .unwrap()
    .encode()
    .to_vec();
    let a = j(&mut s)
        .begin_scoped(ScopedOperation::new(&other, [31; 32]).unwrap(), other, 17)
        .unwrap();
    let mut b = a.binding().clone();
    b.base = Descriptor::from_hex(
        &"01".repeat(16),
        &"04".repeat(16),
        &"08".repeat(16),
        &"08".repeat(32),
    )
    .unwrap()
    .encode()
    .to_vec();
    assert!(matches!(
        j(&mut s)
            .adopt_clean_intent(token(&a, 32), a.id(), b)
            .unwrap(),
        Adoption::Adopted(_)
    ));
}
#[test]
fn admission_quota_and_each_rollback_or_lost_reply_boundary_preserve_exact_rows() {
    for point in [
        Boundary::ViewStored,
        Boundary::OperationStored,
        Boundary::HeadUpdated,
        Boundary::Validated,
        Boundary::Committed,
    ] {
        let t = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&t.path().join("s")).unwrap();
        let v = initial(&mut s);
        let before = rows(&s);
        let op = token(&v, 8);
        let b = binding(8, 0);
        let err = j(&mut s).adopt_inner(op, v.id(), b.clone(), |at| {
            if at == point {
                Err("injected lost result".into())
            } else {
                Ok(())
            }
        });
        assert!(err.is_err());
        if point != Boundary::Committed {
            assert_eq!(rows(&s), before);
            assert_eq!(j(&mut s).recover_adoption(op, v.id()).unwrap(), None);
        } else {
            assert!(j(&mut s).recover_adoption(op, v.id()).unwrap().is_some());
        }
        let next = adopted(j(&mut s).adopt_clean_intent(op, v.id(), b).unwrap());
        assert_eq!(j(&mut s).head(next.binding()).unwrap(), next);
    }
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let v = initial(&mut s);
    let before = rows(&s);
    let options = Options::default()
        .with_limits(Limits {
            views: 1,
            ..Limits::default()
        })
        .unwrap();
    fail(
        Journal::new(&mut s, options).unwrap().adopt_clean_intent(
            token(&v, 8),
            v.id(),
            binding(8, 17),
        ),
        Failure::Quota,
    );
    assert_eq!(rows(&s), before);
}
#[test]
fn attached_trigger_is_refused_before_admission_or_recovery_and_never_frees_a_pin() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let v = initial(&mut s);
    let pin = j(&mut s).pin(v.id()).unwrap();
    let before = rows(&s);
    s.connection.execute_batch("CREATE TRIGGER evil AFTER UPDATE ON sparse_view_files BEGIN DELETE FROM source_read_pins; END").unwrap();
    assert!(Journal::new(&mut s, Options::default()).is_err());
    assert_eq!(rows(&s), before);
    s.connection.execute_batch("DROP TRIGGER evil").unwrap();
    assert_eq!(
        j(&mut s).read_pinned(&pin, 0, 17).unwrap(),
        vec![Piece::Base {
            offset: 0,
            length: 17
        }]
    );
}
#[test]
fn schema37_migration_preserves_literal_rows_but_cannot_launder_new_observation_records() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("s");
    let mut s = State::open_managed(&path).unwrap();
    let v = initial(&mut s);
    let before = rows(&s);
    crate::test_schema::without_creation(&s.connection);
    s.connection
        .pragma_update(None, "user_version", 37)
        .unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
    assert_eq!(rows(&s), before);
    let next = adopted(
        j(&mut s)
            .adopt_clean_intent(token(&v, 8), v.id(), binding(8, 0))
            .unwrap(),
    );
    assert_eq!(next.0.format, 4);
    crate::test_schema::without_creation(&s.connection);
    s.connection
        .pragma_update(None, "user_version", 37)
        .unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        37
    );
}

mod crash;

#[test]
fn real_sqlite_full_adoption_rolls_back_both_binding_and_head_then_exact_retry_succeeds() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("full")).unwrap();
    let mut v = initial(&mut s);
    s.connection
        .execute_batch(
            "PRAGMA journal_mode=DELETE; PRAGMA page_size=512; VACUUM; PRAGMA journal_mode=WAL",
        )
        .unwrap();
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
    let mut failed = None;
    for n in 8..64 {
        let before = rows(&s);
        let op = token(&v, n);
        let b = binding(n, n as u64);
        match j(&mut s).adopt_clean_intent(op, v.id(), b.clone()) {
            Ok(value) => v = adopted(value),
            Err(e) => {
                assert!(crate::cache::is_disk_full(e.as_ref()), "{e}");
                assert_eq!(rows(&s), before);
                assert_eq!(j(&mut s).recover_adoption(op, v.id()).unwrap(), None);
                failed = Some((op, b));
                break;
            }
        }
    }
    let (op, b) = failed.expect("fixture did not reach real SQLITE_FULL");
    s.connection
        .pragma_update(None, "max_page_count", original)
        .unwrap();
    let next = adopted(j(&mut s).adopt_clean_intent(op, v.id(), b).unwrap());
    assert_eq!(j(&mut s).current_head(v.binding()).unwrap(), next);
}
#[test]
fn self_consistent_wrong_coverage_and_operation_kind_are_refused_without_repair() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let v = initial(&mut s);
    let op = token(&v, 8);
    let next = adopted(
        j(&mut s)
            .adopt_clean_intent(op, v.id(), binding(8, 9))
            .unwrap(),
    );
    let dirty_op = token(&next, 9);
    let dirty = j(&mut s).resize_scoped(dirty_op, next.id(), 9).unwrap();
    fail(
        j(&mut s).recover_adoption(dirty_op, next.id()),
        Failure::Conflict,
    );
    let mut forged = next.0.clone();
    forged.spans = vec![model::Span::Zero { length: 9 }];
    s.connection
        .execute(
            "UPDATE sparse_views SET record=?1,state_hash=?2 WHERE view_id=?3",
            params![
                encode(&forged).unwrap(),
                hash(b"s3drive-sparse-view-v1\0", &forged)
                    .unwrap()
                    .as_slice(),
                next.id().as_slice()
            ],
        )
        .unwrap();
    let before = rows(&s);
    assert!(Journal::new(&mut s, Options::default()).is_err());
    assert_eq!(rows(&s), before);
    assert_ne!(dirty.id(), next.id());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_owner_competing_adoptions_preserve_a_shared_old_reader_and_recover_winner() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("worker café 🦀");
        let storage = crate::worker::Storage::open(
            &path,
            crate::cache::Options::default(),
            crate::journal::Options::default(),
        )
        .await
        .unwrap();
        let a = storage.sparse_views(Options::default()).unwrap();
        a.enable_scoped().await.unwrap();
        let b = binding(7, 17);
        let v = a
            .begin_scoped(ScopedOperation::new(&b, [1; 32]).unwrap(), b, 17)
            .await
            .unwrap();
        let guard = a.guard(v.id()).await.unwrap();
        let cloned = guard.clone();
        let other = a.clone();
        let (left, right) = tokio::join!(
            a.adopt_clean_intent(token(&v, 8), v.id(), binding(8, 1)),
            other.adopt_clean_intent(token(&v, 9), v.id(), binding(9, 0))
        );
        let (op, winner) = match (left, right) {
            (Ok(x), Err(e)) => {
                assert_eq!(e.downcast_ref::<Failure>(), Some(&Failure::Conflict));
                (token(&v, 8), adopted(x))
            }
            (Err(e), Ok(x)) => {
                assert_eq!(e.downcast_ref::<Failure>(), Some(&Failure::Conflict));
                (token(&v, 9), adopted(x))
            }
            _ => panic!("expected one CAS winner"),
        };
        assert_eq!(
            cloned.read(0, 17).await.unwrap(),
            vec![Piece::Base {
                offset: 0,
                length: 17
            }]
        );
        assert_eq!(
            a.recover_adoption(op, v.id()).await.unwrap(),
            Some(winner.clone())
        );
        drop(guard);
        drop(cloned);
        storage.close().await.unwrap();
        let storage = crate::worker::Storage::open(
            &path,
            crate::cache::Options::default(),
            crate::journal::Options::default(),
        )
        .await
        .unwrap();
        let a = storage.sparse_views(Options::default()).unwrap();
        assert_eq!(a.recover_adoption(op, v.id()).await.unwrap(), Some(winner));
        storage.close().await.unwrap();
    })
    .await
    .expect("bounded worker acceptance");
}
