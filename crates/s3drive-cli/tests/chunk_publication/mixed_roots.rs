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
    let physical = prep
        .uploader(e.clone())
        .unwrap()
        .upload_tree()
        .await
        .unwrap();
    assert_eq!(physical.new_objects(), 3);
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
    let narrow = executor(config, st.clone(), true, false).await;
    let denied = prep.publisher(narrow, roots::Limits::default()).unwrap();
    s.proxy.clear();
    assert!(denied.resume().await.is_err());
    assert!(
        s.proxy.applies().is_empty(),
        "fresh publish bypassed narrowed policy"
    );
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
