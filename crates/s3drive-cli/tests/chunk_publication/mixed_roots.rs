//! Actual SDK + authenticated authority. No provider/native chunk-write claim.
use super::*;
use s3drive_managed::chunks::{MixedRootRecovery, Reader};
use s3drive_state::sparse_views::{
    self,
    preparation::{self as plans, publication as roots},
};
const C: u64 = s3drive_core::manifest::CHUNK_BYTES;
pub async fn crash(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let seed = setup(config, root, s).await;
    let old = publisher(&seed, s).await;
    let tail = b"unchanged tail!!\0";
    let (oldplan, _) = plan(&seed, s, 10, 40, tail, Some(130 * C + 17), 0).await;
    old.begin(oldplan, id(41)).await.unwrap();
    let initial = old.resume(oldplan).await.unwrap().metadata().clone();
    let st = storage(root, "mixed-root").await;
    let local = st.sparse_views(sparse_views::Options::default()).unwrap();
    let e = executor(config, st.clone(), false, false).await;
    let volume = info(&s.direct).await.id;
    let reader = Reader::open(s.wire.clone(), volume.clone(), vec![(id(200), e.reader())])
        .await
        .unwrap();
    let base = reader.open_file(&id(10)).await.unwrap().pinned_base();
    let v = local
        .begin([1; 32], base.sparse_binding().unwrap(), base.size())
        .await
        .unwrap();
    let bytes = "café 日本語 🦀".as_bytes();
    let edited = local
        .write([2; 32], v.id(), 3, Arc::from(bytes))
        .await
        .unwrap();
    let edit = reader
        .reopen_edited(local.clone(), edited.id())
        .await
        .unwrap();
    let prep = edit
        .begin_preparation([3; 32], [91; 16], plans::Options::default())
        .await
        .unwrap();
    prep.prepare().await.unwrap();
    let publisher = prep.publisher(e.clone(), roots::Limits::default()).unwrap();
    assert!(
        prep.publisher(seed.clone(), roots::Limits::default())
            .is_err()
    );
    assert!(publisher.begin(id(101)).await.is_err());
    s.proxy.mode.store(8, Ordering::SeqCst);
    assert!(publisher.check_semantics().await.is_err());
    s.proxy.mode.store(0, Ordering::SeqCst);
    publisher.check_semantics().await.unwrap();
    assert!(
        publisher.begin(id(101)).await.is_err(),
        "root reserved before physical receipts"
    );
    // Permit the batch's opening check and revoke its final check. A failure
    // here must precede even local physical materialization, not just Publish.
    let plans = local.preparations(plans::Options::default()).unwrap();
    let first = plans.transfers(prep.id(), None, 1).await.unwrap().remove(0);
    assert_eq!(first.position().level, -1);
    assert!(first.generation().is_none());
    s.proxy.clear();
    s.proxy.mode.store(11, Ordering::SeqCst);
    assert!(
        prep.uploader(e.clone())
            .unwrap()
            .upload_object(first.position())
            .await
            .is_err()
    );
    assert_eq!(
        s.proxy
            .requests()
            .iter()
            .filter(|r| matches!(r, Request::ManifestPayload { .. }))
            .count(),
        2
    );
    assert!(
        plans
            .transfer(prep.id(), first.position())
            .await
            .unwrap()
            .unwrap()
            .generation()
            .is_none()
    );
    s.proxy.mode.store(0, Ordering::SeqCst);
    std::fs::write(
        root.join("batch-base-key.txt"),
        &initial.node.payload.as_ref().unwrap().reservation.key,
    )
    .unwrap();
    checkpoint("MIXED_BATCH_BEGIN");
    wait(root, "mixed-batch-begin.ack").await;
    let auth_begin = s.proxy.requests().len();
    let physical = prep
        .uploader(e.clone())
        .unwrap()
        .upload_tree()
        .await
        .unwrap();
    assert_eq!(physical.new_objects(), 3);
    let authorization_calls = s.proxy.requests()[auth_begin..]
        .iter()
        .filter(|r| matches!(r, Request::ManifestPayload { .. }))
        .count();
    assert_eq!(
        authorization_calls, 6,
        "three new objects need two bounded base checks each"
    );
    checkpoint("MIXED_BATCH_END");
    wait(root, "mixed-batch-end.ack").await;
    s.proxy.mode.store(4, Ordering::SeqCst);
    assert!(publisher.begin(id(101)).await.is_err());
    s.proxy.mode.store(0, Ordering::SeqCst);
    s.proxy.clear();
    let reserved_input = publisher.begin(id(101)).await.unwrap();
    assert!(reserved_input.reservation().is_none());
    assert!(s.proxy.applies().is_empty());
    change(
        &s.direct,
        50,
        Mutation::Move {
            node: id(10),
            parent: s3drive_authority::ROOT.into(),
            name: "renamed café 🦀.bin".into(),
        },
    )
    .await;
    s.proxy.mode.store(1, Ordering::SeqCst);
    assert!(
        publisher.resume().await.is_err(),
        "lost reserve response was acknowledged"
    );
    let client = local
        .preparations(plans::Options::default())
        .unwrap()
        .publications(roots::Limits::default())
        .unwrap();
    let r = client.get(prep.id()).await.unwrap();
    assert!(r.reservation().is_none());
    assert_eq!(r.input().reserve(), reserved_input.input().reserve());
    s.proxy.clear();
    let recovered = publisher.recovery().recover(prep.id()).await.unwrap();
    assert!(recovered.reservation().is_some());
    assert!(recovered.publication().is_none());
    only_exact_apply(s, reserved_input.input().reserve());
    for (narrow, expired) in [(true, false), (false, true)] {
        let denied_executor = executor(config, st.clone(), narrow, expired).await;
        assert!(
            denied_executor
                .check_create(
                    physical.root_receipt().info().target.object_key(),
                    physical.root_receipt().info().size
                )
                .is_err()
        );
        let denied = prep
            .publisher(denied_executor, roots::Limits::default())
            .unwrap();
        s.proxy.clear();
        assert!(denied.resume().await.is_err());
        assert!(
            s.proxy.applies().is_empty(),
            "fresh publish bypassed narrowed/expired policy"
        );
    }
    let newest = local
        .write([4; 32], edited.id(), 1, Arc::from(&b"newer"[..]))
        .await
        .unwrap();
    s.proxy.mode.store(2, Ordering::SeqCst);
    assert!(
        publisher.resume().await.is_err(),
        "lost Publish response was acknowledged"
    );
    let r = client.get(prep.id()).await.unwrap();
    assert!(r.publication().is_some());
    assert!(r.receipt().is_none());
    assert_eq!(r.input().view(), edited.id());
    let n = node(&s.direct, &volume, &id(10)).await;
    assert_eq!(n.name, "renamed café 🦀.bin");
    assert_ne!(n.payload, initial.node.payload);
    let file = reader.open_file(&id(10)).await.unwrap();
    assert_eq!(file.size(), 130 * C + 17);
    assert_eq!(file.read_range(3, bytes.len()).await.unwrap(), bytes);
    assert_eq!(file.read_range(130 * C, 17).await.unwrap(), tail);
    assert_eq!(local.head(edited.binding().clone()).await.unwrap(), newest);
    std::fs::write(
        root.join("mixed-root-saved.json"),
        serde_json::to_vec(&r).unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join("mixed-root-newest.json"),
        serde_json::to_vec(&newest.id()).unwrap(),
    )
    .unwrap();
    checkpoint("CRASH_ROOT");
    std::future::pending::<()>().await;
}
pub async fn recover(root: &Path, s: &Server) {
    let saved: roots::Record =
        serde_json::from_slice(&std::fs::read(root.join("mixed-root-saved.json")).unwrap())
            .unwrap();
    let p = saved.input().plan();
    let path = root.join("mixed-root");
    assert!(
        Storage::open(
            &path,
            cache::Options::default(),
            journal::Options::default()
        )
        .await
        .is_err(),
        "strict normal open accepted damaged source bytes"
    );
    let recovery = MixedRootRecovery::open_existing(
        &path,
        s.wire.clone(),
        saved.input().reserve().volume.clone(),
        saved.input().store().id.clone(),
    )
    .await
    .unwrap();
    s.proxy.clear();
    let done = recovery.recover(p).await.unwrap();
    assert!(done.receipt().is_some());
    assert_eq!(done.publication(), saved.publication());
    assert_eq!(done.input(), saved.input());
    only_exact_apply(s, saved.publication().unwrap());
    s.proxy.clear();
    assert_eq!(recovery.recover(p).await.unwrap(), done);
    assert!(
        s.proxy.requests().is_empty(),
        "known root replay issued RPC"
    );
    recovery.close().await.unwrap();
    assert!(
        Storage::open(
            &path,
            cache::Options::default(),
            journal::Options::default()
        )
        .await
        .is_err(),
        "metadata recovery repaired source data"
    );
    let db = rusqlite::Connection::open(path.join("metadata.sqlite3")).unwrap();
    let head: Vec<u8> = db
        .query_row("SELECT head FROM sparse_view_files", [], |r| r.get(0))
        .unwrap();
    let newest: [u8; 32] =
        serde_json::from_slice(&std::fs::read(root.join("mixed-root-newest.json")).unwrap())
            .unwrap();
    assert_eq!(head, newest);
}

pub async fn rivals(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let seed = setup(config, root, s).await;
    let old = publisher(&seed, s).await;
    let bytes = (0..17).map(|x| x as u8).collect::<Vec<_>>();
    let (oldplan, _) = plan(&seed, s, 10, 40, &bytes, None, 0).await;
    old.begin(oldplan, id(41)).await.unwrap();
    old.resume(oldplan).await.unwrap();
    let volume = info(&s.direct).await.id;
    let mut machines = Vec::new();
    for i in 0..3u8 {
        let st = storage(root, &format!("root-machine-{i}")).await;
        let local = st.sparse_views(sparse_views::Options::default()).unwrap();
        let e = executor(config, st.clone(), false, false).await;
        let reader = Reader::open(s.wire.clone(), volume.clone(), vec![(id(200), e.reader())])
            .await
            .unwrap();
        let base = reader.open_file(&id(10)).await.unwrap().pinned_base();
        let initial = local
            .begin([1; 32], base.sparse_binding().unwrap(), base.size())
            .await
            .unwrap();
        let view = local
            .write([2; 32], initial.id(), 3, Arc::from(&[65 + i][..]))
            .await
            .unwrap();
        let edit = reader
            .reopen_edited(local.clone(), view.id())
            .await
            .unwrap();
        let prep = edit
            .begin_preparation([3; 32], [92 + i; 16], plans::Options::default())
            .await
            .unwrap();
        prep.prepare().await.unwrap();
        prep.uploader(e.clone())
            .unwrap()
            .upload_tree()
            .await
            .unwrap();
        let publisher = prep.publisher(e, roots::Limits::default()).unwrap();
        publisher.check_semantics().await.unwrap();
        let client = local
            .preparations(plans::Options::default())
            .unwrap()
            .publications(roots::Limits::default())
            .unwrap();
        if i < 2 {
            publisher.begin(id(110 + u64::from(i))).await.unwrap();
            publisher.recovery().recover(prep.id()).await.unwrap();
        }
        machines.push((st, local, publisher, client, prep.id(), view));
    }
    let seq = info(&s.direct).await.sequence;
    let a = machines[0]
        .3
        .prepare_publish(machines[0].4, seq)
        .await
        .unwrap();
    let b = machines[1]
        .3
        .prepare_publish(machines[1].4, seq)
        .await
        .unwrap();
    s.proxy.clear();
    let ra = machines[0].2.recovery();
    let rb = machines[1].2.recovery();
    let (first, second) = tokio::join!(ra.recover(machines[0].4), rb.recover(machines[1].4));
    assert_ne!(first.is_ok(), second.is_ok());
    let operations = s.proxy.applies();
    assert_eq!(operations.len(), 2);
    assert!(operations.contains(a.publication().unwrap()));
    assert!(operations.contains(b.publication().unwrap()));
    let loser = if first.is_err() { 0 } else { 1 };
    let winner = 1 - loser;
    let rejected = machines[loser].3.get(machines[loser].4).await.unwrap();
    assert!(rejected.rejection().is_some());
    assert_eq!(
        rejected.rejection().unwrap().error(),
        s3drive_authority::Error::Conflict
    );
    assert!(rejected.receipt().is_none());
    let accepted = machines[winner].3.get(machines[winner].4).await.unwrap();
    assert!(accepted.receipt().is_some());
    assert_eq!(
        machines[loser]
            .1
            .head(machines[loser].5.binding().clone())
            .await
            .unwrap(),
        machines[loser].5
    );
    s.proxy.clear();
    assert!(machines[loser].2.resume().await.is_err());
    assert!(s.proxy.requests().is_empty());
    let result = machines[2].2.begin(id(112)).await;
    assert!(matches!(
        result
            .as_ref()
            .err()
            .and_then(|e| e.downcast_ref::<s3drive_managed::chunks::MixedRootFailure>()),
        Some(s3drive_managed::chunks::MixedRootFailure::BaseChanged)
    ));
    assert!(s.proxy.applies().is_empty());
    change(&s.direct, 70, Mutation::Remove { node: id(10) }).await;
    s.proxy.clear();
    let result = machines[2].2.begin(id(112)).await;
    assert!(matches!(
        result
            .as_ref()
            .err()
            .and_then(|e| e.downcast_ref::<s3drive_managed::chunks::MixedRootFailure>()),
        Some(s3drive_managed::chunks::MixedRootFailure::BaseChanged)
    ));
    assert!(s.proxy.applies().is_empty());
    for (st, _, _, client, plan, _) in &machines {
        if *plan == machines[2].4 {
            assert!(client.get(*plan).await.is_err());
        }
        st.close().await.unwrap();
    }
    seed.storage().close().await.unwrap();
}

pub async fn controls(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let seed = setup(config, root, s).await;
    let old = publisher(&seed, s).await;
    let volume = info(&s.direct).await.id;
    for mode in 0..3u8 {
        let n = 10 + u64::from(mode);
        if mode > 0 {
            create(s, n, &format!("root {mode} café 🦀.bin")).await;
        }
        let original = if mode == 0 {
            Vec::new()
        } else {
            (0..17).map(|x| x as u8).collect::<Vec<_>>()
        };
        let op = 50 + u64::from(mode) * 10;
        let (p, _) = plan(&seed, s, n, op, &original, None, 0).await;
        old.begin(p, id(op + 1)).await.unwrap();
        old.resume(p).await.unwrap();
        let st = storage(root, &format!("root-controls-{mode}")).await;
        let local = st.sparse_views(sparse_views::Options::default()).unwrap();
        let e = executor(config, st.clone(), false, false).await;
        let reader = Reader::open(s.wire.clone(), volume.clone(), vec![(id(200), e.reader())])
            .await
            .unwrap();
        let base = reader.open_file(&id(n)).await.unwrap().pinned_base();
        let initial = local
            .begin([1; 32], base.sparse_binding().unwrap(), base.size())
            .await
            .unwrap();
        let (view, expected) = match mode {
            0 => (
                local.resize([2; 32], initial.id(), 0).await.unwrap(),
                Vec::new(),
            ),
            1 => (
                local
                    .write([2; 32], initial.id(), 0, Arc::from(&[0u8; 17][..]))
                    .await
                    .unwrap(),
                vec![0; 17],
            ),
            _ => {
                let v = local.resize([2; 32], initial.id(), 7).await.unwrap();
                let v = local
                    .write([3; 32], v.id(), 0, Arc::from("café".as_bytes()))
                    .await
                    .unwrap();
                let mut expected = original[..7].to_vec();
                expected[..5].copy_from_slice("café".as_bytes());
                (v, expected)
            }
        };
        let edit = reader
            .reopen_edited(local.clone(), view.id())
            .await
            .unwrap();
        let prep = edit
            .begin_preparation([4; 32], [95 + mode; 16], plans::Options::default())
            .await
            .unwrap();
        prep.prepare().await.unwrap();
        let tree = prep
            .uploader(e.clone())
            .unwrap()
            .upload_tree()
            .await
            .unwrap();
        assert_eq!(tree.new_objects(), if mode == 2 { 2 } else { 1 });
        let publisher = prep.publisher(e, roots::Limits::default()).unwrap();
        publisher.check_semantics().await.unwrap();
        publisher.begin(id(1000 + u64::from(mode))).await.unwrap();
        let done = publisher.resume().await.unwrap();
        assert_eq!(done.view_id(), view.id());
        assert_eq!(done.metadata().node.id, id(n));
        let current = reader.open_file(&id(n)).await.unwrap();
        assert_eq!(current.size(), expected.len() as u64);
        assert_eq!(
            current.read_range(0, expected.len() + 1).await.unwrap(),
            expected
        );
        assert_eq!(local.head(view.binding().clone()).await.unwrap(), view);
        st.close().await.unwrap();
    }
    seed.storage().close().await.unwrap();
}

/// Consecutive edits keep their original authenticated source, with each new CAS
/// bound only to an actual prior local publication. No clean-state claim.
// Keep construction of this large test future out of the shared async match's
// poll frame. Every legacy scenario also uses that dispatcher on default stacks.
#[inline(never)]
pub fn lineage<'a>(
    config: &'a s3drive_core::Config,
    root: &'a Path,
    s: &'a Server,
) -> std::pin::Pin<Box<impl std::future::Future<Output = ()> + 'a>> {
    Box::pin(lineage_inner(config, root, s))
}
async fn lineage_inner(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let seed = setup(config, root, s).await;
    let old = publisher(&seed, s).await;
    let original = (0..32).map(|n| n as u8).collect::<Vec<_>>();
    let (p, _) = plan(&seed, s, 10, 40, &original, None, 0).await;
    old.begin(p, id(41)).await.unwrap();
    old.resume(p).await.unwrap();
    let volume = info(&s.direct).await.id;
    let mut binding: Option<sparse_views::Binding> = None;
    let mut previous = None;
    let mut expected = original;
    let mut future = None;
    for round in 0..4u8 {
        let st = storage(root, "lineage").await;
        let local = st.sparse_views(sparse_views::Options::default()).unwrap();
        let e = executor(config, st.clone(), false, false).await;
        let reader = Reader::open(s.wire.clone(), volume.clone(), vec![(id(200), e.reader())])
            .await
            .unwrap();
        let parent = if let Some(b) = &binding {
            local.head(b.clone()).await.unwrap()
        } else {
            let base = reader.open_file(&id(10)).await.unwrap().pinned_base();
            let b = base.sparse_binding().unwrap();
            binding = Some(b.clone());
            local.begin([1; 32], b, base.size()).await.unwrap()
        };
        let view = if round == 2 {
            assert_eq!(parent.id(), future.take().unwrap());
            parent
        } else {
            let bytes = if round == 0 {
                "café".as_bytes()
            } else {
                "日本".as_bytes()
            };
            let offset = 2 + usize::from(round) * 5;
            expected[offset..offset + bytes.len()].copy_from_slice(bytes);
            local
                .write(
                    [10 + round; 32],
                    parent.id(),
                    offset as u64,
                    Arc::from(bytes),
                )
                .await
                .unwrap()
        };
        let edit = reader
            .reopen_edited(local.clone(), view.id())
            .await
            .unwrap();
        let prep = edit
            .begin_preparation(
                [30 + round; 32],
                [130 + round; 16],
                plans::Options::default(),
            )
            .await
            .unwrap();
        prep.prepare().await.unwrap();
        assert_eq!(
            prep.uploader(e.clone())
                .unwrap()
                .upload_tree()
                .await
                .unwrap()
                .new_objects(),
            2
        );
        let publisher = prep.publisher(e, roots::Limits::default()).unwrap();
        publisher.check_semantics().await.unwrap();
        if round == 3 {
            let rival = b"other machine retained its bytes";
            let (p, _) = plan(&seed, s, 10, 70, rival, None, 0).await;
            old.begin(p, id(71)).await.unwrap();
            old.resume(p).await.unwrap();
            s.proxy.clear();
            let error = publisher.begin(id(1103)).await.err().unwrap();
            assert_eq!(
                error.downcast_ref::<s3drive_managed::chunks::MixedRootFailure>(),
                Some(&s3drive_managed::chunks::MixedRootFailure::BaseChanged)
            );
            assert!(s.proxy.applies().is_empty());
            assert_eq!(local.head(binding.clone().unwrap()).await.unwrap(), view);
            assert_eq!(
                reader
                    .open_file(&id(10))
                    .await
                    .unwrap()
                    .read_range(0, 64)
                    .await
                    .unwrap(),
                rival
            );
            st.close().await.unwrap();
            break;
        }
        let input = publisher.begin(id(1100 + u64::from(round))).await.unwrap();
        assert_eq!(input.input().predecessor(), previous);
        let expected_head = if round == 1 {
            let more = "🦀".as_bytes();
            let next = local
                .write([90; 32], view.id(), 24, Arc::from(more))
                .await
                .unwrap();
            future = Some(next.id());
            next
        } else {
            view.clone()
        };
        let done = if round == 1 {
            s.proxy.mode.store(2, Ordering::SeqCst);
            assert!(publisher.resume().await.is_err());
            let client = local
                .preparations(plans::Options::default())
                .unwrap()
                .publications(roots::Limits::default())
                .unwrap();
            let saved = client.get(prep.id()).await.unwrap();
            assert!(saved.receipt().is_none());
            assert_eq!(
                local.head(binding.clone().unwrap()).await.unwrap(),
                expected_head
            );
            st.close().await.unwrap();
            let recovery = MixedRootRecovery::open_existing(
                &root.join("lineage"),
                s.wire.clone(),
                volume.clone(),
                id(200),
            )
            .await
            .unwrap();
            s.proxy.clear();
            let done = recovery.recover(prep.id()).await.unwrap();
            only_exact_apply(s, saved.publication().unwrap());
            s.proxy.clear();
            assert_eq!(recovery.recover(prep.id()).await.unwrap(), done);
            assert!(s.proxy.requests().is_empty());
            recovery.close().await.unwrap();
            let reopened = storage(root, "lineage").await;
            assert_eq!(
                reopened
                    .sparse_views(sparse_views::Options::default())
                    .unwrap()
                    .head(binding.clone().unwrap())
                    .await
                    .unwrap(),
                expected_head
            );
            reopened.close().await.unwrap();
            done
        } else {
            let done = publisher.resume().await.unwrap();
            s.proxy.clear();
            assert_eq!(
                publisher.resume().await.unwrap().metadata(),
                done.metadata()
            );
            assert!(s.proxy.requests().is_empty());
            assert_eq!(
                local.head(binding.clone().unwrap()).await.unwrap(),
                expected_head
            );
            let r = done.record().clone();
            st.close().await.unwrap();
            r
        };
        assert_eq!(done.input().view(), view.id());
        assert_eq!(
            reader
                .open_file(&id(10))
                .await
                .unwrap()
                .read_range(0, 64)
                .await
                .unwrap(),
            expected
        );
        if round == 1 {
            expected[24..28].copy_from_slice("🦀".as_bytes());
        }
        previous = Some(prep.id());
        if round == 0 {
            change(
                &s.direct,
                60,
                Mutation::Move {
                    node: id(10),
                    parent: s3drive_authority::ROOT.into(),
                    name: "renamed 日本語 🦀.bin".into(),
                },
            )
            .await;
        }
    }
    seed.storage().close().await.unwrap();
}
