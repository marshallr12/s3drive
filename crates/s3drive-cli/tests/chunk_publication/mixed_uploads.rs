//! Real SDK and local conditional S3 fixture; no provider/root CAS/native claim.
use super::*;
use s3drive_core::manifest::{
    self, Entry,
    objects::{Namespace, Role},
};
use s3drive_managed::chunks::{MixedRecovery, MixedRecoveryFailure, Reader};
use s3drive_state::sparse_views::{
    self,
    preparation::{self as plans, Position},
};
const C: u64 = manifest::CHUNK_BYTES;
async fn verify(e: &Executor, r: &plans::TreeReceipt, root: &str) {
    let descriptor = r.root();
    let ns = Namespace::new(
        root.into(),
        *descriptor.volume(),
        *descriptor.node(),
        *descriptor.page().store(),
    )
    .unwrap();
    let key = ns.root_key(descriptor).unwrap();
    let h = e.reader().open(key.as_str()).await.unwrap();
    let bytes = h.read_range(0, h.metadata().size as usize).await.unwrap();
    let mut pages = vec![descriptor.verify_page(&bytes).unwrap()];
    let mut data = 0;
    while let Some(page) = pages.pop() {
        for (index, entry) in page.page().entries().iter().enumerate() {
            let (role, reference) = match entry {
                Entry::Data { object, .. } => (Role::Data, object),
                Entry::Child { object, .. } => (Role::Page, object),
                Entry::Hole { .. } => continue,
            };
            let key = ns.key(role, reference).unwrap();
            let h = e.reader().open(key.as_str()).await.unwrap();
            let bytes = h.read_range(0, h.metadata().size as usize).await.unwrap();
            if role == Role::Page {
                pages.push(page.verify_child(index, &bytes).unwrap());
            } else {
                page.verify_data(index, &bytes).unwrap();
                data += 1;
            }
        }
    }
    assert_eq!(
        data, 2,
        "new Unicode region and inherited partial tail must verify"
    );
}
pub async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let seed = setup(config, root, s).await;
    let p = publisher(&seed, s).await;
    let tail = b"unchanged tail!!\0";
    assert_eq!(tail.len(), 17);
    let (old, _) = plan(&seed, s, 10, 40, tail, Some(130 * C + 17), 0).await;
    p.begin(old, id(41)).await.unwrap();
    let committed = p.resume(old).await.unwrap();
    let original = committed.metadata().clone();
    let storage = storage(root, "mixed").await;
    let local = storage
        .sparse_views(sparse_views::Options::default())
        .unwrap();
    let e = executor(config, storage.clone(), false, false).await;
    let reader = Reader::open(
        s.wire.clone(),
        info(&s.direct).await.id,
        vec![(id(200), e.reader())],
    )
    .await
    .unwrap();
    let base = reader.open_file(&id(10)).await.unwrap().pinned_base();
    let view = local
        .begin([1; 32], base.sparse_binding().unwrap(), base.size())
        .await
        .unwrap();
    let edited = local
        .write(
            [2; 32],
            view.id(),
            3,
            Arc::from("café 日本語 🦀".as_bytes()),
        )
        .await
        .unwrap();
    let edit = reader
        .reopen_edited(local.clone(), edited.id())
        .await
        .unwrap();
    let preparation = edit
        .begin_preparation([3; 32], [91; 16], plans::Options::default())
        .await
        .unwrap();
    preparation.prepare().await.unwrap();
    let uploader = preparation.uploader(e.clone()).unwrap();
    assert!(
        preparation.uploader(seed.clone()).is_err(),
        "wrong SQLite owner accepted"
    );
    let data = Position { level: -1, slot: 0 };
    let leaf = Position { level: 0, slot: 0 };
    let root_position = Position { level: 1, slot: 0 };
    assert!(
        uploader.upload_object(root_position).await.is_err(),
        "root sent before new leaf receipt"
    );
    let denied = executor(config, storage.clone(), true, false).await;
    assert!(
        preparation
            .uploader(denied)
            .unwrap()
            .upload_object(data)
            .await
            .is_err()
    );
    let newer = local
        .write([4; 32], edited.id(), 5, Arc::from(&b"newer"[..]))
        .await
        .unwrap();
    s.proxy.mode.store(8, Ordering::SeqCst);
    assert!(
        uploader.upload_object(data).await.is_err(),
        "fresh upload bypassed source authorization"
    );
    s.proxy.mode.store(0, Ordering::SeqCst);
    assert!(
        local
            .preparations(plans::Options::default())
            .unwrap()
            .transfer(preparation.id(), data)
            .await
            .unwrap()
            .unwrap()
            .generation()
            .is_none(),
        "denied source created physical staging"
    );
    let plan_id = preparation.id();
    let client = local.preparations(plans::Options::default()).unwrap();
    let recovery = MixedRecovery::new(client.clone(), e.clone()).unwrap();
    assert!(MixedRecovery::new(client.clone(), seed.clone()).is_err());
    let error = recovery.recover_object(plan_id, data).await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<MixedRecoveryFailure>(),
        Some(&MixedRecoveryFailure::Unclaimed)
    );
    let error = recovery
        .recover_object(plan_id, Position { level: -1, slot: 1 })
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<MixedRecoveryFailure>(),
        Some(&MixedRecoveryFailure::Unclaimed)
    );
    let staged = client
        .materialize(plan_id, data, &storage)
        .await
        .unwrap()
        .unwrap();
    let prepared = storage
        .uploads()
        .prepare(staged.generation().unwrap())
        .await
        .unwrap();
    assert_eq!(prepared.phase, s3drive_state::remote::Phase::Prepared);
    let error = recovery.recover_object(plan_id, data).await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<MixedRecoveryFailure>(),
        Some(&MixedRecoveryFailure::Unclaimed)
    );
    checkpoint("LOSE_PUT");
    wait(root, "put-enabled").await;
    assert!(
        uploader.upload_object(data).await.is_err(),
        "hidden committed object claimed verified"
    );
    let client = local.preparations(plans::Options::default()).unwrap();
    let t = client
        .transfer(preparation.id(), data)
        .await
        .unwrap()
        .unwrap();
    let before = storage
        .uploads()
        .for_generation(t.generation().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.phase, s3drive_state::remote::Phase::Uncertain);
    checkpoint("RESTORE_PUT");
    wait(root, "put-restored").await;
    storage.close().await.unwrap();
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    let input = root.join("mixed-recovery-input.json");
    std::fs::write(
        &input,
        serde_json::to_vec(&(
            plan_id,
            data.level,
            data.slot,
            *before.operation.as_bytes(),
            before.generation.value(),
        ))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join("mixed-recovery-target.txt"),
        t.target().object_key(),
    )
    .unwrap();
    checkpoint("MIXED_RECOVERY_BEGIN");
    wait(root, "mixed-recovery-begin.ack").await;
    let output = tokio::time::timeout(
        Duration::from_secs(45),
        tokio::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "mixed_recovery_child", "--nocapture"])
            .env("S3DRIVE_MIXED_RECOVERY_INPUT", &input)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "nested recovery: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    checkpoint("MIXED_RECOVERY_END");
    wait(root, "mixed-recovery-end.ack").await;
    assert!(
        s.proxy.requests().is_empty(),
        "new recovery process reopened original metadata"
    );
    let proof: ([u8; 32], i64) =
        serde_json::from_slice(&std::fs::read(root.join("mixed-recovery-finished.json")).unwrap())
            .unwrap();
    assert_eq!(
        proof,
        (*before.operation.as_bytes(), before.generation.value())
    );
    let storage = super::storage(root, "mixed").await;
    let local = storage
        .sparse_views(sparse_views::Options::default())
        .unwrap();
    let e = executor(config, storage.clone(), false, false).await;
    let client = local.preparations(plans::Options::default()).unwrap();
    let after = storage
        .uploads()
        .for_generation(t.generation().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.operation, after.operation);
    let recovery = MixedRecovery::new(client, e.clone()).unwrap();
    recovery.recover_object(plan_id, data).await.unwrap();
    assert!(s.proxy.requests().is_empty());
    s.proxy.mode.store(0, Ordering::SeqCst);
    let reader = Reader::open(
        s.wire.clone(),
        info(&s.direct).await.id,
        vec![(id(200), e.reader())],
    )
    .await
    .unwrap();
    let preparation = reader
        .reopen_preparation(local.clone(), plan_id, plans::Options::default())
        .await
        .unwrap();
    let uploader = preparation.uploader(e.clone()).unwrap();
    uploader.upload_object(leaf).await.unwrap().unwrap();
    let receipt = uploader.upload_tree().await.unwrap();
    assert_eq!(receipt.new_objects(), 3);
    assert_eq!(receipt.inherited_objects(), 1);
    assert_eq!(receipt.hole_records(), 127);
    assert_eq!(receipt.plan().view(), &edited);
    assert_eq!(local.head(edited.binding().clone()).await.unwrap(), newer);
    assert_eq!(
        node(&s.direct, &info(&s.direct).await.id, &id(10)).await,
        original.node,
        "physical uploads changed authority root"
    );
    verify(&e, &receipt, "writable/.s3drive/").await;
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    assert_eq!(uploader.upload_tree().await.unwrap().root(), receipt.root());
    assert!(
        s.proxy.requests().is_empty(),
        "completed historical inspection reauthorized source"
    );
    s.proxy.mode.store(0, Ordering::SeqCst);
    storage.close().await.unwrap();
    seed.storage().close().await.unwrap();
}

/// Separate process entry point. Deliberately constructs no authority/Reader/
/// Preparation or publisher; only exact local metadata and provider observation.
pub async fn recovery_child() {
    let Some(input) = std::env::var_os("S3DRIVE_MIXED_RECOVERY_INPUT") else {
        return;
    };
    let (plan, level, slot, operation, generation): ([u8; 32], i8, u64, [u8; 32], i64) =
        serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let root = PathBuf::from(std::env::var("S3DRIVE_CHUNK_ROOT").unwrap());
    let config = s3drive_core::Config::parse(
        &std::fs::read_to_string(std::env::var("S3DRIVE_CHUNK_CONFIG").unwrap()).unwrap(),
    )
    .unwrap();
    let storage = storage(&root, "mixed").await;
    let client = storage
        .sparse_views(sparse_views::Options::default())
        .unwrap()
        .preparations(plans::Options::default())
        .unwrap();
    let e = executor(&config, storage.clone(), true, true).await;
    let p = Position { level, slot };
    let t = client.transfer(plan, p).await.unwrap().unwrap();
    assert_eq!(t.generation().unwrap().value(), generation);
    assert!(e.check_create(t.target().object_key(), t.size()).is_err());
    assert_eq!(
        e.check_create("writable/.s3drive/other/expired-proof", 0),
        Err(s3drive_s3::uploads::Failure::PolicyExpired)
    );
    let receipt = MixedRecovery::new(client, e)
        .unwrap()
        .recover_object(plan, p)
        .await
        .unwrap();
    assert_eq!(receipt.info().operation.as_bytes(), &operation);
    assert_eq!(receipt.info().generation.value(), generation);
    storage.close().await.unwrap();
    std::fs::write(
        root.join("mixed-recovery-finished.json"),
        serde_json::to_vec(&(
            *receipt.info().operation.as_bytes(),
            receipt.info().generation.value(),
        ))
        .unwrap(),
    )
    .unwrap();
}

async fn small_reader(e: &Executor, s: &Server) -> Reader {
    Reader::open(
        s.wire.clone(),
        info(&s.direct).await.id,
        vec![(id(200), e.reader())],
    )
    .await
    .unwrap()
}
async fn seed_small(config: &s3drive_core::Config, root: &Path, s: &Server) -> Executor {
    let seed = setup(config, root, s).await;
    let publisher = publisher(&seed, s).await;
    let (i, _) = plan(&seed, s, 10, 40, b"0123456789abcdef!", None, 0).await;
    publisher.begin(i, id(41)).await.unwrap();
    publisher.resume(i).await.unwrap();
    seed
}
async fn small_bytes(e: &Executor, r: &plans::TreeReceipt) -> Vec<u8> {
    let root = r.root();
    let ns = Namespace::new(
        "writable/.s3drive/".into(),
        *root.volume(),
        *root.node(),
        *root.page().store(),
    )
    .unwrap();
    let key = ns.root_key(root).unwrap();
    let h = e.reader().open(key.as_str()).await.unwrap();
    let page = root
        .verify_page(&h.read_range(0, h.metadata().size as usize).await.unwrap())
        .unwrap();
    let mut result = Vec::new();
    for (index, entry) in page.page().entries().iter().enumerate() {
        match entry {
            Entry::Hole { length } => result.resize(result.len() + *length as usize, 0),
            Entry::Data { object, .. } => {
                let key = ns.key(Role::Data, object).unwrap();
                let h = e.reader().open(key.as_str()).await.unwrap();
                let bytes = h.read_range(0, h.metadata().size as usize).await.unwrap();
                page.verify_data(index, &bytes).unwrap();
                result.extend_from_slice(&bytes);
            }
            Entry::Child { .. } => panic!("small fixture unexpectedly gained a subtree"),
        }
    }
    assert_eq!(result.len() as u64, root.length());
    result
}
pub async fn controls(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let seed = seed_small(config, root, s).await;
    let reader = small_reader(&seed, s).await;
    let original = node(&s.direct, &info(&s.direct).await.id, &id(10)).await;
    for case in 0..5u8 {
        let storage = storage(root, &format!("mixed-control-{case}")).await;
        let local = storage
            .sparse_views(sparse_views::Options::default())
            .unwrap();
        let e = executor(config, storage.clone(), false, false).await;
        let base = reader.open_file(&id(10)).await.unwrap().pinned_base();
        let initial = local
            .begin([1; 32], base.sparse_binding().unwrap(), 17)
            .await
            .unwrap();
        let (view, expected) = match case {
            0 => (
                local.resize([2; 32], initial.id(), 0).await.unwrap(),
                vec![],
            ),
            1 => (
                local.resize([2; 32], initial.id(), 7).await.unwrap(),
                b"0123456".to_vec(),
            ),
            2 => {
                let mut expected = b"0123456789abcdef!".to_vec();
                expected.resize(33, 0);
                (
                    local.resize([2; 32], initial.id(), 33).await.unwrap(),
                    expected,
                )
            }
            3 => (
                local
                    .write([2; 32], initial.id(), 0, Arc::from(vec![0; 17]))
                    .await
                    .unwrap(),
                vec![0; 17],
            ),
            4 => {
                let text = "café 🦀".as_bytes();
                let expected = [b"0123456789abcde".as_slice(), text].concat();
                (
                    local
                        .write([2; 32], initial.id(), 15, Arc::from(text))
                        .await
                        .unwrap(),
                    expected,
                )
            }
            _ => unreachable!(),
        };
        let edit = reader
            .reopen_edited(local.clone(), view.id())
            .await
            .unwrap();
        let preparation = edit
            .begin_preparation([3; 32], [101 + case; 16], plans::Options::default())
            .await
            .unwrap();
        preparation.prepare().await.unwrap();
        let uploader = preparation.uploader(e.clone()).unwrap();
        let receipt = uploader.upload_tree().await.unwrap();
        assert_eq!(small_bytes(&e, &receipt).await, expected);
        assert_eq!(
            receipt.new_objects(),
            if matches!(case, 0 | 3) { 1 } else { 2 }
        );
        assert_eq!(receipt.hole_records(), u32::from(case == 3));
        assert_eq!(local.head(view.binding().clone()).await.unwrap(), view);
        assert_eq!(uploader.upload_tree().await.unwrap().root(), receipt.root());
        storage.close().await.unwrap();
    }
    assert_eq!(
        node(&s.direct, &info(&s.direct).await.id, &id(10)).await,
        original
    );
    seed.storage().close().await.unwrap();
}
pub async fn faults(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let seed = seed_small(config, root, s).await;
    let reader = small_reader(&seed, s).await;
    for (case, bytes) in [vec![0; 17], vec![b'x'; 17]].into_iter().enumerate() {
        let storage = storage(root, &format!("mixed-forged-{case}")).await;
        let local = storage
            .sparse_views(sparse_views::Options::default())
            .unwrap();
        let e = executor(config, storage.clone(), false, false).await;
        let base = reader.open_file(&id(10)).await.unwrap().pinned_base();
        let view = local
            .begin([1; 32], base.sparse_binding().unwrap(), 17)
            .await
            .unwrap();
        let edit = reader
            .reopen_edited(local.clone(), view.id())
            .await
            .unwrap();
        let preparation = edit
            .begin_preparation([3; 32], [131 + case as u8; 16], plans::Options::default())
            .await
            .unwrap();
        // Correct authenticated root context, but deliberately untrusted offline
        // data/hole intent. Only the managed dependency boundary knows semantics.
        let client = local.preparations(plans::Options::default()).unwrap();
        let i = client.info(preparation.id()).await.unwrap();
        let mut i = client
            .append(preparation.id(), i.cursor(), plans::Candidate::Bytes(bytes))
            .await
            .unwrap();
        while i.root().unwrap().is_none() {
            i = client.step(preparation.id(), i.cursor()).await.unwrap();
        }
        let uploader = preparation.uploader(e).unwrap();
        assert!(
            uploader
                .upload_object(Position { level: -1, slot: 0 })
                .await
                .is_err()
        );
        assert!(
            uploader
                .upload_object(Position { level: 0, slot: 0 })
                .await
                .is_err()
        );
        assert!(
            storage
                .staging()
                .pending(None, 256)
                .await
                .unwrap()
                .is_empty(),
            "forged dependency created physical staging"
        );
        assert_eq!(
            client.info(preparation.id()).await.unwrap().root().unwrap(),
            i.root().unwrap()
        );
        assert_eq!(local.head(view.binding().clone()).await.unwrap(), view);
        storage.close().await.unwrap();
    }
    let storage = storage(root, "mixed-corrupt-tail").await;
    let local = storage
        .sparse_views(sparse_views::Options::default())
        .unwrap();
    let e = executor(config, storage.clone(), false, false).await;
    let base = reader.open_file(&id(10)).await.unwrap().pinned_base();
    let initial = local
        .begin([1; 32], base.sparse_binding().unwrap(), 17)
        .await
        .unwrap();
    let view = local.resize([2; 32], initial.id(), 7).await.unwrap();
    let edit = reader
        .reopen_edited(local.clone(), view.id())
        .await
        .unwrap();
    let preparation = edit
        .begin_preparation([3; 32], [141; 16], plans::Options::default())
        .await
        .unwrap();
    preparation.prepare().await.unwrap();
    let uploader = preparation.uploader(e).unwrap();
    checkpoint("MIXED_CORRUPT_TAIL");
    wait(root, "mixed-corrupt-tail.ack").await;
    assert!(
        uploader
            .upload_object(Position { level: -1, slot: 0 })
            .await
            .is_err(),
        "unread original tail was not verified"
    );
    assert!(
        uploader
            .upload_object(Position { level: 0, slot: 0 })
            .await
            .is_err()
    );
    assert!(
        storage
            .staging()
            .pending(None, 256)
            .await
            .unwrap()
            .is_empty()
    );
    checkpoint("MIXED_RESTORE_TAIL");
    wait(root, "mixed-restore-tail.ack").await;
    preparation.dependency(-1, 0).await.unwrap();
    assert_eq!(local.head(view.binding().clone()).await.unwrap(), view);
    storage.close().await.unwrap();
    seed.storage().close().await.unwrap();
}

pub async fn concurrency(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let seed = seed_small(config, root, s).await;
    let reader = small_reader(&seed, s).await;
    let storage = storage(root, "mixed-concurrency").await;
    let local = storage
        .sparse_views(sparse_views::Options::default())
        .unwrap();
    let e = executor(config, storage.clone(), false, false).await;
    let base = reader.open_file(&id(10)).await.unwrap().pinned_base();
    let initial = local
        .begin([1; 32], base.sparse_binding().unwrap(), 17)
        .await
        .unwrap();
    let mut view = local.resize([2; 32], initial.id(), 6 * C).await.unwrap();
    for n in 0..5u8 {
        view = local
            .write(
                [10 + n; 32],
                view.id(),
                n as u64 * C + 2,
                Arc::from(&b"patch"[..]),
            )
            .await
            .unwrap();
    }
    let edit = reader
        .reopen_edited(local.clone(), view.id())
        .await
        .unwrap();
    let preparation = edit
        .begin_preparation([3; 32], [151; 16], plans::Options::default())
        .await
        .unwrap();
    preparation.prepare().await.unwrap();
    let uploader = preparation.uploader(e.clone()).unwrap();
    let client = local.preparations(plans::Options::default()).unwrap();
    checkpoint("MIXED_GATE_ARM");
    wait(root, "mixed-gate-arm.ack").await;
    let mut tasks = Vec::new();
    for slot in 0..4 {
        let upload = uploader.clone();
        tasks.push(tokio::spawn(async move {
            upload.upload_object(Position { level: -1, slot }).await
        }));
    }
    checkpoint("MIXED_GATE_FOUR");
    wait(root, "mixed-gate-four.ack").await;
    let newer = local
        .write(
            [20; 32],
            view.id(),
            5 * C,
            Arc::from(&b"later dirty head"[..]),
        )
        .await
        .unwrap();
    assert_eq!(
        local.head(view.binding().clone()).await.unwrap(),
        newer,
        "SQL was held while PUT replies waited"
    );
    let fifth = Position { level: -1, slot: 4 };
    let error = uploader.upload_object(fifth).await.unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<s3drive_s3::uploads::Error>()
            .unwrap()
            .cause()
            .downcast_ref::<s3drive_s3::uploads::Failure>(),
        Some(&s3drive_s3::uploads::Failure::Busy)
    );
    let staged_fifth = client
        .transfer(preparation.id(), fifth)
        .await
        .unwrap()
        .unwrap()
        .generation()
        .unwrap();
    assert!(
        storage
            .uploads()
            .for_generation(staged_fifth)
            .await
            .unwrap()
            .is_none(),
        "busy executor claimed fifth attempt"
    );
    let first = Position { level: -1, slot: 0 };
    let t = client
        .transfer(preparation.id(), first)
        .await
        .unwrap()
        .unwrap();
    let original = storage
        .uploads()
        .for_generation(t.generation().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(original.phase, s3drive_state::remote::Phase::InFlight);
    let cancelled = tasks.remove(0);
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());
    checkpoint("MIXED_GATE_RELEASE");
    wait(root, "mixed-gate-release.ack").await;
    for task in tasks {
        task.await.unwrap().unwrap().unwrap();
    }
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    let receipt = MixedRecovery::new(client.clone(), e.clone())
        .unwrap()
        .recover_object(preparation.id(), first)
        .await
        .unwrap();
    assert_eq!(receipt.info().operation, original.operation);
    assert!(s.proxy.requests().is_empty());
    s.proxy.mode.store(0, Ordering::SeqCst);
    uploader.upload_object(fifth).await.unwrap().unwrap();
    assert_eq!(
        client
            .transfer(preparation.id(), fifth)
            .await
            .unwrap()
            .unwrap()
            .generation(),
        Some(staged_fifth)
    );
    let complete = uploader.upload_tree().await.unwrap();
    assert_eq!(complete.new_objects(), 6);
    assert_eq!(complete.root().length(), 6 * C);
    assert_eq!(complete.plan().view(), &view);
    assert_eq!(local.head(view.binding().clone()).await.unwrap(), newer);
    storage.close().await.unwrap();
    seed.storage().close().await.unwrap();
}

pub async fn conflict(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let seed = seed_small(config, root, s).await;
    let reader = small_reader(&seed, s).await;
    let storage = storage(root, "mixed-conflict").await;
    let local = storage
        .sparse_views(sparse_views::Options::default())
        .unwrap();
    let e = executor(config, storage.clone(), false, false).await;
    let base = reader.open_file(&id(10)).await.unwrap().pinned_base();
    let initial = local
        .begin([1; 32], base.sparse_binding().unwrap(), 17)
        .await
        .unwrap();
    let view = local
        .write([2; 32], initial.id(), 2, Arc::from(&b"X"[..]))
        .await
        .unwrap();
    let edit = reader
        .reopen_edited(local.clone(), view.id())
        .await
        .unwrap();
    let preparation = edit
        .begin_preparation([3; 32], [161; 16], plans::Options::default())
        .await
        .unwrap();
    preparation.prepare().await.unwrap();
    let plan = preparation.id();
    let uploader = preparation.uploader(e.clone()).unwrap();
    let data = Position { level: -1, slot: 0 };
    checkpoint("MIXED_RIVAL_CREATE");
    wait(root, "mixed-rival-create.ack").await;
    assert!(
        uploader.upload_object(data).await.is_err(),
        "rival immutable object was overwritten"
    );
    let client = local.preparations(plans::Options::default()).unwrap();
    let transfer = client.transfer(plan, data).await.unwrap().unwrap();
    let g = transfer.generation().unwrap();
    let rejected = storage.uploads().for_generation(g).await.unwrap().unwrap();
    assert_eq!(rejected.phase, s3drive_state::remote::Phase::Rejected);
    assert_eq!(rejected.rejection_status, Some(412));
    let expected = b"01X3456789abcdef!";
    assert_eq!(storage.staging().read(g, 0, 17).await.unwrap(), expected);
    assert_eq!(local.head(view.binding().clone()).await.unwrap(), view);
    let rival = e
        .reader()
        .open(transfer.target().object_key())
        .await
        .unwrap();
    assert_eq!(rival.read_range(0, 64).await.unwrap(), b"external creator");
    let root_position = Position { level: 0, slot: 0 };
    assert!(uploader.upload_object(root_position).await.is_err());
    assert!(
        client
            .transfer(plan, root_position)
            .await
            .unwrap()
            .unwrap()
            .generation()
            .is_none()
    );
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    assert!(uploader.upload_tree().await.is_err());
    assert!(s.proxy.requests().is_empty());
    storage.close().await.unwrap();
    let storage = super::storage(root, "mixed-conflict").await;
    let e = executor(config, storage.clone(), false, false).await;
    let local = storage
        .sparse_views(sparse_views::Options::default())
        .unwrap();
    let client = local.preparations(plans::Options::default()).unwrap();
    let error = MixedRecovery::new(client.clone(), e.clone())
        .unwrap()
        .recover_object(plan, data)
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<MixedRecoveryFailure>(),
        Some(&MixedRecoveryFailure::Unverified)
    );
    let again = storage.uploads().for_generation(g).await.unwrap().unwrap();
    assert_eq!(again.operation, rejected.operation);
    assert_eq!(again.phase, rejected.phase);
    assert_eq!(storage.staging().read(g, 0, 17).await.unwrap(), expected);
    assert_eq!(local.head(view.binding().clone()).await.unwrap(), view);
    let rival = e
        .reader()
        .open(transfer.target().object_key())
        .await
        .unwrap();
    assert_eq!(rival.read_range(0, 64).await.unwrap(), b"external creator");
    assert!(s.proxy.requests().is_empty());
    s.proxy.mode.store(0, Ordering::SeqCst);
    storage.close().await.unwrap();
    seed.storage().close().await.unwrap();
}
