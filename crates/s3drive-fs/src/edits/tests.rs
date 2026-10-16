use super::*;
use s3drive_state::{cache, worker::Storage};
use std::error::Error as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Barrier, Notify};

fn target(name: &str) -> Target {
    Target::new([1; 32], &"a".repeat(64), name).unwrap()
}
async fn storage(path: &std::path::Path, max_bytes: u64) -> Storage {
    Storage::open(
        path,
        cache::Options::default(),
        journal::Options {
            max_bytes,
            ..journal::Options::default()
        },
    )
    .await
    .unwrap()
}
fn bytes(data: &[u8]) -> Arc<[u8]> {
    Arc::from(data)
}
struct Memory {
    data: Vec<u8>,
    calls: AtomicUsize,
    fail_at: AtomicUsize,
    short_at: AtomicUsize,
    change_at: AtomicUsize,
    revision: AtomicUsize,
    stall: AtomicUsize,
    started: Notify,
    race: Option<Arc<Barrier>>,
}
impl Memory {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            calls: AtomicUsize::new(0),
            fail_at: AtomicUsize::new(0),
            short_at: AtomicUsize::new(0),
            change_at: AtomicUsize::new(0),
            revision: AtomicUsize::new(1),
            stall: AtomicUsize::new(0),
            started: Notify::new(),
            race: None,
        }
    }
}
impl PinnedReader for Memory {
    fn read(&self, offset: u64, length: usize) -> ReadFuture<'_> {
        Box::pin(async move {
            assert!(length <= journal::CHUNK_BYTES);
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            self.started.notify_one();
            if let Some(barrier) = &self.race
                && call <= 2
            {
                barrier.wait().await;
            }
            if self.stall.load(Ordering::SeqCst) != 0 {
                std::future::pending::<()>().await;
            }
            if call == self.change_at.load(Ordering::SeqCst) {
                self.revision.store(2, Ordering::SeqCst);
            }
            if self.revision.load(Ordering::SeqCst) != 1 {
                return Err("pinned revision changed".into());
            }
            if call == self.fail_at.load(Ordering::SeqCst) {
                return Err("injected base read failure".into());
            }
            let mut result = self.data[offset as usize..offset as usize + length].to_vec();
            if call == self.short_at.load(Ordering::SeqCst) {
                result.pop();
            }
            Ok(result)
        })
    }
}
fn remote(coordinator: &Coordinator, memory: Arc<Memory>) -> Editor {
    Editor {
        coordinator: coordinator.clone(),
        origin: Origin::Remote(Arc::new(Remote {
            target: target("日本語/é.txt"),
            base: Base::Existing {
                etag: Some("old-revision".into()),
                version: None,
            },
            size: memory.data.len() as u64,
            reader: memory,
        })),
    }
}

#[tokio::test]
async fn writes_truncation_and_sparse_growth_keep_old_snapshots() {
    let temp = tempfile::tempdir().unwrap();
    let storage = storage(&temp.path().join("state"), 1024 * 1024).await;
    let coordinator = Coordinator::new(storage.staging());
    let first = coordinator
        .new_file(target("file"))
        .write(0, bytes(b"abcdef"))
        .unwrap()
        .execute()
        .await
        .unwrap();
    let next = coordinator
        .local(first.metadata().id)
        .await
        .unwrap()
        .write(2, bytes(b"XY"))
        .unwrap()
        .execute()
        .await
        .unwrap();
    assert_eq!(next.read(0, 20).await.unwrap(), b"abXYef");
    assert_eq!(first.read(0, 20).await.unwrap(), b"abcdef");
    let clipped = coordinator
        .local(next.metadata().id)
        .await
        .unwrap()
        .truncate(3)
        .unwrap()
        .execute()
        .await
        .unwrap();
    assert_eq!(clipped.read(0, 20).await.unwrap(), b"abX");
    let hole = 3 * 1024_u64.pow(4);
    let sparse = coordinator
        .local(clipped.metadata().id)
        .await
        .unwrap()
        .write(hole, bytes(b"Z"))
        .unwrap()
        .execute()
        .await
        .unwrap();
    assert_eq!(sparse.metadata().size, hole + 1);
    assert_eq!(sparse.read(0, 6).await.unwrap(), b"abX\0\0\0");
    assert_eq!(sparse.read(hole - 2, 10).await.unwrap(), b"\0\0Z");
    let grown = coordinator
        .local(sparse.metadata().id)
        .await
        .unwrap()
        .truncate(hole + 10)
        .unwrap()
        .execute()
        .await
        .unwrap();
    assert_eq!(grown.read(hole, 20).await.unwrap(), b"Z\0\0\0\0\0\0\0\0\0");
    let empty = coordinator
        .local(grown.metadata().id)
        .await
        .unwrap()
        .truncate(0)
        .unwrap()
        .execute()
        .await
        .unwrap();
    assert!(empty.read(0, 20).await.unwrap().is_empty());
    assert_eq!(empty.metadata().base, Base::Missing);
    storage.close().await.unwrap();
}

#[tokio::test]
async fn deterministic_model_covers_overwrite_extend_shrink_and_restart() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let store = storage(&path, 1024 * 1024).await;
    let coordinator = Coordinator::new(store.staging());
    let mut editor = coordinator.new_file(target("model"));
    let mut model = Vec::<u8>::new();
    let mut rng = 0xa39b_0515_835a_09a1_u64;
    let mut random = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let mut old = Vec::new();
    let mut last = None;
    for step in 0..120 {
        let plan = if random() % 3 == 0 {
            let size = (random() % 512) as usize;
            model.resize(size, 0);
            editor.truncate(size as u64).unwrap()
        } else {
            let offset = (random() % (model.len() as u64 + 65)) as usize;
            let length = (random() % 29 + 1) as usize;
            let body = (0..length).map(|_| random() as u8).collect::<Vec<_>>();
            model.resize(model.len().max(offset + length), 0);
            model[offset..offset + length].copy_from_slice(&body);
            editor.write(offset as u64, Arc::from(body)).unwrap()
        };
        let result = plan.execute().await.unwrap();
        assert_eq!(result.read(0, 1024).await.unwrap(), model, "step {step}");
        if step % 29 == 0 {
            old.push((result.clone(), model.clone()));
        }
        editor = coordinator.local(result.metadata().id).await.unwrap();
        last = Some(result.metadata().id);
    }
    for (snapshot, expected) in old {
        assert_eq!(snapshot.read(0, 1024).await.unwrap(), expected);
    }
    store.close().await.unwrap();
    let reopened = storage(&path, 1024).await;
    assert_eq!(
        reopened
            .staging()
            .read(last.unwrap(), 0, 1024)
            .await
            .unwrap(),
        model
    );
    reopened.close().await.unwrap();
}

#[tokio::test]
async fn remote_hydration_is_bounded_and_retry_recovers_the_same_builder_after_restart() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let store = storage(&path, 16 * 1024 * 1024).await;
    let coordinator = Coordinator::new(store.staging());
    let memory = Arc::new(Memory::new(
        (0..journal::CHUNK_BYTES * 2 + 137)
            .map(|i| (i % 251) as u8)
            .collect(),
    ));
    memory.fail_at.store(2, Ordering::SeqCst);
    let plan = remote(&coordinator, memory.clone())
        .write(memory.data.len() as u64, bytes(b"tail"))
        .unwrap();
    let key = plan.operation_key();
    let failure = plan.execute().await.err().unwrap();
    let id = failure.generation.unwrap();
    assert_eq!(
        store.staging().info(id).await.unwrap().filled,
        journal::CHUNK_BYTES as u64
    );
    assert!(
        store
            .staging()
            .head(target("日本語/é.txt"))
            .await
            .unwrap()
            .is_none()
    );
    store.close().await.unwrap();
    let reopened = storage(&path, 16 * 1024 * 1024).await;
    let coordinator = Coordinator::new(reopened.staging());
    let retry = remote(&coordinator, memory.clone())
        .write(memory.data.len() as u64, bytes(b"tail"))
        .unwrap();
    assert_eq!(retry.operation_key(), key);
    let result = retry.execute().await.unwrap();
    assert_eq!(result.metadata().id, id);
    let mut expected = memory.data.clone();
    expected.extend_from_slice(b"tail");
    assert_eq!(result.read(0, expected.len()).await.unwrap(), expected);
    assert_eq!(
        result.metadata().base,
        Base::Existing {
            etag: Some("old-revision".into()),
            version: None
        }
    );
    assert_eq!(
        reopened.staging().pending(None, 100).await.unwrap().len(),
        1
    );
    // A completed plan replay uses the retained manifest, even after the base
    // is unavailable. A newer local head is never rewound by that replay.
    let newer = coordinator
        .local(id)
        .await
        .unwrap()
        .truncate(1)
        .unwrap()
        .execute()
        .await
        .unwrap();
    let calls = memory.calls.load(Ordering::SeqCst);
    memory.revision.store(2, Ordering::SeqCst);
    assert_eq!(retry.execute().await.unwrap().metadata().id, id);
    assert_eq!(memory.calls.load(Ordering::SeqCst), calls);
    assert_eq!(
        reopened
            .staging()
            .head(target("日本語/é.txt"))
            .await
            .unwrap()
            .unwrap()
            .id,
        newer.metadata().id
    );
    reopened.close().await.unwrap();
}

#[tokio::test]
async fn changed_revision_and_short_ranges_never_publish_a_mixed_file() {
    for short in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let store = storage(&temp.path().join("state"), 8 * 1024 * 1024).await;
        let coordinator = Coordinator::new(store.staging());
        let memory = Arc::new(Memory::new(vec![5; journal::CHUNK_BYTES * 2]));
        if short {
            memory.short_at.store(2, Ordering::SeqCst);
        } else {
            memory.change_at.store(2, Ordering::SeqCst);
        }
        let failure = remote(&coordinator, memory.clone())
            .write(memory.data.len() as u64, bytes(b"x"))
            .unwrap()
            .execute()
            .await
            .err()
            .unwrap();
        let draft = failure.generation.unwrap();
        assert_eq!(
            store.staging().info(draft).await.unwrap().filled,
            journal::CHUNK_BYTES as u64
        );
        assert_eq!(
            store.staging().read_draft(draft, 0, 8).await.unwrap(),
            [5; 8]
        );
        assert!(store.staging().pending(None, 100).await.unwrap().is_empty());
        assert!(
            store
                .staging()
                .head(target("日本語/é.txt"))
                .await
                .unwrap()
                .is_none()
        );
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn different_edits_conflict_and_identical_concurrent_edits_share_one_receipt() {
    for identical in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let store = storage(&temp.path().join("state"), 1024).await;
        let coordinator = Coordinator::new(store.staging());
        let mut memory = Memory::new(b"abcdef".to_vec());
        memory.race = Some(Arc::new(Barrier::new(2)));
        let input = remote(&coordinator, Arc::new(memory));
        let a = input.write(2, bytes(b"A")).unwrap();
        let b = input
            .write(2, bytes(if identical { b"A" } else { b"B" }))
            .unwrap();
        let (a, b) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(a.execute(), b.execute())
        })
        .await
        .unwrap();
        if identical {
            assert_eq!(a.unwrap().metadata().id, b.unwrap().metadata().id);
            assert!(store.staging().drafts(None, 100).await.unwrap().is_empty());
        } else {
            let failure = match (a, b) {
                (Ok(_), Err(error)) | (Err(error), Ok(_)) => error,
                _ => panic!("one edit must conflict"),
            };
            let id = failure.generation.unwrap();
            assert_eq!(
                store.staging().read_draft(id, 0, 100).await.unwrap().len(),
                6
            );
            assert!(
                failure
                    .source()
                    .unwrap()
                    .downcast_ref::<journal::Failure>()
                    .is_some()
            );
        }
        assert_eq!(store.staging().pending(None, 100).await.unwrap().len(), 1);
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn network_stall_does_not_hold_sqlite_and_cancellation_preserves_retriable_draft() {
    let temp = tempfile::tempdir().unwrap();
    let store = storage(&temp.path().join("state"), 1024).await;
    let coordinator = Coordinator::new(store.staging());
    let memory = Arc::new(Memory::new(b"abcdef".to_vec()));
    memory.stall.store(1, Ordering::SeqCst);
    let plan = remote(&coordinator, memory.clone())
        .write(2, bytes(b"X"))
        .unwrap();
    let copy = plan.clone();
    let running = tokio::spawn(async move { copy.execute().await });
    tokio::time::timeout(Duration::from_secs(5), memory.started.notified())
        .await
        .unwrap();
    let drafts = tokio::time::timeout(Duration::from_secs(1), store.staging().drafts(None, 100))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(drafts.len(), 1);
    running.abort();
    assert!(running.await.is_err_and(|error| error.is_cancelled()));
    assert_eq!(coordinator.0.slots.available_permits(), 4);
    memory.stall.store(0, Ordering::SeqCst);
    let result = plan.execute().await.unwrap();
    assert_eq!(result.metadata().id, drafts[0].id);
    assert_eq!(result.read(0, 100).await.unwrap(), b"abXdef");
    store.close().await.unwrap();
}

#[tokio::test]
async fn hydration_deadline_is_explicit_and_does_not_publish() {
    let temp = tempfile::tempdir().unwrap();
    let store = storage(&temp.path().join("state"), 1024).await;
    let coordinator = Coordinator(Arc::new(Shared {
        staging: store.staging(),
        slots: Semaphore::new(4),
        read_deadline: Duration::from_millis(20),
    }));
    let memory = Arc::new(Memory::new(b"abcdef".to_vec()));
    memory.stall.store(1, Ordering::SeqCst);
    let failure = remote(&coordinator, memory)
        .truncate(3)
        .unwrap()
        .execute()
        .await
        .err()
        .unwrap();
    assert!(failure.to_string().contains("timed out"));
    assert_eq!(
        store
            .staging()
            .info(failure.generation.unwrap())
            .await
            .unwrap()
            .filled,
        0
    );
    assert!(store.staging().pending(None, 100).await.unwrap().is_empty());
    store.close().await.unwrap();
}

#[tokio::test]
async fn quota_failure_preserves_old_snapshot_and_operation_intent() {
    let temp = tempfile::tempdir().unwrap();
    let store = storage(&temp.path().join("state"), 8).await;
    let coordinator = Coordinator::new(store.staging());
    let original = coordinator
        .new_file(target("quota"))
        .write(0, bytes(b"12345678"))
        .unwrap()
        .execute()
        .await
        .unwrap();
    let plan = coordinator
        .local(original.metadata().id)
        .await
        .unwrap()
        .write(3, bytes(b"X"))
        .unwrap();
    let failure = plan.execute().await.err().unwrap();
    assert_eq!(
        failure.source().unwrap().downcast_ref::<journal::Failure>(),
        Some(&journal::Failure::QuotaExceeded)
    );
    assert_eq!(original.read(0, 100).await.unwrap(), b"12345678");
    assert_eq!(
        store
            .staging()
            .head(target("quota"))
            .await
            .unwrap()
            .unwrap()
            .id,
        original.metadata().id
    );
    assert_eq!(
        plan.execute().await.err().unwrap().generation,
        failure.generation
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn admission_and_operation_identity_are_bounded_and_distinct() {
    let temp = tempfile::tempdir().unwrap();
    let store = storage(&temp.path().join("state"), 1024).await;
    let coordinator = Coordinator::new(store.staging());
    let editor = coordinator.new_file(target("é"));
    assert!(editor.write(0, bytes(b"")).is_err());
    assert!(editor.write(u64::MAX, bytes(b"a")).is_err());
    assert!(
        editor
            .write(0, Arc::from(vec![0; journal::MAX_READ_BYTES + 1]))
            .is_err()
    );
    assert!(editor.truncate(u64::MAX).is_err());
    let a = editor.write(0, bytes(b"abc")).unwrap().operation_key();
    for other in [
        editor.write(1, bytes(b"abc")).unwrap(),
        editor.write(0, bytes(b"abd")).unwrap(),
        editor.truncate(3).unwrap(),
        coordinator
            .new_file(target("e\u{301}"))
            .write(0, bytes(b"abc"))
            .unwrap(),
    ] {
        assert_ne!(a, other.operation_key());
    }
    assert!(store.staging().drafts(None, 100).await.unwrap().is_empty());
    let _all = coordinator.0.slots.acquire_many(4).await.unwrap();
    let failure = editor.truncate(0).unwrap().execute().await.err().unwrap();
    assert!(failure.generation.is_none());
    assert!(failure.to_string().contains("concurrency limit"));
    assert!(store.staging().drafts(None, 100).await.unwrap().is_empty());
    store.close().await.unwrap();
}
