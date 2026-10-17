use super::*;
use crate::{
    S3Reader,
    probe::tests::{Mode, Object, Server, source_at},
};
use aws_sdk_s3::config::{Credentials, SharedCredentialsProvider};
use s3drive_state::{
    cache,
    journal::{self, Base},
    remote::Observation as LocalObservation,
};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

const KEY: &str = "nested/日本語 %2F + café.txt";

async fn make(session: Session, path: &Path) -> Executor {
    let storage = Storage::open(path, cache::Options::default(), journal::Options::default())
        .await
        .unwrap();
    let policy = policy::tests::fixture_policy(&session);
    Executor::open(session.reader, storage, policy)
        .await
        .unwrap()
}
fn target(executor: &Executor, key: &str) -> Target {
    Target::new(
        executor.0.session.binding_id,
        executor.0.session.reader.configuration_fingerprint(),
        key,
    )
    .unwrap()
}
pub(super) async fn stage(
    executor: &Executor,
    key: &str,
    parent: Option<GenerationId>,
    bytes: &[u8],
) -> GenerationId {
    let staging = executor.0.storage.staging();
    let base = if let Some(id) = parent {
        staging.info(id).await.unwrap().base
    } else {
        Base::Missing
    };
    let id = staging
        .begin(target(executor, key), base, parent, bytes.len() as u64)
        .await
        .unwrap();
    let mut offset = 0;
    for chunk in bytes.chunks(journal::CHUNK_BYTES) {
        staging
            .append_at(id, offset, Arc::from(chunk))
            .await
            .unwrap();
        offset += chunk.len() as u64;
    }
    staging.seal(id).await.unwrap();
    id
}
fn reader_at(endpoint: &str, deadline: Duration) -> S3Reader {
    S3Reader::with_credentials(
        source_at(endpoint),
        SharedCredentialsProvider::new(Credentials::new(
            "TESTACCESS",
            "TESTSECRET",
            Some("TESTTOKEN".into()),
            None,
            "test",
        )),
        deadline,
        3,
    )
    .unwrap()
}
async fn request_received(server: &Server) {
    tokio::time::timeout(Duration::from_secs(5), server.received.notified())
        .await
        .unwrap();
}

#[tokio::test]
async fn exact_unicode_and_empty_generations_verify_full_bytes_without_replaying() {
    let maximum = (0..transport::MAX_PUT)
        .map(|i| (i % 251) as u8)
        .collect::<Vec<_>>();
    for bytes in [
        b"payload with binary\0\xff".as_slice(),
        b"",
        maximum.as_slice(),
    ] {
        let server = Server::new(Mode::NullVersion).await;
        let temp = tempfile::tempdir().unwrap();
        let executor = make(
            Session::pin(reader_at(&server.endpoint, Duration::from_secs(8)))
                .await
                .unwrap(),
            &temp.path().join("state"),
        )
        .await;
        let id = stage(&executor, KEY, None, bytes).await;
        let info = executor.submit(id).await.unwrap();
        assert_eq!(info.phase, Phase::Verified);
        assert_eq!(info.revision.unwrap().version(), None);
        let puts = server.count("PUT");
        let heads = server.count("HEAD");
        let gets = server.count("GET");
        assert_eq!((puts, heads, gets), (1, 1, 1));
        assert_eq!(executor.submit(id).await.unwrap().phase, Phase::Verified);
        assert_eq!(executor.reconcile(id).await.unwrap().phase, Phase::Verified);
        assert_eq!(
            (
                server.count("PUT"),
                server.count("HEAD"),
                server.count("GET")
            ),
            (puts, heads, gets)
        );
        assert_eq!(
            server.model.lock().unwrap().objects[&format!("/bucket/root/{KEY}")].bytes,
            bytes
        );
        assert_eq!(
            executor
                .0
                .storage
                .staging()
                .read(id, 0, bytes.len())
                .await
                .unwrap(),
            bytes
        );
        assert_eq!(
            executor
                .0
                .storage
                .staging()
                .pending(None, 100)
                .await
                .unwrap()
                .len(),
            1
        );
        executor.0.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn flush_acknowledges_exact_unicode_and_empty_generations_and_never_replays() {
    for body in [b"binary\0\xff".as_slice(), b""] {
        let server = Server::new(Mode::DropAfterCommit).await;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state");
        let executor = make(server.session().await, &path).await;
        let first = stage(&executor, KEY, None, b"superseded locally").await;
        let selected = stage(&executor, KEY, Some(first), body).await;
        let snapshot = executor.0.storage.staging().snapshot(first).await.unwrap();
        let receipt = executor.flush(selected).await.unwrap();
        assert_eq!(receipt.info().generation, selected);
        assert_eq!(receipt.covered_generations(), 2);
        assert_eq!(server.count("PUT"), 1);
        assert_eq!(
            executor.flush(selected).await.unwrap().checksum(),
            receipt.checksum()
        );
        assert!(
            executor
                .0
                .storage
                .staging()
                .pending(None, 100)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(snapshot.read(0, 100).await.unwrap(), b"superseded locally");
        let newer = stage(&executor, KEY, Some(selected), b"newer dirty").await;
        assert_eq!(
            executor.flush(selected).await.unwrap().checksum(),
            receipt.checksum()
        );
        assert_eq!(
            executor
                .0
                .storage
                .staging()
                .pending(None, 100)
                .await
                .unwrap()[0]
                .id,
            newer
        );
        executor.0.storage.close().await.unwrap();
        let executor = make(server.session().await, &path).await;
        assert_eq!(
            executor.flush(selected).await.unwrap().checksum(),
            receipt.checksum()
        );
        assert_eq!(server.count("PUT"), 1);
        let next = executor.flush(newer).await.unwrap();
        assert_eq!(
            next.info().guard,
            Guard::Match(receipt.info().revision.as_ref().unwrap().etag().into())
        );
        assert_eq!(server.count("PUT"), 2);
        assert_eq!(next.covered_generations(), 3);
        assert_eq!(
            executor
                .0
                .storage
                .staging()
                .read(selected, 0, 100)
                .await
                .unwrap(),
            body
        );
        executor.0.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn uncertain_and_rejected_flushes_never_acknowledge_or_discard_local_edits() {
    for mode in [
        Mode::DropBeforeCommit,
        Mode::DeleteAfterCommit,
        Mode::Status503,
        Mode::Reject409,
        Mode::CorruptGet,
        Mode::OtherAfterCommit,
    ] {
        let server = Server::new(mode).await;
        let temp = tempfile::tempdir().unwrap();
        let executor = make(server.session().await, &temp.path().join("state")).await;
        let id = stage(&executor, KEY, None, b"retain failed flush").await;
        assert!(executor.flush(id).await.is_err());
        assert!(
            executor
                .0
                .storage
                .uploads()
                .flush_receipt(id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            executor
                .0
                .storage
                .uploads()
                .settlement(id)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            executor
                .0
                .storage
                .staging()
                .pending(None, 100)
                .await
                .unwrap()[0]
                .id,
            id
        );
        assert_eq!(
            executor.0.storage.staging().read(id, 0, 100).await.unwrap(),
            b"retain failed flush"
        );
        assert!(executor.flush(id).await.is_err());
        assert_eq!(server.count("PUT"), 1);
        executor.0.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn lost_or_malformed_success_is_verified_through_metadata_and_complete_get() {
    for mode in [Mode::DropAfterCommit, Mode::MissingEtag] {
        let server = Server::new(mode).await;
        let temp = tempfile::tempdir().unwrap();
        let executor = make(server.session().await, &temp.path().join("state")).await;
        let id = stage(&executor, KEY, None, b"committed").await;
        assert_eq!(executor.submit(id).await.unwrap().phase, Phase::Verified);
        assert_eq!(server.count("PUT"), 1);
        assert_eq!(server.count("GET"), 1);
        executor.0.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn absent_or_rival_observations_and_service_failures_never_authorize_another_put() {
    for mode in [
        Mode::DropBeforeCommit,
        Mode::DeleteAfterCommit,
        Mode::Status503,
        Mode::OtherAfterCommit,
    ] {
        let server = Server::new(mode).await;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state");
        let executor = make(server.session().await, &path).await;
        let id = stage(&executor, KEY, None, b"keep").await;
        let info = executor.submit(id).await.unwrap();
        assert_eq!(info.phase, Phase::Uncertain);
        assert_eq!(
            info.observation,
            if mode == Mode::OtherAfterCommit {
                LocalObservation::Other
            } else {
                LocalObservation::Absent
            }
        );
        assert_eq!(executor.submit(id).await.unwrap().operation, info.operation);
        assert_eq!(server.count("PUT"), 1);
        executor.0.storage.close().await.unwrap();
        let reopened = make(server.session().await, &path).await;
        assert_eq!(reopened.submit(id).await.unwrap().phase, Phase::Uncertain);
        assert_eq!(server.count("PUT"), 1);
        let next = stage(&reopened, KEY, Some(id), b"newer").await;
        let error = reopened.submit(next).await.unwrap_err();
        assert_eq!(
            error.cause().downcast_ref::<remote::Failure>(),
            Some(&remote::Failure::TargetBlocked)
        );
        assert_eq!(
            reopened.0.storage.staging().read(id, 0, 100).await.unwrap(),
            b"keep"
        );
        assert_eq!(
            reopened
                .0
                .storage
                .staging()
                .head(target(&reopened, KEY))
                .await
                .unwrap()
                .unwrap()
                .id,
            next
        );
        reopened.0.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn conditional_rejection_preserves_local_conflict_bytes_and_blocks_descendants() {
    for status in [409, 412] {
        let server = Server::new(if status == 409 {
            Mode::Reject409
        } else {
            Mode::Normal
        })
        .await;
        if status == 412 {
            server.model.lock().unwrap().objects.insert(
                format!("/bucket/root/{KEY}"),
                Object {
                    bytes: b"foreign".to_vec(),
                    tag: "\"foreign\"".into(),
                    operation: "foreign".into(),
                    checksum: "foreign".into(),
                },
            );
        }
        let temp = tempfile::tempdir().unwrap();
        let executor = make(server.session().await, &temp.path().join("state")).await;
        let id = stage(&executor, KEY, None, b"conflict").await;
        let rejected = executor.submit(id).await.unwrap();
        assert_eq!(rejected.phase, Phase::Rejected);
        assert_eq!(rejected.rejection_status, Some(status));
        assert_eq!(executor.submit(id).await.unwrap().phase, Phase::Rejected);
        assert_eq!(executor.reconcile(id).await.unwrap().phase, Phase::Rejected);
        assert_eq!(server.count("PUT"), 1);
        assert_eq!(server.count("GET"), 0);
        assert_eq!(
            executor.0.storage.staging().read(id, 0, 100).await.unwrap(),
            b"conflict"
        );
        executor.0.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn independent_local_journals_race_creates_and_replacements_with_one_winner() {
    let server = Server::new(Mode::Normal).await;
    let temp = tempfile::tempdir().unwrap();
    let mut executors = vec![];
    let mut base = Base::Missing;
    for round in 0..2 {
        let left = make(
            server.session().await,
            &temp.path().join(format!("left-{round}")),
        )
        .await;
        let right = make(
            server.session().await,
            &temp.path().join(format!("right-{round}")),
        )
        .await;
        let mut ids = vec![];
        for (executor, bytes) in [(&left, b"left".as_slice()), (&right, b"right".as_slice())] {
            let staging = executor.0.storage.staging();
            let id = staging
                .begin(
                    target(executor, KEY),
                    base.clone(),
                    None,
                    bytes.len() as u64,
                )
                .await
                .unwrap();
            staging.append_at(id, 0, Arc::from(bytes)).await.unwrap();
            staging.seal(id).await.unwrap();
            ids.push(id);
        }
        let (a, b) = tokio::join!(left.submit(ids[0]), right.submit(ids[1]));
        let a = a.unwrap();
        let b = b.unwrap();
        let winner = match (a.phase, b.phase) {
            (Phase::Verified, Phase::Rejected) => {
                assert_eq!(b.rejection_status, Some(412));
                a
            }
            (Phase::Rejected, Phase::Verified) => {
                assert_eq!(a.rejection_status, Some(412));
                b
            }
            other => panic!("expected exactly one guarded winner, got {other:?}"),
        };
        base = Base::Existing {
            etag: Some(winner.revision.unwrap().etag().into()),
            version: None,
        };
        assert_eq!(
            left.0.storage.staging().read(ids[0], 0, 100).await.unwrap(),
            b"left"
        );
        assert_eq!(
            right
                .0
                .storage
                .staging()
                .read(ids[1], 0, 100)
                .await
                .unwrap(),
            b"right"
        );
        executors.extend([left, right]);
    }
    assert_eq!(server.count("PUT"), 4);
    for executor in executors {
        executor.0.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn corrupted_short_or_wrong_metadata_reads_do_not_verify_receipts() {
    for mode in [
        Mode::CorruptGet,
        Mode::ShortGet,
        Mode::WrongMetadata,
        Mode::FalseReceipt,
        Mode::RevokeReads,
    ] {
        let server = Server::new(mode).await;
        let temp = tempfile::tempdir().unwrap();
        let executor = make(server.session().await, &temp.path().join("state")).await;
        let id = stage(&executor, KEY, None, b"verify these exact bytes").await;
        let error = executor.submit(id).await.unwrap_err();
        assert_eq!(error.generation, id);
        assert!(error.operation.is_some());
        let info = executor
            .0
            .storage
            .uploads()
            .for_generation(id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(info.phase, Phase::Uncertain);
        assert!(info.revision.is_none());
        server.model.lock().unwrap().mode = Mode::Normal;
        assert_eq!(executor.reconcile(id).await.unwrap().phase, Phase::Verified);
        assert_eq!(server.count("PUT"), 1);
        executor.0.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn stalled_put_or_observation_has_a_deadline_and_no_hidden_retry() {
    for mode in [Mode::StallPut, Mode::StallGet] {
        let server = Server::new(mode).await;
        let temp = tempfile::tempdir().unwrap();
        let executor = make(
            Session::pin(reader_at(&server.endpoint, Duration::from_millis(500)))
                .await
                .unwrap(),
            &temp.path().join("state"),
        )
        .await;
        let id = stage(&executor, KEY, None, b"timeout").await;
        let result = tokio::time::timeout(Duration::from_secs(5), executor.submit(id))
            .await
            .unwrap();
        if mode == Mode::StallPut {
            assert_eq!(result.unwrap().phase, Phase::Uncertain);
        } else {
            assert_eq!(
                result
                    .unwrap_err()
                    .cause()
                    .downcast_ref::<crate::Error>()
                    .unwrap()
                    .kind,
                ErrorKind::Timeout
            );
        }
        let info = executor
            .0
            .storage
            .uploads()
            .for_generation(id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(info.phase, Phase::Uncertain);
        assert_eq!(info.reason, Reason::Timeout);
        server.model.lock().unwrap().mode = Mode::Normal;
        let recovered = executor.reconcile(id).await.unwrap();
        assert_eq!(
            recovered.phase,
            if mode == Mode::StallPut {
                Phase::Uncertain
            } else {
                Phase::Verified
            }
        );
        assert_eq!(server.count("PUT"), 1);
        executor.0.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn observation_while_the_original_put_is_waiting_preserves_newer_dirty_heads() {
    let server = Server::new(Mode::StallAfterCommit).await;
    let temp = tempfile::tempdir().unwrap();
    let executor = make(server.session().await, &temp.path().join("state")).await;
    let id = stage(&executor, KEY, None, b"first").await;
    let worker = executor.clone();
    let running = tokio::spawn(async move { worker.submit(id).await });
    request_received(&server).await;
    let next = stage(&executor, KEY, Some(id), b"second").await;
    let verified = executor.submit(id).await.unwrap();
    assert_eq!(verified.phase, Phase::Verified);
    assert_eq!(
        executor
            .0
            .storage
            .staging()
            .head(target(&executor, KEY))
            .await
            .unwrap()
            .unwrap()
            .id,
        next
    );
    running.abort();
    let _ = running.await;
    server.model.lock().unwrap().mode = Mode::Normal;
    let child = executor.submit(next).await.unwrap();
    assert_eq!(child.phase, Phase::Verified);
    assert_eq!(child.anchor, Some(verified.operation));
    assert_eq!(
        child.guard,
        Guard::Match(verified.revision.unwrap().etag().into())
    );
    assert_eq!(server.count("PUT"), 2);
    assert_eq!(
        executor
            .0
            .storage
            .staging()
            .pending(None, 100)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        executor.0.storage.staging().read(id, 0, 100).await.unwrap(),
        b"first"
    );
    executor.0.storage.close().await.unwrap();
}

#[tokio::test]
async fn reconcile_never_prepares_or_sends_and_runtime_binding_is_checked_before_prepare() {
    let server = Server::new(Mode::Normal).await;
    let temp = tempfile::tempdir().unwrap();
    let executor = make(server.session().await, &temp.path().join("state")).await;
    let id = stage(&executor, KEY, None, b"bound").await;
    assert_eq!(
        executor
            .reconcile(id)
            .await
            .unwrap_err()
            .cause()
            .downcast_ref::<Failure>(),
        Some(&Failure::MissingIntent)
    );
    let session = Session::pin(
        S3Reader::with_credentials(
            server.source(),
            SharedCredentialsProvider::new(Credentials::new(
                "TESTACCESS",
                "CHANGEDSECRET",
                Some("CHANGEDTOKEN".into()),
                None,
                "test",
            )),
            Duration::from_secs(3),
            3,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let wrong = Executor(Arc::new(Shared {
        policy: policy::tests::fixture_policy(&session),
        session,
        storage: executor.0.storage.clone(),
        slots: Semaphore::new(4),
    }));
    assert_eq!(
        wrong
            .submit(id)
            .await
            .unwrap_err()
            .cause()
            .downcast_ref::<Failure>(),
        Some(&Failure::BindingMismatch)
    );
    assert!(
        executor
            .0
            .storage
            .uploads()
            .for_generation(id)
            .await
            .unwrap()
            .is_none()
    );
    let prepared = executor.0.storage.uploads().prepare(id).await.unwrap();
    assert_eq!(executor.reconcile(id).await.unwrap().phase, Phase::Prepared);
    assert_eq!(
        wrong
            .reconcile(id)
            .await
            .unwrap_err()
            .cause()
            .downcast_ref::<Failure>(),
        Some(&Failure::BindingMismatch)
    );
    assert_eq!(
        (
            server.count("PUT"),
            server.count("HEAD"),
            server.count("GET")
        ),
        (0, 0, 0)
    );
    assert_eq!(
        executor.submit(id).await.unwrap().operation,
        prepared.operation
    );
    executor.0.storage.close().await.unwrap();
}

#[tokio::test]
async fn four_execution_slots_bound_network_work_and_reject_before_journal_admission() {
    let server = Server::new(Mode::StallPut).await;
    let temp = tempfile::tempdir().unwrap();
    let executor = make(server.session().await, &temp.path().join("state")).await;
    let mut tasks = vec![];
    for index in 0..4 {
        let id = stage(&executor, &format!("key-{index}"), None, b"bounded").await;
        let worker = executor.clone();
        tasks.push(tokio::spawn(async move { worker.submit(id).await }));
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        while server.count("PUT") < 4 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let extra = stage(&executor, "extra", None, b"not admitted").await;
    assert_eq!(
        executor
            .submit(extra)
            .await
            .unwrap_err()
            .cause()
            .downcast_ref::<Failure>(),
        Some(&Failure::Busy)
    );
    assert!(
        executor
            .0
            .storage
            .uploads()
            .for_generation(extra)
            .await
            .unwrap()
            .is_none()
    );
    for task in tasks {
        task.abort();
        let _ = task.await;
    }
    assert_eq!(server.count("PUT"), 4);
    executor.0.storage.close().await.unwrap();
}

#[tokio::test]
async fn lost_local_ack_after_a_real_commit_recovers_without_sending_again() {
    let server = Server::new(Mode::StallAfterCommit).await;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let executor = make(
        Session::pin(reader_at(&server.endpoint, Duration::from_millis(750)))
            .await
            .unwrap(),
        &path,
    )
    .await;
    let id = stage(&executor, KEY, None, b"keep after closed owner").await;
    let worker = executor.clone();
    let running = tokio::spawn(async move { worker.submit(id).await });
    request_received(&server).await;
    executor.0.storage.close().await.unwrap();
    let error = running.await.unwrap().unwrap_err();
    assert_eq!(
        error
            .cause()
            .downcast_ref::<s3drive_state::worker::Failure>(),
        Some(&s3drive_state::worker::Failure::Closed)
    );
    let reopened = make(server.session().await, &path).await;
    let info = reopened
        .0
        .storage
        .uploads()
        .for_generation(id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(info.phase, Phase::Uncertain);
    assert_eq!(info.reason, Reason::Interrupted);
    assert_eq!(reopened.reconcile(id).await.unwrap().phase, Phase::Verified);
    assert_eq!(server.count("PUT"), 1);
    assert_eq!(
        reopened.0.storage.staging().read(id, 0, 100).await.unwrap(),
        b"keep after closed owner"
    );
    reopened.0.storage.close().await.unwrap();
}

#[tokio::test]
async fn invalid_full_keys_and_large_generations_never_reach_the_network() {
    let server = Server::new(Mode::Normal).await;
    let temp = tempfile::tempdir().unwrap();
    let executor = make(server.session().await, &temp.path().join("state")).await;
    let bad = stage(&executor, &"x".repeat(1024), None, b"keep").await;
    assert!(executor.submit(bad).await.is_err());
    assert!(
        executor
            .0
            .storage
            .uploads()
            .for_generation(bad)
            .await
            .unwrap()
            .is_none()
    );
    let staging = executor.0.storage.staging();
    let size = transport::MAX_PUT as u64 + 1;
    let huge = staging
        .begin(target(&executor, "too-large"), Base::Missing, None, size)
        .await
        .unwrap();
    staging.append_zero_at(huge, 0, size).await.unwrap();
    staging.seal(huge).await.unwrap();
    assert_eq!(
        executor
            .submit(huge)
            .await
            .unwrap_err()
            .cause()
            .downcast_ref::<Failure>(),
        Some(&Failure::PolicySize)
    );
    assert_eq!(
        (
            server.count("PUT"),
            server.count("HEAD"),
            server.count("GET")
        ),
        (0, 0, 0)
    );
    assert_eq!(staging.info(huge).await.unwrap().size, size);
    executor.0.storage.close().await.unwrap();
}

#[tokio::test]
async fn executor_crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_TEST_EXECUTOR_STATE") else {
        return;
    };
    let endpoint = std::env::var("S3DRIVE_TEST_EXECUTOR_ENDPOINT").unwrap();
    let executor = make(
        Session::pin(reader_at(&endpoint, Duration::from_secs(30)))
            .await
            .unwrap(),
        Path::new(&path),
    )
    .await;
    let id = stage(&executor, KEY, None, b"process death bytes").await;
    executor.flush(id).await.unwrap();
}

#[tokio::test]
async fn process_death_after_server_commit_reopens_and_observes_without_replay() {
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let server = Server::new(Mode::StallAfterCommit).await;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let mut child = Child(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "uploads::tests::executor_crash_child",
                "--nocapture",
            ])
            .env("S3DRIVE_TEST_EXECUTOR_STATE", &path)
            .env("S3DRIVE_TEST_EXECUTOR_ENDPOINT", &server.endpoint)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    request_received(&server).await;
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let reopened = make(server.session().await, &path).await;
    let id = reopened
        .0
        .storage
        .staging()
        .head(target(&reopened, KEY))
        .await
        .unwrap()
        .unwrap()
        .id;
    let before = reopened
        .0
        .storage
        .uploads()
        .for_generation(id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.phase, Phase::Uncertain);
    assert_eq!(before.reason, Reason::Interrupted);
    assert_eq!(
        reopened.flush(id).await.unwrap().info().phase,
        Phase::Verified
    );
    assert!(
        reopened
            .0
            .storage
            .staging()
            .pending(None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(server.count("PUT"), 1);
    assert_eq!(
        reopened.0.storage.staging().read(id, 0, 100).await.unwrap(),
        b"process death bytes"
    );
    reopened.0.storage.close().await.unwrap();
}
