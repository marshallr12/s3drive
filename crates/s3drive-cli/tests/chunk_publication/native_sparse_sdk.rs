//! Dedicated original-open SDK lifecycle, not native driver acceptance.
use super::*;
use s3drive_managed::chunks::{NativeSparseFile, Reader};
use s3drive_state::{
    chunk_mount, namespace_client as names,
    namespace_client::opens,
    sparse_views::{
        self as views,
        preparation::{self as plans, publication as roots},
    },
};

pub async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server, mode: u8) {
    let (e, chunks, creator, draft) = creator_import::setup_owner(config, root, s).await;
    let created = creator.create(draft).await.unwrap();
    let volume = chunks.key().volume().to_owned();
    let node = created.metadata().node.id.clone();
    let store = empty_creation::store(s).await;
    let target = e.recovery_target("writable/.s3drive/binding").unwrap();
    let opens = chunks.opens();
    opens.start_session(id(80), volume.clone()).await.unwrap();
    opens
        .prepare(opens::OpenInput {
            id: id(71),
            session: id(80),
            volume: volume.clone(),
            node: node.clone(),
            store,
            source: *target.source(),
            fingerprint: target.fingerprint().into(),
            delete_on_close: false,
            controlled: true,
            session_bound: true,
            posix: true,
            admission: None,
            creation: None,
        })
        .await
        .unwrap();
    opens.activate(id(71), id(72)).await.unwrap();
    let reader = Reader::open(s.wire.clone(), volume.clone(), vec![(id(200), e.reader())])
        .await
        .unwrap();
    let base = reader.open_file(&node).await.unwrap().pinned_base();
    let local = e.storage().sparse_views(views::Options::default()).unwrap();
    local.enable_scoped().await.unwrap();
    let binding = base.sparse_binding().unwrap();
    let clean = local
        .begin_scoped(
            views::ScopedOperation::new(&binding, [1; 32]).unwrap(),
            binding,
            0,
        )
        .await
        .unwrap();
    let original = reader
        .reopen_edited(local.clone(), clean.id())
        .await
        .unwrap();
    let native = NativeSparseFile::attach(
        &chunks,
        local.clone(),
        id(71),
        views::Options::default(),
        plans::Options::default(),
    )
    .await
    .unwrap();
    if !mode.is_multiple_of(3) {
        remove(&chunks, s, &node, mode % 3 == 2).await;
    }
    let first = native
        .write(&original, [2; 32], 3, "日本語 🦀\0".as_bytes().to_vec())
        .await
        .unwrap();
    let edited = native.refresh(&original, &first).await.unwrap();
    assert_eq!(
        edited.read_range(3, 64).await.unwrap(),
        "日本語 🦀\0".as_bytes()
    );
    let earlier = native.prepare(&edited).await.unwrap();
    let second = native
        .write(&edited, [3; 32], 1, b"z".to_vec())
        .await
        .unwrap();
    let latest = native.refresh(&edited, &second).await.unwrap();
    let future = native.close(id(90));
    drop(future); // No poll: the real callback fence must already be active.
    assert!(
        native
            .write(&latest, [4; 32], 0, b"forbidden".to_vec())
            .await
            .is_err()
    );
    assert!(native.resize(&latest, [5; 32], 0).await.is_err());
    assert!(native.close(id(91)).await.is_err());
    let closed = native.close(id(90)).await.unwrap();
    assert_eq!(closed.plans.len(), 2);
    assert_eq!(closed.plans[0].id().unwrap(), earlier.id());
    assert_eq!(closed.plans[1].view(), &second);
    assert!(native.complete(id(90), id(92)).await.is_err());
    if mode >= 3 {
        let content = Arc::new(Content {
            native: native.clone(),
            reader: reader.clone(),
            executor: e.clone(),
            inject_open: (mode == 6).then(|| chunks.opens()),
            flushes: std::sync::atomic::AtomicUsize::new(0),
        });
        let controller = s3drive_managed::disposition::Controller::attach_native(
            chunks.clone(),
            e.clone(),
            s.wire.clone(),
            empty_creation::store(s).await,
            id(80),
            content.clone(),
            Arc::new(tokio::sync::Mutex::new(())),
        )
        .await
        .unwrap();
        if mode == 6 {
            assert!(
                chunks
                    .uncontended_close(id(80), id(71), id(90))
                    .await
                    .unwrap()
                    .is_some()
            );
        }
        controller.close(&id(71), &id(90)).await.unwrap();
        if mode == 6 {
            assert_eq!(content.flushes.load(std::sync::atomic::Ordering::SeqCst), 1);
            let other = opens.get(id(95)).await.unwrap();
            assert!(other.active());
            assert!(other.close().is_none());
            assert!(other.pending().is_empty());
            let remaining = opens.records().await.unwrap();
            assert!(remaining.iter().any(|r| r.input.id == id(95)));
            assert!(!remaining.iter().any(|r| r.input.id == id(71)));
            eprintln!(
                "NATIVE_CONTROLLER_INTERLEAVING flushes=1 original_retired=true second_active=true"
            );
        }
        let saved = opens.get(id(71)).await.unwrap();
        assert!(saved.completed(&id(90)));
        assert!(saved.cleanup().is_none());
        assert_eq!(
            chunks
                .namespace()
                .owned_removal(volume.clone(), node.clone())
                .await
                .unwrap(),
            None
        );
        assert!(matches!(
            ask(&s.direct, Request::PendingRemoval { volume, node }).await,
            Reply::PendingRemoval(None)
        ));
        assert_eq!(latest.read_range(0, 3).await.unwrap(), &[0, b'z', 0]);
        e.storage().close().await.unwrap();
        return;
    }
    let mut predecessor = None;
    for (index, info) in closed.plans.iter().enumerate() {
        assert_eq!(info.native_ownership().unwrap().open(), id(71));
        assert_eq!(info.ownership().is_some(), !mode.is_multiple_of(3));
        let prep = native.reopen(&reader, info.id().unwrap()).await.unwrap();
        prep.prepare().await.unwrap();
        prep.uploader(e.clone())
            .unwrap()
            .upload_tree()
            .await
            .unwrap();
        let publisher = prep.publisher(e.clone(), roots::Limits::default()).unwrap();
        publisher.check_semantics().await.unwrap();
        let saved = publisher.begin(id(100 + index as u64)).await.unwrap();
        assert_eq!(saved.input().predecessor(), predecessor);
        publisher.resume().await.unwrap();
        // A remote publish receipt is not the exact local source acknowledgment.
        assert!(native.complete(id(90), id(92)).await.is_err());
        let ack = publisher
            .acknowledge_scoped(
                views::ScopedOperation::new(info.view().binding(), [120 + index as u8; 32])
                    .unwrap(),
                views::Options::default(),
            )
            .await
            .unwrap();
        assert_eq!(ack.clean_view_id().is_some(), index == 1);
        predecessor = Some(prep.id());
    }
    let completed = native.complete(id(90), id(92)).await.unwrap();
    assert!(completed.completed(&id(90)));
    assert!(
        completed.cleanup().is_none(),
        "POSIX must not invent Windows Cleanup"
    );
    assert_eq!(native.complete(id(90), id(92)).await.unwrap(), completed);
    assert!(native.write(&latest, [6; 32], 0, vec![1]).await.is_err());
    assert_eq!(latest.read_range(0, 3).await.unwrap(), &[0, b'z', 0]);
    let held = native.reopen(&reader, earlier.id()).await.unwrap();
    assert_eq!(held.id(), earlier.id());
    e.storage().close().await.unwrap();
}
async fn remove(chunks: &chunk_mount::Client, s: &Server, node_id: &str, committed: bool) {
    let volume = chunks.key().volume().to_owned();
    let remote = node(&s.direct, &volume, node_id).await;
    let parent = node(&s.direct, &volume, s3drive_authority::ROOT).await;
    let control = chunks.opens().get(id(71)).await.unwrap().input;
    let mut input = names::Input {
        request: Apply {
            volume: volume.clone(),
            operation: id(81),
            expected_sequence: info(&s.direct).await.sequence,
            mutation: Mutation::ArmRemoval {
                node: s3drive_authority::NodeGuard {
                    id: remote.id.clone(),
                    revision: remote.revision,
                },
                parent: s3drive_authority::NodeGuard {
                    id: parent.id,
                    revision: parent.revision,
                },
                owner: chunks.key().owner().into(),
            },
        },
        store: control.store,
        source: control.source,
        fingerprint: control.fingerprint,
        entries: vec![names::Entry::from_node(&remote).unwrap()],
        destination_control: None,
    };
    for sequence in 0..(if committed { 2 } else { 1 }) {
        chunks.namespace().begin(input.clone()).await.unwrap();
        let Reply::Applied(receipt) = ask(
            &s.direct,
            Request::Apply {
                request: input.request.clone(),
            },
        )
        .await
        else {
            panic!()
        };
        chunks
            .namespace()
            .acknowledge(input.request.operation.clone(), Ok(receipt.clone()))
            .await
            .unwrap();
        if sequence == 0 && committed {
            let parent = node(&s.direct, &volume, s3drive_authority::ROOT).await;
            input.request = Apply {
                volume: volume.clone(),
                operation: id(82),
                expected_sequence: info(&s.direct).await.sequence,
                mutation: Mutation::CommitRemoval {
                    node: s3drive_authority::NodeGuard {
                        id: remote.id.clone(),
                        revision: receipt.node.revision,
                    },
                    parent: s3drive_authority::NodeGuard {
                        id: parent.id,
                        revision: parent.revision,
                    },
                    fence: s3drive_authority::RemovalFence {
                        intent: id(81),
                        owner: chunks.key().owner().into(),
                    },
                },
            };
            input.entries = vec![names::Entry::from_node(&receipt.node).unwrap()];
        }
    }
}

struct Content {
    native: NativeSparseFile,
    reader: Reader,
    executor: Executor,
    inject_open: Option<s3drive_state::worker::Opens>,
    flushes: std::sync::atomic::AtomicUsize,
}
impl s3drive_managed::disposition::NativeContent for Content {
    fn flush<'a>(&'a self, _node: &'a str) -> s3drive_managed::disposition::Task<'a, ()> {
        Box::pin(async {
            let flush = self
                .flushes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            for info in self.native.plans().await? {
                let prep = self.native.reopen(&self.reader, info.id()?).await?;
                prep.prepare().await?;
                prep.uploader(self.executor.clone())?.upload_tree().await?;
                let publisher = prep.publisher(self.executor.clone(), roots::Limits::default())?;
                publisher.check_semantics().await?;
                let operation: String = info.id()?[..16]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                publisher.begin(operation).await?;
                publisher.resume().await?;
                publisher
                    .acknowledge_scoped(
                        views::ScopedOperation::new(info.view().binding(), info.id()?)?,
                        views::Options::default(),
                    )
                    .await?;
            }
            if flush == 0
                && let Some(opens) = &self.inject_open
            {
                // A separate valid owner command changes live-handle state
                // during the real controller flush, after its prefix selection.
                let mut input = opens.get(self.native.open().into()).await?.input;
                input.id = id(95);
                opens.prepare(input).await?;
                opens.activate(id(95), id(96)).await?;
            }
            Ok(())
        })
    }
    fn complete<'a>(
        &'a self,
        open: &'a str,
        callback: &'a str,
        completion: String,
    ) -> s3drive_managed::disposition::Task<'a, ()> {
        Box::pin(async move {
            assert_eq!(open, self.native.open());
            self.native.complete(callback.into(), completion).await?;
            Ok(())
        })
    }
}
