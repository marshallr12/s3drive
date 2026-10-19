//! Data preparation against real private SQLite, authority history and SDK I/O.
use super::*;
use composition::storage;
use s3drive_managed::chunks::{ChunkContent, PreparedChunk};
use std::{path::Path, sync::Arc};

fn inherited(p: &PreparedChunk, base: &s3drive_managed::chunks::BaseFile, chunk: u64) -> bool {
    let ChunkContent::Inherited(proof) = p.content() else {
        panic!("expected authenticated inherited chunk")
    };
    assert_eq!(proof.offset(), chunk * C);
    assert_eq!(proof.length(), p.length());
    let checked = base.verify_chunk_proof(&proof.encode()).unwrap();
    assert_eq!(checked.reference(), proof.reference());
    proof.reference().is_some()
}
fn content(p: &PreparedChunk, expected: &[u8]) {
    let ChunkContent::Bytes { bytes, sha256 } = p.content() else {
        panic!("expected new verified bytes")
    };
    assert_eq!(bytes, expected);
    assert_eq!(*sha256, <[u8; 32]>::from(Sha256::digest(expected)));
    assert_eq!(p.length(), expected.len() as u64);
}
pub async fn flow(f: &mut Fixture, path: &Path) {
    f.tree(
        10,
        130 * C + 17,
        BTreeMap::from([
            (0, pattern(0, C)),
            (127, pattern(127, C)),
            (128, pattern(128, C)),
            (130, pattern(130, 17)),
        ]),
    )
    .await;
    let r = f.reader().await;
    let base = r.open_file(&id(10)).await.unwrap().pinned_base();
    let local = path.join("prepared café 🦀");
    let (s, c) = storage(&local).await;
    let initial = c
        .begin([1; 32], base.sparse_binding().unwrap(), base.size())
        .await
        .unwrap();
    let view = c
        .write(
            [2; 32],
            initial.id(),
            127 * C + 5,
            Arc::from([255].as_slice()),
        )
        .await
        .unwrap();
    let e = r.reopen_edited(c.clone(), view.id()).await.unwrap();
    assert!(std::mem::size_of_val(&e.prepare_chunk(127)) <= 8192);
    let gets = data_gets(f);
    let changed = e.prepare_chunk(127).await.unwrap();
    let mut expected = pattern(127, C);
    expected[5] = 255;
    content(&changed, &expected);
    assert_eq!(changed.view_id(), view.id());
    assert_eq!(changed.binding(), view.binding());
    assert_eq!(changed.offset(), 127 * C);
    assert_eq!(
        data_gets(f) - gets,
        1,
        "one-byte edit fetched unchanged data"
    );
    let gets = data_gets(f);
    assert!(inherited(&e.prepare_chunk(128).await.unwrap(), &base, 128));
    assert!(!inherited(&e.prepare_chunk(129).await.unwrap(), &base, 129));
    assert!(inherited(&e.prepare_chunk(130).await.unwrap(), &base, 130));
    assert_eq!(data_gets(f), gets, "unchanged proof downloaded data");
    // A changed final length must verify all 17 old bytes, not inherit a prefix.
    let short = c.resize([3; 32], view.id(), 130 * C + 7).await.unwrap();
    let shrunk = r.reopen_edited(c.clone(), short.id()).await.unwrap();
    let gets = data_gets(f);
    content(&shrunk.prepare_chunk(130).await.unwrap(), &pattern(130, 7));
    assert_eq!(data_gets(f) - gets, 1);
    let grown = c.resize([4; 32], short.id(), 130 * C + 33).await.unwrap();
    let g = r.reopen_edited(c.clone(), grown.id()).await.unwrap();
    let mut expected = pattern(130, 7);
    expected.resize(33, 0);
    content(&g.prepare_chunk(130).await.unwrap(), &expected);
    // Growing the original old partial chunk also cannot inherit 17 as 33.
    f.create(11, "partial growth 🦀", 200, false).await;
    f.tree(11, 17, BTreeMap::from([(0, pattern(0, 17))])).await;
    let b = r.open_file(&id(11)).await.unwrap().pinned_base();
    let v = c
        .begin([5; 32], b.sparse_binding().unwrap(), 33)
        .await
        .unwrap();
    let partial = r.reopen_edited(c.clone(), v.id()).await.unwrap();
    let mut expected = pattern(0, 17);
    expected.resize(33, 0);
    content(&partial.prepare_chunk(0).await.unwrap(), &expected);
    let huge = c.resize([6; 32], grown.id(), 65_536 * C).await.unwrap();
    let h = r.reopen_edited(c.clone(), huge.id()).await.unwrap();
    let gets = data_gets(f);
    let hole = h.prepare_chunk(65_535).await.unwrap();
    assert!(matches!(hole.content(), ChunkContent::Hole));
    assert_eq!(hole.length(), C);
    assert_eq!(data_gets(f), gets);
    failed(h.prepare_chunk(65_536).await, ReadFailure::Bounds);
    failed(h.prepare_chunk(u64::MAX).await, ReadFailure::Bounds);
    // New heads do not replace a pinned preparation view.
    content(&e.prepare_chunk(127).await.unwrap(), &{
        let mut b = pattern(127, C);
        b[5] = 255;
        b
    });
    s.close().await.unwrap();
    assert!(e.prepare_chunk(128).await.is_err());
    f.restart_authority(path).await;
    let (s, c) = storage(&local).await;
    let r = f.reader().await;
    let reopened = r.reopen_edited(c.clone(), view.id()).await.unwrap();
    content(&reopened.prepare_chunk(127).await.unwrap(), &{
        let mut b = pattern(127, C);
        b[5] = 255;
        b
    });
    assert_eq!(c.head(view.binding().clone()).await.unwrap(), huge);
    s.close().await.unwrap();
}

pub async fn faults(f: &Fixture, path: &Path) {
    let tree = f
        .tree(
            10,
            C + 17,
            BTreeMap::from([(0, pattern(0, C)), (1, pattern(1, 17))]),
        )
        .await;
    let r = f.reader().await;
    let b = r.open_file(&id(10)).await.unwrap().pinned_base();
    let (s, c) = storage(&path.join("local")).await;
    let v = c
        .begin([1; 32], b.sparse_binding().unwrap(), b.size())
        .await
        .unwrap();
    let unchanged = r.reopen_edited(c.clone(), v.id()).await.unwrap();
    let patch = c
        .write([2; 32], v.id(), 0, Arc::from(b"XX".as_slice()))
        .await
        .unwrap();
    let changed = r.reopen_edited(c.clone(), patch.id()).await.unwrap();
    let short = c.resize([3; 32], patch.id(), C + 7).await.unwrap();
    let shortened = r.reopen_edited(c.clone(), short.id()).await.unwrap();
    let zero = c.resize([4; 32], short.id(), 0).await.unwrap();
    let zero = c.resize([5; 32], zero.id(), C).await.unwrap();
    let hole = r.reopen_edited(c.clone(), zero.id()).await.unwrap();
    let all_local = c
        .write([6; 32], zero.id(), 0, Arc::from("café 🦀".as_bytes()))
        .await
        .unwrap();
    let all_local = c
        .resize([7; 32], all_local.id(), "café 🦀".len() as u64)
        .await
        .unwrap();
    let local = r.reopen_edited(c.clone(), all_local.id()).await.unwrap();
    for mode in 31..=37 {
        f.mode.store(mode, Ordering::SeqCst);
        let before = f.s3.events().len();
        for e in [&unchanged, &changed, &hole, &local] {
            assert!(e.prepare_chunk(0).await.is_err(), "history mode{mode}");
        }
        assert_eq!(f.s3.events().len(), before);
    }
    f.mode.store(0, Ordering::SeqCst);
    for fault in [Fault::HeadDenied, Fault::Missing, Fault::BigHead] {
        f.s3.fault(Target::Exact(tree.root.clone()), fault);
        for e in [&unchanged, &changed, &hole, &local] {
            assert!(e.prepare_chunk(0).await.is_err(), "root fault{fault:?}");
        }
        f.s3.clear();
    }
    // Inheritance is explicitly reachability, not a new data-health receipt.
    f.s3.fault(Target::Data, Fault::GetDenied);
    let gets = data_gets(f);
    assert!(inherited(&unchanged.prepare_chunk(0).await.unwrap(), &b, 0));
    assert_eq!(data_gets(f), gets);
    assert!(changed.prepare_chunk(0).await.is_err());
    f.s3.clear();
    let original =
        f.s3.objects
            .lock()
            .unwrap()
            .get(&tree.data[1])
            .unwrap()
            .clone();
    *f.s3
        .objects
        .lock()
        .unwrap()
        .get_mut(&tree.data[1])
        .unwrap()
        .bytes
        .last_mut()
        .unwrap() ^= 1;
    assert!(
        shortened.prepare_chunk(1).await.is_err(),
        "truncation ignored corrupt unread old tail"
    );
    f.s3.objects
        .lock()
        .unwrap()
        .insert(tree.data[1].clone(), original);
    let db = rusqlite::Connection::open(path.join("local/metadata.sqlite3")).unwrap();
    let old: Vec<u8> = db
        .query_row(
            "SELECT data FROM sparse_view_blocks WHERE length(data)=2",
            [],
            |r| r.get(0),
        )
        .unwrap();
    db.execute(
        "UPDATE sparse_view_blocks SET data=?1 WHERE length(data)=2",
        [b"YY".as_slice()],
    )
    .unwrap();
    let before = f.s3.events().len();
    for e in [&unchanged, &changed, &hole, &local] {
        let error = e.prepare_chunk(0).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<s3drive_state::sparse_views::Failure>(),
            Some(&s3drive_state::sparse_views::Failure::Corrupt)
        );
    }
    assert_eq!(f.s3.events().len(), before);
    let retained: Vec<u8> = db
        .query_row(
            "SELECT data FROM sparse_view_blocks WHERE length(data)=2",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, b"YY");
    db.execute(
        "UPDATE sparse_view_blocks SET data=?1 WHERE length(data)=2",
        [old],
    )
    .unwrap();
    drop(db);
    assert_eq!(c.head(v.binding().clone()).await.unwrap(), all_local);
    content(&local.prepare_chunk(0).await.unwrap(), "café 🦀".as_bytes());
    s.close().await.unwrap();
}

pub async fn limits(f: &Fixture, path: &Path) {
    f.tree(10, C, data(0, 1)).await;
    let r = f.reader().await;
    let b = r.open_file(&id(10)).await.unwrap().pinned_base();
    let (s, c) = storage(&path.join("local")).await;
    let v = c
        .begin([1; 32], b.sparse_binding().unwrap(), C)
        .await
        .unwrap();
    let patch = c
        .write([2; 32], v.id(), 0, Arc::from([255].as_slice()))
        .await
        .unwrap();
    let e = r.reopen_edited(c.clone(), patch.id()).await.unwrap();
    f.s3.fault(Target::Data, Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let a = e.clone();
    let task = tokio::spawn(async move { a.prepare_chunk(0).await });
    f.s3.wait(arrived + 1).await;
    let newer = tokio::time::timeout(Duration::from_secs(3), c.resize([3; 32], patch.id(), 7))
        .await
        .unwrap()
        .unwrap();
    f.s3.clear();
    f.s3.permits.add_permits(1);
    let mut expected = pattern(0, C);
    expected[0] = 255;
    content(&task.await.unwrap().unwrap(), &expected);
    assert_eq!(c.head(v.binding().clone()).await.unwrap(), newer);
    f.s3.settle().await;
    f.s3.fault(Target::Data, Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let a = e.clone();
        tasks.push(tokio::spawn(async move { a.prepare_chunk(0).await }));
    }
    f.s3.wait(arrived + 4).await;
    failed(e.prepare_chunk(0).await, ReadFailure::Busy);
    failed(e.read_range(0, 1).await, ReadFailure::Busy);
    failed(b.prove_chunk(0).await, ReadFailure::Busy);
    failed(r.open_file(&id(10)).await, ReadFailure::Busy);
    for t in tasks {
        t.abort();
        assert!(t.await.unwrap_err().is_cancelled());
    }
    f.s3.clear();
    f.s3.permits.add_permits(4);
    f.s3.settle().await;
    content(&e.prepare_chunk(0).await.unwrap(), &expected);
    let short = f
        .reader_options(ReadOptions::new(Duration::from_secs(3)).unwrap())
        .await;
    let e = short.reopen_edited(c.clone(), patch.id()).await.unwrap();
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    f.s3.fault(Target::Data, Fault::Gate);
    let task = tokio::spawn(async move { e.prepare_chunk(0).await });
    f.s3.wait(arrived + 1).await;
    failed(task.await.unwrap(), ReadFailure::Timeout);
    f.s3.clear();
    f.s3.permits.add_permits(1);
    f.s3.settle().await;
    assert_eq!(b.read_range(0, 1).await.unwrap(), pattern(0, 1));
    s.close().await.unwrap();
}
