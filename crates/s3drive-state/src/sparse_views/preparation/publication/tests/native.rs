//! Durable protocol fixtures only. Modeled receipts are not SDK/native acceptance.
use super::*;
use crate::sparse_views::preparation::native::{Client, Context, Token};
use crate::worker::Storage;
struct Native {
    storage: Storage,
    client: Client,
    key: crate::chunk_mount::Key,
    token: Token,
    view: View,
}
async fn setup(path: &std::path::Path, plans: usize) -> Native {
    let storage = Storage::open(
        path,
        crate::cache::Options::default(),
        crate::journal::Options::default(),
    )
    .await
    .unwrap();
    let owner = storage.cache().owner;
    let (key, input, view) = owner
        .call(|store| {
            let s = &mut store.state;
            let f = fixture();
            let key = chunk(s, &f);
            let input = open_with_posix(s, &f, &key, true);
            views::scoped::enable(s)?;
            let view = views::Journal::new(s, views::Options::default())?.begin_scoped(
                views::ScopedOperation::new(&f.binding, [1; 32])?,
                f.binding,
                17,
            )?;
            Ok((key, input, view))
        })
        .await
        .unwrap();
    let client = Client::new(
        owner,
        key.clone(),
        views::Options::default(),
        Options {
            plans,
            ..Options::default()
        },
    )
    .unwrap();
    let token = client.bind(input.id).await.unwrap();
    Native {
        storage,
        client,
        key,
        token,
        view,
    }
}
fn context(view: &View) -> Context {
    let f = fixture();
    Context::new(view, f.base.root.encode().to_vec(), f.base.page.encode()).unwrap()
}
async fn second(n: &Native) -> Token {
    let mut input = n.token.input().clone();
    input.id = hex(&[69; 16]);
    let key = n.key.clone();
    let id = n
        .storage
        .cache()
        .owner
        .call(move |store| {
            let mut journal = opens(&mut store.state, &key);
            journal.prepare(input.clone())?;
            journal.activate(&input.id, &hex(&[70; 16]))?;
            Ok(input.id)
        })
        .await
        .unwrap();
    n.client.bind(id).await.unwrap()
}
async fn dirty(n: &Native, view: &View, nonce: u8, offset: u64) -> View {
    n.client
        .write(
            n.token.clone(),
            context(view),
            [nonce; 32],
            offset,
            vec![255],
        )
        .await
        .unwrap()
}
async fn record(n: &Native, token: &Token) -> opens::OpenRecord {
    let id = token.input().id.clone();
    n.storage
        .cache()
        .owner
        .call(move |store| opens::sparse_open(&store.state.connection, &id))
        .await
        .unwrap()
}

#[tokio::test]
async fn native_sparse_atomic_close_captures_latest_multi_handle_head_without_lineage_adoption() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("native café 🦀"), 16).await;
    let other = second(&n).await;
    let first = dirty(&n, &n.view, 2, 5).await;
    let last = n
        .client
        .write(other.clone(), context(&first), [3; 32], 6, vec![254])
        .await
        .unwrap();
    n.token.observe_close();
    assert!(
        n.client
            .write(other.clone(), context(&last), [4; 32], 7, vec![253])
            .await
            .is_err()
    );
    assert!(record(&n, &n.token).await.close().is_none());
    let closed = close_native(&n, &n.token, 71).await.unwrap();
    assert_eq!(closed.plans.len(), 1);
    assert_eq!(closed.plans[0].view().id(), last.id());
    assert_eq!(
        closed.plans[0].native_ownership().unwrap().open(),
        n.token.input().id
    );
    assert!(closed.plans[0].ownership().is_none());
    assert!(closed.open.close().is_some());
    // A known exact answer is not adoption or fresh admission. Its original
    // publisher and frozen plan identity remain unchanged for every handle.
    let replay = n
        .client
        .prepare(other.clone(), context(&last))
        .await
        .unwrap();
    assert_eq!(replay.id().unwrap(), closed.plans[0].id().unwrap());
    assert_eq!(replay.view(), closed.plans[0].view());
    assert_eq!(replay.cursor(), closed.plans[0].cursor());
    assert_eq!(
        replay.native_ownership().unwrap().open(),
        n.token.input().id
    );
    assert!(n.client.prepare(other, context(&first)).await.is_err());
    assert!(
        n.client
            .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
            .await
            .is_err()
    );
    assert_eq!(
        n.client
            .close(n.token.clone(), hex(&[71; 16]))
            .await
            .unwrap()
            .plans[0]
            .id()
            .unwrap(),
        closed.plans[0].id().unwrap()
    );
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_nonpublisher_close_blocks_mutation_but_does_not_capture_another_open() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("nonpublisher"), 16).await;
    let other = second(&n).await;
    let view = dirty(&n, &n.view, 2, 5).await;
    let closed = close_native(&n, &other, 71).await.unwrap();
    assert!(closed.plans.is_empty());
    assert!(
        n.client
            .complete(other, hex(&[71; 16]), hex(&[72; 16]))
            .await
            .is_err()
    );
    assert!(
        n.client
            .write(n.token.clone(), context(&view), [3; 32], 6, vec![1])
            .await
            .is_err()
    );
    let plan = n
        .client
        .prepare(n.token.clone(), context(&view))
        .await
        .unwrap();
    assert_eq!(plan.native_ownership().unwrap().open(), n.token.input().id);
    let closed = close_native(&n, &n.token, 73).await.unwrap();
    assert_eq!(closed.plans.len(), 1);
    assert_eq!(closed.plans[0].id().unwrap(), plan.id().unwrap());
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_failed_final_admission_preserves_close_fact_without_frozen_permission() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("quota"), 1).await;
    let first = dirty(&n, &n.view, 2, 5).await;
    let plan = n
        .client
        .prepare(n.token.clone(), context(&first))
        .await
        .unwrap();
    let last = dirty(&n, &first, 3, 6).await;
    assert!(
        n.client
            .close(n.token.clone(), hex(&[71; 16]))
            .await
            .is_err()
    );
    let saved = record(&n, &n.token).await;
    assert!(saved.close().is_some());
    assert!(
        !saved
            .events
            .iter()
            .any(|e| matches!(e.action, opens::Action::FreezeSparse { .. }))
    );
    let retained = n.client.plans(n.token.input().node.clone()).await.unwrap();
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].id().unwrap(), plan.id().unwrap());
    assert!(
        n.client
            .write(n.token.clone(), context(&last), [4; 32], 7, vec![1])
            .await
            .is_err()
    );
    let id = plan.id().unwrap();
    assert!(
        n.storage
            .cache()
            .owner
            .call(move |store| {
                let info = pj(&mut store.state).info(id)?;
                crate::sparse_views::preparation::owned::fresh(
                    &store.state.connection,
                    store.state.incarnation.as_deref(),
                    &store.state.native_sparse,
                    &info,
                )
            })
            .await
            .is_err()
    );
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_clean_close_requires_atomic_witness_and_blocks_generic_completion() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("clean"), 16).await;
    let closed = close_native(&n, &n.token, 71).await.unwrap();
    assert!(closed.plans.is_empty());
    let key = n.key.clone();
    let input = n.token.input().clone();
    assert!(
        n.storage
            .cache()
            .owner
            .call(move |store| {
                opens(&mut store.state, &key).complete(&input.id, &hex(&[72; 16]), &hex(&[71; 16]))
            })
            .await
            .is_err()
    );
    let completed = n
        .client
        .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
        .await
        .unwrap();
    assert!(completed.completed(&hex(&[71; 16])));
    assert!(completed.events.iter().any(|e| matches!(
        e.action,
        opens::Action::CompleteSparse { view: Some(_), .. }
    )));
    assert!(
        n.client
            .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
            .await
            .unwrap()
            .completed(&hex(&[71; 16]))
    );
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_cold_frozen_history_does_not_restore_runtime_or_fresh_work() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("cold");
    let n = setup(&path, 16).await;
    dirty(&n, &n.view, 2, 5).await;
    let closed = close_native(&n, &n.token, 71).await.unwrap();
    let plan = closed.plans[0].id().unwrap();
    n.storage.close().await.unwrap();
    let storage = Storage::open(
        &path,
        crate::cache::Options::default(),
        crate::journal::Options::default(),
    )
    .await
    .unwrap();
    let client = Client::new(
        storage.cache().owner,
        n.key,
        views::Options::default(),
        Options::default(),
    )
    .unwrap();
    assert!(client.bind(n.token.input().id.clone()).await.is_err());
    assert!(
        client
            .write(
                n.token.clone(),
                context(closed.plans[0].view()),
                [3; 32],
                6,
                vec![1]
            )
            .await
            .is_err()
    );
    assert!(
        storage
            .cache()
            .owner
            .call(move |store| {
                let info = pj(&mut store.state).info(plan)?;
                crate::sparse_views::preparation::owned::fresh(
                    &store.state.connection,
                    store.state.incarnation.as_deref(),
                    &store.state.native_sparse,
                    &info,
                )
            })
            .await
            .is_err()
    );
    assert_eq!(
        client
            .plans(n.token.input().node.clone())
            .await
            .unwrap()
            .len(),
        1
    );
    storage.close().await.unwrap();
}

async fn close_native(
    n: &Native,
    token: &Token,
    callback: u8,
) -> Result<crate::sparse_views::preparation::native::Close> {
    token.observe_close();
    n.client.close(token.clone(), hex(&[callback; 16])).await
}

#[tokio::test]
async fn native_sparse_close_completion_requires_exact_source_receipt_not_staged_root() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("source receipt"), 16).await;
    dirty(&n, &n.view, 2, 5).await;
    let closed = close_native(&n, &n.token, 71).await.unwrap();
    let plan = closed.plans[0].id().unwrap();
    assert!(
        n.client
            .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
            .await
            .is_err()
    );
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let s = &mut store.state;
            let info = pj(s).info(plan)?;
            let f = fixture();
            let mut bytes = f.bytes.clone();
            bytes[5] = 255;
            let info = pj(s).append(plan, info.cursor(), Candidate::Bytes(bytes))?;
            let info = crate::sparse_views::preparation::tests::fixture::finish(s, info);
            assert!(info.root()?.is_some());
            j(s).check(plan, 0, Some(&proof(&f)))?;
            physical(s, plan);
            let root = j(s).begin(plan, f.node, f.store, 5, hex(&[10; 16]))?;
            let root = j(s).reserved(plan, reserved(&root))?;
            let root = j(s).prepare_publish(plan, root.reservation().unwrap().sequence)?;
            j(s).published(plan, published(&root))?;
            Ok(())
        })
        .await
        .unwrap();
    // Even a saved successful Root outcome cannot stand in for local source
    // acknowledgement, and another callback's completion is never adopted.
    assert!(
        n.client
            .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
            .await
            .is_err()
    );
    n.storage
        .cache()
        .owner
        .call(move |store| {
            j(&mut store.state).acknowledge_scoped(
                views::ScopedOperation::new(&fixture().binding, [11; 32])?,
                plan,
                views::Options::default(),
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let done = n
        .client
        .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
        .await
        .unwrap();
    assert!(done.completed(&hex(&[71; 16])));
    assert_eq!(
        n.client
            .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
            .await
            .unwrap(),
        done
    );
    assert!(
        n.client
            .complete(n.token.clone(), hex(&[71; 16]), hex(&[73; 16]))
            .await
            .is_err()
    );
    let key = n.key.clone();
    let open = n.token.input().id.clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            opens(&mut store.state, &key).retire(&open)?;
            crate::chunk_mount::require(&store.state.connection, &key)?;
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(
        n.client
            .plans(n.token.input().node.clone())
            .await
            .unwrap()
            .len(),
        1
    );
    n.storage.close().await.unwrap();
    // Retiring live capacity must not erase the publisher's historical proof.
    drop(State::open_managed(&temp.path().join("source receipt")).unwrap());
}

#[tokio::test]
async fn native_sparse_epoch_context_cannot_change_plain_or_domain_separated_page_identity() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("context identities"), 16).await;
    let view = dirty(&n, &n.view, 2, 5).await;
    let mut input = n.token.input().clone();
    input.id = hex(&[67; 16]);
    input.node = hex(&[68; 16]);
    let key = n.key.clone();
    let open = n
        .storage
        .cache()
        .owner
        .call(move |store| {
            let mut journal = opens(&mut store.state, &key);
            journal.prepare(input.clone())?;
            journal.activate(&input.id, &hex(&[69; 16]))?;
            Ok(input.id)
        })
        .await
        .unwrap();
    let other_node = n.client.bind(open).await.unwrap();
    assert!(n.client.prepare(other_node, context(&view)).await.is_err());
    let foreign = base(17, BTreeMap::from([(0, vec![4; 17])]));
    let context =
        Context::new(&view, foreign.root.encode().to_vec(), foreign.page.encode()).unwrap();
    assert!(
        n.client
            .prepare(n.token.clone(), context.clone())
            .await
            .is_err()
    );
    assert!(
        n.client
            .write(n.token.clone(), context, [3; 32], 6, vec![254])
            .await
            .is_err()
    );
    assert!(
        n.client
            .plans(n.token.input().node.clone())
            .await
            .unwrap()
            .is_empty()
    );
    let closed = close_native(&n, &n.token, 71).await.unwrap();
    assert_eq!(closed.plans.len(), 1);
    assert_eq!(closed.plans[0].view().id(), view.id());
    let input = &closed.plans[0].input;
    assert_eq!(input.base_page, digest(&fixture().base.page.encode()));
    assert_ne!(input.base_page, *fixture().base.root.page().sha256());
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_cold_namespace_metadata_never_reads_or_repairs_dirty_bodies() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("metadata café 🦀");
    let n = setup(&path, 16).await;
    dirty(&n, &n.view, 2, 5).await;
    let closed = close_native(&n, &n.token, 71).await.unwrap();
    let id = n.token.input().id.clone();
    let volume = n.token.input().volume.clone();
    n.storage.close().await.unwrap();
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.execute(
            "UPDATE sparse_view_blocks SET data=zeroblob(length(data))",
            []
        )
        .unwrap(),
        1
    );
    let corrupt: Vec<u8> = db
        .query_row("SELECT data FROM sparse_view_blocks", [], |row| row.get(0))
        .unwrap();
    drop(db);
    assert!(State::open_managed(&path).is_err());
    let mut recovery =
        crate::namespace_client::recovery::RecoveryStore::open(&path, &volume).unwrap();
    assert_eq!(recovery.open_record(&id).unwrap(), closed.open);
    drop(recovery);
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    let after: Vec<u8> = db
        .query_row("SELECT data FROM sparse_view_blocks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(after, corrupt);
    drop(db);
    assert!(State::open_managed(&path).is_err());
}

#[tokio::test]
async fn native_sparse_crash_child() {
    let Ok(kind) = std::env::var("S3DRIVE_NATIVE_SPARSE_CHILD") else {
        return;
    };
    let path = std::path::PathBuf::from(std::env::var_os("S3DRIVE_NATIVE_SPARSE_DIR").unwrap());
    let n = setup(&path, if kind == "native-failed-close" { 1 } else { 16 }).await;
    if kind == "native-complete" {
        close_native(&n, &n.token, 71).await.unwrap();
        n.client
            .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
            .await
            .unwrap();
    } else {
        let view = dirty(&n, &n.view, 2, 5).await;
        if kind == "native-failed-close" {
            n.client
                .prepare(n.token.clone(), context(&view))
                .await
                .unwrap();
            dirty(&n, &view, 3, 6).await;
        }
        if kind != "native-edit" {
            let _ = close_native(&n, &n.token, 71).await;
        }
    }
    panic!("expected native transaction checkpoint was not reached");
}

#[test]
fn native_sparse_killed_owners_preserve_atomic_epoch_close_and_completion() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    for (kind, points) in [
        (
            "native-edit",
            &[
                "BlockStored",
                "ViewStored",
                "OperationStored",
                "HeadUpdated",
                "EpochStored",
                "Validated",
                "Committed",
            ][..],
        ),
        (
            "native-close",
            &[
                "PlanStored",
                "CloseStored",
                "FreezeStored",
                "Validated",
                "Committed",
            ][..],
        ),
        (
            "native-complete",
            &["WitnessStored", "Validated", "Committed"][..],
        ),
        ("native-failed-close", &["FactCommitted"][..]),
    ] {
        for point in points {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("native crash café 🦀");
            let ready = temp.path().join("ready");
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "sparse_views::preparation::publication::tests::barriers::native::native_sparse_crash_child", "--nocapture"])
                .env("S3DRIVE_NATIVE_SPARSE_CHILD", kind).env("S3DRIVE_NATIVE_SPARSE_DIR", &path)
                .env("S3DRIVE_MIXED_CRASH_KIND", if kind == "native-failed-close" { "native-close" } else { kind })
                .env("S3DRIVE_MIXED_CRASH_POINT", point).env("S3DRIVE_MIXED_CRASH_READY", &ready)
                .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
            let start = Instant::now();
            while !ready.exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    use std::io::Read;
                    let mut error = String::new();
                    child
                        .stderr
                        .take()
                        .unwrap()
                        .read_to_string(&mut error)
                        .unwrap();
                    panic!("{kind}/{point}: {status}: {error}");
                }
                if start.elapsed() > Duration::from_secs(45) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("{kind}/{point}: checkpoint timeout");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(State::open_managed(&path).is_err());
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            let mut state = State::open_managed(&path).unwrap();
            let records = opens::sparse_records(&state.connection).unwrap();
            assert_eq!(records.len(), 1);
            let original = &records[0];
            let plans =
                crate::sparse_views::preparation::ledger::inspect(&state.connection).unwrap();
            let head = views::Journal::new(&mut state, views::Options::default())
                .unwrap()
                .current_head(&fixture().binding)
                .unwrap();
            let committed = *point == "Committed";
            if kind == "native-edit" {
                assert_eq!(head.is_clean(), !committed, "{kind}/{point}");
                assert_eq!(
                    original
                        .events
                        .iter()
                        .filter(|e| matches!(e.action, opens::Action::OwnSparseEpoch { .. }))
                        .count(),
                    usize::from(committed)
                );
                assert!(original.close().is_none());
                assert!(plans.plans.is_empty());
            } else if kind == "native-complete" {
                assert!(head.is_clean());
                assert!(original.close().is_some());
                assert_eq!(original.completed(&hex(&[71; 16])), committed);
                assert_eq!(
                    original
                        .events
                        .iter()
                        .filter(|e| matches!(e.action, opens::Action::CompleteSparse { .. }))
                        .count(),
                    usize::from(committed)
                );
            } else {
                let failed = kind == "native-failed-close";
                assert!(!head.is_clean());
                assert_eq!(original.close().is_some(), committed || failed);
                assert_eq!(plans.plans.len(), usize::from(committed || failed));
                assert_eq!(
                    original
                        .events
                        .iter()
                        .filter(|e| matches!(e.action, opens::Action::FreezeSparse { .. }))
                        .count(),
                    usize::from(committed)
                );
                for id in plans.plans.keys() {
                    let info = plans.info(*id).unwrap();
                    assert_eq!(info.view().id() == head.id(), !failed);
                    assert!(
                        crate::sparse_views::preparation::owned::fresh(
                            &state.connection,
                            state.incarnation.as_deref(),
                            &state.native_sparse,
                            &info
                        )
                        .is_err()
                    );
                }
            }
        }
    }
}

async fn reopen_native(path: &std::path::Path, old: &Native, recover: bool) -> Native {
    let storage = Storage::open(
        path,
        crate::cache::Options::default(),
        crate::journal::Options::default(),
    )
    .await
    .unwrap();
    let key = old.key.clone();
    let mut input = old.token.input().clone();
    input.id = hex(&[76; 16]);
    input.session = hex(&[75; 16]);
    let (input, view) = storage
        .cache()
        .owner
        .call({
            let key = key.clone();
            move |store| {
                let s = &mut store.state;
                let mut journal = opens(s, &key);
                journal.start_session(&input.session, &input.volume)?;
                if recover {
                    let proof = journal.prepare_recovery(
                        &input.session,
                        &input.volume,
                        &input.node,
                        None,
                    )?;
                    assert!(
                        journal
                            .finish_recovery(&input.session, &proof.input.id)?
                            .finished
                    );
                    for old in &proof.input.opens {
                        journal.retire(&old.id)?;
                    }
                }
                if !recover {
                    // No current open exists yet: the old unprepared dirty
                    // epoch itself must prevent abandonment.
                    assert!(
                        journal
                            .prepare_recovery(&input.session, &input.volume, &input.node, None)
                            .is_err()
                    );
                }
                if recover {
                    journal.prepare(input.clone())?;
                    journal.activate(&input.id, &hex(&[77; 16]))?;
                } else {
                    assert!(journal.prepare(input.clone()).is_err());
                }
                let view = views::Journal::new(s, views::Options::default())?
                    .current_head(&fixture().binding)?;
                Ok((input, view))
            }
        })
        .await
        .unwrap();
    let client = Client::new(
        storage.cache().owner,
        key.clone(),
        views::Options::default(),
        Options::default(),
    )
    .unwrap();
    let token = if recover {
        client.bind(input.id).await.unwrap()
    } else {
        assert!(client.bind(old.token.input().id.clone()).await.is_err());
        old.token.clone() // Deliberately stale capability for refusal assertions only.
    };
    Native {
        storage,
        client,
        key,
        token,
        view,
    }
}

#[tokio::test]
async fn native_sparse_cold_clean_session_retirement_does_not_fabricate_close_completion() {
    for windows in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("clean remount");
        let old = setup(&path, 16).await;
        if windows {
            let key = old.key.clone();
            let id = old.token.input().id.clone();
            old.storage
                .cache()
                .owner
                .call(move |store| {
                    let mut journal = opens(&mut store.state, &key);
                    journal.callback(
                        &id,
                        &hex(&[81; 16]),
                        opens::Callback::Cleanup { flags: 0 },
                    )?;
                    journal.complete(&id, &hex(&[82; 16]), &hex(&[81; 16]))?;
                    Ok(())
                })
                .await
                .unwrap();
        }
        close_native(&old, &old.token, 71).await.unwrap();
        old.storage.close().await.unwrap();
        let n = reopen_native(&path, &old, true).await;
        let first = dirty(&n, &n.view, 2, 5).await;
        let last = dirty(&n, &first, 3, 6).await;
        let original = record(&n, &old.token).await;
        assert!(original.close().is_some());
        assert!(!original.completed(&hex(&[71; 16])));
        assert_eq!(original.cleanup().is_some(), windows);
        let key = n.key.clone();
        let id = original.input.id;
        n.storage
            .cache()
            .owner
            .call(move |store| {
                let mut journal = opens(&mut store.state, &key);
                assert!(journal.recovered(&id)?);
                assert!(
                    journal
                        .complete(&id, &hex(&[72; 16]), &hex(&[71; 16]))
                        .is_err()
                );
                Ok(())
            })
            .await
            .unwrap();
        let plan = n
            .client
            .prepare(n.token.clone(), context(&last))
            .await
            .unwrap();
        assert_eq!(plan.native_ownership().unwrap().open(), n.token.input().id);
        n.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn native_sparse_cold_unprepared_dirty_epoch_cannot_be_abandoned_or_adopted() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("unprepared remount");
    let old = setup(&path, 16).await;
    let retained = dirty(&old, &old.view, 2, 5).await;
    old.storage.close().await.unwrap();
    let n = reopen_native(&path, &old, false).await;
    assert_eq!(n.view.id(), retained.id());
    let key = n.key.clone();
    let input = n.token.input().clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            assert!(
                opens(&mut store.state, &key)
                    .prepare_recovery(&input.session, &input.volume, &input.node, None)
                    .is_err()
            );
            Ok(())
        })
        .await
        .unwrap();
    assert!(
        n.client
            .write(n.token.clone(), context(&retained), [3; 32], 6, vec![254])
            .await
            .is_err()
    );
    assert!(
        n.client
            .prepare(n.token.clone(), context(&retained))
            .await
            .is_err()
    );
    assert!(
        n.client
            .plans(n.token.input().node.clone())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(record(&n, &old.token).await.close().is_none());
    let current = n
        .storage
        .cache()
        .owner
        .call(|store| {
            views::Journal::new(&mut store.state, views::Options::default())?
                .current_head(&fixture().binding)
        })
        .await
        .unwrap();
    assert_eq!(current, retained);
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_actual_sqlite_full_rolls_back_epoch_and_close_without_losing_callback_identity()
 {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("state"), 16).await;
    async fn pressure(n: &Native, enable: bool) {
        n.storage
            .cache()
            .owner
            .call(move |store| {
                let db = &store.state.connection;
                let pages: i64 = db.pragma_query_value(None, "page_count", |r| r.get(0))?;
                if enable {
                    db.execute_batch("CREATE TABLE native_sparse_full_probe(data BLOB)")?;
                    let pages: i64 = db.pragma_query_value(None, "page_count", |r| r.get(0))?;
                    db.pragma_update(None, "max_page_count", pages)?;
                    // The test-only event-boundary probe requests actual pages
                    // inside the transaction. Trigger rejection remains intact.
                } else {
                    db.pragma_update(None, "max_page_count", pages + 1024)?;
                    db.execute_batch("DROP TABLE native_sparse_full_probe")?;
                }
                Ok(())
            })
            .await
            .unwrap();
    }
    fn full(error: Box<dyn std::error::Error + Send + Sync>) {
        assert!(
            matches!(error.downcast_ref::<rusqlite::Error>(), Some(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::DiskFull),
            "{error}"
        );
    }
    let before = record(&n, &n.token).await;
    pressure(&n, true).await;
    full(
        n.client
            .write(n.token.clone(), context(&n.view), [2; 32], 5, vec![255])
            .await
            .unwrap_err(),
    );
    assert_eq!(record(&n, &n.token).await, before);
    let key = n.key.clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let _ = crate::chunk_mount::require(&store.state.connection, &key)?;
            let head = views::Journal::new(&mut store.state, views::Options::default())?
                .current_head(&fixture().binding)?;
            assert!(head.is_clean());
            Ok(())
        })
        .await
        .unwrap();
    pressure(&n, false).await;
    let view = dirty(&n, &n.view, 3, 5).await;
    let before = record(&n, &n.token).await;
    pressure(&n, true).await;
    full(close_native(&n, &n.token, 71).await.unwrap_err());
    assert_eq!(record(&n, &n.token).await, before);
    assert!(
        n.client
            .plans(n.token.input().node.clone())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        n.client
            .write(n.token.clone(), context(&view), [4; 32], 6, vec![1])
            .await
            .is_err()
    );
    pressure(&n, false).await;
    assert!(close_native(&n, &n.token, 72).await.is_err());
    let closed = close_native(&n, &n.token, 71).await.unwrap();
    assert_eq!(closed.plans.len(), 1);
    assert_eq!(closed.plans[0].view.id(), view.id());
    assert!(
        n.client
            .complete(n.token.clone(), hex(&[71; 16]), hex(&[73; 16]))
            .await
            .is_err()
    );
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_schema47_marker_cannot_launder_native_event_or_plan() {
    for prepare in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("native schema marker");
        let n = setup(&path, 16).await;
        if prepare {
            let view = dirty(&n, &n.view, 2, 5).await;
            n.client
                .prepare(n.token.clone(), context(&view))
                .await
                .unwrap();
        }
        n.storage.close().await.unwrap();
        let db = rusqlite::Connection::open(path.join("metadata.sqlite3")).unwrap();
        db.pragma_update(None, "user_version", 47).unwrap();
        drop(db);
        let bytes = || -> BTreeMap<_, _> {
            std::fs::read_dir(&path)
                .unwrap()
                .map(|p| p.unwrap().path())
                .filter(|p| {
                    p.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("metadata.sqlite3")
                })
                .map(|p| (p.file_name().unwrap().to_owned(), std::fs::read(p).unwrap()))
                .collect()
        };
        let before = bytes();
        assert!(State::open_managed(&path).is_err());
        assert_eq!(bytes(), before);
        assert!(
            crate::namespace_client::recovery::RecoveryStore::open(&path, &hex(&[1; 16])).is_err()
        );
        assert_eq!(bytes(), before);
    }
}

#[tokio::test]
async fn native_sparse_generic_mutation_and_preparation_cannot_bypass_original_open() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("state"), 16).await;
    let view = dirty(&n, &n.view, 2, 5).await;
    let v = view.clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let f = fixture();
            let s = &mut store.state;
            let token = views::ScopedOperation::new(&f.binding, [60; 32])?;
            assert!(
                views::Journal::new(s, views::Options::default())?
                    .write_scoped(token, v.id(), 6, &[1])
                    .is_err()
            );
            assert!(pj(s).begin(request(&f, &v, 61)).is_err());
            assert_eq!(
                views::Journal::new(s, views::Options::default())?
                    .current_head(&f.binding)?
                    .id(),
                v.id()
            );
            Ok(())
        })
        .await
        .unwrap();
    close_native(&n, &n.token, 71).await.unwrap();
    assert!(
        n.client
            .prepare(n.token.clone(), context(&view))
            .await
            .is_err()
    );
    assert!(
        n.client
            .resize(n.token.clone(), context(&view), [62; 32], 5)
            .await
            .is_err()
    );
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_sealed_posix_retirement_releases_token_not_historical_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("state"), 16).await;
    let other = second(&n).await;
    close_native(&n, &n.token, 71).await.unwrap();
    n.client
        .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
        .await
        .unwrap();
    let before = record(&n, &n.token).await;
    assert!(before.cleanup().is_none());
    let key = n.key.clone();
    let id = n.token.input().id.clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            opens(&mut store.state, &key).retire(&id)?;
            store
                .state
                .native_sparse
                .prune_retired(&store.state.connection)?;
            assert!(opens::sparse_retired(&store.state.connection, &id)?);
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(record(&n, &n.token).await, before);
    assert!(n.client.bind(n.token.input().id.clone()).await.is_err());
    let view = n
        .client
        .write(other, context(&n.view), [3; 32], 5, vec![255])
        .await
        .unwrap();
    assert!(!view.is_clean());
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_frozen_owned_plans_keep_original_open_and_exact_removal_claim() {
    for committed in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let n = setup(&temp.path().join("state"), 16).await;
        let key = n.key.clone();
        let claim = n
            .storage
            .cache()
            .owner
            .call(move |store| {
                let s = &mut store.state;
                let f = fixture();
                let mut input = removal(&f, &f.node, 80);
                let ns::Mutation::RemoveEntry { node, parent } = input.request.mutation.clone()
                else {
                    panic!()
                };
                input.request.mutation = ns::Mutation::ArmRemoval {
                    node,
                    parent,
                    owner: key.owner().into(),
                };
                names(s, &key).begin(input.clone())?;
                let fence = ns::RemovalFence {
                    intent: input.request.operation.clone(),
                    owner: key.owner().into(),
                };
                let mut node = f.node.clone();
                node.revision = 6;
                names(s, &key).acknowledge(
                    &input.request.operation,
                    Ok(ns::Receipt {
                        sequence: 6,
                        node: node.clone(),
                        artifact: Some(ns::Artifact::Removal(fence.clone())),
                    }),
                )?;
                let mut claim = ns::RemovalPublicationClaim {
                    fence: fence.clone(),
                    previous: input.request.operation,
                    mode: ns::RemovalPublicationMode::Pending,
                };
                if committed {
                    let mut input = removal(&f, &node, 81);
                    let ns::Mutation::RemoveEntry {
                        node: guard,
                        parent,
                    } = input.request.mutation.clone()
                    else {
                        panic!()
                    };
                    input.request.mutation = ns::Mutation::CommitRemoval {
                        node: guard,
                        parent,
                        fence: fence.clone(),
                    };
                    names(s, &key).begin(input.clone())?;
                    node.revision = 7;
                    node.deleted = true;
                    names(s, &key).acknowledge(
                        &input.request.operation,
                        Ok(ns::Receipt {
                            sequence: 7,
                            node,
                            artifact: Some(ns::Artifact::Removal(fence)),
                        }),
                    )?;
                    claim.previous = input.request.operation;
                    claim.mode = ns::RemovalPublicationMode::Committed;
                }
                Ok(claim)
            })
            .await
            .unwrap();
        let view = dirty(&n, &n.view, 2, 5).await;
        let closed = close_native(&n, &n.token, 71).await.unwrap();
        let info = closed.plans[0].clone();
        assert_eq!(info.ownership().unwrap().claim(), &claim);
        assert_eq!(
            info.ownership().unwrap().original_open().id,
            n.token.input().id
        );
        assert_eq!(info.native_ownership().unwrap().open(), n.token.input().id);
        assert_eq!(info.view().id(), view.id());
        n.storage
            .cache()
            .owner
            .call(move |store| {
                let mut bytes = fixture().bytes;
                bytes[5] = 255;
                let result = pj(&mut store.state).append(
                    info.id()?,
                    info.cursor(),
                    Candidate::Bytes(bytes),
                )?;
                assert_eq!(result.ownership().unwrap().claim(), &claim);
                Ok(())
            })
            .await
            .unwrap();
        assert!(
            n.client
                .prepare(n.token.clone(), context(&view))
                .await
                .is_err()
        );
        n.storage.close().await.unwrap();
    }
}
