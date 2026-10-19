use super::*;
use s3drive_core::manifest::base::Descriptor;
fn binding(node: u8, base_size: u64) -> Binding {
    let hex = |b: u8| format!("{b:02x}").repeat(16);
    Binding::new(
        Descriptor::from_hex(&hex(1), &hex(node), &hex(3), &"4".repeat(64)).unwrap(),
        [5; 16],
        [6; 32],
        "a".repeat(64),
        "managed/".into(),
        base_size,
    )
    .unwrap()
}
fn failure<T>(r: Result<T>, e: Failure) {
    match r {
        Ok(_) => panic!("unexpected success"),
        Err(actual) => assert_eq!(actual.downcast_ref::<Failure>(), Some(&e)),
    }
}
#[test]
fn initial_sparse_views_reopen_exact_identity_and_plan_bounded_base_and_zero_ranges() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("private café 🦀");
    let mut state = State::open_managed(&path).unwrap();
    let b = binding(2, 17);
    let mut j = Journal::new(&mut state, Options::default()).unwrap();
    let v = j.begin([7; 32], b.clone(), MAX_FILE_BYTES).unwrap();
    assert_eq!(v.size(), MAX_FILE_BYTES);
    assert_eq!(v.binding(), &b);
    assert_eq!(
        v.range(14, 8).unwrap(),
        vec![
            Range::Base {
                offset: 14,
                length: 3
            },
            Range::Zero { length: 5 }
        ]
    );
    assert_eq!(
        v.range(MAX_FILE_BYTES - 3, 8).unwrap(),
        vec![Range::Zero { length: 3 }]
    );
    assert!(v.range(u64::MAX, 8).unwrap().is_empty());
    failure(v.range(0, MAX_RANGE_BYTES + 1), Failure::Bounds);
    assert_eq!(j.begin([7; 32], b.clone(), MAX_FILE_BYTES).unwrap(), v);
    failure(j.begin([7; 32], b.clone(), 0), Failure::Conflict);
    failure(j.begin([8; 32], b, 0), Failure::Conflict);
    drop(state);
    let mut state = State::open_managed(&path).unwrap();
    let mut j = Journal::new(&mut state, Options::new(1).unwrap()).unwrap();
    assert_eq!(j.view(v.id()).unwrap(), v);
    assert_eq!(
        j.begin([7; 32], v.binding().clone(), MAX_FILE_BYTES)
            .unwrap(),
        v
    );
    failure(j.begin([8; 32], binding(9, 0), 0), Failure::Quota);
}

mod faults;
mod migration;
mod worker;

fn rows(db: &Connection) -> Vec<Vec<String>> {
    [
        "SELECT quote(singleton)||','||quote(format)||','||quote(state_hash) FROM sparse_view_role ORDER BY singleton",
        "SELECT quote(target)||','||quote(binding)||','||quote(head)||','||quote(state_hash) FROM sparse_view_files ORDER BY target",
        "SELECT quote(view_id)||','||quote(target)||','||quote(record)||','||quote(state_hash) FROM sparse_views ORDER BY view_id",
        "SELECT quote(operation)||','||quote(view_id)||','||quote(request)||','||quote(state_hash) FROM sparse_view_operations ORDER BY operation",
        "SELECT quote(block)||','||quote(data) FROM sparse_view_blocks ORDER BY block",
    ].iter().map(|sql| {
        db.prepare(sql).unwrap().query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<Vec<String>>>().unwrap()
    }).collect()
}

#[test]
fn base_binding_initial_size_and_operation_changes_never_replace_an_admitted_view() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let b = binding(2, 17);
    let v = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin([7; 32], b.clone(), 17)
        .unwrap();
    let before = rows(&s.connection);
    let mut changes = Vec::new();
    let mut altered = b.clone();
    altered.source = [9; 32];
    changes.push(altered);
    let mut altered = b.clone();
    altered.store = [9; 16];
    changes.push(altered);
    let mut altered = b.clone();
    altered.fingerprint = "b".repeat(64);
    changes.push(altered);
    let mut altered = b.clone();
    altered.root = "other/".into();
    changes.push(altered);
    let mut altered = b.clone();
    altered.base_size = 18;
    changes.push(altered);
    let mut altered = b.clone();
    altered.base = Descriptor::from_hex(
        &"01".repeat(16),
        &"02".repeat(16),
        &"09".repeat(16),
        &"a".repeat(64),
    )
    .unwrap()
    .encode()
    .to_vec();
    changes.push(altered);
    for altered in changes {
        let mut j = Journal::new(&mut s, Options::default()).unwrap();
        failure(j.begin([7; 32], altered.clone(), 17), Failure::Conflict);
        failure(j.begin([8; 32], altered, 17), Failure::Conflict);
        assert_eq!(j.view(v.id()).unwrap(), v);
        assert_eq!(rows(&s.connection), before);
    }
    let mut invalid = Vec::new();
    let mut altered = b.clone();
    altered.base = vec![0];
    invalid.push(altered);
    let mut altered = b.clone();
    altered.source = [0; 32];
    invalid.push(altered);
    let mut altered = b.clone();
    altered.store = [0; 16];
    invalid.push(altered);
    let mut altered = b.clone();
    altered.fingerprint = "A".repeat(64);
    invalid.push(altered);
    let mut altered = b.clone();
    altered.root = "../bad/".into();
    invalid.push(altered);
    let mut altered = b.clone();
    altered.root = "x".repeat(1024);
    invalid.push(altered);
    let mut altered = b.clone();
    altered.base_size = MAX_FILE_BYTES + 1;
    invalid.push(altered);
    for altered in invalid {
        failure(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .begin([8; 32], altered, 17),
            Failure::Invalid,
        );
    }
    failure(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .begin([0; 32], b.clone(), 17),
        Failure::Invalid,
    );
    failure(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .begin([8; 32], b, MAX_FILE_BYTES + 1),
        Failure::Invalid,
    );
    assert!(Options::new(0).is_err());
    assert!(Options::new(257).is_err());
    assert_eq!(rows(&s.connection), before);
}
#[test]
fn initial_empty_truncate_and_sparse_growth_use_only_bounded_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let mut j = Journal::new(&mut s, Options::default()).unwrap();
    for (node, base, size) in [(2, 17, 0), (3, 0, 0), (4, 0, MAX_FILE_BYTES), (5, 17, 8)] {
        let v = j.begin([node; 32], binding(node, base), size).unwrap();
        assert_eq!(v.size(), size);
        assert!(v.0.spans.len() <= 2);
        assert_eq!(v.range(0, 0).unwrap(), Vec::<Range>::new());
        if size == 0 {
            assert!(v.range(0, 8).unwrap().is_empty());
        }
        if base == 0 && size > 0 {
            assert_eq!(
                v.range(size - 3, 8).unwrap(),
                vec![Range::Zero { length: 3 }]
            );
        }
        if size == 8 {
            assert_eq!(
                v.range(6, 8).unwrap(),
                vec![Range::Base {
                    offset: 6,
                    length: 2
                }]
            );
        }
    }
    let bytes: i64 = s
        .connection
        .query_row("SELECT sum(length(record)) FROM sparse_views", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(
        bytes < 8192,
        "four metadata views unexpectedly scale with logical bytes"
    );
}
#[test]
fn narrowed_file_quota_preserves_old_views_and_exact_replay() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let mut j = Journal::new(&mut s, Options::new(2).unwrap()).unwrap();
    let a = j.begin([7; 32], binding(2, 17), 17).unwrap();
    let mut other = binding(3, 0);
    other.store = [8; 16];
    other.source = [9; 32];
    other.fingerprint = "b".repeat(64);
    other.root = "other/".into();
    let b = j.begin([8; 32], other, MAX_FILE_BYTES).unwrap();
    assert_eq!(b.binding().store(), &[8; 16]);
    assert_eq!(b.binding().source(), &[9; 32]);
    let mut j = Journal::new(&mut s, Options::new(1).unwrap()).unwrap();
    assert_eq!(j.view(a.id()).unwrap(), a);
    assert_eq!(j.view(b.id()).unwrap(), b);
    assert_eq!(j.begin([8; 32], b.binding().clone(), b.size()).unwrap(), b);
    failure(j.begin([9; 32], binding(4, 0), 0), Failure::Quota);
    failure(j.view([0; 32]), Failure::NotFound);
}

mod integrity;
mod patches;
mod recovery;

mod scoped;
