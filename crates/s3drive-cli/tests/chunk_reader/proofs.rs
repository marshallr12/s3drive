use super::*;
use s3drive_managed::chunks::MAX_CHUNK_PROOF_BYTES;

pub async fn flow(f: &Fixture) {
    let tree = f
        .tree(
            10,
            129 * C + 17,
            BTreeMap::from([
                (127, pattern(127, C)),
                (128, pattern(128, C)),
                (129, pattern(129, 17)),
            ]),
        )
        .await;
    let reader = f.reader().await;
    let old = reader.open_file(&id(10)).await.unwrap();
    let start = f.s3.events().len();
    let requests = f.requests();
    let mut saved = Vec::new();
    for index in [0, 64, 127, 128, 129] {
        let proof = old.prove_chunk(index).await.unwrap();
        assert_eq!(proof.offset(), index * C);
        assert_eq!(proof.length(), if index == 129 { 17 } else { C });
        assert_eq!(proof.reference().is_none(), index < 127);
        let bytes = proof.encode();
        assert!(bytes.len() <= MAX_CHUNK_PROOF_BYTES);
        let before = f.s3.events().len();
        let recovered = old.verify_chunk_proof(&bytes).unwrap();
        assert_eq!(recovered.encode(), bytes);
        assert_eq!(recovered.reference(), proof.reference());
        assert_eq!(recovered.base_binding(), proof.base_binding());
        assert_eq!(before, f.s3.events().len(), "pure validation accessed S3");
        saved.push(bytes);
    }
    assert_eq!(
        requests,
        f.requests(),
        "proof discovery queried current namespace"
    );
    assert!(
        f.s3.events()[start..]
            .iter()
            .all(|e| !e.path.contains("/data/")),
        "proof fetched data"
    );
    let witness = &saved[3];
    for length in [
        0,
        7,
        8,
        39,
        40,
        47,
        48,
        49,
        witness.len() / 2,
        witness.len() - 1,
    ] {
        assert!(
            old.verify_chunk_proof(&witness[..length]).is_err(),
            "truncated witness accepted: {length}"
        );
    }
    let mut bad = witness.clone();
    bad.push(0);
    assert!(old.verify_chunk_proof(&bad).is_err());
    let mut bad = witness.clone();
    bad[0] ^= 1;
    failed(old.verify_chunk_proof(&bad), ReadFailure::Integrity);
    let mut bad = witness.clone();
    bad[8] ^= 1;
    failed(old.verify_chunk_proof(&bad), ReadFailure::Binding);
    let mut bad = witness.clone();
    bad[48] = 3;
    failed(old.verify_chunk_proof(&bad), ReadFailure::Bounds);
    let mut bad = witness.clone();
    bad[40..48].copy_from_slice(&u64::MAX.to_le_bytes());
    failed(old.verify_chunk_proof(&bad), ReadFailure::Bounds);
    let mut bad = witness.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert!(
        old.verify_chunk_proof(&bad).is_err(),
        "corrupt authenticated child accepted"
    );
    assert!(
        old.verify_chunk_proof(&vec![0; MAX_CHUNK_PROOF_BYTES + 1])
            .is_err()
    );
    let before = f.s3.events().len();
    failed(old.prove_chunk(130).await, ReadFailure::Bounds);
    failed(old.prove_chunk(u64::MAX).await, ReadFailure::Bounds);
    assert_eq!(before, f.s3.events().len());
    f.s3.fault(Target::Pages, Fault::HeadDenied);
    assert!(
        old.prove_chunk(128).await.is_err(),
        "denied path yielded a proof"
    );
    f.s3.clear();
    assert_eq!(old.prove_chunk(128).await.unwrap().encode(), *witness);

    // A canceled/stalled path releases shared capacity; overall deadline applies.
    let short = f
        .reader_options(ReadOptions::new(Duration::from_secs(1)).unwrap())
        .await;
    let short = short.open_file(&id(10)).await.unwrap();
    f.s3.fault(Target::Pages, Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let task = {
        let file = short.clone();
        tokio::spawn(async move { file.prove_chunk(128).await })
    };
    f.s3.wait(arrived + 1).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    f.s3.clear();
    f.s3.permits.add_permits(1);
    short.prove_chunk(128).await.unwrap();
    f.s3.fault(Target::Pages, Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let task = {
        let file = short.clone();
        tokio::spawn(async move { file.prove_chunk(128).await })
    };
    f.s3.wait(arrived + 1).await;
    let result = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
    failed(result, ReadFailure::Timeout);
    f.s3.clear();
    f.s3.permits.add_permits(1);
    short.prove_chunk(128).await.unwrap();

    f.change(
        180,
        Mutation::Move {
            node: id(10),
            parent: s3drive_authority::ROOT.into(),
            name: "renamed 日本語.bin".into(),
        },
    )
    .await;
    let renamed = reader.open_file(&id(10)).await.unwrap();
    assert_eq!(
        renamed.verify_chunk_proof(witness).unwrap().encode(),
        *witness
    );
    f.create(11, "other 日本語.bin", 200, false).await;
    f.tree(11, 129 * C + 17, BTreeMap::new()).await;
    let other = reader.open_file(&id(11)).await.unwrap();
    failed(other.verify_chunk_proof(witness), ReadFailure::Binding);
    f.tree(10, 4, BTreeMap::from([(0, b"new!".to_vec())])).await;
    let fresh = reader.open_file(&id(10)).await.unwrap();
    failed(fresh.verify_chunk_proof(witness), ReadFailure::Binding);
    assert_eq!(old.verify_chunk_proof(witness).unwrap().encode(), *witness);
    assert_eq!(fresh.prove_chunk(0).await.unwrap().encode().len(), 49);

    // A compressed hole can contain many logical chunks, including a short EOF.
    f.publish_page(
        10,
        200,
        Page::new(
            bytes(&f.volume),
            bytes(&id(10)),
            0,
            0,
            vec![Entry::Hole { length: 3 * C + 7 }],
        )
        .unwrap(),
        None,
    )
    .await;
    let hole = reader.open_file(&id(10)).await.unwrap();
    for (index, length) in [(0, C), (2, C), (3, 7)] {
        let proof = hole.prove_chunk(index).await.unwrap();
        assert!(proof.reference().is_none());
        assert_eq!(proof.length(), length);
        assert_eq!(proof.offset(), index * C);
        hole.verify_chunk_proof(&proof.encode()).unwrap();
    }
    // Reachability is not a byte-health receipt: no such data object exists.
    let reference = Reference::new(
        bytes(&id(200)),
        [3; 16],
        999,
        Sha256::digest(b"missing").into(),
    )
    .unwrap();
    f.publish_page(
        10,
        200,
        Page::new(
            bytes(&f.volume),
            bytes(&id(10)),
            0,
            0,
            vec![Entry::Data {
                length: C,
                object: reference.clone(),
            }],
        )
        .unwrap(),
        None,
    )
    .await;
    let missing = reader.open_file(&id(10)).await.unwrap();
    let before = f.s3.events().len();
    assert_eq!(
        missing.prove_chunk(0).await.unwrap().reference(),
        Some(&reference)
    );
    assert_eq!(before, f.s3.events().len());
    assert!(missing.read_range(0, 1).await.is_err());

    f.tree(10, 65536 * C, BTreeMap::new()).await;
    let large = reader.open_file(&id(10)).await.unwrap();
    let start = f.s3.events().len();
    let proof = large.prove_chunk(65535).await.unwrap();
    assert!(proof.reference().is_none());
    assert_eq!(proof.length(), C);
    assert_eq!(
        f.s3.events().len() - start,
        4,
        "max tree exceeded two metadata pages"
    );
    assert!(
        f.s3.events()[start..]
            .iter()
            .all(|e| !e.path.contains("/data/"))
    );
    large.verify_chunk_proof(&proof.encode()).unwrap();
    f.create(12, "empty 日本語", 200, false).await;
    let empty = reader.open_file(&id(12)).await.unwrap();
    failed(empty.prove_chunk(0).await, ReadFailure::Bounds);
    // Keep the seeded original data alive for the old immutable snapshot.
    assert!(!tree.data.is_empty());
}
