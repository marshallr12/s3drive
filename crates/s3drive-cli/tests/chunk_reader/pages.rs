use super::*;
use s3drive_managed::chunks::MAX_PAGE_PROOF_BYTES;

pub async fn flow(f: &Fixture) {
    let reader = f.reader().await;
    let metadata_empty = reader.open_file(&id(10)).await.unwrap();
    failed(
        metadata_empty.prove_root().await,
        ReadFailure::UnsupportedContent,
    );
    f.tree(
        10,
        130 * C,
        BTreeMap::from([(127, pattern(127, C)), (128, pattern(128, C))]),
    )
    .await;
    let file = reader.open_file(&id(10)).await.unwrap();
    let start = f.s3.events().len();
    let requests = f.requests();
    let root = file.prove_root().await.unwrap();
    assert_eq!(root.level(), 1);
    assert_eq!(root.offset(), 0);
    assert_eq!(root.length(), 130 * C);
    assert_eq!(root.encode().len(), 50);
    assert_eq!(start, f.s3.events().len(), "root proof refetched metadata");
    let leaf = file.prove_page_containing(129 * C + 7, 0).await.unwrap();
    assert_eq!(leaf.offset(), 128 * C);
    assert_eq!(leaf.length(), 2 * C);
    assert_eq!(leaf.level(), 0);
    assert_eq!(
        f.s3.events().len() - start,
        2,
        "leaf proof exceeded one metadata page"
    );
    let chunk = file.prove_chunk(128).await.unwrap();
    assert_eq!(chunk.base_binding(), leaf.base_binding());
    assert_eq!(root.base_binding(), leaf.base_binding());
    assert_eq!(f.requests(), requests, "proof requested current namespace");
    assert!(
        f.s3.events()[start..]
            .iter()
            .all(|e| !e.path.contains("/data/"))
    );
    let witness = leaf.encode();
    assert!(witness.len() <= MAX_PAGE_PROOF_BYTES);
    let before = f.s3.events().len();
    let recovered = file.verify_page_proof(&witness).unwrap();
    assert_eq!(recovered.reference(), leaf.reference());
    assert_eq!(recovered.page(), leaf.page());
    assert_eq!(recovered.encode(), witness);
    assert_eq!(f.s3.events().len(), before);
    assert!(file.verify_chunk_proof(&witness).is_err());
    assert!(file.verify_page_proof(&chunk.encode()).is_err());
    for len in [0, 7, 8, 39, 40, 48, 49, 50, witness.len() - 1] {
        assert!(
            file.verify_page_proof(&witness[..len]).is_err(),
            "truncated page proof accepted: {len}"
        );
    }
    let mut bad = witness.clone();
    bad.push(0);
    assert!(file.verify_page_proof(&bad).is_err());
    let mut bad = witness.clone();
    bad[8] ^= 1;
    failed(file.verify_page_proof(&bad), ReadFailure::Binding);
    let mut bad = witness.clone();
    bad[48] = 3;
    failed(file.verify_page_proof(&bad), ReadFailure::Bounds);
    let mut bad = witness.clone();
    bad[49] = 3;
    failed(file.verify_page_proof(&bad), ReadFailure::Bounds);
    let mut bad = witness.clone();
    bad[40..48].copy_from_slice(&(129 * C).to_le_bytes());
    assert!(
        file.verify_page_proof(&bad).is_err(),
        "page relocated to a subrange"
    );
    let mut bad = witness.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert!(file.verify_page_proof(&bad).is_err());
    assert!(
        file.verify_page_proof(&vec![0; MAX_PAGE_PROOF_BYTES + 1])
            .is_err()
    );
    failed(
        file.prove_page_containing(130 * C, 0).await,
        ReadFailure::Bounds,
    );
    failed(file.prove_page_containing(0, 2).await, ReadFailure::Bounds);
    f.s3.fault(Target::Pages, Fault::HeadDenied);
    assert!(file.prove_page_containing(128 * C, 0).await.is_err());
    f.s3.clear();

    // Root proofs share admission even when their metadata is already cached.
    f.s3.fault(Target::Pages, Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let file = file.clone();
        tasks.push(tokio::spawn(async move {
            file.prove_page_containing(128 * C, 0).await
        }));
    }
    f.s3.wait(arrived + 4).await;
    let before = f.s3.events().len();
    failed(file.prove_root().await, ReadFailure::Busy);
    failed(file.read_range(0, 1).await, ReadFailure::Busy);
    assert_eq!(before, f.s3.events().len());
    let task = tasks.remove(0);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    f.s3.clear();
    f.s3.permits.add_permits(4);
    file.prove_page_containing(128 * C, 0).await.unwrap();
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    let short = f
        .reader_options(ReadOptions::new(Duration::from_secs(3)).unwrap())
        .await;
    let short = short.open_file(&id(10)).await.unwrap();
    f.s3.fault(Target::Pages, Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let task = {
        let file = short.clone();
        tokio::spawn(async move { file.prove_page_containing(128 * C, 0).await })
    };
    f.s3.wait(arrived + 1).await;
    failed(
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap(),
        ReadFailure::Timeout,
    );
    f.s3.clear();
    f.s3.permits.add_permits(1);
    short.prove_root().await.unwrap();
    f.change(
        180,
        Mutation::Move {
            node: id(10),
            parent: s3drive_authority::ROOT.into(),
            name: "renamed pages 日本語".into(),
        },
    )
    .await;
    let renamed = reader.open_file(&id(10)).await.unwrap();
    renamed.verify_page_proof(&witness).unwrap();
    f.create(11, "other page file", 200, false).await;
    f.tree(11, 130 * C, BTreeMap::new()).await;
    let other = reader.open_file(&id(11)).await.unwrap();
    failed(other.verify_page_proof(&witness), ReadFailure::Binding);
    f.tree(10, 65536 * C, BTreeMap::new()).await;
    let large = reader.open_file(&id(10)).await.unwrap();
    failed(large.verify_page_proof(&witness), ReadFailure::Binding);
    file.verify_page_proof(&witness).unwrap();
    let before = f.s3.events().len();
    let deep = large.prove_page_containing(65535 * C + 3, 0).await.unwrap();
    assert_eq!(deep.offset(), 65408 * C);
    assert_eq!(deep.length(), 128 * C);
    assert_eq!(
        f.s3.events().len() - before,
        4,
        "deep proof exceeded two pages"
    );
    assert!(deep.encode().len() <= MAX_PAGE_PROOF_BYTES);
    large.verify_page_proof(&deep.encode()).unwrap();
    f.create(13, "published empty page 🦀", 200, false).await;
    f.tree(13, 0, BTreeMap::new()).await;
    let empty = reader.open_file(&id(13)).await.unwrap();
    let root = empty.prove_root().await.unwrap();
    assert_eq!((root.offset(), root.length(), root.level()), (0, 0, 0));
    assert_eq!(root.encode().len(), 50);
    empty.verify_page_proof(&root.encode()).unwrap();
    failed(empty.prove_page_containing(1, 0).await, ReadFailure::Bounds);
    root_as_child(f, &reader).await;
}
async fn root_as_child(f: &Fixture, reader: &Reader) {
    f.create(12, "root reused as child 日本語", 200, false)
        .await;
    let old_tree = f
        .tree(12, 128 * C, BTreeMap::from([(0, pattern(0, C))]))
        .await;
    let old = reader.open_file(&id(12)).await.unwrap();
    let before = f.s3.events().len();
    let old_root = old.prove_root().await.unwrap();
    assert_eq!(f.s3.events().len(), before);
    let ns = Namespace::new(
        "managed/".into(),
        bytes(&f.volume),
        bytes(&id(12)),
        bytes(&id(200)),
    )
    .unwrap();
    assert_eq!(
        f.absolute(ns.key(Role::Page, old_root.reference()).unwrap().as_str()),
        old_tree.root
    );
    let second = Page::new(
        bytes(&f.volume),
        bytes(&id(12)),
        128 * C,
        0,
        vec![Entry::Hole { length: C }],
    )
    .unwrap();
    let second_ref = Reference::new(bytes(&id(200)), bytes(&id(900)), 77, second.digest()).unwrap();
    f.s3.put(
        f.absolute(ns.key(Role::Page, &second_ref).unwrap().as_str()),
        second.encode(),
        "inherited-second",
    );
    let parent = Page::new(
        bytes(&f.volume),
        bytes(&id(12)),
        0,
        1,
        vec![
            Entry::Child {
                length: 128 * C,
                object: old_root.reference().clone(),
            },
            Entry::Child {
                length: C,
                object: second_ref.clone(),
            },
        ],
    )
    .unwrap();
    let wrong_span = Page::new(
        bytes(&f.volume),
        bytes(&id(12)),
        0,
        1,
        vec![
            Entry::Child {
                length: C,
                object: old_root.reference().clone(),
            },
            Entry::Child {
                length: 128 * C,
                object: second_ref.clone(),
            },
        ],
    )
    .unwrap();
    f.publish_page(12, 200, wrong_span, None).await;
    let invalid = reader.open_file(&id(12)).await.unwrap();
    assert!(
        invalid.prove_page_containing(7, 0).await.is_err(),
        "wrong child geometry produced a witness"
    );
    f.publish_page(12, 200, parent, None).await;
    let newer = reader.open_file(&id(12)).await.unwrap();
    let before = f.s3.events().len();
    let inherited = newer.prove_page_containing(7, 0).await.unwrap();
    assert_eq!(inherited.reference(), old_root.reference());
    assert_eq!(inherited.page(), old_root.page());
    assert_ne!(inherited.base_binding(), old_root.base_binding());
    assert_eq!(
        f.s3.events().len() - before,
        2,
        "old root was not a normal page child"
    );
    assert!(
        f.s3.events()[before..]
            .iter()
            .all(|e| !e.path.contains("/data/"))
    );
    newer.verify_page_proof(&inherited.encode()).unwrap();
    let tail = newer.prove_page_containing(128 * C + 7, 0).await.unwrap();
    assert_eq!(tail.reference(), &second_ref);
    assert_eq!(tail.offset(), 128 * C);
    assert_eq!(tail.length(), C);

    failed(
        newer.verify_page_proof(&old_root.encode()),
        ReadFailure::Binding,
    );
    let mut relabeled = old_root.encode();
    relabeled[8..40].copy_from_slice(inherited.base_binding());
    assert!(
        newer.verify_page_proof(&relabeled).is_err(),
        "base relabel omitted parent inclusion proof"
    );
    old.verify_page_proof(&old_root.encode()).unwrap();
    assert_eq!(
        newer.read_range(0, 2 * C as usize).await.unwrap(),
        [pattern(0, C), vec![0; C as usize]].concat()
    );
}
