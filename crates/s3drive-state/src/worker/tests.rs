use super::*;
use crate::State;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc as blocking,
};

fn target(key: &str) -> Target {
    Target::new([1; 32], &"a".repeat(64), key).unwrap()
}
fn key(name: &str, size: u64) -> cache::Key {
    cache::Key::new([1; 32], &"a".repeat(64), name, Some("v1"), None, size).unwrap()
}
async fn open(path: &Path) -> Storage {
    Storage::open(path, cache::Options::default(), journal::Options::default())
        .await
        .unwrap()
}
async fn generation(staging: &Staging, parent: Option<GenerationId>, data: &[u8]) -> Receipt {
    let id = staging
        .begin(
            target("日本語/é.txt"),
            Base::Missing,
            parent,
            data.len() as u64,
        )
        .await
        .unwrap();
    if !data.is_empty() {
        staging.append_at(id, 0, Arc::from(data)).await.unwrap();
    }
    staging.seal(id).await.unwrap()
}
fn assert_worker_failure<T>(result: Result<T>, expected: Failure) {
    let error = result.err().expect("expected worker error");
    assert_eq!(error.downcast_ref::<Failure>(), Some(&expected));
}

// A deterministic owner-thread pause. It never blocks the async runtime and
// has its own fatal deadline so a broken test cannot strand a worker forever.
async fn pause(owner: &Owner) -> blocking::Sender<()> {
    let (release, wait) = blocking::channel();
    let (ready, started) = oneshot::channel();
    owner
        .sender
        .try_send(Command::Work(Box::new(move |_| {
            ready.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(10)).unwrap();
        })))
        .unwrap_or_else(|_| panic!("cannot pause owner"));
    tokio::time::timeout(Duration::from_secs(5), started)
        .await
        .unwrap()
        .unwrap();
    release
}

#[tokio::test]
async fn shared_owner_keeps_dirty_data_across_cache_pressure_restart_and_close() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let options = cache::Options {
        max_bytes: 3,
        max_file_bytes: 3,
        max_entries: 1,
    };
    let storage = Storage::open(&path, options, journal::Options::default())
        .await
        .unwrap();
    let cache = storage.cache();
    let staging = storage.staging();
    assert!(State::open_managed(&path).is_err());
    let runtime_thread = std::thread::current().id();
    let owner_thread = storage
        .owner
        .call(|_| Ok(std::thread::current().id()))
        .await
        .unwrap();
    assert_ne!(runtime_thread, owner_thread);
    let first = generation(&staging, None, b"dirty data").await;
    let snapshot = staging.snapshot(first.generation).await.unwrap();
    for name in ["a", "b", "c"] {
        drop(
            cache
                .put(key(name, 3), Arc::from(b"abc".as_slice()))
                .await
                .unwrap(),
        );
    }
    assert!(cache.get(key("a", 3)).await.unwrap().is_none());
    assert_eq!(snapshot.read(0, 100).await.unwrap(), b"dirty data");
    assert_eq!(staging.pending(None, 100).await.unwrap().len(), 1);
    cache.close().await.unwrap(); // closes staging and all storage clones too
    assert_worker_failure(staging.info(first.generation).await, Failure::Closed);
    assert_worker_failure(snapshot.read(0, 3).await, Failure::Closed);
    let reopened = open(&path).await;
    assert_eq!(
        reopened
            .staging()
            .read(first.generation, 0, 100)
            .await
            .unwrap(),
        b"dirty data"
    );
    reopened.close().await.unwrap();
    // Even stale clones do not retain the kernel directory lock after close.
    drop(State::open_managed(&path).unwrap());
}

#[tokio::test]
async fn shared_queue_saturates_and_rejects_invalid_inputs_before_admission() {
    let temp = tempfile::tempdir().unwrap();
    let storage = open(&temp.path().join("state")).await;
    let staging = storage.staging();
    let id = staging
        .begin(target("bounds"), Base::Missing, None, 0)
        .await
        .unwrap();
    let release = pause(&storage.owner).await;
    let calls = Arc::new(AtomicUsize::new(0));
    for _ in 0..QUEUE_SIZE {
        let calls = calls.clone();
        storage
            .owner
            .sender
            .try_send(Command::Work(Box::new(move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
            })))
            .unwrap_or_else(|_| panic!("queue filled early"));
    }
    assert_worker_failure(staging.drafts(None, 1).await, Failure::QueueFull);
    assert_worker_failure(storage.cache().get(key("a", 3)).await, Failure::QueueFull);
    // Validation must not consume a slot or wait for the paused thread.
    for error in [
        staging.drafts(None, 0).await.err().unwrap(),
        staging
            .begin(target("a"), Base::Missing, None, u64::MAX)
            .await
            .err()
            .unwrap(),
        staging
            .begin(
                target("a"),
                Base::Existing {
                    etag: None,
                    version: None,
                },
                None,
                1,
            )
            .await
            .err()
            .unwrap(),
        staging
            .read_range(id, 0, journal::MAX_READ_BYTES + 1, false)
            .await
            .err()
            .unwrap(),
        staging
            .append_at(id, 0, Arc::from(vec![0; journal::CHUNK_BYTES + 1]))
            .await
            .err()
            .unwrap(),
    ] {
        assert!(error.downcast_ref::<Failure>().is_none());
    }
    release.send(()).unwrap();
    storage.close().await.unwrap(); // drains all eight accepted jobs
    assert_eq!(calls.load(Ordering::SeqCst), QUEUE_SIZE);
}

#[tokio::test]
async fn timed_out_queued_mutation_is_skipped_and_begin_is_not_falsely_acknowledged() {
    let temp = tempfile::tempdir().unwrap();
    let storage = open(&temp.path().join("state")).await;
    let release = pause(&storage.owner).await;
    let result = storage
        .owner
        .call_with_deadline(
            |store| {
                Journal::new(&mut store.state, journal::Options::default())?.begin(
                    &target("canceled"),
                    Base::Missing,
                    None,
                    0,
                )
            },
            Duration::from_millis(20),
        )
        .await;
    assert_worker_failure(result, Failure::OutcomeUnknown);
    release.send(()).unwrap();
    assert!(
        storage
            .staging()
            .drafts(None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        storage
            .staging()
            .pending(None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    storage.close().await.unwrap();
}

#[tokio::test]
async fn lost_running_append_and_seal_replies_reconcile_without_duplicate_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let storage = open(&temp.path().join("state")).await;
    let staging = storage.staging();
    let id = staging
        .begin(target("lost"), Base::Missing, None, 3)
        .await
        .unwrap();
    for seal in [false, true] {
        let (release, wait) = blocking::channel();
        let (ready, committed) = oneshot::channel();
        let owner = storage.owner.clone();
        let running = tokio::spawn(async move {
            owner
                .call_with_deadline(
                    move |store| {
                        let mut journal =
                            Journal::new(&mut store.state, journal::Options::default())?;
                        if seal {
                            journal.seal(id)?;
                        } else {
                            journal.append_at(id, 0, b"abc")?;
                        }
                        ready.send(()).unwrap();
                        wait.recv_timeout(Duration::from_secs(10)).unwrap();
                        Ok(())
                    },
                    Duration::from_secs(2),
                )
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), committed)
            .await
            .unwrap()
            .unwrap();
        assert_worker_failure(running.await.unwrap(), Failure::OutcomeUnknown);
        release.send(()).unwrap();
        if seal {
            let receipt = staging.seal(id).await.unwrap();
            assert_eq!(receipt.generation, id);
            assert_eq!(staging.head(target("lost")).await.unwrap().unwrap().id, id);
            assert_eq!(staging.pending(None, 100).await.unwrap().len(), 1);
            assert_eq!(staging.read(id, 0, 10).await.unwrap(), b"abc");
        } else {
            staging
                .append_at(id, 0, Arc::from(b"abc".as_slice()))
                .await
                .unwrap();
            assert_eq!(staging.info(id).await.unwrap().filled, 3);
            assert!(
                staging
                    .append_at(id, 0, Arc::from(b"def".as_slice()))
                    .await
                    .is_err()
            );
            assert_eq!(staging.read_draft(id, 0, 10).await.unwrap(), b"abc");
        }
    }
    storage.close().await.unwrap();
}

#[tokio::test]
async fn lost_remote_claim_reply_never_reissues_permission_across_restart() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let storage = open(&path).await;
    let receipt = generation(&storage.staging(), None, b"keep dirty").await;
    let uploads = storage.uploads();
    let intent = uploads.prepare(receipt.generation).await.unwrap();
    let operation = intent.operation;
    let (release, wait) = blocking::channel();
    let (ready, committed) = oneshot::channel();
    let owner = storage.owner.clone();
    let running = tokio::spawn(async move {
        owner
            .call_with_deadline(
                move |store| {
                    let permit =
                        remote::Journal::new(&mut store.state, remote::Options::default())?
                            .claim(operation, &target("日本語/é.txt"))?;
                    ready.send(()).unwrap();
                    wait.recv_timeout(Duration::from_secs(10)).unwrap();
                    Ok(permit)
                },
                Duration::from_secs(2),
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), committed)
        .await
        .unwrap()
        .unwrap();
    assert_worker_failure(running.await.unwrap(), Failure::OutcomeUnknown);
    release.send(()).unwrap();
    assert_eq!(
        uploads.info(operation).await.unwrap().phase,
        remote::Phase::InFlight
    );
    let error = uploads
        .claim(operation, target("日本語/é.txt"))
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<remote::Failure>(),
        Some(&remote::Failure::AlreadyDispatched)
    );
    storage.close().await.unwrap();
    assert_worker_failure(uploads.info(operation).await, Failure::Closed);
    let reopened = open(&path).await;
    let recovered = reopened
        .uploads()
        .for_generation(receipt.generation)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.operation, operation);
    assert_eq!(recovered.phase, remote::Phase::Uncertain);
    assert_eq!(recovered.reason, remote::Reason::Interrupted);
    let error = reopened
        .uploads()
        .claim(operation, target("日本語/é.txt"))
        .await
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<remote::Failure>(),
        Some(&remote::Failure::AlreadyDispatched)
    );
    assert_eq!(
        reopened
            .staging()
            .read(receipt.generation, 0, 100)
            .await
            .unwrap(),
        b"keep dirty"
    );
    assert_eq!(
        reopened.staging().pending(None, 100).await.unwrap().len(),
        1
    );
    reopened.close().await.unwrap();
}

#[tokio::test]
async fn snapshots_keep_their_generation_while_racing_builders_report_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let storage = open(&temp.path().join("state")).await;
    let staging = storage.staging();
    let first = generation(&staging, None, b"old").await;
    let snapshot = staging.snapshot(first.generation).await.unwrap();
    let a = staging
        .begin(
            target("日本語/é.txt"),
            Base::Missing,
            Some(first.generation),
            3,
        )
        .await
        .unwrap();
    let b = staging
        .begin(
            target("日本語/é.txt"),
            Base::Missing,
            Some(first.generation),
            3,
        )
        .await
        .unwrap();
    staging
        .append_at(a, 0, Arc::from(b"aaa".as_slice()))
        .await
        .unwrap();
    staging
        .append_at(b, 0, Arc::from(b"bbb".as_slice()))
        .await
        .unwrap();
    let (a_result, b_result) = tokio::join!(staging.seal(a), staging.seal(b));
    let (winner, loser, error) = match (a_result, b_result) {
        (Ok(receipt), Err(error)) => (receipt, b, error),
        (Err(error), Ok(receipt)) => (receipt, a, error),
        _ => panic!("exactly one builder must publish"),
    };
    assert_eq!(
        error.downcast_ref::<journal::Failure>(),
        Some(&journal::Failure::LocalConflict)
    );
    assert_eq!(
        staging.info(loser).await.unwrap().phase,
        journal::Phase::Building
    );
    assert_eq!(staging.read_draft(loser, 0, 100).await.unwrap().len(), 3);
    assert_eq!(snapshot.read(0, 100).await.unwrap(), b"old");
    assert_eq!(snapshot.metadata().id, first.generation);
    assert_eq!(staging.seal(first.generation).await.unwrap(), first);
    assert_eq!(
        staging
            .head(target("日本語/é.txt"))
            .await
            .unwrap()
            .unwrap()
            .id,
        winner.generation
    );
    staging.abort(loser).await.unwrap();
    assert!(staging.abort(winner.generation).await.is_err());
    storage.close().await.unwrap();
}

#[tokio::test]
async fn sqlite_disk_full_crosses_worker_and_preserves_published_and_builder_state() {
    let temp = tempfile::tempdir().unwrap();
    let storage = open(&temp.path().join("state")).await;
    let staging = storage.staging();
    let keep = generation(&staging, None, b"keep").await;
    let draft = staging
        .begin(
            target("large"),
            Base::Missing,
            None,
            journal::CHUNK_BYTES as u64,
        )
        .await
        .unwrap();
    storage
        .owner
        .call(|store| {
            let pages: i64 =
                store
                    .state
                    .connection
                    .pragma_query_value(None, "page_count", |r| r.get(0))?;
            store
                .state
                .connection
                .pragma_update(None, "max_page_count", pages)?;
            Ok(())
        })
        .await
        .unwrap();
    let error = staging
        .append_at(draft, 0, Arc::from(vec![1; journal::CHUNK_BYTES]))
        .await
        .unwrap_err();
    assert!(cache::is_disk_full(error.as_ref()));
    assert_eq!(staging.info(draft).await.unwrap().filled, 0);
    assert_eq!(
        staging.read(keep.generation, 0, 100).await.unwrap(),
        b"keep"
    );
    assert_eq!(
        staging
            .head(target("日本語/é.txt"))
            .await
            .unwrap()
            .unwrap()
            .id,
        keep.generation
    );
    storage.close().await.unwrap();
}

#[tokio::test]
async fn journal_waits_for_sqlite_without_blocking_the_runtime() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let storage = open(&path).await;
    let staging = storage.staging();
    let other = rusqlite::Connection::open(path.join("metadata.sqlite3")).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut pending = tokio::spawn(async move {
        staging
            .begin(target("locked"), Base::Missing, None, 0)
            .await
    });
    tokio::select! {
        result = &mut pending => panic!("mutation completed with writer lock held: {}", result.is_ok()),
        _ = tokio::time::sleep(Duration::from_millis(100)) => {},
    }
    other.execute_batch("ROLLBACK").unwrap();
    let id = pending.await.unwrap().unwrap();
    assert_eq!(storage.staging().drafts(None, 1).await.unwrap()[0].id, id);
    storage.close().await.unwrap();
}

#[tokio::test]
async fn invalid_limits_are_checked_before_creating_a_directory() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    assert!(
        Storage::open(
            &path,
            cache::Options::default(),
            journal::Options {
                max_bytes: 0,
                ..journal::Options::default()
            }
        )
        .await
        .is_err()
    );
    assert!(!path.exists());
}

#[tokio::test]
async fn worker_failure_after_commit_reports_unknown_and_releases_ownership() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let storage = open(&path).await;
    let id = storage
        .staging()
        .begin_operation([5; 32], target("crashed"), Base::Missing, None, 3)
        .await
        .unwrap();
    let result: Result<()> = storage
        .owner
        .call(move |store| {
            let mut journal = Journal::new(&mut store.state, journal::Options::default())?;
            journal.append_at(id, 0, b"abc")?;
            journal.seal(id)?;
            panic!("injected worker failure after commit");
        })
        .await;
    assert_worker_failure(result, Failure::OutcomeUnknown);
    // A reply can disappear during unwinding before the connection has dropped.
    // Wait for the same worker to finish closing its receiving end, then reopen.
    tokio::time::timeout(Duration::from_secs(5), async {
        while !storage.owner.sender.is_closed() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let reopened = open(&path).await;
    assert_eq!(
        reopened
            .staging()
            .begin_operation([5; 32], target("crashed"), Base::Missing, None, 3)
            .await
            .unwrap(),
        id
    );
    assert_eq!(reopened.staging().read(id, 0, 100).await.unwrap(), b"abc");
    assert_eq!(
        reopened
            .staging()
            .head(target("crashed"))
            .await
            .unwrap()
            .unwrap()
            .id,
        id
    );
    reopened.close().await.unwrap();
}

#[tokio::test]
async fn lost_begin_operation_reply_recovers_the_same_builder() {
    let temp = tempfile::tempdir().unwrap();
    let storage = open(&temp.path().join("state")).await;
    let (release, wait) = blocking::channel();
    let (ready, committed) = oneshot::channel();
    let owner = storage.owner.clone();
    let running = tokio::spawn(async move {
        owner
            .call_with_deadline(
                move |store| {
                    let id = Journal::new(&mut store.state, journal::Options::default())?
                        .begin_operation(&[6; 32], &target("lost-begin"), Base::Missing, None, 0)?;
                    ready.send(id).unwrap();
                    wait.recv_timeout(Duration::from_secs(10)).unwrap();
                    Ok(id)
                },
                Duration::from_secs(2),
            )
            .await
    });
    let expected = tokio::time::timeout(Duration::from_secs(5), committed)
        .await
        .unwrap()
        .unwrap();
    assert_worker_failure(running.await.unwrap(), Failure::OutcomeUnknown);
    release.send(()).unwrap();
    let id = storage
        .staging()
        .begin_operation([6; 32], target("lost-begin"), Base::Missing, None, 0)
        .await
        .unwrap();
    assert_eq!(id, expected);
    assert_eq!(storage.staging().drafts(None, 100).await.unwrap().len(), 1);
    storage.close().await.unwrap();
}

#[tokio::test]
async fn last_client_drop_drains_accepted_work_and_snapshots_hold_the_owner() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let storage = open(&path).await;
    let staging = storage.staging();
    let receipt = generation(&staging, None, b"abc").await;
    let snapshot = staging.snapshot(receipt.generation).await.unwrap();
    drop(staging);
    drop(storage);
    assert!(State::open_managed(&path).is_err());
    assert_eq!(snapshot.read(0, 10).await.unwrap(), b"abc");
    let owner = snapshot.staging.owner.clone();
    let release = pause(&owner).await;
    let (reply, receive) = oneshot::channel();
    owner
        .sender
        .try_send(Command::Work(Box::new(move |store| {
            let mut journal = Journal::new(&mut store.state, journal::Options::default()).unwrap();
            let id = journal
                .begin(&target("drained"), Base::Missing, None, 0)
                .unwrap();
            journal.seal(id).unwrap();
            reply.send(()).unwrap();
        })))
        .unwrap_or_else(|_| panic!("cannot enqueue drain fixture"));
    // Only the receiving end and its accepted work remain after these drops.
    drop(snapshot);
    drop(owner);
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), receive)
        .await
        .unwrap()
        .unwrap();
    let reopened = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match Storage::open(
                &path,
                cache::Options::default(),
                journal::Options::default(),
            )
            .await
            {
                Ok(storage) => break storage,
                Err(error) => {
                    assert!(error.to_string().contains("already in use"), "{error}");
                    tokio::task::yield_now().await;
                }
            }
        }
    })
    .await
    .unwrap();
    assert!(
        reopened
            .staging()
            .head(target("drained"))
            .await
            .unwrap()
            .is_some()
    );
    reopened.close().await.unwrap();
}
