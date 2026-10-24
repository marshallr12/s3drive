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
    setup_mode(path, plans, false).await
}
async fn setup_mode(path: &std::path::Path, plans: usize, retry: bool) -> Native {
    let storage = Storage::open(
        path,
        crate::cache::Options::default(),
        crate::journal::Options::default(),
    )
    .await
    .unwrap();
    let owner = storage.cache().owner;
    let (key, input, view) = owner
        .call(move |store| {
            let s = &mut store.state;
            let f = fixture();
            let key = if retry {
                crate::chunk_mount::admit_retry(
                    s,
                    crate::chunk_mount::Input {
                        volume: f.binding.base()?.volume().into(),
                        controls: vec![names::Control {
                            store: f.store.clone(),
                            source: *f.binding.source(),
                            fingerprint: f.binding.fingerprint().into(),
                        }],
                    },
                )?
            } else {
                chunk(s, &f)
            };
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
        crate::test_schema::demote(&db, 47).unwrap();
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

#[tokio::test]
async fn native_sparse_metadata_batch_does_not_repeat_epoch_or_plan_loads() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("batch"), 16).await;
    let view = dirty(&n, &n.view, 2, 5).await;
    n.client
        .prepare(n.token.clone(), context(&view))
        .await
        .unwrap();
    n.storage
        .cache()
        .owner
        .call(|store| {
            let db = &store.state.connection;
            // Source-isolation checks independently inspect the namespace
            // role, including original opens. They remain mandatory and are
            // not elided by the provenance replay optimization.
            let start = opens::load_count();
            crate::sparse_views::ledger::inspect_metadata(db)?;
            let source_checks = opens::load_count() - start;
            assert!(source_checks > 0);
            let before = opens::load_count();
            crate::sparse_views::preparation::native::validate_metadata(db)?;
            assert_eq!(
                opens::load_count() - before,
                source_checks + 1,
                "provenance checks replayed the same original per epoch/plan"
            );
            Ok(())
        })
        .await
        .unwrap();
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_metadata_batch_rechecks_unselected_open_after_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("batch-corrupt"), 16).await;
    let other = second(&n).await;
    let view = dirty(&n, &n.view, 2, 5).await;
    n.client
        .prepare(n.token.clone(), context(&view))
        .await
        .unwrap();
    n.storage
        .cache()
        .owner
        .call(|store| {
            crate::sparse_views::preparation::native::validate_metadata(&store.state.connection)
        })
        .await
        .unwrap();
    let id = other.input().id.clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let db = &store.state.connection;
            assert!(
                db.execute(
                    "UPDATE windows_open_events SET state_hash=zeroblob(32) WHERE open_id=?1",
                    [id]
                )? > 0
            );
            Ok(())
        })
        .await
        .unwrap();
    n.storage
        .cache()
        .owner
        .call(|store| {
            let db = &store.state.connection;
            assert!(crate::sparse_views::preparation::native::validate_metadata(db).is_err());
            let damaged: i64 = db.query_row(
                "SELECT count(*) FROM windows_open_events WHERE state_hash=zeroblob(32)",
                [],
                |r| r.get(0),
            )?;
            assert!(
                damaged > 0,
                "validation repaired unselected callback evidence"
            );
            Ok(())
        })
        .await
        .unwrap();
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_source_isolation_once_per_snapshot_with_live_pin() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("isolation-once"), 16).await;
    let id = n.view.id();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let pin = crate::sparse_views::pins::acquire(&mut store.state, id)?;
            let db = &store.state.connection;
            let before = crate::sparse_views::isolation_count();
            crate::sparse_views::ledger::inspect(db)?;
            assert_eq!(crate::sparse_views::isolation_count() - before, 1);
            let before = crate::sparse_views::isolation_count();
            crate::sparse_views::ledger::inspect_metadata(db)?;
            assert_eq!(crate::sparse_views::isolation_count() - before, 1);
            crate::sparse_views::pins::release(&mut store.state, &pin)?;
            Ok(())
        })
        .await
        .unwrap();
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_source_isolation_rechecks_late_unselected_open_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("isolation-corrupt"), 16).await;
    let other = second(&n).await;
    let id = other.input().id.clone();
    n.storage.cache().owner.call(move |store| {
        let db = &store.state.connection;
        crate::sparse_views::ledger::inspect(db)?;
        assert!(db.execute("UPDATE windows_open_events SET state_hash=zeroblob(32) WHERE open_id=?", [&id])? > 0);
        assert!(crate::sparse_views::ledger::inspect(db).is_err());
        assert!(crate::sparse_views::ledger::inspect_metadata(db).is_err());
        let damaged: i64 = db.query_row("SELECT count(*) FROM windows_open_events WHERE open_id=? AND state_hash=zeroblob(32)", [&id], |r|r.get(0))?;
        assert!(damaged > 0, "isolation must not repair callback evidence");
        Ok(())
    }).await.unwrap();
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_source_isolation_rechecks_late_trigger_without_repair() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("isolation-trigger"), 16).await;
    n.storage.cache().owner.call(|store| {
        let db = &store.state.connection;
        crate::sparse_views::ledger::inspect(db)?;
        db.execute_batch("CREATE TRIGGER native_scope_probe AFTER INSERT ON sparse_view_blocks BEGIN SELECT 1; END")?;
        assert!(crate::sparse_views::ledger::inspect(db).is_err());
        assert!(crate::sparse_views::ledger::inspect_metadata(db).is_err());
        let retained: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='native_scope_probe')", [], |r|r.get(0))?;
        assert!(retained);
        db.execute_batch("DROP TRIGGER native_scope_probe")?;
        crate::sparse_views::ledger::inspect(db)?;
        Ok(())
    }).await.unwrap();
    n.storage.close().await.unwrap();
}

mod history;

#[tokio::test]
async fn native_retry_fresh_contract_closes_ids_before_transactional_retirement() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("fresh native retention");
    let n = setup_mode(&path, 16, true).await;
    dirty(&n, &n.view, 2, 5).await;
    let closed = close_native(&n, &n.token, 71).await.unwrap();
    let info = closed.plans[0].clone();
    let plan = info.id().unwrap();
    let retry = info.retry_operation().unwrap();
    assert_eq!(info.operation(), retry.id().unwrap());
    assert_eq!(*info.generation(), retry.generation().unwrap());
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let s = &mut store.state;
            let f = fixture();
            let mut bytes = f.bytes.clone();
            bytes[5] = 255;
            let info = pj(s).append(plan, info.cursor(), Candidate::Bytes(bytes))?;
            crate::sparse_views::preparation::tests::fixture::finish(s, info);
            j(s).check(plan, 0, Some(&proof(&f)))?;
            physical(s, plan);
            assert!(
                j(s).begin(plan, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
                    .is_err()
            );
            let r = j(s).begin(plan, f.node, f.store, 5, retry.publish_operation()?)?;
            let r = j(s).reserved(plan, reserved(&r))?;
            let r = j(s).prepare_publish(plan, r.reservation().unwrap().sequence)?;
            j(s).published(plan, published(&r))?;
            j(s).acknowledge_scoped(
                views::ScopedOperation::new(&fixture().binding, [11; 32])?,
                plan,
                views::Options::default(),
            )?;
            assert!(!pj(s).checkpoint(plan)?.unblocked());
            Ok(())
        })
        .await
        .unwrap();
    n.client
        .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
        .await
        .unwrap();
    let key = n.key.clone();
    let original = n.token.input().clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let s = &mut store.state;
            opens(s, &key).retire(&original.id)?;
            let cp = pj(s).checkpoint(plan)?;
            assert!(cp.unblocked());
            j(s).retire_checkpoint(&cp, views::Options::default())?;
            assert!(opens(s, &key).prepare(original).is_err());
            assert!(pj(s).info(plan).is_err());
            crate::chunk_mount::require(&s.connection, &key)?;
            assert_eq!(
                s.connection
                    .query_row("SELECT count(*) FROM windows_open_journal", [], |r| r
                        .get::<_, i64>(0))?,
                0
            );
            assert_eq!(
                s.connection
                    .query_row("SELECT count(*) FROM sparse_preparations", [], |r| r
                        .get::<_, i64>(0))?,
                0
            );
            Ok(())
        })
        .await
        .unwrap();
    assert!(
        n.client
            .close(n.token.clone(), hex(&[71; 16]))
            .await
            .is_err()
    );
    n.storage.close().await.unwrap();
    drop(State::open_managed(&path).unwrap());
}

async fn retry_cycle(n: &Native, f: Fixture, iteration: u64) -> Fixture {
    retry_cycle_mode(n, f, iteration, true).await
}
async fn retry_cycle_mode(n: &Native, mut f: Fixture, iteration: u64, collect: bool) -> Fixture {
    let bytes = vec![(iteration % 251) as u8; 17];
    let context =
        Context::new(&n.view, f.base.root.encode().to_vec(), f.base.page.encode()).unwrap();
    n.client
        .write(n.token.clone(), context, [2; 32], 0, bytes.clone())
        .await
        .unwrap();
    let callback = hex(&hash(b"retry-cycle-close", &n.token.input().id).unwrap()[..16]);
    let completion = hex(&hash(b"retry-cycle-completion", &n.token.input().id).unwrap()[..16]);
    n.token.observe_close();
    let close = n
        .client
        .close(n.token.clone(), callback.clone())
        .await
        .unwrap();
    let info = close.plans[0].clone();
    let plan = info.id().unwrap();
    let f =
        n.storage
            .cache()
            .owner
            .call(move |store| {
                let s = &mut store.state;
                let retry = info.retry_operation().unwrap();
                let info = pj(s).append(plan, info.cursor(), Candidate::Bytes(bytes.clone()))?;
                let info = crate::sparse_views::preparation::tests::fixture::finish(s, info);
                j(s).check(plan, 0, None)?;
                physical(s, plan);
                let r = j(s).begin(
                    plan,
                    f.node.clone(),
                    f.store.clone(),
                    f.node.revision,
                    retry.publish_operation()?,
                )?;
                let r = lineage::commit(s, r);
                j(s).acknowledge_scoped(
                    views::ScopedOperation::new(info.view().binding(), [11; 32])?,
                    plan,
                    views::Options::default(),
                )?;
                let page = crate::journal::Journal::new(s, crate::journal::Options::default())?
                    .read(r.physical.generation, 0, r.physical.size as usize)?;
                let page = s3drive_core::manifest::Page::decode(&page)?;
                f.base = Base {
                    root: info.root()?.unwrap(),
                    page: page.clone(),
                    pages: BTreeMap::from([((0, 0), page)]),
                };
                f.node = r.receipt().unwrap().node.clone();
                f.binding = views::Journal::new(s, views::Options::default())?
                    .current_head(&f.binding)?
                    .binding()
                    .clone();
                f.bytes = bytes;
                Ok(f)
            })
            .await
            .unwrap();
    n.client
        .complete(n.token.clone(), callback, completion)
        .await
        .unwrap();
    let key = n.key.clone();
    let open = n.token.input().id.clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            opens(&mut store.state, &key).retire(&open)?;
            Ok(())
        })
        .await
        .unwrap();
    if collect {
        assert!(n.client.retire_linked(f.node.id.clone()).await.unwrap());
    }
    f
}
async fn retry_reopen(n: &mut Native, f: &Fixture) {
    let mut input = n.token.input().clone();
    let key = n.key.clone();
    let binding = f.binding.clone();
    let (id, view) = n
        .storage
        .cache()
        .owner
        .call(move |store| {
            let s = &mut store.state;
            input.id = crate::chunk_mount::admission::next(
                &s.connection,
                crate::chunk_mount::AdmissionDomain::Open,
            )?
            .unwrap();
            let activation = hex(&hash(b"test-native-activation", &input.id)?[..16]);
            opens(s, &key).prepare(input.clone())?;
            opens(s, &key).activate(&input.id, &activation)?;
            let view = views::Journal::new(s, views::Options::default())?.current_head(&binding)?;
            Ok((input.id, view))
        })
        .await
        .unwrap();
    n.token = n.client.bind(id).await.unwrap();
    n.view = view;
}
#[tokio::test]
async fn native_retry_retention_300_linked_cycles_bound_all_local_history() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("native cycles");
    let mut n = setup_mode(&path, 16, true).await;
    let mut f = fixture();
    let mut peak = (0, 0);
    let mut late = (0, 0);
    for cycle in 0..300 {
        f = retry_cycle(&n, f, cycle).await;
        n.storage
            .cache()
            .owner
            .call(|store| {
                for (table, count) in [
                    ("windows_open_journal", 0),
                    ("windows_open_events", 0),
                    ("retired_windows_opens", 0),
                    ("retained_windows_opens", 0),
                    ("sparse_preparations", 0),
                    ("sparse_publications", 0),
                    ("sparse_retention_anchors", 1),
                    ("sparse_views", 1),
                    ("windows_mount_sessions", 1),
                ] {
                    assert_eq!(
                        store.state.connection.query_row(
                            &format!("SELECT count(*) FROM {table}"),
                            [],
                            |r| r.get::<_, i64>(0)
                        )?,
                        count,
                        "{table}"
                    );
                }
                crate::chunk_mount::validate_all(&store.state.connection)?;
                Ok(())
            })
            .await
            .unwrap();
        let db = std::fs::metadata(path.join("metadata.sqlite3"))
            .unwrap()
            .len();
        let wal = std::fs::metadata(path.join("metadata.sqlite3-wal"))
            .map(|m| m.len())
            .unwrap_or(0);
        peak = (peak.0.max(db), peak.1.max(wal));
        if cycle >= 150 {
            late = (late.0.max(db), late.1.max(wal));
        }
        if cycle < 299 {
            retry_reopen(&mut n, &f).await;
        }
    }
    eprintln!(
        "NATIVE_RETENTION_300 peak_db={} peak_wal={} late_db={} late_wal={}",
        peak.0, peak.1, late.0, late.1
    );
    assert!(peak.0 < 4 * 1024 * 1024 && peak.1 < 8 * 1024 * 1024);
    n.storage.close().await.unwrap();
    drop(State::open_managed(&path).unwrap());
}

#[tokio::test]
async fn native_retry_resealed_terminal_fields_and_missing_independent_witness_fail_closed() {
    for case in 0..8 {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("forged terminal");
        let n = setup_mode(&path, 16, true).await;
        retry_cycle(&n, fixture(), 1).await;
        let key = n.key.clone();
        n.storage
            .cache()
            .owner
            .call(move |store| {
                let s = &mut store.state;
                let raw: Vec<u8> = s.connection.query_row(
                    "SELECT record FROM sparse_retention_anchors",
                    [],
                    |r| r.get(0),
                )?;
                let mut v: serde_json::Value = serde_json::from_slice(&raw)?;
                match case {
                    0 => v["native"]["close"] = serde_json::json!(hex(&[201; 16])),
                    1 => v["native"]["completion"] = serde_json::json!(hex(&[202; 16])),
                    2 => v["native"]["frozen"] = serde_json::to_value([[99u8; 32]])?,
                    3 => v["native"]["frontier"] = serde_json::json!(0),
                    4 => v["native"]["first_operation"] = serde_json::to_value([203u8; 32])?,
                    5 => v["native"]["suffix"]["previous"] = serde_json::to_value([204u8; 32])?,
                    6 => v["native"]["suffix"]["ordinal"] = serde_json::json!(usize::MAX),
                    _ => (),
                }
                if case < 7 {
                    super::super::retention::rewrite_anchor(s, v);
                } else {
                    let raw: Vec<u8> =
                        s.connection
                            .query_row("SELECT record FROM chunk_mount_role", [], |r| r.get(0))?;
                    let mut b: serde_json::Value = serde_json::from_slice(&raw)?;
                    b["terminals"] = serde_json::json!({});
                    let typed: crate::chunk_mount::Binding = serde_json::from_value(b)?;
                    s.connection.execute(
                        "UPDATE chunk_mount_role SET record=?1,state_hash=?2",
                        params![
                            serde_json::to_vec(&typed)?,
                            hash(b"s3drive-chunk-mount-binding-v1\0", &typed)?.as_slice()
                        ],
                    )?;
                }
                let before = native_snapshot(s);
                assert!(crate::chunk_mount::require(&s.connection, &key).is_err());
                assert_eq!(before, native_snapshot(s));
                Ok(())
            })
            .await
            .unwrap();
        n.storage.close().await.unwrap();
        assert!(State::open_managed(&path).is_err());
    }
}

#[tokio::test]
async fn native_retry_retirement_sqlite_full_rolls_back_independent_witness_and_history() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("native full");
    let n = setup_mode(&path, 16, true).await;
    retry_cycle_mode(&n, fixture(), 1, false).await;
    n.storage
        .cache()
        .owner
        .call(|store| {
            let s = &mut store.state;
            let plan: Vec<u8> =
                s.connection
                    .query_row("SELECT plan_id FROM sparse_preparations", [], |r| r.get(0))?;
            let cp = pj(s).checkpoint(plan.try_into().unwrap())?;
            s.connection
                .execute_batch("CREATE TABLE retention_pressure(data BLOB)")?;
            let pages: i64 = s
                .connection
                .pragma_query_value(None, "page_count", |r| r.get(0))?;
            s.connection.pragma_update(None, "max_page_count", pages)?;
            loop {
                match s
                    .connection
                    .execute("INSERT INTO retention_pressure VALUES(zeroblob(3500))", [])
                {
                    Ok(_) => (),
                    Err(e) if e.sqlite_error_code() == Some(rusqlite::ErrorCode::DiskFull) => break,
                    Err(e) => return Err(e.into()),
                }
            }
            let before = native_snapshot(s);
            let error = j(s)
                .retire_checkpoint(&cp, views::Options::default())
                .unwrap_err();
            assert_eq!(
                error
                    .downcast_ref::<rusqlite::Error>()
                    .and_then(|e| e.sqlite_error_code()),
                Some(rusqlite::ErrorCode::DiskFull)
            );
            assert_eq!(native_snapshot(s), before);
            s.connection
                .pragma_update(None, "max_page_count", pages + 1024)?;
            j(s).retire_checkpoint(&cp, views::Options::default())?;
            crate::chunk_mount::validate_all(&s.connection)?;
            Ok(())
        })
        .await
        .unwrap();
    n.storage.close().await.unwrap();
    drop(State::open_managed(&path).unwrap());
}
#[tokio::test]
async fn native_retry_killed_retirement_owners_preserve_old_or_complete_new_state() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    for point in [
        "NativeWitnessStored",
        "Eligible",
        "AnchorStored",
        "SourceStored",
        "PhysicalStored",
        "NativeStored",
        "Validated",
        "Committed",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("native killed owner");
        let ready = tmp.path().join("ready");
        let n = setup_mode(&path, 16, true).await;
        retry_cycle_mode(&n, fixture(), 1, false).await;
        let (before, cp) = n
            .storage
            .cache()
            .owner
            .call(|store| {
                let s = &mut store.state;
                let plan: Vec<u8> =
                    s.connection
                        .query_row("SELECT plan_id FROM sparse_preparations", [], |r| r.get(0))?;
                let cp = pj(s).checkpoint(plan.try_into().unwrap())?;
                Ok((native_snapshot(s), cp))
            })
            .await
            .unwrap();
        n.storage.close().await.unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sparse_views::preparation::publication::tests::retention::retention_crash_child",
                "--nocapture",
            ])
            .env("S3DRIVE_RETENTION_CRASH_DIR", &path)
            .env("S3DRIVE_MIXED_CRASH_KIND", "retire")
            .env("S3DRIVE_MIXED_CRASH_POINT", point)
            .env("S3DRIVE_MIXED_CRASH_READY", &ready)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        while !ready.exists() {
            if let Some(status) = child.try_wait().unwrap() {
                use std::io::Read;
                let mut err = String::new();
                child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut err)
                    .unwrap();
                panic!("{point}: {status}: {err}");
            }
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("native retirement checkpoint timeout: {point}");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(State::open_managed(&path).is_err());
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        let mut state = State::open_managed(&path).unwrap();
        if point != "Committed" {
            assert_eq!(native_snapshot(&state), before);
        }
        j(&mut state)
            .retire_checkpoint(&cp, views::Options::default())
            .unwrap();
        crate::chunk_mount::validate_all(&state.connection).unwrap();
        assert_eq!(
            state
                .connection
                .query_row("SELECT count(*) FROM windows_open_journal", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[tokio::test]
async fn native_retry_reader_clones_and_concurrent_reads_hold_exact_source_and_physical_history() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("native readers");
    let mut n = setup_mode(&path, 16, true).await;
    let mut f = retry_cycle_mode(&n, fixture(), 1, false).await;
    let (view, generation) = n
        .storage
        .cache()
        .owner
        .call(|store| {
            let plan: Vec<u8> = store.state.connection.query_row(
                "SELECT plan_id FROM sparse_preparations",
                [],
                |r| r.get(0),
            )?;
            let plan = plan.try_into().unwrap();
            let view = pj(&mut store.state).info(plan)?.view().id();
            let root = j(&mut store.state).get(plan)?;
            Ok((view, root.physical.generation))
        })
        .await
        .unwrap();
    let local = n.storage.sparse_views(views::Options::default()).unwrap();
    let reader = local.guard(view).await.unwrap();
    let logical = reader.clone();
    drop(reader);
    let reader = n.storage.staging().snapshot(generation).await.unwrap();
    let physical = reader.clone();
    drop(reader);
    let bytes = physical.read(0, 4096).await.unwrap();
    assert_eq!(
        logical.read(0, 17).await.unwrap(),
        vec![views::Piece::Local(vec![1; 17])]
    );
    retry_reopen(&mut n, &f).await;
    f = retry_cycle_mode(&n, f, 2, false).await;
    let (source, root, collected) = tokio::join!(
        logical.read(0, 17),
        physical.read(0, 4096),
        n.client.retire_linked(f.node.id.clone())
    );
    assert_eq!(source.unwrap(), vec![views::Piece::Local(vec![1; 17])]);
    assert_eq!(root.unwrap(), bytes);
    assert!(!collected.unwrap());
    drop(logical);
    assert!(!n.client.retire_linked(f.node.id.clone()).await.unwrap());
    assert_eq!(physical.read(0, 4096).await.unwrap(), bytes);
    drop(physical);
    assert!(n.client.retire_linked(f.node.id).await.unwrap());
    n.storage.close().await.unwrap();
    let state = State::open_managed(&path).unwrap();
    assert!(
        !state
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM staged_generations WHERE generation_id=?1)",
                [generation.value()],
                |r| r.get::<_, bool>(0)
            )
            .unwrap(),
        "old physical root must actually be retired"
    );
}
#[tokio::test]
async fn native_retry_retained_open_capacity_recovers_after_last_reader_without_identity_reuse() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("native reader pressure");
    let mut n = setup_mode(&path, 16, true).await;
    // Keep a prior compact anchor while exercising the full retained-open cap.
    let f = retry_cycle_mode(&n, fixture(), 0, true).await;
    retry_reopen(&mut n, &f).await;
    let f = retry_cycle_mode(&n, f, 1, false).await;
    let local = n.storage.sparse_views(views::Options::default()).unwrap();
    let reader = local.guard(n.view.id()).await.unwrap();
    let clone = reader.clone();
    drop(reader);
    for _ in 0..255 {
        retry_reopen(&mut n, &f).await;
        let callback = hex(&hash(b"pressure-close", &n.token.input().id).unwrap()[..16]);
        let completion = hex(&hash(b"pressure-completion", &n.token.input().id).unwrap()[..16]);
        n.token.observe_close();
        let close = n
            .client
            .close(n.token.clone(), callback.clone())
            .await
            .unwrap();
        assert!(close.plans.is_empty());
        n.client
            .complete(n.token.clone(), callback, completion)
            .await
            .unwrap();
        let key = n.key.clone();
        let id = n.token.input().id.clone();
        n.storage
            .cache()
            .owner
            .call(move |store| {
                opens(&mut store.state, &key).retire(&id)?;
                Ok(())
            })
            .await
            .unwrap();
    }
    let before = n
        .storage
        .cache()
        .owner
        .call(|_| Ok(crate::namespace_client::opens::load_count()))
        .await
        .unwrap();
    assert!(!n.client.retire_linked(f.node.id.clone()).await.unwrap());
    let after = n
        .storage
        .cache()
        .owner
        .call(|_| Ok(crate::namespace_client::opens::load_count()))
        .await
        .unwrap();
    assert!(
        after - before <= 300,
        "pinned refusal repeated retired histories: {}",
        after - before
    );
    let key = n.key.clone();
    let mut input = n.token.input().clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let s = &mut store.state;
            let before_scope = crate::namespace_client::opens::scope_counts();
            let before_loads = crate::namespace_client::opens::load_count();
            let expected_events: i64 =
                s.connection
                    .query_row("SELECT count(*) FROM windows_open_events", [], |r| r.get(0))?;
            assert!(crate::chunk_mount::compatible(&s.connection)?);
            let after_scope = crate::namespace_client::opens::scope_counts();
            assert_eq!(
                crate::namespace_client::opens::load_count() - before_loads,
                256
            );
            assert_eq!(after_scope.0 - before_scope.0, 1);
            assert_eq!(after_scope.1 - before_scope.1, 1);
            assert_eq!(after_scope.3 - before_scope.3, 4);
            assert_eq!(after_scope.2 - before_scope.2, expected_events as usize);
            let loads = crate::chunk_mount::load_count();
            crate::chunk_mount::admission::validate(&s.connection)?;
            assert_eq!(
                crate::chunk_mount::load_count() - loads,
                1,
                "retained frontier validates one binding, not one per ID"
            );
            input.id = crate::chunk_mount::admission::next(
                &s.connection,
                crate::chunk_mount::AdmissionDomain::Open,
            )?
            .unwrap();
            let before = native_snapshot(s);
            assert!(opens(s, &key).prepare(input).is_err());
            assert_eq!(before, native_snapshot(s));
            Ok(())
        })
        .await
        .unwrap();
    assert!(!clone.read(0, 17).await.unwrap().is_empty());
    drop(clone);
    assert!(n.client.retire_linked(f.node.id.clone()).await.unwrap());
    retry_reopen(&mut n, &f).await;
    close_native(&n, &n.token, 71).await.unwrap();
    n.client
        .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
        .await
        .unwrap();
    let key = n.key.clone();
    let id = n.token.input().id.clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            opens(&mut store.state, &key).retire(&id)?;
            Ok(())
        })
        .await
        .unwrap();
    assert!(n.client.retire_linked(f.node.id).await.unwrap());
    n.storage.close().await.unwrap();
}

// Include mode contents as well as the full schema and remaining table rows.
fn native_snapshot(s: &State) -> Vec<String> {
    let mut rows = super::super::retry_safety::migration_rows(s);
    for table in ["sparse_scoped_mode", "sparse_retry_mode"] {
        let mut q = s.connection.prepare(&format!("SELECT quote(singleton)||','||quote(format)||','||quote(state_hash) FROM {table} ORDER BY singleton")).unwrap();
        for row in q.query_map([], |r| r.get::<_, String>(0)).unwrap() {
            rows.push(format!("{table}:{}", row.unwrap()));
        }
    }
    rows
}

#[tokio::test]
async fn native_retry_validation_work_pinned_refusal_rechecks_resealed_unselected_edges_frontier_and_modes()
 {
    for case in 0..11 {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("pinned semantic corruption");
        let mut n = setup_mode(&path, 16, true).await;
        let original = n.token.input().id.clone();
        let mut f = retry_cycle_mode(&n, fixture(), 1, false).await;
        let reader = n
            .storage
            .sparse_views(views::Options::default())
            .unwrap()
            .guard(n.view.id())
            .await
            .unwrap();
        retry_reopen(&mut n, &f).await;
        f = retry_cycle_mode(&n, f, 2, false).await;
        assert!(!n.client.retire_linked(f.node.id.clone()).await.unwrap());
        let before = n
            .storage
            .cache()
            .owner
            .call(move |store| {
                let s = &mut store.state;
                match case {
                    0 => crate::namespace_client::opens::rewrite_epoch_for_test(
                        &s.connection,
                        &original,
                    )?,
                    1 => {
                        let binding = crate::chunk_mount::load(&s.connection)?.unwrap();
                        let mut value = serde_json::to_value(&binding)?;
                        value["frontier"] =
                            serde_json::json!(value["frontier"].as_u64().unwrap() - 1);
                        let binding: crate::chunk_mount::Binding = serde_json::from_value(value)?;
                        s.connection.execute(
                            "UPDATE chunk_mount_role SET record=?1,state_hash=?2",
                            params![
                                serde_json::to_vec(&binding)?,
                                hash(b"s3drive-chunk-mount-binding-v1\0", &binding)?.as_slice()
                            ],
                        )?;
                    }
                    2 => {
                        s.connection
                            .execute("UPDATE sparse_retry_mode SET state_hash=zeroblob(32)", [])?;
                    }
                    3 => {
                        s.connection.execute(
                            "UPDATE windows_mount_sessions SET state_hash=zeroblob(32)",
                            [],
                        )?;
                    }
                    4 => {
                        s.connection.execute(
                            "UPDATE retained_client_identity SET state_hash=zeroblob(32)",
                            [],
                        )?;
                    }
                    5 => {
                        s.connection
                            .execute("UPDATE sparse_scoped_mode SET state_hash=zeroblob(32)", [])?;
                    }
                    6 => {
                        s.connection.execute("UPDATE windows_open_journal SET owner_id=?1 WHERE open_id=?2", params![hex(&[88;16]), original])?;
                    }
                    7 => {
                        s.connection.execute("UPDATE windows_open_events SET state_hash=zeroblob(32) WHERE open_id=?1 AND ordinal=(SELECT max(ordinal) FROM windows_open_events WHERE open_id=?1)", [&original])?;
                    }
                    8 | 9 => {
                        crate::namespace_client::opens::rewrite_session_for_test(&s.connection, &original, case == 9)?;
                    }
                    _ => {
                        let session = hex(&[77;16]);
                        let runtime = hex(&[78;16]);
                        assert_eq!(s.connection.query_row("SELECT count(*) FROM windows_mount_sessions WHERE session_id=?1 OR runtime_id=?2", params![session, runtime], |r| r.get::<_, i64>(0))?, 0);
                        assert_eq!(s.connection.execute("INSERT INTO windows_mount_sessions SELECT ?1,volume_id,?2,owner_id,zeroblob(32) FROM windows_mount_sessions LIMIT 1", params![session, runtime])?, 1);
                        // The unused row must exist before its deliberately bad seal
                        // reaches the full session binding check, not a setup error.
                        assert_eq!(crate::chunk_mount::compatible(&s.connection).unwrap_err().to_string(), "mount session binding changed");
                    }
                }
                Ok(native_snapshot(s))
            })
            .await
            .unwrap();
        assert!(
            n.client.retire_linked(f.node.id.clone()).await.is_err(),
            "corruption case {case} was hidden by the pin"
        );
        let capability = crate::chunk_mount::Client::new(
            n.storage.cache().owner,
            crate::journal::Options::default(),
            n.key.clone(),
        );
        let mut unrelated = n.token.input().clone();
        unrelated.node = hex(&[200; 16]);
        assert!(
            capability.uncontended_open(unrelated).await.is_err(),
            "empty-node preflight must recheck unselected corruption case {case}"
        );
        assert!(
            n.client.plans(f.node.id).await.is_err(),
            "plans must recheck late corruption case {case}"
        );
        n.storage
            .cache()
            .owner
            .call(move |store| {
                assert!(store.state.connection.is_autocommit());
                assert_eq!(native_snapshot(&store.state), before);
                Ok(())
            })
            .await
            .unwrap();
        drop(reader);
        n.storage.close().await.unwrap();
    }
}

// Diagnostic protocol fixture only, not a substitute for the filesystem gate.
async fn native_work(n: &Native) -> [usize; 9] {
    n.storage
        .cache()
        .owner
        .call(|_| {
            let (scopes, sessions, events, queries) =
                crate::namespace_client::opens::scope_counts();
            Ok([
                scopes,
                sessions,
                events,
                queries,
                crate::namespace_client::opens::load_count(),
                crate::chunk_mount::load_count(),
                views::ledger::inspection_count(),
                crate::sparse_views::preparation::ledger::inspection_count(),
                crate::sparse_views::preparation::native::epoch_source_visits(),
            ])
        })
        .await
        .unwrap()
}
#[tokio::test]
async fn native_retry_validation_work_per_open_close() {
    let tmp = tempfile::tempdir().unwrap();
    let mut n = setup_mode(&tmp.path().join("native scan cost"), 16, true).await;
    let f = retry_cycle_mode(&n, fixture(), 1, false).await;
    let local = n.storage.sparse_views(views::Options::default()).unwrap();
    let reader = local.guard(n.view.id()).await.unwrap();
    let capability = crate::chunk_mount::Client::new(
        n.storage.cache().owner,
        crate::journal::Options::default(),
        n.key.clone(),
    );
    for i in 0..16 {
        macro_rules! measured {
            ($name:literal, $work:expr) => {{
                let before = native_work(&n).await;
                let start = std::time::Instant::now();
                let result = $work;
                let elapsed = start.elapsed();
                let after = native_work(&n).await;
                let delta = std::array::from_fn::<_, 9, _>(|j| after[j] - before[j]);
                match $name {
                    "heads" => assert_eq!(delta[0], 2),
                    "guard acquire" => assert_eq!(delta[0], 4),
                    "guard release" => assert_eq!(delta[0], 3),
                    "plans" | "uncontended open" => assert_eq!(delta[0], 1),
                    "Close" => assert_eq!(delta[0], 4),
                    "completion" => assert_eq!(delta[0], 5),
                    _ => {}
                }
                if [0, 3, 15].contains(&i) {
                    eprintln!("NATIVE_VALIDATION_WORK retained={} phase={} ms={} scopes_sessions_events_queries_opens_mounts_sources_preps_epoch_sources={delta:?}", i+1, $name, elapsed.as_millis());
                }
                result
            }};
        }
        assert!(!measured!(
            "pre-open sweep",
            n.client.retire_linked(f.node.id.clone()).await.unwrap()
        ));
        assert!(measured!(
            "uncontended open",
            capability
                .uncontended_open(n.token.input().clone())
                .await
                .unwrap()
        ));
        measured!("heads", local.heads().await.unwrap());
        measured!(
            "namespace pending",
            capability.namespace().pending(None, 256).await.unwrap()
        );
        measured!(
            "creator records",
            capability
                .creations(crate::manifest_creation::Limits::default())
                .unwrap()
                .records()
                .await
                .unwrap()
        );
        measured!(
            "root records",
            local
                .preparations(crate::sparse_views::preparation::Options::default())
                .unwrap()
                .publications(Limits::default())
                .unwrap()
                .records()
                .await
                .unwrap()
        );
        measured!(
            "pending recovery",
            capability
                .opens()
                .pending_recovery(n.token.input().volume.clone(), n.token.input().node.clone())
                .await
                .unwrap()
        );

        measured!(
            "current session",
            capability
                .opens()
                .current_session(
                    n.token.input().session.clone(),
                    n.token.input().volume.clone()
                )
                .await
                .unwrap()
        );
        measured!("records", capability.opens().records().await.unwrap());
        measured!(
            "get",
            capability
                .opens()
                .get(n.token.input().id.clone())
                .await
                .unwrap()
        );
        measured!(
            "owned removal",
            capability
                .namespace()
                .owned_removal(n.token.input().volume.clone(), n.token.input().node.clone())
                .await
                .unwrap()
        );
        let guard = measured!("guard acquire", local.guard(n.view.id()).await.unwrap());
        measured!("guard release", {
            drop(guard);
            n.storage.cache().owner.call(|_| Ok(())).await.unwrap();
        });
        let next = measured!(
            "next admission",
            capability
                .next_admission(crate::chunk_mount::AdmissionDomain::Open)
                .await
                .unwrap()
                .unwrap()
        );
        let mut input = n.token.input().clone();
        let key = n.key.clone();
        let binding = f.binding.clone();
        let (id, view) = measured!(
            "admission activation head",
            n.storage
                .cache()
                .owner
                .call(move |store| {
                    let s = &mut store.state;
                    input.id = next;
                    let activation = hex(&hash(b"test-native-activation", &input.id)?[..16]);
                    opens(s, &key).prepare(input.clone())?;
                    opens(s, &key).activate(&input.id, &activation)?;
                    let view = views::Journal::new(s, views::Options::default())?
                        .current_head(&binding)?;
                    Ok((input.id, view))
                })
                .await
                .unwrap()
        );
        n.token = measured!("bind", n.client.bind(id).await.unwrap());
        n.view = view;
        measured!("plans", n.client.plans(f.node.id.clone()).await.unwrap());
        let callback = hex(&hash(b"diagnostic-close", &n.token.input().id).unwrap()[..16]);
        let completion = hex(&hash(b"diagnostic-completion", &n.token.input().id).unwrap()[..16]);
        n.token.observe_close();
        let close = measured!(
            "Close",
            n.client
                .close(n.token.clone(), callback.clone())
                .await
                .unwrap()
        );
        assert!(close.plans.is_empty());
        measured!(
            "completion",
            n.client
                .complete(n.token.clone(), callback, completion)
                .await
                .unwrap()
        );
        let key = n.key.clone();
        let id = n.token.input().id.clone();
        measured!(
            "retire",
            n.storage
                .cache()
                .owner
                .call(move |store| {
                    opens(&mut store.state, &key).retire(&id)?;
                    Ok(())
                })
                .await
                .unwrap()
        );
        assert!(!measured!(
            "post-Close sweep",
            n.client.retire_linked(f.node.id.clone()).await.unwrap()
        ));
    }
    let key = n.key.clone();
    let original = n.token.input().id.clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let s = &mut store.state;
            let before = native_snapshot(s);
            assert!(s.connection.is_autocommit());
            let scans = crate::namespace_client::opens::scope_counts().0;
            crate::chunk_mount::require(&s.connection, &key)?;
            assert_eq!(crate::namespace_client::opens::scope_counts().0 - scans, 1);
            assert!(s.connection.is_autocommit());
            {
                let tx = s
                    .connection
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let scans = crate::namespace_client::opens::scope_counts().0;
                crate::chunk_mount::require(&tx, &key)?;
                assert_eq!(crate::namespace_client::opens::scope_counts().0 - scans, 1);
                assert!(!tx.is_autocommit());
                tx.execute(
                    "UPDATE windows_open_events SET state_hash=zeroblob(32) WHERE open_id=?",
                    [&original],
                )?;
                assert!(
                    crate::chunk_mount::require(&tx, &key).is_err(),
                    "a prior read scope cannot cover a later write in the same transaction"
                );
                assert!(!tx.is_autocommit());
                tx.rollback()?;
            }
            let mode_hash: Vec<u8> =
                s.connection
                    .query_row("SELECT state_hash FROM sparse_retry_mode", [], |r| r.get(0))?;
            s.connection
                .execute("UPDATE sparse_retry_mode SET state_hash=zeroblob(32)", [])?;
            assert!(crate::chunk_mount::require(&s.connection, &key).is_err());
            assert!(
                s.connection.is_autocommit(),
                "failed owned read must also finish its transaction"
            );
            s.connection
                .execute("UPDATE sparse_retry_mode SET state_hash=?", [mode_hash])?;
            crate::chunk_mount::require(&s.connection, &key)?;
            assert!(s.connection.is_autocommit());
            assert_eq!(native_snapshot(s), before);
            Ok(())
        })
        .await
        .unwrap();
    drop(reader);
    assert!(n.client.retire_linked(f.node.id).await.unwrap());
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_retry_validation_work_completion_keeps_unsettled_barrier() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup_mode(&temp.path().join("unsettled completion snapshot"), 16, true).await;
    dirty(&n, &n.view, 2, 5).await;
    let closed = close_native(&n, &n.token, 71).await.unwrap();
    assert_eq!(closed.plans.len(), 1);
    let before = n
        .storage
        .cache()
        .owner
        .call(|store| Ok(native_snapshot(&store.state)))
        .await
        .unwrap();
    match n
        .client
        .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
        .await
    {
        Err(error) => assert_eq!(
            error.to_string(),
            "settle every sparse preparation before closing this node",
            "the full barrier must reject before the later dirty-head guard"
        ),
        Ok(_) => panic!("unsettled preparation was completed"),
    }
    assert_eq!(
        n.client
            .plans(n.token.input().node.clone())
            .await
            .unwrap()
            .len(),
        1
    );
    n.storage
        .cache()
        .owner
        .call(move |store| {
            assert!(store.state.connection.is_autocommit());
            assert_eq!(native_snapshot(&store.state), before);
            Ok(())
        })
        .await
        .unwrap();
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_retry_validation_work_close_snapshots_keep_dirty_and_empty_modes() {
    for retry in [false, true] {
        for changed in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let n = setup_mode(&tmp.path().join("close read intervals"), 16, retry).await;
            let expected = if changed {
                Some(dirty(&n, &n.view, 2, 5).await.id())
            } else {
                None
            };
            let closed = close_native(&n, &n.token, 71).await.unwrap();
            assert_eq!(closed.plans.len(), usize::from(changed));
            assert_eq!(closed.plans.first().map(|p| p.view().id()), expected);
            let retried = close_native(&n, &n.token, 71).await.unwrap();
            assert_eq!(closed.open, retried.open);
            assert_eq!(
                closed
                    .plans
                    .iter()
                    .map(|p| p.id().unwrap())
                    .collect::<Vec<_>>(),
                retried
                    .plans
                    .iter()
                    .map(|p| p.id().unwrap())
                    .collect::<Vec<_>>()
            );
            assert!(close_native(&n, &n.token, 72).await.is_err());
            n.storage.close().await.unwrap();
        }
    }
}

#[tokio::test]
async fn native_retry_validation_work_final_close_snapshot_rechecks_after_initial_read() {
    for case in 0..3 {
        let tmp = tempfile::tempdir().unwrap();
        let mut n = setup_mode(&tmp.path().join("late close read corruption"), 16, true).await;
        let original = n.token.input().id.clone();
        let f = retry_cycle_mode(&n, fixture(), 1, false).await;
        retry_reopen(&mut n, &f).await;
        let key = n.key.clone();
        let current = n.token.input().id.clone();
        n.storage.cache().owner.call(move |store| {
            let s = &mut store.state;
            let before = native_snapshot(s);
            let tx = s.connection.transaction()?;
            {
                let scope = crate::chunk_mount::read_scope(&tx, &key)?;
                crate::sparse_views::preparation::ledger::inspect_scoped(&scope)?;
            }
            let open = opens::sparse_open(&tx, &current)?;
            match case {
                0 => crate::namespace_client::opens::rewrite_epoch_for_test(&tx, &original)?,
                1 => {
                    let binding = crate::chunk_mount::load(&tx)?.unwrap();
                    let mut value = serde_json::to_value(&binding)?;
                    value["frontier"] = serde_json::json!(value["frontier"].as_u64().unwrap() - 1);
                    let binding: crate::chunk_mount::Binding = serde_json::from_value(value)?;
                    tx.execute("UPDATE chunk_mount_role SET record=?1,state_hash=?2", params![serde_json::to_vec(&binding)?, hash(b"s3drive-chunk-mount-binding-v1\0", &binding)?.as_slice()])?;
                }
                _ => {
                    assert_eq!(tx.execute("INSERT INTO windows_mount_sessions SELECT ?1,volume_id,?2,owner_id,zeroblob(32) FROM windows_mount_sessions LIMIT 1", params![hex(&[77;16]),hex(&[78;16])])?, 1);
                }
            }
            // Actual final Close read helper, not an injected mid-Close crash.
            assert!(crate::sparse_views::preparation::native::validated_result(&tx, &key, open).is_err());
            tx.rollback()?;
            assert!(s.connection.is_autocommit());
            assert_eq!(native_snapshot(s), before);
            crate::chunk_mount::require(&s.connection, &key)?;
            Ok(())
        }).await.unwrap();
        n.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn native_retry_validation_work_open_preflight_is_selection_not_admission() {
    let tmp = tempfile::tempdir().unwrap();
    let n = setup_mode(&tmp.path().join("preflight freshness"), 16, true).await;
    let capability = crate::chunk_mount::Client::new(
        n.storage.cache().owner,
        crate::journal::Options::default(),
        n.key.clone(),
    );
    let mut input = n.token.input().clone();
    assert!(!capability.uncontended_open(input.clone()).await.unwrap());
    input.node = hex(&[199; 16]);
    // Global collision still falls back even when this target has no live open.
    assert!(!capability.uncontended_open(input).await.unwrap());
    let mut input = n.token.input().clone();
    input.session = hex(&[198; 16]);
    assert!(capability.uncontended_open(input).await.is_err());
    retry_cycle_mode(&n, fixture(), 1, false).await;
    let mut input = n.token.input().clone();
    input.id = capability
        .next_admission(crate::chunk_mount::AdmissionDomain::Open)
        .await
        .unwrap()
        .unwrap();
    assert!(capability.uncontended_open(input.clone()).await.unwrap());
    let original = n.token.input().id.clone();
    let before = n
        .storage
        .cache()
        .owner
        .call(move |store| {
            crate::namespace_client::opens::rewrite_epoch_for_test(
                &store.state.connection,
                &original,
            )?;
            Ok(native_snapshot(&store.state))
        })
        .await
        .unwrap();
    // No preflight result authorizes the later admission after another command.
    assert!(capability.opens().prepare(input).await.is_err());
    n.storage
        .cache()
        .owner
        .call(move |store| {
            assert_eq!(native_snapshot(&store.state), before);
            Ok(())
        })
        .await
        .unwrap();
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_retry_validation_work_open_preflight_keeps_obsolete_and_pending_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("preflight recovery fallback");
    let old = setup(&path, 16).await;
    close_native(&old, &old.token, 71).await.unwrap();
    old.storage.close().await.unwrap();
    let storage = Storage::open(
        &path,
        crate::cache::Options::default(),
        crate::journal::Options::default(),
    )
    .await
    .unwrap();
    let capability = crate::chunk_mount::Client::new(
        storage.cache().owner,
        crate::journal::Options::default(),
        old.key.clone(),
    );
    let mut input = old.token.input().clone();
    input.id = hex(&[76; 16]);
    input.session = hex(&[75; 16]);
    let journal = capability.opens();
    journal
        .start_session(input.session.clone(), input.volume.clone())
        .await
        .unwrap();
    assert!(!capability.uncontended_open(input.clone()).await.unwrap());
    let pending = journal
        .prepare_recovery(
            input.session.clone(),
            input.volume.clone(),
            input.node.clone(),
            None,
        )
        .await
        .unwrap();
    assert!(!capability.uncontended_open(input.clone()).await.unwrap());
    assert!(
        journal
            .finish_recovery(input.session.clone(), pending.input.id)
            .await
            .unwrap()
            .finished
    );
    for old in pending.input.opens {
        journal.retire(old.id).await.unwrap();
    }
    assert!(capability.uncontended_open(input).await.unwrap());
    storage.close().await.unwrap();
}
