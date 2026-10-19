//! Retention checked through real managed reader + SDK, never another SQLite connection.
use super::*;
use s3drive_state::sparse_views::{self, Binding, Client, Id};
use std::{path::Path, sync::Arc};

fn not_admitted(error: &(dyn std::error::Error + Send + Sync + 'static)) -> bool {
    error.downcast_ref::<sparse_views::pins::Failure>() == Some(&sparse_views::pins::Failure::Quota)
        || error.downcast_ref::<s3drive_state::worker::Failure>()
            == Some(&s3drive_state::worker::Failure::QueueFull)
}
async fn open_after_cleanup(r: &Reader, local: &Client, view: Id) -> EditedFile {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match r.reopen_edited(local.clone(), view).await {
                Ok(file) => return file,
                Err(error) if not_admitted(error.as_ref()) => tokio::task::yield_now().await,
                Err(error) => panic!("guard reopen: {error}"),
            }
        }
    })
    .await
    .expect("guard cleanup did not restore admission")
}
async fn available(local: &Client, view: Id) {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match local.pin(view).await {
                Ok(pin) => {
                    assert!(local.release_pin(pin).await.unwrap());
                    return;
                }
                Err(error) if not_admitted(error.as_ref()) => tokio::task::yield_now().await,
                Err(error) => panic!("guard capacity: {error}"),
            }
        }
    })
    .await
    .expect("guard capacity did not return");
}
async fn quota(f: &Fixture, r: &Reader, local: &Client, view: Id) {
    f.s3.settle().await;
    let requests = f.requests();
    let events = f.s3.events().len();
    let error = r
        .reopen_edited(local.clone(), view)
        .await
        .expect_err("live guard must occupy the last pin");
    assert_eq!(
        error.downcast_ref::<sparse_views::pins::Failure>(),
        Some(&sparse_views::pins::Failure::Quota)
    );
    assert_eq!(
        f.requests(),
        requests,
        "quota refusal performed a history request"
    );
    assert_eq!(
        f.s3.events().len(),
        events,
        "quota refusal touched the provider"
    );
}
pub async fn flow(f: &Fixture, path: &Path) {
    let tree = f.tree(10, C, data(0, 1)).await;
    let r = f.reader().await;
    let base = r.open_file(&id(10)).await.unwrap().pinned_base();
    let binding = base.sparse_binding().unwrap();
    let (storage, local) = composition::storage(&path.join("guard café 🦀")).await;
    let initial = local.begin([1; 32], binding.clone(), C).await.unwrap();
    let view = local
        .write([2; 32], initial.id(), 0, Arc::from("café 🦀".as_bytes()))
        .await
        .unwrap();
    let mut manual = Vec::new();
    for _ in 0..sparse_views::pins::MAX_ACTIVE - 1 {
        manual.push(local.pin(view.id()).await.unwrap());
    }
    // A descriptor mismatch fails after acquiring its local guard. A subsequent
    // successful open proves that failed open did not leak the last pin.
    let descriptor = binding.base().unwrap();
    let wrong = Binding::new(
        s3drive_core::manifest::base::Descriptor::from_hex(
            &id(99),
            descriptor.node(),
            descriptor.reservation(),
            descriptor.payload_reference(),
        )
        .unwrap(),
        *binding.store(),
        *binding.source(),
        binding.fingerprint().into(),
        binding.root().into(),
        binding.base_size(),
    )
    .unwrap();
    let bad = local.begin([3; 32], wrong, C).await.unwrap();
    failed(
        r.reopen_edited(local.clone(), bad.id()).await,
        ReadFailure::Binding,
    );
    let file = open_after_cleanup(&r, &local, view.id()).await;
    let clone = file.clone();
    drop(file);
    quota(f, &r, &local, view.id()).await;
    assert_eq!(
        clone.read_range(0, "café 🦀".len()).await.unwrap(),
        "café 🦀".as_bytes()
    );
    let prep = clone
        .begin_preparation(
            [4; 32],
            [17; 16],
            sparse_views::preparation::Options::default(),
        )
        .await
        .unwrap();
    let prep_id = prep.id();
    let prep_clone = prep.clone();
    drop(clone);
    drop(prep);
    quota(f, &r, &local, view.id()).await;
    assert_eq!(prep_clone.info().await.unwrap().view().id(), view.id());
    drop(prep_clone);
    available(&local, view.id()).await;
    let reopened = r
        .reopen_preparation(
            local.clone(),
            prep_id,
            sparse_views::preparation::Options::default(),
        )
        .await
        .unwrap();
    assert_eq!(reopened.view().id(), view.id());
    quota(f, &r, &local, view.id()).await;
    drop(reopened);
    available(&local, view.id()).await;
    // Cancel while fetching the historical root, after local acquisition. The
    // fixture handshake establishes the open reached provider I/O.
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    f.s3.fault(Target::Exact(tree.root.clone()), Fault::Gate);
    let task = {
        let reader = r.clone();
        let local = local.clone();
        let id = view.id();
        tokio::spawn(async move { reader.reopen_edited(local, id).await })
    };
    f.s3.wait(arrived + 1).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    f.s3.clear();
    f.s3.permits.add_permits(1);
    f.s3.settle().await;
    let final_file = open_after_cleanup(&r, &local, view.id()).await;
    assert_eq!(
        final_file.read_range(0, "café 🦀".len()).await.unwrap(),
        "café 🦀".as_bytes()
    );
    drop(final_file);
    available(&local, view.id()).await;
    for pin in manual {
        assert!(local.release_pin(pin).await.unwrap());
    }
    storage.close().await.unwrap();
}
