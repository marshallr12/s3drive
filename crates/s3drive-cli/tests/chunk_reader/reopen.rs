//! Persisted descriptor + real authority restart + independent reader process.
use super::*;
use s3drive_managed::chunks::BaseDescriptor;

pub async fn flow(f: &mut Fixture, path: &std::path::Path) {
    let reader = f.reader().await;
    failed(
        reader.open_file(&id(10)).await.unwrap().base_descriptor(),
        ReadFailure::UnsupportedContent,
    );
    let tree = f
        .tree(
            10,
            130 * C,
            BTreeMap::from([
                (0, pattern(0, C)),
                (127, pattern(127, C)),
                (128, pattern(128, C)),
            ]),
        )
        .await;
    let file = reader.open_file(&id(10)).await.unwrap();
    let metadata = file.metadata().clone();
    let desc = file.base_descriptor().unwrap();
    assert_eq!(desc.volume(), f.volume);
    assert_eq!(desc.node(), id(10));
    assert_eq!(
        Some(desc.payload_reference().to_owned()),
        s3drive_authority::payload_reference(&metadata.payload)
    );
    assert_ne!(
        metadata
            .payload
            .as_ref()
            .unwrap()
            .reservation
            .guard
            .as_ref()
            .unwrap()
            .expected
            .as_deref(),
        Some(desc.payload_reference())
    );
    let encoded = desc.encode();
    assert_eq!(encoded.len(), 88);
    assert_eq!(&encoded[..8], b"S3DBASE1");
    assert_eq!(BaseDescriptor::decode(&encoded).unwrap(), desc);
    for length in [0, 7, 8, 23, 39, 55, 87] {
        assert!(BaseDescriptor::decode(&encoded[..length]).is_err());
    }
    let mut extra = encoded.to_vec();
    extra.push(0);
    assert!(BaseDescriptor::decode(&extra).is_err());
    let mut wrong = encoded;
    wrong[0] ^= 1;
    assert!(BaseDescriptor::decode(&wrong).is_err());
    for start in [8, 24, 40] {
        let mut wrong = encoded;
        wrong[start..start + 16].fill(0);
        failed(BaseDescriptor::decode(&wrong), ReadFailure::Binding);
    }
    let descriptor_path = path.join("base café 日本語 🦀.bin");
    std::fs::write(&descriptor_path, encoded).unwrap();
    let chunk = file.prove_chunk(128).await.unwrap();
    let page = file.prove_page_containing(128 * C, 0).await.unwrap();
    std::fs::write(path.join("chunk.proof"), chunk.encode()).unwrap();
    std::fs::write(path.join("page.proof"), page.encode()).unwrap();
    let before = f.s3.events().len();
    let base = reader.reopen_base(&desc).await.unwrap();
    assert_eq!(
        f.s3.events().len() - before,
        2,
        "reopen fetched more than root HEAD+GET"
    );
    assert_eq!(base.size(), 130 * C);
    base.verify_chunk_proof(&chunk.encode()).unwrap();
    base.verify_page_proof(&page.encode()).unwrap();
    assert_eq!(base.base_descriptor().unwrap(), desc);

    for (at, error) in [
        (8, ReadFailure::Binding),
        (
            24,
            ReadFailure::Authority(s3drive_authority::Error::NotFound),
        ),
        (
            40,
            ReadFailure::Authority(s3drive_authority::Error::NotFound),
        ),
        (
            56,
            ReadFailure::Authority(s3drive_authority::Error::Conflict),
        ),
    ] {
        let mut wrong = encoded;
        wrong[at] ^= 1;
        let wrong = BaseDescriptor::decode(&wrong).unwrap();
        let before = f.s3.events().len();
        let requests = f.requests();
        failed(reader.reopen_base(&wrong).await, error);
        assert_eq!(f.s3.events().len(), before);
        if at == 8 {
            assert_eq!(f.requests(), requests, "foreign volume requested authority");
        }
    }
    for mode in 31..=37 {
        f.mode.store(mode, Ordering::SeqCst);
        let before = f.s3.events().len();
        assert!(
            reader.reopen_base(&desc).await.is_err(),
            "history proxy mode{mode}"
        );
        assert_eq!(f.s3.events().len(), before, "unbound history reached S3");
    }
    f.mode.store(30, Ordering::SeqCst);
    let legacy_service = f.reader().await;
    let ordinary = legacy_service.open_file(&id(10)).await.unwrap();
    assert_eq!(ordinary.size(), 130 * C);
    failed(ordinary.base_descriptor(), ReadFailure::UnsupportedContent);
    let before = f.s3.events().len();
    let requests = f.requests();
    failed(
        legacy_service.reopen_base(&desc).await,
        ReadFailure::UnsupportedContent,
    );
    assert_eq!(f.s3.events().len(), before);
    assert_eq!(f.requests(), requests);
    f.mode.store(0, Ordering::SeqCst);
    drop(ordinary);
    drop(legacy_service);

    f.tree(10, 3, BTreeMap::from([(0, b"NEW".to_vec())])).await;
    let current = reader.open_file(&id(10)).await.unwrap();
    let current_desc = current.base_descriptor().unwrap();
    assert_ne!(current_desc, desc);
    assert_eq!(
        reader
            .reopen_base(&current_desc)
            .await
            .unwrap()
            .read_range(0, 4)
            .await
            .unwrap(),
        b"NEW"
    );
    failed(
        current.verify_chunk_proof(&chunk.encode()),
        ReadFailure::Binding,
    );
    f.change(
        180,
        Mutation::Move {
            node: id(10),
            parent: s3drive_authority::ROOT.into(),
            name: "renamed restart 🦀".into(),
        },
    )
    .await;
    assert_eq!(
        file.metadata(),
        &metadata,
        "File namespace snapshot changed"
    );
    f.change(181, Mutation::Remove { node: id(10) }).await;
    failed(reader.open_file(&id(10)).await, ReadFailure::NotFound);
    assert_eq!(
        reader
            .reopen_base(&desc)
            .await
            .unwrap()
            .read_range(0, 8)
            .await
            .unwrap(),
        pattern(0, 8)
    );
    f.restart_authority(path).await;
    assert_eq!(reader.reopen_base(&desc).await.unwrap().size(), 130 * C);

    // The retained old root keeps its original credentials; a new process must
    // load the rotated dummy profile and reauthorize this historical root.
    std::fs::write(
        std::env::var_os("AWS_SHARED_CREDENTIALS_FILE").unwrap(),
        "[fixture]\naws_access_key_id=TESTROTATED\naws_secret_access_key=TESTROTATEDSECRET\n",
    )
    .unwrap();
    *f.s3.expected_key.lock().unwrap() = "TESTROTATED".into();
    assert!(
        base.read_range(0, 1).await.is_err(),
        "old credential chain unexpectedly survived revocation"
    );
    let before = f.s3.events().len();
    let child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "manifest_base_reopen_worker", "--nocapture"])
        .env("S3DRIVE_BASE_WORKER_FILE", &descriptor_path)
        .env("S3DRIVE_BASE_WORKER_ENDPOINT", f.endpoint())
        .env("S3DRIVE_BASE_WORKER_VOLUME", &f.volume)
        .env(
            "S3DRIVE_BASE_WORKER_SOURCE",
            serde_json::to_string(&f.source).unwrap(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    check_child(
        child,
        "fresh historical base process",
        Duration::from_secs(45),
    )
    .await;
    assert!(
        f.s3.events()[before..]
            .iter()
            .all(|e| e.credential == "TESTROTATED")
    );
    let reader = f.reader().await;
    let base = reader.reopen_base(&desc).await.unwrap();
    for fault in [
        Fault::HeadDenied,
        Fault::Missing,
        Fault::BigHead,
        Fault::BadEtag,
        Fault::GetDenied,
        Fault::Short,
        Fault::Extra,
        Fault::BadRange,
    ] {
        f.s3.fault(Target::Exact(tree.root.clone()), fault);
        assert!(
            reader.reopen_base(&desc).await.is_err(),
            "root fault{fault:?}"
        );
    }
    f.s3.clear();
    let original =
        f.s3.objects
            .lock()
            .unwrap()
            .get(&tree.root)
            .unwrap()
            .clone();
    {
        let mut objects = f.s3.objects.lock().unwrap();
        objects.get_mut(&tree.root).unwrap().bytes[0] ^= 1;
    }
    failed(reader.reopen_base(&desc).await, ReadFailure::Integrity);
    f.s3.objects
        .lock()
        .unwrap()
        .insert(tree.root.clone(), original);
    f.s3.fault(Target::Data, Fault::Missing);
    let missing = reader.reopen_base(&desc).await.unwrap();
    assert!(missing.read_range(0, 1).await.is_err());
    missing.verify_chunk_proof(&chunk.encode()).unwrap();
    f.s3.clear();
    drop(missing);
    f.s3.fault(Target::Exact(tree.pages[1].clone()), Fault::Missing);
    let missing = reader.reopen_base(&desc).await.unwrap();
    assert!(missing.read_range(128 * C, 1).await.is_err());
    missing.verify_page_proof(&page.encode()).unwrap();
    f.s3.clear();
    drop(missing);

    // Reopened versions share admission with reads/proofs and ordinary opens.
    f.s3.fault(Target::Exact(tree.root.clone()), Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let reader = reader.clone();
        let desc = desc.clone();
        tasks.push(tokio::spawn(async move { reader.reopen_base(&desc).await }));
    }
    f.s3.wait(arrived + 4).await;
    let requests = f.requests();
    let before = f.s3.events().len();
    failed(reader.reopen_base(&desc).await, ReadFailure::Busy);
    failed(base.read_range(0, 1).await, ReadFailure::Busy);
    failed(base.prove_root().await, ReadFailure::Busy);
    failed(reader.open_file(&id(10)).await, ReadFailure::Busy);
    assert_eq!(f.requests(), requests);
    assert_eq!(f.s3.events().len(), before);
    let task = tasks.remove(0);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    f.s3.clear();
    f.s3.permits.add_permits(4);
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    reader.reopen_base(&desc).await.unwrap();
    let short = f
        .reader_options(ReadOptions::new(Duration::from_secs(3)).unwrap())
        .await;
    f.s3.fault(Target::Exact(tree.root.clone()), Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let task = {
        let reader = short.clone();
        let desc = desc.clone();
        tokio::spawn(async move { reader.reopen_base(&desc).await })
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
    short.reopen_base(&desc).await.unwrap();

    f.create(11, "retained empty permit", 200, false).await;
    let pool = f.reader().await;
    let empty = pool.open_file(&id(11)).await.unwrap();
    let retained = empty.pinned_base();
    drop(empty);
    let mut handles = Vec::new();
    for _ in 1..256 {
        handles.push(pool.open_file(&id(11)).await.unwrap());
    }
    let requests = f.requests();
    let before = f.s3.events().len();
    failed(pool.reopen_base(&desc).await, ReadFailure::Busy);
    failed(pool.open_file(&id(11)).await, ReadFailure::Busy);
    assert_eq!(f.requests(), requests);
    assert_eq!(f.s3.events().len(), before);
    handles.pop();
    let available = pool.reopen_base(&desc).await.unwrap();
    drop(available);
    drop(handles);
    drop(retained);
    pool.reopen_base(&desc).await.unwrap();
    f.create(12, "large sparse base", 200, false).await;
    f.tree(12, 65536 * C, BTreeMap::new()).await;
    let large = reader
        .open_file(&id(12))
        .await
        .unwrap()
        .base_descriptor()
        .unwrap();
    let before = f.s3.events().len();
    let large = reader.reopen_base(&large).await.unwrap();
    assert_eq!(f.s3.events().len() - before, 2);
    assert_eq!(large.size(), 65536 * C);
    assert_eq!(
        large.read_range(65536 * C - 3, 8).await.unwrap(),
        vec![0; 3]
    );
    f.create(13, "published empty base 🦀", 200, false).await;
    f.tree(13, 0, BTreeMap::new()).await;
    let empty = reader
        .open_file(&id(13))
        .await
        .unwrap()
        .base_descriptor()
        .unwrap();
    let empty = reader.reopen_base(&empty).await.unwrap();
    assert_eq!(empty.size(), 0);
    assert!(empty.read_range(0, 1).await.unwrap().is_empty());
    assert_eq!(empty.prove_root().await.unwrap().length(), 0);
    failed(empty.prove_chunk(0).await, ReadFailure::Bounds);
}

pub async fn worker() {
    let Some(path) = std::env::var_os("S3DRIVE_BASE_WORKER_FILE") else {
        return;
    };
    let path = PathBuf::from(path);
    let descriptor = BaseDescriptor::decode(&std::fs::read(&path).unwrap()).unwrap();
    let source: s3drive_core::Source =
        serde_json::from_str(&std::env::var("S3DRIVE_BASE_WORKER_SOURCE").unwrap()).unwrap();
    let wire = s3drive_authority::WireClient::loopback(
        &std::env::var("S3DRIVE_BASE_WORKER_ENDPOINT").unwrap(),
        s3drive_authority::Token::new(TOKEN.into()).unwrap(),
    )
    .unwrap();
    let reader = Reader::open(
        wire,
        std::env::var("S3DRIVE_BASE_WORKER_VOLUME").unwrap(),
        vec![(id(200), S3Reader::from_source(&source).await.unwrap())],
    )
    .await
    .unwrap();
    let base = reader.reopen_base(&descriptor).await.unwrap();
    assert_eq!(base.size(), 130 * C);
    assert_eq!(base.read_range(0, 8).await.unwrap(), pattern(0, 8));
    let expected = [pattern(127, C)[C as usize - 3..].to_vec(), pattern(128, 5)].concat();
    assert_eq!(base.read_range(128 * C - 3, 8).await.unwrap(), expected);
    base.verify_chunk_proof(&std::fs::read(path.parent().unwrap().join("chunk.proof")).unwrap())
        .unwrap();
    base.verify_page_proof(&std::fs::read(path.parent().unwrap().join("page.proof")).unwrap())
        .unwrap();
    assert_eq!(
        base.base_descriptor().unwrap().encode(),
        descriptor.encode()
    );
}
