use super::*;
use crate::sparse_views::{Binding, Client, Id, Options, Piece};
use std::{future::Future, task::Poll, time::Duration};

async fn fixture(path: &Path) -> (Storage, Client, Id) {
    let path = path.join("state");
    let storage = Storage::open(
        &path,
        cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap();
    let client = storage.sparse_views(Options::default()).unwrap();
    let binding = Binding::new(
        s3drive_core::manifest::base::Descriptor::from_hex(
            &"01".repeat(16),
            &"02".repeat(16),
            &"03".repeat(16),
            &"04".repeat(32),
        )
        .unwrap(),
        [5; 16],
        [6; 32],
        "07".repeat(32),
        "objects/".into(),
        32,
    )
    .unwrap();
    let view = client.begin([1; 32], binding, 32).await.unwrap();
    let view = client
        .write([2; 32], view.id(), 0, Arc::from("café 🦀".as_bytes()))
        .await
        .unwrap();
    (storage, client, view.id())
}
async fn active(storage: &Storage) -> u16 {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let result = storage
                .owner
                .call(|store| {
                    Ok(store.state.connection.query_row(
                        "SELECT count(*) FROM source_read_pins",
                        [],
                        |r| r.get::<_, u16>(0),
                    )?)
                })
                .await;
            match result {
                Ok(count) => return count,
                Err(error) if error.downcast_ref::<Failure>() == Some(&Failure::QueueFull) => {
                    // Read-only admission retry only; never retry an unknown mutation.
                    tokio::task::yield_now().await;
                }
                Err(error) => panic!("pin count: {error}"),
            }
        }
    })
    .await
    .expect("pin count admission timed out")
}
async fn cleaned_while_idle(storage: &Storage) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while !storage.owner.read_pins.lock().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("idle pin cleanup timed out");
}
async fn submit_only<F: Future>(future: std::pin::Pin<&mut F>) {
    let mut future = future;
    std::future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
async fn block(storage: &Storage) -> std::sync::mpsc::Sender<()> {
    let (started, ready) = oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    storage
        .owner
        .sender
        .try_send(Command::Work(Box::new(move |_| {
            started.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(10)).unwrap();
        })))
        .unwrap_or_else(|_| panic!("block command admission"));
    ready.await.unwrap();
    release
}
#[tokio::test]
async fn clones_retain_one_exact_unicode_view_until_the_final_drop() {
    let temp = tempfile::tempdir().unwrap();
    let (storage, client, id) = fixture(temp.path()).await;
    let first = client.guard(id).await.unwrap();
    let second = first.clone();
    assert_eq!(first.view().id(), id);
    assert_eq!(active(&storage).await, 1);
    drop(first);
    assert_eq!(active(&storage).await, 1);
    assert_eq!(
        second.read(0, "café 🦀".len()).await.unwrap(),
        vec![Piece::Local("café 🦀".as_bytes().to_vec())]
    );
    drop(second);
    cleaned_while_idle(&storage).await;
    assert_eq!(active(&storage).await, 0);
    storage.close().await.unwrap();
}
#[tokio::test]
async fn final_drop_survives_a_completely_full_eight_command_queue() {
    let temp = tempfile::tempdir().unwrap();
    let (storage, client, id) = fixture(temp.path()).await;
    let guard = client.guard(id).await.unwrap();
    let release = block(&storage).await;
    let (done, ready) = oneshot::channel();
    for _ in 0..QUEUE_SIZE - 1 {
        storage
            .owner
            .sender
            .try_send(Command::Work(Box::new(|_| {})))
            .unwrap_or_else(|_| panic!("queue fill"));
    }
    storage
        .owner
        .sender
        .try_send(Command::Work(Box::new(move |_| {
            let _ = done.send(());
        })))
        .unwrap_or_else(|_| panic!("last queue slot"));
    drop(guard);
    assert_eq!(storage.owner.read_pins.lock().len(), 1);
    assert_eq!(storage.owner.sender.capacity(), 0);
    release.send(()).unwrap();
    ready.await.unwrap();
    assert_eq!(active(&storage).await, 0);
    assert!(storage.owner.read_pins.lock().is_empty());
    storage.close().await.unwrap();
}
#[tokio::test]
async fn canceled_queued_acquisition_and_unread_committed_reply_leave_no_pin() {
    let temp = tempfile::tempdir().unwrap();
    let (storage, client, id) = fixture(temp.path()).await;
    let release = block(&storage).await;
    let mut queued = Box::pin(client.guard(id));
    submit_only(queued.as_mut()).await;
    drop(queued);
    release.send(()).unwrap();
    assert_eq!(active(&storage).await, 0);
    let release = block(&storage).await;
    let mut committed = Box::pin(client.guard(id));
    submit_only(committed.as_mut()).await;
    release.send(()).unwrap();
    // FIFO barrier proves the guard was committed and sent, but its future has
    // not consumed the reply. Dropping that receiver drops the guard itself.
    assert_eq!(active(&storage).await, 1);
    drop(committed);
    assert_eq!(active(&storage).await, 0);
    storage.close().await.unwrap();
}
#[tokio::test]
async fn cleanup_failure_preserves_the_pin_and_retries_after_fault_removal() {
    let temp = tempfile::tempdir().unwrap();
    let (storage, client, id) = fixture(temp.path()).await;
    let guard = client.guard(id).await.unwrap();
    storage.owner.call(|s| { s.state.connection.execute_batch(
        "CREATE TRIGGER deny_guard_release BEFORE DELETE ON source_read_pins BEGIN SELECT RAISE(ABORT,'deny'); END;"
    )?; Ok(()) }).await.unwrap();
    drop(guard);
    assert_eq!(active(&storage).await, 1);
    assert_eq!(storage.owner.read_pins.lock().len(), 1);
    storage
        .owner
        .call(|s| {
            s.state
                .connection
                .execute_batch("DROP TRIGGER deny_guard_release")?;
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(active(&storage).await, 0);
    assert!(storage.owner.read_pins.lock().is_empty());
    storage.close().await.unwrap();
}

#[tokio::test]
async fn active_quota_recovers_and_lifetime_opens_do_not_accumulate_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let (storage, client, id) = fixture(temp.path()).await;
    let mut guards = Vec::new();
    for _ in 0..crate::sparse_views::pins::MAX_ACTIVE {
        guards.push(client.guard(id).await.unwrap());
    }
    assert_eq!(active(&storage).await, 256);
    let error = client.guard(id).await.expect_err("pin quota must apply");
    assert_eq!(
        error.downcast_ref::<crate::sparse_views::pins::Failure>(),
        Some(&crate::sparse_views::pins::Failure::Quota)
    );
    drop(guards);
    assert_eq!(active(&storage).await, 0);
    for _ in 0..300 {
        let guard = client.guard(id).await.unwrap();
        drop(guard);
    }
    assert_eq!(active(&storage).await, 0);
    assert!(storage.owner.read_pins.lock().is_empty());
    let issued: i64 = storage
        .owner
        .call(|s| {
            Ok(s.state.connection.query_row(
                "SELECT issued FROM source_read_pin_session",
                [],
                |r| r.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(issued, 556);
    storage.close().await.unwrap();
}

#[tokio::test]
async fn dropping_a_guard_after_close_cannot_release_a_replacement_owners_pin() {
    let temp = tempfile::tempdir().unwrap();
    let (storage, client, id) = fixture(temp.path()).await;
    let old = client.guard(id).await.unwrap();
    storage.close().await.unwrap();
    assert!(old.read(0, 1).await.is_err());
    let next = Storage::open(
        &temp.path().join("state"),
        cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap();
    let current = next
        .sparse_views(Options::default())
        .unwrap()
        .guard(id)
        .await
        .unwrap();
    drop(old);
    assert_eq!(active(&next).await, 1);
    assert_eq!(
        current.read(0, "café 🦀".len()).await.unwrap(),
        vec![Piece::Local("café 🦀".as_bytes().to_vec())]
    );
    drop(current);
    assert_eq!(active(&next).await, 0);
    next.close().await.unwrap();
}

#[tokio::test]
async fn a_burst_of_final_drops_queues_one_wake_and_preserves_seven_command_slots() {
    let temp = tempfile::tempdir().unwrap();
    let (storage, client, id) = fixture(temp.path()).await;
    let mut guards = Vec::new();
    for _ in 0..4 {
        guards.push(client.guard(id).await.unwrap());
    }
    let release = block(&storage).await;
    drop(guards);
    assert_eq!(storage.owner.read_pins.lock().len(), 4);
    assert_eq!(storage.owner.sender.capacity(), QUEUE_SIZE - 1);
    release.send(()).unwrap();
    cleaned_while_idle(&storage).await;
    assert_eq!(active(&storage).await, 0);
    storage.close().await.unwrap();
}
