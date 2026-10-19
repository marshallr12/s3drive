use super::*;
fn rendered(j: &mut Journal<'_>, v: &View, base: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for p in j.read(v.id(), 0, v.size() as usize).unwrap() {
        match p {
            Piece::Base { offset, length } => {
                bytes.extend_from_slice(&base[offset as usize..(offset + length) as usize])
            }
            Piece::Zero { length } => bytes.resize(bytes.len() + length as usize, 0),
            Piece::Local(data) => bytes.extend_from_slice(&data),
        }
    }
    bytes
}
#[test]
fn literal_initial_format_and_ids_survive_patch_extension_without_reserializing_old_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let root = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin([7; 32], binding(2, 17), MAX_FILE_BYTES)
        .unwrap();
    assert_eq!(
        encode(&root.0.request).unwrap(),
        include_bytes!("../fixtures/initial-request.json")
    );
    assert_eq!(
        encode(&root.0).unwrap(),
        include_bytes!("../fixtures/initial-record.json")
    );
    assert_eq!(
        root.id()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        "fb600e7dca122bae555cfe2375daa22271f80b5400a83e5b56d221e1024791b5"
    );
    // Valid noncanonical JSON whitespace must remain byte-exact, too.
    for (table, column) in [
        ("sparse_view_files", "binding"),
        ("sparse_views", "record"),
        ("sparse_view_operations", "request"),
    ] {
        s.connection
            .execute_batch(&format!(
                "UPDATE {table} SET {column}=CAST(x'20'||{column}||x'0a' AS BLOB)"
            ))
            .unwrap();
    }
    let before = rows(&s.connection);
    let child = Journal::new(&mut s, Options::default())
        .unwrap()
        .write([8; 32], root.id(), MAX_FILE_BYTES - 4, b"tail")
        .unwrap();
    let after = rows(&s.connection);
    for i in [2, 3] {
        assert!(
            after[i].contains(&before[i][0]),
            "changed an old immutable BLOB"
        );
    }
    let old_binding: Vec<u8> = s
        .connection
        .query_row("SELECT binding FROM sparse_view_files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(old_binding.first(), Some(&b' '));
    assert_eq!(old_binding.last(), Some(&b'\n'));
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    let mut j = Journal::new(&mut s, Options::default()).unwrap();
    assert_eq!(
        j.begin([7; 32], binding(2, 17), MAX_FILE_BYTES).unwrap(),
        root
    );
    assert_eq!(j.head(root.binding()).unwrap(), child);
    assert_eq!(
        j.read(child.id(), MAX_FILE_BYTES - 4, 8).unwrap(),
        vec![Piece::Local(b"tail".to_vec())]
    );
}
#[test]
fn overlapping_edits_match_independent_byte_model_keep_old_views_and_replay_original_results() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    let base = b"0123456789abcdef";
    let mut j = Journal::new(&mut s, Options::default()).unwrap();
    let root = j
        .begin([7; 32], binding(2, base.len() as u64), base.len() as u64)
        .unwrap();
    let mut bytes = base.to_vec();
    let mut current = root.clone();
    let mut history = vec![(root.clone(), bytes.clone())];
    let mut operations = Vec::new();
    for n in 0..48u8 {
        let op = [n + 50; 32];
        let parent = current.id();
        if n % 4 == 0 {
            let size = (n as usize * 37) % 96;
            bytes.resize(size, 0);
            current = j.resize(op, parent, size as u64).unwrap();
            operations.push((
                op,
                parent,
                Some(size as u64),
                0,
                Vec::new(),
                current.clone(),
            ));
        } else {
            let offset = (n as usize * 19) % 100;
            let data = (0..=n % 17).map(|x| x.wrapping_add(n)).collect::<Vec<_>>();
            if bytes.len() < offset + data.len() {
                bytes.resize(offset + data.len(), 0);
            }
            bytes[offset..offset + data.len()].copy_from_slice(&data);
            current = j.write(op, parent, offset as u64, &data).unwrap();
            operations.push((op, parent, None, offset as u64, data, current.clone()));
        }
        assert_eq!(rendered(&mut j, &current, base), bytes, "round{n}");
        history.push((current.clone(), bytes.clone()));
    }
    let head = current.clone();
    for (op, parent, size, offset, data, result) in &operations {
        let replay = if let Some(size) = size {
            j.resize(*op, *parent, *size)
        } else {
            j.write(*op, *parent, *offset, data)
        }
        .unwrap();
        assert_eq!(&replay, result);
    }
    assert_eq!(j.head(root.binding()).unwrap(), head);
    failure(
        j.write([250; 32], root.id(), 0, b"stale"),
        Failure::Conflict,
    );
    failure(
        j.write(operations[1].0, [0; 32], 0, b"other"),
        Failure::Conflict,
    );
    failure(
        j.resize(operations[0].0, operations[0].1, 1),
        Failure::Conflict,
    );
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    let mut j = Journal::new(&mut s, Options::default()).unwrap();
    for (view, expected) in history {
        assert_eq!(j.view(view.id()).unwrap(), view);
        assert_eq!(rendered(&mut j, &view, base), expected);
    }
    let mut changed = root.binding().clone();
    changed.source = [44; 32];
    failure(j.head(&changed), Failure::Conflict);
    assert_eq!(j.head(root.binding()).unwrap(), head);
}
#[test]
fn sparse_sixty_four_gib_shrink_regrow_and_boundary_ranges_never_resurrect_old_tail() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let mut j = Journal::new(&mut s, Options::default()).unwrap();
    let root = j.begin([7; 32], binding(2, 17), 17).unwrap();
    let tail = j
        .write([8; 32], root.id(), MAX_FILE_BYTES - 4, b"tail")
        .unwrap();
    assert_eq!(tail.0.spans.len(), 3);
    assert_eq!(tail.size(), MAX_FILE_BYTES);
    assert_eq!(
        j.read(tail.id(), MAX_FILE_BYTES - 6, 8).unwrap(),
        vec![Piece::Zero { length: 2 }, Piece::Local(b"tail".to_vec())]
    );
    let short = j.resize([9; 32], tail.id(), 7).unwrap();
    let grown = j.resize([10; 32], short.id(), MAX_FILE_BYTES).unwrap();
    assert_eq!(
        j.read(grown.id(), 5, 8).unwrap(),
        vec![
            Piece::Base {
                offset: 5,
                length: 2
            },
            Piece::Zero { length: 6 }
        ]
    );
    assert_eq!(
        j.read(grown.id(), MAX_FILE_BYTES - 4, 8).unwrap(),
        vec![Piece::Zero { length: 4 }]
    );
    assert_eq!(
        j.read(tail.id(), MAX_FILE_BYTES - 4, 8).unwrap(),
        vec![Piece::Local(b"tail".to_vec())]
    );
    assert!(j.read(grown.id(), u64::MAX, 8).unwrap().is_empty());
    failure(j.read(grown.id(), 0, MAX_RANGE_BYTES + 1), Failure::Bounds);
    failure(
        j.write([11; 32], grown.id(), MAX_FILE_BYTES, b"x"),
        Failure::Bounds,
    );
    failure(
        j.write([11; 32], grown.id(), u64::MAX, b"x"),
        Failure::Bounds,
    );
    failure(j.write([11; 32], grown.id(), 0, b""), Failure::Invalid);
    failure(
        j.resize([11; 32], grown.id(), MAX_FILE_BYTES + 1),
        Failure::Bounds,
    );
    failure(j.write([0; 32], grown.id(), 0, b"x"), Failure::Invalid);
    let blank = j.resize([11; 32], grown.id(), 0).unwrap();
    assert!(j.read(blank.id(), 0, 8).unwrap().is_empty());
    let regenerated = j.resize([12; 32], blank.id(), 17).unwrap();
    assert_eq!(
        j.read(regenerated.id(), 0, 17).unwrap(),
        vec![Piece::Zero { length: 17 }]
    );
    let metadata: i64 = s
        .connection
        .query_row("SELECT sum(length(record)) FROM sparse_views", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(metadata < 32 * 1024);
    let dirty: i64 = s
        .connection
        .query_row(
            "SELECT sum(length(data)) FROM sparse_view_blocks",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(dirty, 4);
}
#[test]
fn bounded_blocks_split_partial_reads_deduplicate_and_refuse_oversized_input() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let mut data = vec![0; MAX_RANGE_BYTES];
    for (n, c) in data.chunks_mut(PATCH_BYTES).enumerate() {
        c.fill(n as u8);
    }
    let mut j = Journal::new(&mut s, Options::default()).unwrap();
    let root = j.begin([7; 32], binding(2, 0), 0).unwrap();
    let large = j.write([8; 32], root.id(), 0, &data).unwrap();
    assert_eq!(
        j.read(large.id(), PATCH_BYTES as u64 - 2, 6).unwrap(),
        vec![Piece::Local(vec![0; 2]), Piece::Local(vec![1; 4])]
    );
    assert_eq!(rendered(&mut j, &large, b""), data);
    let repeat = j.write([9; 32], large.id(), 0, &data).unwrap();
    assert_eq!(rendered(&mut j, &repeat, b""), data);
    failure(
        j.write([10; 32], repeat.id(), 0, &vec![0; MAX_RANGE_BYTES + 1]),
        Failure::Bounds,
    );
    let blocks: u32 = s
        .connection
        .query_row("SELECT count(*) FROM sparse_view_blocks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(blocks, 8);
    let dirty: i64 = s
        .connection
        .query_row(
            "SELECT sum(length(data)) FROM sparse_view_blocks",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(dirty, MAX_RANGE_BYTES as i64);
}
#[test]
fn independent_admission_quotas_preserve_dirty_bytes_old_views_and_late_exact_replay() {
    for kind in 0..6 {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let mut j = Journal::new(&mut s, Options::default()).unwrap();
        let root = j.begin([7; 32], binding(2, 0), 0).unwrap();
        let patch = j.write([8; 32], root.id(), 0, b"abc").unwrap();
        let head = j.resize([9; 32], patch.id(), 8).unwrap();
        let before = rows(&s.connection);
        let mut limits = Limits::default();
        match kind {
            0 => limits.views = 3,
            1 => limits.operations = 3,
            2 => limits.extents = 3,
            3 => limits.metadata_bytes = 1,
            4 => limits.dirty_bytes = 3,
            5 => limits.dirty_bytes = 1,
            _ => unreachable!(),
        }
        let options = Options::new(1).unwrap().with_limits(limits).unwrap();
        let mut j = Journal::new(&mut s, options).unwrap();
        assert_eq!(j.write([8; 32], root.id(), 0, b"abc").unwrap(), patch);
        assert_eq!(j.head(root.binding()).unwrap(), head);
        assert_eq!(
            j.read(patch.id(), 0, 3).unwrap(),
            vec![Piece::Local(b"abc".to_vec())]
        );
        failure(j.write([10; 32], head.id(), 0, b"new"), Failure::Quota);
        failure(j.begin([11; 32], binding(3, 0), 0), Failure::Quota);
        assert_eq!(rows(&s.connection), before);
    }
    for limits in [
        Limits {
            views: 0,
            ..Limits::default()
        },
        Limits {
            operations: HARD_VIEWS + 1,
            ..Limits::default()
        },
        Limits {
            extents: HARD_EXTENTS + 1,
            ..Limits::default()
        },
        Limits {
            metadata_bytes: HARD_METADATA + 1,
            ..Limits::default()
        },
        Limits {
            dirty_bytes: HARD_DIRTY + 1,
            ..Limits::default()
        },
    ] {
        assert!(Options::default().with_limits(limits).is_err());
    }
}
