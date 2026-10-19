//! Real SDK reads + authenticated Rust authority; mock objects are seeded in
//! owned local memory. No provider uploads or mounted chunk acceptance.
#[path = "chunk_reader/fixture.rs"]
mod fixture;
#[path = "chunk_reader/native.rs"]
mod native;

#[path = "chunk_reader/composition.rs"]
mod composition;
#[path = "chunk_reader/pages.rs"]
mod pages;
#[path = "chunk_reader/proofs.rs"]
mod proofs;
#[path = "chunk_reader/reopen.rs"]
mod reopen;
use fixture::*;
use s3drive_authority::Mutation;
use s3drive_core::manifest::{
    Entry, Page, Reference,
    objects::{Namespace, Role},
};
use s3drive_managed::chunks::{ReadFailure, ReadOptions, Reader};
use s3drive_s3::S3Reader;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap, path::PathBuf, process::Stdio, sync::atomic::Ordering, time::Duration,
};
use tokio::io::AsyncReadExt;
fn bytes(s: &str) -> [u8; 16] {
    std::array::from_fn(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
}
fn failed<T>(r: s3drive_state::Result<T>, expected: ReadFailure) {
    let e = match r {
        Ok(_) => panic!("unexpected read success"),
        Err(e) => e,
    };
    assert_eq!(e.downcast_ref::<ReadFailure>(), Some(&expected));
}
fn data(start: u64, count: u64) -> BTreeMap<u64, Vec<u8>> {
    (start..start + count).map(|i| (i, pattern(i, C))).collect()
}
fn get_count(f: &Fixture) -> usize {
    f.s3.events().iter().filter(|e| e.method == "GET").count()
}
fn data_gets(f: &Fixture) -> usize {
    f.s3.events()
        .iter()
        .filter(|e| e.method == "GET" && e.path.contains("/data/"))
        .count()
}
async fn flow(f: &Fixture) {
    let r = f.reader().await;
    let before = f.s3.events().len();
    let empty = r.open_file(&id(10)).await.unwrap();
    assert_eq!(empty.size(), 0);
    assert!(empty.read_range(0, 1).await.unwrap().is_empty());
    assert_eq!(f.s3.events().len(), before);
    failed(
        r.open_file(s3drive_authority::ROOT).await,
        ReadFailure::IsDirectory,
    );
    let mut map = BTreeMap::new();
    for i in [0, 127, 128] {
        map.insert(i, pattern(i, C));
    }
    map.insert(129, pattern(129, 17));
    let tree = f.tree(10, 129 * C + 17, map).await;
    let before = f.s3.events().len();
    f.s3.settle().await;
    let served = f.s3.served.load(Ordering::SeqCst);
    let old = r.open_file(&id(10)).await.unwrap();
    assert_eq!(old.size(), tree.size);
    assert_eq!(old.metadata().name, "café 日本語 🦀.bin");
    assert_eq!(f.s3.events().len() - before, 2);
    f.s3.settle().await;
    assert_eq!(
        f.s3.served.load(Ordering::SeqCst) - served,
        tree.root_bytes as u64
    );
    assert_eq!(data_gets(f), 0, "open fetched data");
    let before = f.s3.events().len();
    let result = old.read_range(128 * C - 3, 8).await.unwrap();
    let expected = [pattern(127, C)[C as usize - 3..].to_vec(), pattern(128, 5)].concat();
    assert!(result == expected, "cross-page bytes differ");
    assert_eq!(
        f.s3.events().len() - before,
        8,
        "range fetched unexpected tree objects"
    );
    let meta = f.requests();
    let before = f.s3.events().len();
    let zeros = old.read_range(2 * C, 8 * C as usize).await.unwrap();
    assert_eq!(zeros.len(), 8 * C as usize);
    assert!(zeros.iter().all(|b| *b == 0));
    assert_eq!(f.s3.events().len() - before, 2);
    assert_eq!(f.requests(), meta, "range queried current namespace");
    let before = f.s3.events().len();
    assert!(old.read_range(u64::MAX, 1).await.unwrap().is_empty());
    assert!(old.read_range(1, 0).await.unwrap().is_empty());
    failed(
        old.read_range(0, 8 * C as usize + 1).await,
        ReadFailure::Bounds,
    );
    assert_eq!(f.s3.events().len(), before);
    assert!(old.read_range(129 * C, 99).await.unwrap() == pattern(129, 17));
    let content = "new root\0 日本語".as_bytes().to_vec();
    f.tree(
        10,
        content.len() as u64,
        BTreeMap::from([(0, content.clone())]),
    )
    .await;
    f.change(
        90,
        Mutation::Move {
            node: id(10),
            parent: s3drive_authority::ROOT.into(),
            name: "CON:a. café 🦀".into(),
        },
    )
    .await;
    let new = r.open_file(&id(10)).await.unwrap();
    assert_eq!(new.metadata().name, "CON:a. café 🦀");
    assert!(new.read_range(0, 99).await.unwrap() == content);
    let meta = f.requests();
    assert!(old.read_range(129 * C, 99).await.unwrap() == pattern(129, 17));
    assert_eq!(old.metadata().name, "café 日本語 🦀.bin");
    assert_eq!(f.requests(), meta);
    f.create(11, "published empty 🦀", 200, true).await;
    f.tree(11, 0, BTreeMap::new()).await;
    let before = f.s3.events().len();
    let e = r.open_file(&id(11)).await.unwrap();
    assert_eq!(e.size(), 0);
    assert_eq!(f.s3.events().len() - before, 2);
    assert!(e.read_range(0, 99).await.unwrap().is_empty());
    // Maximum unaligned range intersects nine complete chunks, not eight.
    f.create(12, "maximum binary", 200, true).await;
    let full = f.tree(12, 10 * C, data(0, 10)).await;
    assert_eq!(full.data.len(), 10);
    let file = r.open_file(&id(12)).await.unwrap();
    f.s3.settle().await;
    let before = f.s3.served.load(Ordering::SeqCst);
    let gets = data_gets(f);
    let out = file.read_range(1, 8 * C as usize).await.unwrap();
    assert_eq!(out.len(), 8 * C as usize);
    assert_eq!(data_gets(f) - gets, 9);
    f.s3.settle().await;
    assert_eq!(f.s3.served.load(Ordering::SeqCst) - before, 9 * C);
    assert!(
        out.iter().enumerate().all(|(i, b)| {
            let at = i as u64 + 1;
            *b == ((at / C * 17 + at % C) % 251) as u8
        }),
        "maximum range bytes differ"
    );
    let mut source = f.source.clone();
    source.alias = "other".into();
    source.bucket = "other-bucket".into();
    source.prefix = "secondary/".into();
    f.change(
        201,
        Mutation::RegisterStore {
            route: s3drive_s3::storage_fingerprint(&source).unwrap(),
            root: "managed/".into(),
        },
    )
    .await;
    f.create(13, "second bucket 日本語", 201, true).await;
    let value = b"second".to_vec();
    let object = Reference::new(
        bytes(&id(201)),
        bytes(&id(150)),
        0,
        Sha256::digest(&value).into(),
    )
    .unwrap();
    let ns = Namespace::new(
        "managed/".into(),
        bytes(&f.volume),
        bytes(&id(13)),
        bytes(&id(201)),
    )
    .unwrap();
    let path = format!(
        "/{}/{}{}",
        source.bucket,
        source.prefix,
        ns.key(Role::Data, &object).unwrap().as_str()
    );
    f.s3.put(path, value.clone(), "second");
    let page = Page::new(
        bytes(&f.volume),
        bytes(&id(13)),
        0,
        0,
        vec![Entry::Data {
            length: value.len() as u64,
            object,
        }],
    )
    .unwrap();
    let tree2 = f.publish_page(13, 201, page, None).await;
    let prefix = format!("/{}/{}", f.source.bucket, f.source.prefix);
    let relative = tree2.root.strip_prefix(&prefix).unwrap();
    let moved = format!("/{}/{}{relative}", source.bucket, source.prefix);
    {
        let mut objects = f.s3.objects.lock().unwrap();
        let root = objects.remove(&tree2.root).unwrap();
        objects.insert(moved, root);
    }
    let before = f.s3.events().len();
    failed(r.open_file(&id(13)).await, ReadFailure::Binding);
    assert_eq!(before, f.s3.events().len());
    let mapped = Reader::open(
        f.wire.clone(),
        f.volume.clone(),
        vec![
            (id(200), S3Reader::from_source(&f.source).await.unwrap()),
            (id(201), S3Reader::from_source(&source).await.unwrap()),
        ],
    )
    .await
    .unwrap();
    let second = mapped.open_file(&id(13)).await.unwrap();
    assert!(second.read_range(0, 99).await.unwrap() == value);
    let events = f.s3.events();
    assert_eq!(events.len() - before, 4);
    assert!(
        events[before..]
            .iter()
            .all(|e| e.path.starts_with("/other-bucket/secondary/"))
    );
    for e in f.s3.events().iter().filter(|e| e.method == "GET") {
        assert!(e.range.is_some() && e.matched.is_some());
        assert!(e.version.is_none());
    }
    assert!(get_count(f) > 0);
    assert_eq!(tree.pages.len(), 2);
    // A new root may reuse an immutable chunk from an older publication.
    let old_chunk = Reference::new(
        bytes(&id(200)),
        bytes(&id(40)),
        0,
        Sha256::digest(pattern(0, C)).into(),
    )
    .unwrap();
    let reused = Page::new(
        bytes(&f.volume),
        bytes(&id(10)),
        0,
        0,
        vec![Entry::Data {
            length: C,
            object: old_chunk,
        }],
    )
    .unwrap();
    f.publish_page(10, 200, reused, None).await;
    let reused = r.open_file(&id(10)).await.unwrap();
    assert!(reused.read_range(0, 19).await.unwrap() == pattern(0, 19));

    // Sparse 64 GiB metadata exercises the deepest supported tree without
    // allocating the logical file. The range crosses a root-child boundary.
    f.create(14, "maximum depth 日本語", 200, true).await;
    let mut map = data(16383, 9);
    map.insert(65535, pattern(65535, C));
    let deepest = f.tree(14, 65536 * C, map).await;
    assert_eq!(deepest.pages.len(), 516);
    let before = f.s3.events().len();
    let file = r.open_file(&id(14)).await.unwrap();
    assert_eq!(f.s3.events().len() - before, 2);
    let before = f.s3.events().len();
    let gets = data_gets(f);
    let start = 16383 * C + 1;
    let out = file.read_range(start, 8 * C as usize).await.unwrap();
    assert_eq!(out.len(), 8 * C as usize);
    assert_eq!(data_gets(f) - gets, 9);
    assert_eq!(f.s3.events().len() - before, 54);
    assert!(out.iter().enumerate().all(|(i, b)| {
        let at = start + i as u64;
        *b == ((at / C * 17 + at % C) % 251) as u8
    }));
    assert!(
        file.read_range(65536 * C - 3, 99).await.unwrap() == pattern(65535, C)[C as usize - 3..]
    );
}
async fn integrity(f: &Fixture) {
    f.tree(10, 2 * C, data(0, 2)).await;
    let r = f.reader().await;
    let good = r.open_file(&id(10)).await.unwrap();
    for mode in [6, 7, 8, 9, 10, 11, 12, 14] {
        f.mode.store(mode, Ordering::SeqCst);
        let before = f.s3.events().len();
        assert!(
            r.open_file(&id(10)).await.is_err(),
            "invalid metadata accepted"
        );
        assert_eq!(f.s3.events().len(), before);
    }
    for mode in [15, 16] {
        f.mode.store(mode, Ordering::SeqCst);
        assert!(r.open_file(&id(10)).await.is_err());
    }
    f.mode.store(0, Ordering::SeqCst);
    for mode in 1..=5 {
        f.mode.store(mode, Ordering::SeqCst);
        assert!(
            Reader::open(
                f.wire.clone(),
                f.volume.clone(),
                vec![(id(200), S3Reader::from_source(&f.source).await.unwrap())]
            )
            .await
            .is_err()
        );
    }
    f.mode.store(13, Ordering::SeqCst);
    assert!(
        Reader::open(
            f.wire.clone(),
            f.volume.clone(),
            vec![(id(200), S3Reader::from_source(&f.source).await.unwrap())]
        )
        .await
        .is_err()
    );
    f.mode.store(0, Ordering::SeqCst);
    let cfg = S3Reader::from_source(&f.source).await.unwrap();
    let count = f.requests();
    assert!(
        Reader::open(
            f.wire.clone(),
            f.volume.clone(),
            vec![(id(200), cfg.clone()), (id(200), cfg)]
        )
        .await
        .is_err()
    );
    assert_eq!(f.requests(), count);
    let events = f.s3.events();
    let key = events
        .iter()
        .find(|e| e.path.contains("/data/"))
        .map(|e| e.path.clone());
    assert!(key.is_none());
    let before = data_gets(f);
    assert!(good.read_range(0, 4).await.unwrap() == pattern(0, 4));
    assert_eq!(data_gets(f) - before, 1);
    let data_path =
        f.s3.events()
            .iter()
            .find(|e| e.path.contains("/data/"))
            .unwrap()
            .path
            .clone();
    {
        let mut m = f.s3.objects.lock().unwrap();
        m.get_mut(&data_path).unwrap().bytes[500] ^= 1;
    }
    assert!(
        good.read_range(0, 4).await.is_err(),
        "tamper outside returned subrange accepted"
    );
    {
        let mut m = f.s3.objects.lock().unwrap();
        m.get_mut(&data_path).unwrap().bytes[500] ^= 1;
    }
    assert!(good.read_range(0, 4).await.unwrap() == pattern(0, 4));
    // Late failure after a correct first chunk must not return its prefix.
    let second =
        f.s3.objects
            .lock()
            .unwrap()
            .keys()
            .find(|k| k.contains("/data/") && k.ends_with("0000000000000001"))
            .unwrap()
            .clone();
    {
        let mut m = f.s3.objects.lock().unwrap();
        m.get_mut(&second).unwrap().bytes[500] ^= 1;
    }
    assert!(good.read_range(C - 4, 8).await.is_err());
    {
        let mut m = f.s3.objects.lock().unwrap();
        m.get_mut(&second).unwrap().bytes[500] ^= 1;
    }
    let expected = [pattern(0, C)[C as usize - 4..].to_vec(), pattern(1, 4)].concat();
    assert!(good.read_range(C - 4, 8).await.unwrap() == expected);
    // Raw-object hash and page-domain hash are distinct mandatory checks.
    let page = Page::new(
        bytes(&f.volume),
        bytes(&id(10)),
        0,
        0,
        vec![Entry::Hole { length: 3 }],
    )
    .unwrap();
    let raw: [u8; 32] = Sha256::digest(page.encode()).into();
    f.publish_page(10, 200, page, Some(raw)).await;
    assert!(r.open_file(&id(10)).await.is_err());
    let wrong = Page::new(
        bytes(&f.volume),
        bytes(&id(999)),
        0,
        0,
        vec![Entry::Hole { length: 3 }],
    )
    .unwrap();
    f.publish_page(10, 200, wrong, None).await;
    assert!(r.open_file(&id(10)).await.is_err());
    let child = Reference::new(bytes(&id(200)), bytes(&id(99)), 1 << 56, [1; 32]).unwrap();
    let wrong_level = Page::new(
        bytes(&f.volume),
        bytes(&id(10)),
        0,
        1,
        vec![Entry::Child {
            length: 3,
            object: child,
        }],
    )
    .unwrap();
    f.publish_page(10, 200, wrong_level, None).await;
    failed(r.open_file(&id(10)).await, ReadFailure::Integrity);
    let foreign = Reference::new(bytes(&id(999)), bytes(&id(99)), 0, [1; 32]).unwrap();
    let page = Page::new(
        bytes(&f.volume),
        bytes(&id(10)),
        0,
        0,
        vec![Entry::Data {
            length: 3,
            object: foreign,
        }],
    )
    .unwrap();
    f.publish_page(10, 200, page, None).await;
    failed(r.open_file(&id(10)).await, ReadFailure::Binding);
    // Authenticated child bytes with wrong identity/length/level/encoding.
    for mode in 0..6 {
        let size = 129 * C;
        let child = match mode {
            0 => Page::new(
                bytes(&f.volume),
                bytes(&id(999)),
                0,
                0,
                vec![Entry::Hole { length: size }],
            )
            .unwrap(),
            1 => Page::new(
                bytes(&f.volume),
                bytes(&id(10)),
                0,
                0,
                vec![Entry::Hole { length: size - C }],
            )
            .unwrap(),
            4 => Page::new(
                bytes(&f.volume),
                bytes(&id(10)),
                0,
                1,
                vec![Entry::Child {
                    length: size,
                    object: Reference::new(bytes(&id(200)), bytes(&id(199)), 1 << 56, [3; 32])
                        .unwrap(),
                }],
            )
            .unwrap(),
            5 => Page::new(
                bytes(&f.volume),
                bytes(&id(10)),
                0,
                0,
                vec![
                    Entry::Hole { length: 128 * C },
                    Entry::Data {
                        length: C,
                        object: Reference::new(bytes(&id(999)), bytes(&id(199)), 0, [3; 32])
                            .unwrap(),
                    },
                ],
            )
            .unwrap(),
            _ => Page::new(
                bytes(&f.volume),
                bytes(&id(10)),
                0,
                0,
                vec![Entry::Hole { length: size }],
            )
            .unwrap(),
        };
        let mut b = child.encode();
        if mode == 2 {
            b[0] ^= 1;
        }
        let mut hash = Sha256::new();
        hash.update(b"s3drive-manifest-page-v1\0");
        hash.update(&b);
        let digest: [u8; 32] = hash.finalize().into();
        let reference =
            Reference::new(bytes(&id(200)), bytes(&id(100 + mode)), 1 << 56, digest).unwrap();
        let ns = Namespace::new(
            "managed/".into(),
            bytes(&f.volume),
            bytes(&id(10)),
            bytes(&id(200)),
        )
        .unwrap();
        let path = f.absolute(
            ns.key(if mode == 3 { Role::Data } else { Role::Page }, &reference)
                .unwrap()
                .as_str(),
        );
        f.s3.put(path, b, "child");
        let parent = Page::new(
            bytes(&f.volume),
            bytes(&id(10)),
            0,
            1,
            vec![Entry::Child {
                length: size,
                object: reference,
            }],
        )
        .unwrap();
        f.publish_page(10, 200, parent, None).await;
        let file = r.open_file(&id(10)).await.unwrap();
        assert!(file.read_range(0, 4).await.is_err());
    }
    // Legacy payloads stay explicitly unsupported by the new reader.
    let n = f.node(10).await;
    f.change(
        180,
        Mutation::ReserveContent {
            node: n.id,
            expected_revision: n.revision,
            store: id(200),
            size: 3,
            sha256: "a".repeat(64),
            expected_payload: s3drive_authority::payload_reference(&n.payload),
        },
    )
    .await;
    f.change(
        181,
        Mutation::Publish {
            reservation: id(180),
            upload_operation: "d".repeat(64),
            etag: "\"legacy\"".into(),
            version: None,
        },
    )
    .await;
    failed(r.open_file(&id(10)).await, ReadFailure::UnsupportedContent);
}
async fn transport(f: &Fixture) {
    let tree = f.tree(10, C, data(0, 1)).await;
    f.s3.objects
        .lock()
        .unwrap()
        .get_mut(&tree.data[0])
        .unwrap()
        .version = Some("initial-v1".into());
    let r = f.reader().await;
    let file = r.open_file(&id(10)).await.unwrap();
    for fault in [
        Fault::HeadDenied,
        Fault::GetDenied,
        Fault::Missing,
        Fault::BigHead,
        Fault::BadRange,
        Fault::BadEtag,
        Fault::BadVersion,
        Fault::Short,
        Fault::Extra,
    ] {
        f.s3.fault(Target::Exact(tree.data[0].clone()), fault);
        assert!(
            file.read_range(0, 4).await.is_err(),
            "bad data response accepted: {fault:?}"
        );
        f.s3.clear();
        assert!(file.read_range(0, 4).await.unwrap() == pattern(0, 4));
    }
    for fault in [
        Fault::HeadDenied,
        Fault::GetDenied,
        Fault::Missing,
        Fault::BigHead,
        Fault::BadRange,
        Fault::BadEtag,
        Fault::Short,
        Fault::Extra,
    ] {
        f.s3.fault(Target::Exact(tree.root.clone()), fault);
        assert!(r.open_file(&id(10)).await.is_err());
        f.s3.clear();
    }
    let mut map = BTreeMap::new();
    map.insert(128, pattern(128, 4));
    let tree = f.tree(10, 128 * C + 4, map).await;
    let file = r.open_file(&id(10)).await.unwrap();
    for fault in [
        Fault::HeadDenied,
        Fault::GetDenied,
        Fault::Missing,
        Fault::BigHead,
        Fault::BadRange,
        Fault::BadEtag,
        Fault::Short,
        Fault::Extra,
    ] {
        f.s3.fault(Target::Exact(tree.pages[1].clone()), fault);
        assert!(file.read_range(128 * C, 4).await.is_err());
        f.s3.clear();
    }
    // A version-bearing HEAD pins its exact version on the GET.
    {
        let mut m = f.s3.objects.lock().unwrap();
        m.get_mut(&tree.data[0]).unwrap().version = Some("child-v1".into());
    }
    assert!(file.read_range(128 * C, 4).await.unwrap() == pattern(128, 4));
    let e = f.s3.events().last().unwrap().clone();
    assert_eq!(e.version.as_deref(), Some("child-v1"));
    f.s3.fault(Target::Exact(tree.data[0].clone()), Fault::BadVersion);
    assert!(file.read_range(128 * C, 4).await.is_err());
    f.s3.clear();
    let before = f.s3.events().len();
    failed(
        file.read_range(0, 8 * C as usize + 1).await,
        ReadFailure::Bounds,
    );
    assert_eq!(before, f.s3.events().len());
    assert!(ReadOptions::new(Duration::ZERO).is_err());
    assert!(ReadOptions::new(Duration::from_secs(31)).is_err());
    assert_eq!(ReadOptions::default().deadline(), Duration::from_secs(30));
}
async fn budgets(f: &Fixture) {
    f.create(11, "metadata empty", 200, false).await;
    let tree = f.tree(10, C, data(0, 1)).await;
    let r = f.reader().await;
    let file = r.open_file(&id(10)).await.unwrap();
    f.s3.fault(Target::Data, Fault::Gate);
    let base = f.s3.arrived.load(Ordering::SeqCst);
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let file = file.clone();
        tasks.push(tokio::spawn(async move { file.read_range(0, 4).await }));
    }
    f.s3.wait(base + 4).await;
    let events = f.s3.events().len();
    let meta = f.requests();
    failed(file.read_range(0, 4).await, ReadFailure::Busy);
    failed(file.prove_chunk(0).await, ReadFailure::Busy);
    failed(r.open_file(&id(11)).await, ReadFailure::Busy);
    failed(r.node(&id(11)).await, ReadFailure::Busy);
    failed(r.list(s3drive_authority::ROOT).await, ReadFailure::Busy);
    assert!(file.read_range(u64::MAX, 1).await.unwrap().is_empty());
    assert_eq!(events, f.s3.events().len());
    assert_eq!(meta, f.requests());
    let task = tasks.remove(0);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    f.s3.clear();
    assert!(file.read_range(0, 4).await.unwrap() == pattern(0, 4));
    f.s3.permits.add_permits(4);
    for task in tasks {
        assert!(task.await.unwrap().unwrap() == pattern(0, 4));
    }
    drop(file);
    // Open operations share active slots and release reserved snapshots on cancel.
    f.s3.fault(Target::Pages, Fault::Gate);
    let base = f.s3.arrived.load(Ordering::SeqCst);
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let r = r.clone();
        tasks.push(tokio::spawn(async move { r.open_file(&id(10)).await }));
    }
    f.s3.wait(base + 4).await;
    let meta = f.requests();
    failed(r.open_file(&id(11)).await, ReadFailure::Busy);
    assert_eq!(meta, f.requests());
    let task = tasks.remove(0);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    f.s3.clear();
    drop(r.open_file(&id(11)).await.unwrap());
    f.s3.permits.add_permits(4);
    for task in tasks {
        drop(task.await.unwrap().unwrap());
    }
    let mut snapshots = Vec::new();
    for _ in 0..256 {
        snapshots.push(r.open_file(&id(11)).await.unwrap());
    }
    let meta = f.requests();
    failed(r.open_file(&id(11)).await, ReadFailure::Busy);
    assert_eq!(meta, f.requests());
    let shared = snapshots[0].clone();
    drop(snapshots.remove(0));
    failed(r.open_file(&id(11)).await, ReadFailure::Busy);
    drop(shared);
    let one = r.open_file(&id(11)).await.unwrap();
    drop(snapshots);
    drop(one);
    drop(r.open_file(&id(10)).await.unwrap());
    // Overall deadline is shorter than the unchanged SDK deadline; no partial result.
    let r = f
        .reader_options(ReadOptions::new(Duration::from_secs(3)).unwrap())
        .await;
    let file = r.open_file(&id(10)).await.unwrap();
    f.s3.fault(Target::Exact(tree.data[0].clone()), Fault::Gate);
    let base = f.s3.arrived.load(Ordering::SeqCst);
    let task = {
        let file = file.clone();
        tokio::spawn(async move { file.read_range(0, 4).await })
    };
    f.s3.wait(base + 1).await;
    let result = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
    failed(result, ReadFailure::Timeout);
    f.s3.clear();
    f.s3.permits.add_permits(1);
    assert!(file.read_range(0, 4).await.unwrap() == pattern(0, 4));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manifest_reader_child() {
    let Ok(mode) = std::env::var("S3DRIVE_READER_CASE") else {
        return;
    };
    let path = PathBuf::from(std::env::var("S3DRIVE_READER_ROOT").unwrap());
    let mut f = Fixture::open(&path, mode == "flow").await;
    match mode.as_str() {
        "flow" => Box::pin(flow(&f)).await,
        "accounting" => Box::pin(accounting(&f)).await,
        "composition" => Box::pin(composition::flow(&mut f, &path)).await,
        "composition-faults" => Box::pin(composition::faults(&f, &path)).await,
        "composition-limits" => Box::pin(composition::limits(&f, &path)).await,
        "proofs" => Box::pin(proofs::flow(&f)).await,
        "pages" => Box::pin(pages::flow(&f)).await,
        "reopen" => Box::pin(reopen::flow(&mut f, &path)).await,
        "integrity" => Box::pin(integrity(&f)).await,
        "transport" => Box::pin(transport(&f)).await,
        "budgets" => Box::pin(budgets(&f)).await,
        "engine" => Box::pin(native::engine(&f, &path)).await,
        "formats" => Box::pin(native::formats(&f, &path)).await,
        #[cfg(any(target_os = "linux", windows))]
        "native" => Box::pin(native::native(&f, &path)).await,
        _ => panic!(),
    };
    f.close().await;
}
async fn run(mode: &str) {
    let tmp = tempfile::tempdir().unwrap();
    let credentials = tmp.path().join("credentials");
    let config = tmp.path().join("aws-config");
    std::fs::write(
        &credentials,
        "[fixture]\naws_access_key_id=TESTPROFILE\naws_secret_access_key=TESTPROFILESECRET\n",
    )
    .unwrap();
    std::fs::write(&config, "").unwrap();
    let child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "manifest_reader_child", "--nocapture"])
        .env("S3DRIVE_READER_CASE", mode)
        .env("S3DRIVE_READER_ROOT", tmp.path())
        .env("AWS_SHARED_CREDENTIALS_FILE", credentials)
        .env("AWS_CONFIG_FILE", config)
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    check_child(child, mode, Duration::from_secs(120)).await;
}
async fn check_child(mut child: tokio::process::Child, mode: &str, deadline: Duration) {
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stdout.take(65537).read_to_end(&mut bytes).await.unwrap();
        bytes
    });
    let err = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr.take(65537).read_to_end(&mut bytes).await.unwrap();
        bytes
    });
    let status = tokio::time::timeout(deadline, child.wait()).await;
    if status.is_err() {
        tokio::time::timeout(Duration::from_secs(15), child.kill())
            .await
            .unwrap()
            .unwrap();
    }
    let code = tokio::time::timeout(Duration::from_secs(15), child.wait())
        .await
        .unwrap()
        .unwrap();
    let out = tokio::time::timeout(Duration::from_secs(5), out)
        .await
        .unwrap()
        .unwrap();
    let err = tokio::time::timeout(Duration::from_secs(5), err)
        .await
        .unwrap()
        .unwrap();
    assert!(
        status.is_ok() && code.success() && out.len() <= 65536 && err.len() <= 65536,
        "reader case {mode}: {code}: {} {}",
        String::from_utf8_lossy(&out),
        String::from_utf8_lossy(&err)
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manifest_ranges_sparse_unicode_empty_cross_page_pins_and_maximum() {
    run("flow").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manifest_ranges_refuse_metadata_hash_context_role_and_late_chunk_faults() {
    run("integrity").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manifest_ranges_refuse_denied_missing_oversized_truncated_or_changed_objects() {
    run("transport").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manifest_ranges_bound_slots_snapshots_cancel_and_overall_deadline() {
    run("budgets").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manifest_chunk_proofs_bind_snapshot_without_data_reads_and_reject_forgery() {
    run("proofs").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manifest_page_proofs_bind_subtrees_and_reuse_old_root_without_data_reads() {
    run("pages").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manifest_base_descriptors_reopen_exact_history_with_fresh_auth_and_shared_bounds() {
    run("reopen").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manifest_base_reopen_worker() {
    reopen::worker().await;
}

async fn accounting(f: &Fixture) {
    let tree = f.tree(10, C, data(0, 1)).await;
    let r = f.reader().await;
    f.s3.settle().await;
    let before = f.s3.served.load(Ordering::SeqCst);
    f.s3.fault(Target::Exact(tree.root.clone()), Fault::AccountingGate);
    let file = r.open_file(&id(10)).await.unwrap();
    f.s3.wait(1).await;
    assert_eq!(
        f.s3.served.load(Ordering::SeqCst),
        before,
        "owned gate did not delay post-body accounting"
    );
    let settled = f.s3.settle();
    tokio::pin!(settled);
    tokio::select! {biased;
        _=&mut settled=>panic!("settlement returned while response accounting remained gated"),
        _=tokio::task::yield_now()=>(),
    }
    f.s3.permits.add_permits(1);
    settled.await;
    assert_eq!(
        f.s3.served.load(Ordering::SeqCst) - before,
        tree.root_bytes as u64
    );
    f.s3.clear();
    assert_eq!(file.read_range(0, 3).await.unwrap(), pattern(0, 3));
    f.s3.settle().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn response_accounting_waits_for_owned_handler_after_client_body_completion() {
    run("accounting").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn edited_views_compose_exact_history_local_patches_zeros_and_fragmented_chunks_after_restart()
 {
    run("composition").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn edited_views_refuse_wrong_bindings_auth_denials_late_faults_and_principal_rebinding() {
    run("composition-faults").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn edited_views_share_reader_limits_cancel_deadline_and_release_sql_owner_before_network() {
    run("composition-limits").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn edited_view_reopen_worker() {
    composition::worker().await;
}
