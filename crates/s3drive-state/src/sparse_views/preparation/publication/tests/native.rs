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
    setup_mode_posix(path, plans, retry, true).await
}
async fn setup_mode_posix(
    path: &std::path::Path,
    plans: usize,
    retry: bool,
    posix: bool,
) -> Native {
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
            let input = open_with_posix(s, &f, &key, posix);
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
    let input = closed.plans[0].input();
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
                let retry = info.retry_operation();
                let info = pj(s).append(plan, info.cursor(), Candidate::Bytes(bytes.clone()))?;
                let info = crate::sparse_views::preparation::tests::fixture::finish(s, info);
                j(s).check(plan, 0, None)?;
                physical(s, plan);
                let r = j(s).begin(
                    plan,
                    f.node.clone(),
                    f.store.clone(),
                    f.node.revision,
                    match retry {
                        Some(retry) => retry.publish_operation()?,
                        None => hex(&hash(b"test-native-root-publication", &plan)?[..16]),
                    },
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
        let corrupt_plan = n.client.plans(f.node.id.clone()).await.unwrap()[0]
            .id()
            .unwrap();
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
            n.client
                .flush_selection(n.view.binding().clone(), Limits::default())
                .await
                .is_err(),
            "flush selection must recheck unselected corruption case {case}"
        );
        assert!(
            capability
                .uncontended_close(
                    n.token.input().session.clone(),
                    n.token.input().id.clone(),
                    hex(&[71; 16])
                )
                .await
                .is_err(),
            "Close prefix must reject late corruption {case}"
        );
        assert!(n.client.bind(n.token.input().id.clone()).await.is_err());
        assert!(
            n.client
                .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
                .await
                .is_err()
        );
        assert!(
            n.client.quiet_recovery_targets().await.is_err(),
            "quiet recovery must reject unselected corruption {case}"
        );
        assert!(
            n.client.plans(f.node.id).await.is_err(),
            "plans must recheck late corruption case {case}"
        );
        n.storage
            .cache()
            .owner
            .call(move |store| {
                assert!(
                    pj(&mut store.state)
                        .materialize(
                            corrupt_plan,
                            crate::sparse_views::preparation::Position { level: 0, slot: 0 },
                            crate::journal::Options::default()
                        )
                        .is_err()
                );
                let position = crate::sparse_views::preparation::Position { level: 0, slot: 0 };
                assert!(
                    pj(&mut store.state)
                        .upload_selection(corrupt_plan, position)
                        .is_err()
                );
                assert!(store.state.connection.is_autocommit());
                assert!(
                    pj(&mut store.state)
                        .dependency_selection(corrupt_plan, position)
                        .is_err()
                );
                assert!(store.state.connection.is_autocommit());
                assert!(j(&mut store.state).begin_selection(corrupt_plan).is_err());
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
fn native_work_here() -> [usize; 9] {
    let (scopes, sessions, events, queries) = crate::namespace_client::opens::scope_counts();
    [
        scopes,
        sessions,
        events,
        queries,
        crate::namespace_client::opens::load_count(),
        crate::chunk_mount::load_count(),
        views::ledger::inspection_count(),
        crate::sparse_views::preparation::ledger::inspection_count(),
        crate::sparse_views::preparation::native::epoch_source_visits(),
    ]
}
async fn native_work(n: &Native) -> [usize; 9] {
    n.storage
        .cache()
        .owner
        .call(|_| Ok(native_work_here()))
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
                    "heads" => assert_eq!(delta[0], 1),
                    "guard acquire" => assert_eq!(delta[0], 2),
                    "guard release" => assert_eq!(delta[0], 2),
                    "plans" | "uncontended open" | "flush selection" | "quiet recovery" => assert_eq!(delta[0], 1),
                    "Close" => assert_eq!(delta[0], 3),
                    "completion" => assert_eq!(delta[0], 2),
                    "bind" | "uncontended close" => assert_eq!(delta[0], 1),
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
        measured!(
            "flush selection",
            n.client
                .flush_selection(n.view.binding().clone(), Limits::default())
                .await
                .unwrap()
        );
        let targets = measured!(
            "quiet recovery",
            n.client.quiet_recovery_targets().await.unwrap()
        )
        .unwrap();
        let heads = measured!("heads", local.heads().await.unwrap());
        assert_eq!(
            targets,
            heads
                .iter()
                .map(|h| h.binding().base().unwrap().node().to_owned())
                .collect::<Vec<_>>()
        );
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
        assert!(
            measured!(
                "uncontended close",
                capability
                    .uncontended_close(
                        n.token.input().session.clone(),
                        n.token.input().id.clone(),
                        callback.clone()
                    )
                    .await
                    .unwrap()
            )
            .is_some()
        );
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
            if changed {
                assert_eq!(
                    n.client
                        .complete(n.token.clone(), hex(&[71; 16]), hex(&[73; 16]))
                        .await
                        .unwrap_err()
                        .to_string(),
                    "settle every sparse preparation before closing this node"
                );
            } else {
                assert!(
                    n.client
                        .complete(n.token.clone(), hex(&[71; 16]), hex(&[73; 16]))
                        .await
                        .unwrap()
                        .completed(&hex(&[71; 16]))
                );
            }
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
            assert!(crate::sparse_views::preparation::native::validate_completed(&tx, &key).is_err());
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
    assert!(
        capability
            .uncontended_close(
                input.session.clone(),
                old.token.input().id.clone(),
                hex(&[71; 16])
            )
            .await
            .unwrap()
            .is_none()
    );
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

#[tokio::test]
async fn native_retry_validation_work_flush_selection_rechecks_preparation_and_scope() {
    for retry in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let n = setup_mode(&tmp.path().join("flush selection modes"), 16, retry).await;
        let initial = n
            .client
            .flush_selection(n.view.binding().clone(), Limits::default())
            .await
            .unwrap();
        assert_eq!(initial.head.id(), n.view.id());
        assert!(initial.plans.is_empty());
        assert!(initial.acknowledged.is_empty());
        let dirty = dirty(&n, &n.view, 2, 5).await;
        let before = n
            .client
            .flush_selection(n.view.binding().clone(), Limits::default())
            .await
            .unwrap();
        assert_eq!(before.head.id(), dirty.id());
        assert!(before.plans.is_empty());
        drop(before);
        let prepared = n
            .client
            .prepare(n.token.clone(), context(&dirty))
            .await
            .unwrap();
        let selected = n
            .client
            .flush_selection(n.view.binding().clone(), Limits::default())
            .await
            .unwrap();
        assert_eq!(selected.head.id(), dirty.id());
        assert_eq!(selected.plans.len(), 1);
        assert_eq!(selected.plans[0].id().unwrap(), prepared.id().unwrap());
        assert!(selected.acknowledged.is_empty());
        for case in 0..3 {
            let mut bad = n.view.binding().clone();
            match case {
                0 => bad.source = [199; 32],
                1 => bad.store = [199; 16],
                _ => {
                    let base = bad.base().unwrap();
                    bad.base = s3drive_core::manifest::base::Descriptor::from_hex(
                        base.volume(),
                        &hex(&[199; 16]),
                        base.reservation(),
                        base.payload_reference(),
                    )
                    .unwrap()
                    .encode()
                    .to_vec();
                }
            }
            assert!(
                n.client
                    .flush_selection(bad, Limits::default())
                    .await
                    .is_err()
            );
        }
        assert!(
            n.client
                .flush_selection(
                    n.view.binding().clone(),
                    Limits {
                        plans: 0,
                        ..Limits::default()
                    }
                )
                .await
                .is_err()
        );
        n.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn native_retry_validation_work_flush_selection_checks_unselected_acknowledged_physical_proof()
 {
    let tmp = tempfile::tempdir().unwrap();
    let n = setup_mode(&tmp.path().join("flush physical proof"), 16, true).await;
    let old_binding = n.view.binding().clone();
    let f = retry_cycle_mode(&n, fixture(), 1, false).await;
    let selected = n
        .client
        .flush_selection(old_binding.clone(), Limits::default())
        .await
        .unwrap();
    assert!(selected.head.is_clean());
    assert_ne!(selected.head.binding(), &old_binding);
    assert_eq!(selected.head.binding(), &f.binding);
    assert_eq!(selected.plans.len(), 1);
    let plan = selected.plans[0].id().unwrap();
    assert!(selected.acknowledged.contains(&plan));
    drop(selected);
    let mut unrelated = old_binding.clone();
    let base = unrelated.base().unwrap();
    unrelated.base = s3drive_core::manifest::base::Descriptor::from_hex(
        base.volume(),
        &hex(&[199; 16]),
        base.reservation(),
        base.payload_reference(),
    )
    .unwrap()
    .encode()
    .to_vec();
    let binding = unrelated.clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            views::Journal::new(&mut store.state, views::Options::default())?.begin_scoped(
                views::ScopedOperation::new(&binding, [199; 32])?,
                binding,
                17,
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let selected = n
        .client
        .flush_selection(unrelated.clone(), Limits::default())
        .await
        .unwrap();
    assert!(selected.plans.is_empty());
    assert!(selected.acknowledged.contains(&plan));
    drop(selected);
    let targets = n.client.quiet_recovery_targets().await.unwrap().unwrap();
    let heads = n
        .storage
        .sparse_views(views::Options::default())
        .unwrap()
        .heads()
        .await
        .unwrap();
    assert_eq!(
        targets,
        heads
            .iter()
            .map(|h| h.binding().base().unwrap().node().to_owned())
            .collect::<Vec<_>>()
    );
    assert_eq!(targets.len(), 2);
    let before = n
        .storage
        .cache()
        .owner
        .call(move |store| {
            let s = &mut store.state;
            let mut meta = super::super::ledger::inspect(&s.connection)?;
            let mut root = j(s).get(plan)?;
            root.physical.checksum[0] ^= 1;
            root.validate()?;
            s.connection.execute(
                "UPDATE sparse_publications SET record=?1,state_hash=?2 WHERE plan_id=?3",
                params![
                    serde_json::to_vec(&root)?,
                    hash(b"s3drive-mixed-root-record-v1\0", &root)?.as_slice(),
                    plan.as_slice()
                ],
            )?;
            meta.records.insert(plan, root);
            assert_eq!(
                s.connection.execute(
                    "UPDATE sparse_publication_role SET state_hash=?1",
                    [meta.role()?.as_slice()],
                )?,
                1
            );
            // Both row and aggregate graph seals are deliberately recomputed;
            // the independent original physical receipt must still reject it.
            assert!(super::super::ledger::inspect(&s.connection)? == meta);
            Ok(native_snapshot(s))
        })
        .await
        .unwrap();
    assert!(
        n.client
            .flush_selection(unrelated, Limits::default())
            .await
            .is_err()
    );
    assert!(n.client.quiet_recovery_targets().await.is_err());
    for target in targets {
        assert!(!matches!(n.client.retire_linked(target).await, Ok(true)));
    }
    n.storage
        .cache()
        .owner
        .call(move |store| {
            assert!(j(&mut store.state).check_base(plan, &f.node).is_err());
            assert!(store.state.connection.is_autocommit());
            assert_eq!(native_snapshot(&store.state), before);
            Ok(())
        })
        .await
        .unwrap();
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_retry_validation_work_completion_preserves_exact_retries_in_both_modes() {
    for retry in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let n = setup_mode(&tmp.path().join("complete paired proof modes"), 16, retry).await;
        retry_cycle_mode(&n, fixture(), 1, false).await;
        let callback = hex(&hash(b"retry-cycle-close", &n.token.input().id).unwrap()[..16]);
        let completion = hex(&hash(b"retry-cycle-completion", &n.token.input().id).unwrap()[..16]);
        let before = n
            .storage
            .cache()
            .owner
            .call(|store| Ok(native_snapshot(&store.state)))
            .await
            .unwrap();
        assert!(
            n.client
                .complete(n.token.clone(), callback.clone(), completion)
                .await
                .unwrap()
                .completed(&callback)
        );
        assert!(
            n.client
                .complete(n.token.clone(), callback, hex(&[196; 16]))
                .await
                .is_err()
        );
        assert!(
            n.client
                .complete(n.token.clone(), hex(&[195; 16]), hex(&[196; 16]))
                .await
                .is_err()
        );
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
}

#[tokio::test]
async fn native_retry_validation_work_close_prefix_requires_actual_sole_unfinished_close() {
    for retry in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let mut n = setup_mode(&tmp.path().join("sole close selection"), 16, retry).await;
        let capability = crate::chunk_mount::Client::new(
            n.storage.cache().owner,
            crate::journal::Options::default(),
            n.key.clone(),
        );
        let token = n.token.clone();
        let session = token.input().session.clone();
        let open = token.input().id.clone();
        assert!(
            capability
                .uncontended_close(session.clone(), open.clone(), hex(&[71; 16]))
                .await
                .unwrap()
                .is_none()
        );
        close_native(&n, &token, 71).await.unwrap();
        assert!(
            capability
                .uncontended_close(session.clone(), open.clone(), hex(&[70; 16]))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            capability
                .uncontended_close(session.clone(), open.clone(), hex(&[71; 16]))
                .await
                .unwrap()
                .is_some()
        );
        // The selection does not replace the required post-flush records read:
        // a new live handle introduced by another command is visible there.
        let other = if retry {
            retry_reopen(&mut n, &fixture()).await;
            n.token.clone()
        } else {
            second(&n).await
        };
        assert!(
            capability
                .opens()
                .records()
                .await
                .unwrap()
                .iter()
                .any(|r| r.input.id == other.input().id && r.close().is_none())
        );
        assert!(
            capability
                .uncontended_close(session.clone(), open.clone(), hex(&[71; 16]))
                .await
                .unwrap()
                .is_none()
        );
        close_native(&n, &other, 73).await.unwrap();
        n.client
            .complete(other.clone(), hex(&[73; 16]), hex(&[74; 16]))
            .await
            .unwrap();
        capability
            .opens()
            .retire(other.input().id.clone())
            .await
            .unwrap();
        assert!(
            capability
                .uncontended_close(session.clone(), open.clone(), hex(&[71; 16]))
                .await
                .unwrap()
                .is_some()
        );
        n.client
            .complete(token.clone(), hex(&[71; 16]), hex(&[72; 16]))
            .await
            .unwrap();
        assert!(
            capability
                .uncontended_close(session.clone(), open.clone(), hex(&[71; 16]))
                .await
                .unwrap()
                .is_none()
        );
        capability.opens().retire(open.clone()).await.unwrap();
        assert!(
            capability
                .uncontended_close(session, open, hex(&[71; 16]))
                .await
                .unwrap()
                .is_none()
        );
        n.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn native_retry_validation_work_close_prefix_keeps_other_callbacks_and_windows_policy() {
    for cleanup in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let n = setup(&tmp.path().join("close callback fallback"), 16).await;
        let capability = crate::chunk_mount::Client::new(
            n.storage.cache().owner,
            crate::journal::Options::default(),
            n.key.clone(),
        );
        capability
            .opens()
            .callback(
                n.token.input().id.clone(),
                hex(&[69; 16]),
                if cleanup {
                    opens::Callback::Cleanup { flags: 0 }
                } else {
                    opens::Callback::SetDelete { delete: false }
                },
            )
            .await
            .unwrap();
        close_native(&n, &n.token, 71).await.unwrap();
        assert!(
            capability
                .uncontended_close(
                    n.token.input().session.clone(),
                    n.token.input().id.clone(),
                    hex(&[71; 16])
                )
                .await
                .unwrap()
                .is_none()
        );
        n.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn native_retry_validation_work_close_prefix_does_not_change_nonposix_policy() {
    for retry in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let n = setup_mode_posix(
            &tmp.path().join("nonposix Close fallback"),
            16,
            retry,
            false,
        )
        .await;
        close_native(&n, &n.token, 71).await.unwrap();
        let capability = crate::chunk_mount::Client::new(
            n.storage.cache().owner,
            crate::journal::Options::default(),
            n.key.clone(),
        );
        assert!(
            capability
                .uncontended_close(
                    n.token.input().session.clone(),
                    n.token.input().id.clone(),
                    hex(&[71; 16])
                )
                .await
                .unwrap()
                .is_none()
        );
        n.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn native_retry_validation_work_quiet_recovery_preserves_root_work_and_pins() {
    let tmp = tempfile::tempdir().unwrap();
    let n = setup_mode(&tmp.path().join("quiet recovery work"), 16, true).await;
    let f = fixture();
    assert!(n.client.quiet_recovery_targets().await.unwrap().is_some());
    let dirty = n
        .client
        .write(n.token.clone(), context(&n.view), [2; 32], 0, vec![4; 17])
        .await
        .unwrap();
    // Dirty source without an admitted publication was not recovery-list work.
    assert!(n.client.quiet_recovery_targets().await.unwrap().is_some());
    let prepared = n
        .client
        .prepare(n.token.clone(), context(&dirty))
        .await
        .unwrap();
    assert!(n.client.quiet_recovery_targets().await.unwrap().is_some());
    let plan = prepared.id().unwrap();
    let binding = dirty.binding().clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let s = &mut store.state;
            let retry = prepared.retry_operation().unwrap();
            let info = pj(s).append(plan, prepared.cursor(), Candidate::Bytes(vec![4; 17]))?;
            crate::sparse_views::preparation::tests::fixture::finish(s, info);
            j(s).check(plan, 0, None)?;
            physical(s, plan);
            j(s).begin(
                plan,
                f.node.clone(),
                f.store,
                f.node.revision,
                retry.publish_operation()?,
            )?;
            Ok(())
        })
        .await
        .unwrap();
    // A newly admitted root after the earlier selection is now recoverable work.
    assert!(n.client.quiet_recovery_targets().await.unwrap().is_none());
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let s = &mut store.state;
            let root = j(s).get(plan)?;
            lineage::commit(s, root);
            j(s).acknowledge_scoped(
                views::ScopedOperation::new(&binding, [11; 32])?,
                plan,
                views::Options::default(),
            )?;
            Ok(())
        })
        .await
        .unwrap();
    close_native(&n, &n.token, 71).await.unwrap();
    n.client
        .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
        .await
        .unwrap();
    let capability = crate::chunk_mount::Client::new(
        n.storage.cache().owner,
        crate::journal::Options::default(),
        n.key.clone(),
    );
    capability
        .opens()
        .retire(n.token.input().id.clone())
        .await
        .unwrap();
    let targets = n.client.quiet_recovery_targets().await.unwrap().unwrap();
    let reader = n
        .storage
        .sparse_views(views::Options::default())
        .unwrap()
        .guard(n.view.id())
        .await
        .unwrap();
    assert!(!n.client.retire_linked(targets[0].clone()).await.unwrap());
    drop(reader);
    assert!(n.client.retire_linked(targets[0].clone()).await.unwrap());
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_retry_validation_work_quiet_recovery_requires_mode_and_empty_role_integrity() {
    let tmp = tempfile::tempdir().unwrap();
    let old = setup(&tmp.path().join("ordinary native fallback"), 16).await;
    assert!(old.client.quiet_recovery_targets().await.unwrap().is_none());
    old.storage.close().await.unwrap();
    let n = setup_mode(&tmp.path().join("empty creator role"), 16, true).await;
    let before = n
        .storage
        .cache()
        .owner
        .call(|store| {
            assert_eq!(
                store.state.connection.execute(
                    "INSERT INTO manifest_creation_role VALUES(1,1,zeroblob(32))",
                    []
                )?,
                1
            );
            assert_eq!(
                store.state.connection.query_row(
                    "SELECT count(*) FROM manifest_creations",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                0
            );
            Ok(native_snapshot(&store.state))
        })
        .await
        .unwrap();
    assert_eq!(
        n.client
            .quiet_recovery_targets()
            .await
            .unwrap_err()
            .downcast_ref::<crate::manifest_creation::Failure>(),
        Some(&crate::manifest_creation::Failure::Corrupt)
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

// Dirty-save primitive costs, including a compact anchor and a zero payload.
// Modeled physical receipts and owner commands do not reproduce SDK/FS timing.
#[tokio::test]
async fn native_retry_validation_work_dirty_save_phases() {
    let tmp = tempfile::tempdir().unwrap();
    let mut n = setup_mode(&tmp.path().join("dirty phase cost"), 16, true).await;
    let mut f = fixture();
    for cycle in 0..3_u64 {
        macro_rules! measured {
            ($name:literal, $work:expr) => {{
                let before = native_work(&n).await;
                let identities = n.storage.cache().owner.call(|_| Ok(crate::sparse_views::preparation::model::identity_work())).await.unwrap();
                let start = std::time::Instant::now();
                let result = $work;
                let elapsed = start.elapsed();
                let after = native_work(&n).await;
                let current = n.storage.cache().owner.call(|_| Ok(crate::sparse_views::preparation::model::identity_work())).await.unwrap();
                let identity_delta = std::array::from_fn::<_,3,_>(|j| current[j] - identities[j]);
                eprintln!("NATIVE_ID_WORK cycle={cycle} phase={} input_hashes_info_ids_construction_attempts={identity_delta:?}", $name);
                let delta = std::array::from_fn::<_, 9, _>(|j| after[j] - before[j]);
                if cycle > 0 {
                    match $name {
                        "prepare" => assert_eq!(delta[0], 5),
                        "collect" => assert!(delta[0] <= 15, "collection repeats pre-cut snapshots: {delta:?}"),
                        _ => {}
                    }
                }
                eprintln!("NATIVE_DIRTY_WORK cycle={cycle} phase={} ms={} scopes_sessions_events_queries_opens_mounts_sources_preps_epoch_sources={delta:?}", $name, elapsed.as_millis());
                result
            }};
        }
        let bytes = vec![(cycle % 251) as u8; 17];
        let context =
            Context::new(&n.view, f.base.root.encode().to_vec(), f.base.page.encode()).unwrap();
        n.view = measured!(
            "write",
            n.client
                .write(n.token.clone(), context, [2; 32], 0, bytes.clone())
                .await
                .unwrap()
        );
        let context =
            Context::new(&n.view, f.base.root.encode().to_vec(), f.base.page.encode()).unwrap();
        let info = measured!(
            "prepare",
            n.client.prepare(n.token.clone(), context).await.unwrap()
        );
        let plan = info.id().unwrap();
        f = n.storage.cache().owner.call(move |store| {
            let s = &mut store.state;
            macro_rules! phase {
                ($name:literal, $work:expr) => {{
                    let before = native_work_here();
                    let identities = crate::sparse_views::preparation::model::identity_work();
                    let start = std::time::Instant::now();
                    let result = $work;
                    let elapsed = start.elapsed();
                    let after = native_work_here();
                    let current = crate::sparse_views::preparation::model::identity_work();
                    let identity_delta = std::array::from_fn::<_,3,_>(|j| current[j] - identities[j]);
                    eprintln!("NATIVE_ID_WORK cycle={cycle} phase={} input_hashes_info_ids_construction_attempts={identity_delta:?}", $name);
                    let delta = std::array::from_fn::<_, 9, _>(|j| after[j] - before[j]);
                    if cycle > 0 {
                        match $name {
                            "physical staging" => assert_eq!(delta[0], 22),
                            "check base" => assert_eq!(delta[0], 1),
                            "publication begin" => assert_eq!(delta[0], 5),
                            "acknowledge" => assert_eq!(delta[0], 4),
                            _ => {}
                        }
                    }
                    eprintln!("NATIVE_DIRTY_WORK cycle={cycle} phase={} ms={} scopes_sessions_events_queries_opens_mounts_sources_preps_epoch_sources={delta:?}", $name, elapsed.as_millis());
                    result
                }};
            }
            let retry = info.retry_operation().unwrap();
            let info = phase!("append data", pj(s).append(plan, info.cursor(), Candidate::Bytes(bytes.clone()))?);
            let info = phase!("finish pages", crate::sparse_views::preparation::tests::fixture::finish(s, info));
            phase!("physical staging", physical(s, plan));
            phase!("semantic check", j(s).check(plan, 0, None)?);
            phase!("check base", j(s).check_base(plan, &f.node)?);
            let r = phase!("publication begin", j(s).begin(plan, f.node.clone(), f.store.clone(), f.node.revision, retry.publish_operation()?)?);
            let r = phase!("publication commit", {
                let sequence = r.input.reserve.expected_sequence + 1;
                let reserve = ns::Receipt { sequence, node: r.input.base.clone(), artifact: Some(ns::Artifact::Reservation(r.input.reservation()?)) };
                j(s).reserved(plan, reserve)?;
                let loads = crate::sparse_views::preparation::publication::retention::inspection_counts();
                let r = j(s).prepare_publish(plan, sequence)?;
                let after = crate::sparse_views::preparation::publication::retention::inspection_counts();
                assert_eq!(after.0 - loads.0, 2, "fresh prepare_publish pre/post anchors");
                if cycle > 0 { assert_eq!(after.1 - loads.1, 2); }
                let mut receipt = published(&r);
                receipt.sequence = sequence + 1;
                receipt.node.revision = sequence + 1;
                j(s).published(plan, receipt)?
            });
            let sequence = r.input.reserve.expected_sequence + 1;
            let snapshot = native_snapshot(s);
            let loads = crate::sparse_views::preparation::publication::retention::inspection_counts();
            assert_eq!(j(s).prepare_publish(plan, sequence)?, r);
            assert_eq!(crate::sparse_views::preparation::publication::retention::inspection_counts().0 - loads.0, 2);
            assert!(j(s).prepare_publish(plan, sequence + 1).is_err());
            assert_eq!(native_snapshot(s), snapshot);
            phase!("acknowledge", j(s).acknowledge_scoped(views::ScopedOperation::new(info.view().binding(), [11; 32])?, plan, views::Options::default())?);
            let page = crate::journal::Journal::new(s, crate::journal::Options::default())?.read(r.physical.generation, 0, r.physical.size as usize)?;
            let page = s3drive_core::manifest::Page::decode(&page)?;
            f.base = Base { root: info.root()?.unwrap(), page: page.clone(), pages: BTreeMap::from([((0, 0), page)]) };
            f.node = r.receipt().unwrap().node.clone();
            f.binding = views::Journal::new(s, views::Options::default())?.current_head(&f.binding)?.binding().clone();
            f.bytes = bytes;
            Ok(f)
        }).await.unwrap();
        let callback = hex(&hash(b"dirty-cost-close", &n.token.input().id).unwrap()[..16]);
        let completion = hex(&hash(b"dirty-cost-complete", &n.token.input().id).unwrap()[..16]);
        n.token.observe_close();
        measured!(
            "Close",
            n.client
                .close(n.token.clone(), callback.clone())
                .await
                .unwrap()
        );
        measured!(
            "complete",
            n.client
                .complete(n.token.clone(), callback, completion)
                .await
                .unwrap()
        );
        let key = n.key.clone();
        let open = n.token.input().id.clone();
        measured!(
            "retire open",
            n.storage
                .cache()
                .owner
                .call(move |store| {
                    opens(&mut store.state, &key).retire(&open)?;
                    Ok(())
                })
                .await
                .unwrap()
        );
        assert!(measured!(
            "collect",
            n.client.retire_linked(f.node.id.clone()).await.unwrap()
        ));
        if cycle < 2 {
            measured!("reopen", retry_reopen(&mut n, &f).await);
        }
    }
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_retry_validation_work_current_scope_factory_keeps_modes_and_errors() {
    for retry in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let n = setup_mode(&tmp.path().join("scope factory"), 16, retry).await;
        n.storage
            .cache()
            .owner
            .call(move |store| {
                let s = &mut store.state;
                let before = native_snapshot(s);
                let tx = s.connection.transaction()?;
                assert_eq!(
                    crate::chunk_mount::read_scope_current(&tx)?.is_some(),
                    retry
                );
                tx.execute("UPDATE chunk_mount_role SET state_hash=zeroblob(32)", [])?;
                assert!(crate::chunk_mount::read_scope_current(&tx).is_err());
                tx.rollback()?;
                assert_eq!(native_snapshot(s), before);
                Ok(())
            })
            .await
            .unwrap();
        n.storage.close().await.unwrap();
    }
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("absent mount")).unwrap();
    let tx = s.connection.transaction().unwrap();
    assert!(
        crate::chunk_mount::read_scope_current(&tx)
            .unwrap()
            .is_none()
    );
    tx.rollback().unwrap();
}

#[tokio::test]
async fn native_retry_validation_work_materialization_requires_dependencies_in_both_modes() {
    for retry in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let mut n = setup_mode(&tmp.path().join("materialize dependencies"), 16, retry).await;
        n.view = n
            .client
            .write(n.token.clone(), context(&n.view), [9; 32], 0, vec![7; 17])
            .await
            .unwrap();
        let info = n
            .client
            .prepare(n.token.clone(), context(&n.view))
            .await
            .unwrap();
        let plan = info.id().unwrap();
        n.storage
            .cache()
            .owner
            .call(move |store| {
                let s = &mut store.state;
                let info = pj(s).append(plan, info.cursor(), Candidate::Bytes(vec![7; 17]))?;
                crate::sparse_views::preparation::tests::fixture::finish(s, info);
                let before = native_snapshot(s);
                let root = crate::sparse_views::preparation::Position { level: 0, slot: 0 };
                assert!(
                    pj(s)
                        .materialize(plan, root, crate::journal::Options::default())
                        .is_err()
                );
                assert_eq!(native_snapshot(s), before);
                let data = crate::sparse_views::preparation::Position { level: -1, slot: 0 };
                assert!(
                    pj(s)
                        .materialize(plan, data, crate::journal::Options::default())?
                        .is_some()
                );
                let staged = native_snapshot(s);
                // Merely staged data is not an acknowledged physical dependency.
                assert!(
                    pj(s)
                        .materialize(plan, root, crate::journal::Options::default())
                        .is_err()
                );
                assert_eq!(native_snapshot(s), staged);
                physical(s, plan);
                assert!(
                    pj(s)
                        .materialize(plan, root, crate::journal::Options::default())?
                        .is_some()
                );
                Ok(())
            })
            .await
            .unwrap();
        n.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn native_retry_validation_work_post_materialization_rechecks_late_damage() {
    let tmp = tempfile::tempdir().unwrap();
    let mut n = setup_mode(&tmp.path().join("post materialize freshness"), 16, true).await;
    let original = n.token.input().id.clone();
    let f = retry_cycle_mode(&n, fixture(), 1, false).await;
    retry_reopen(&mut n, &f).await;
    let context =
        Context::new(&n.view, f.base.root.encode().to_vec(), f.base.page.encode()).unwrap();
    n.view = n
        .client
        .write(n.token.clone(), context, [9; 32], 0, vec![7; 17])
        .await
        .unwrap();
    let context =
        Context::new(&n.view, f.base.root.encode().to_vec(), f.base.page.encode()).unwrap();
    let info = n.client.prepare(n.token.clone(), context).await.unwrap();
    let plan = info.id().unwrap();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let s = &mut store.state;
            let info = pj(s).append(plan, info.cursor(), Candidate::Bytes(vec![7; 17]))?;
            crate::sparse_views::preparation::tests::fixture::finish(s, info);
            let position = crate::sparse_views::preparation::Position { level: -1, slot: 0 };
            let transfer = pj(s)
                .materialize(plan, position, crate::journal::Options::default())?
                .unwrap();
            let generation = transfer.generation().unwrap();
            let expected = crate::sparse_views::preparation::ledger::inspect(&s.connection)?;
            let before = native_snapshot(s);
            for case in 0..4 {
                let tx = s.connection.transaction()?;
                {
                    let scope = crate::chunk_mount::read_scope_current(&tx)?.unwrap();
                    crate::sparse_views::preparation::ledger::inspect_scoped(&scope)?;
                }
                let mut selected = generation;
                let mut bytes = vec![7; 17];
                match case {
                    0 => crate::namespace_client::opens::rewrite_epoch_for_test(&tx, &original)?,
                    1 => {
                        tx.execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])?;
                    }
                    2 => {
                        selected =
                            crate::journal::GenerationId::from_database(generation.value() + 1000)?;
                    }
                    _ => bytes[0] = 8,
                }
                // Actual final helper, not an injected mid-staging callback.
                assert!(
                    crate::sparse_views::preparation::transfers::validate_materialized(
                        &tx, &expected, plan, position, &transfer, selected, &bytes
                    )
                    .is_err()
                );
                tx.rollback()?;
                assert_eq!(native_snapshot(s), before);
            }
            let tx = s.connection.transaction()?;
            crate::sparse_views::preparation::transfers::validate_materialized(
                &tx, &expected, plan, position, &transfer, generation, &[7; 17],
            )?;
            tx.rollback()?;
            Ok(())
        })
        .await
        .unwrap();
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_retry_validation_work_checkpoint_reuse_rechecks_new_readers() {
    let tmp = tempfile::tempdir().unwrap();
    let n = setup_mode(&tmp.path().join("checkpoint snapshot reuse"), 16, true).await;
    retry_cycle_mode(&n, fixture(), 1, false).await;
    let (proof, view, generation) = n
        .storage
        .cache()
        .owner
        .call(|store| {
            let s = &mut store.state;
            let tx = s.connection.transaction()?;
            let before = crate::sparse_views::preparation::ledger::inspect(&tx)?;
            let plan = *before.plans.keys().next().unwrap();
            let meta = ledger::inspect(&tx)?;
            assert!(crate::sparse_views::preparation::retry::enabled(&tx)?);
            let expected = crate::sparse_views::preparation::retry::checkpoint(&tx, plan, &[])?;
            let actual = crate::sparse_views::preparation::retry::checkpoint_snapshot(
                &tx,
                plan,
                &[],
                &before,
                &meta,
            )?;
            assert_eq!(actual, expected);
            assert!(actual.unblocked());
            let view = before.info(plan)?.view().id();
            let generation = meta.records.get(&plan).unwrap().physical.generation;
            tx.rollback()?;
            Ok((actual, view, generation))
        })
        .await
        .unwrap();
    let local = n.storage.sparse_views(views::Options::default()).unwrap();
    let logical = local.guard(view).await.unwrap();
    let cp = proof.clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let before = native_snapshot(&store.state);
            assert!(
                j(&mut store.state)
                    .retire_checkpoint(&cp, views::Options::default())
                    .is_err()
            );
            assert_eq!(native_snapshot(&store.state), before);
            Ok(())
        })
        .await
        .unwrap();
    drop(logical);
    n.storage.cache().owner.call(|_| Ok(())).await.unwrap();
    let physical = n.storage.staging().snapshot(generation).await.unwrap();
    let cp = proof.clone();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let before = native_snapshot(&store.state);
            assert!(
                j(&mut store.state)
                    .retire_checkpoint(&cp, views::Options::default())
                    .is_err()
            );
            assert_eq!(native_snapshot(&store.state), before);
            Ok(())
        })
        .await
        .unwrap();
    drop(physical);
    n.storage
        .cache()
        .owner
        .call(move |store| {
            assert_eq!(
                j(&mut store.state).retire_checkpoint(&proof, views::Options::default())?,
                proof.anchor()
            );
            crate::chunk_mount::validate_all(&store.state.connection)?;
            Ok(())
        })
        .await
        .unwrap();
    n.storage.close().await.unwrap();
}

// Isolation is shared only inside one synchronous inspection. It is not a
// source/native proof, and every subsequent call must re-read changed evidence.
#[tokio::test]
async fn native_retry_validation_work_isolation_context_rechecks_two_anchors_and_late_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut n = setup_mode(&tmp.path().join("two anchor isolation"), 16, true).await;
    retry_cycle(&n, fixture(), 1).await;
    let f = fixture_node([3; 16]);
    let mut input = n.token.input().clone();
    input.node = f.node.id.clone();
    let binding = f.binding.clone();
    let key = n.key.clone();
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
            let activation = hex(&hash(b"second-target-activation", &input.id)?[..16]);
            opens(s, &key).prepare(input.clone())?;
            opens(s, &key).activate(&input.id, &activation)?;
            let view = views::Journal::new(s, views::Options::default())?.begin_scoped(
                views::ScopedOperation::new(&binding, [1; 32])?,
                binding,
                17,
            )?;
            Ok((input.id, view))
        })
        .await
        .unwrap();
    n.token = n.client.bind(id).await.unwrap();
    n.view = view;
    let f = retry_cycle(&n, f, 2).await;
    retry_reopen(&mut n, &f).await;
    retry_cycle_mode(&n, f, 3, false).await;
    let key = n.key.clone();
    n.storage.cache().owner.call(move |store| {
        let s = &mut store.state;
        let anchors = crate::sparse_views::preparation::publication::retention::load(&s.connection)?;
        assert_eq!(anchors.len(), 2);
        let retained: i64 = s.connection.query_row("SELECT count(*) FROM windows_open_journal", [], |r|r.get(0))?;
        let events: i64 = s.connection.query_row("SELECT count(*) FROM windows_open_events", [], |r|r.get(0))?;
        assert!(retained > 0 && events > 0);
        for prep in [false, true] {
            let before = native_work_here();
            if prep { crate::sparse_views::preparation::ledger::inspect(&s.connection)?; }
            else { views::ledger::inspect(&s.connection)?; }
            let after = native_work_here();
            assert_eq!(after[0] - before[0], 1, "one isolation scan, prep={prep}");
            assert_eq!(after[4] - before[4], retained as usize * if prep { 2 } else { 1 }, "every retained open, prep={prep}");
            assert_eq!(after[2] - before[2], events as usize * if prep { 2 } else { 1 }, "every event, prep={prep}");
        }
        let original = native_snapshot(s);
        {
            let tx = s.connection.transaction()?;
            let before = crate::sparse_views::preparation::publication::retention::inspection_counts();
            let headers_before = views::ledger::inspection_count();
            let roots_before = ledger::inspection_count();
            let bodies_before = views::ledger::body_read_count();
            let scope = crate::chunk_mount::read_scope(&tx, &key)?;
            assert!(scope.source_metadata()?.is_some());
            let constructed = crate::sparse_views::preparation::publication::retention::inspection_counts();
            assert_eq!((constructed.0 - before.0, constructed.1 - before.1), (1, 2));
            // Source/native metadata/preparation reuse only the fully validated
            // two-anchor set in this same unchanged read transaction.
            views::ledger::inspect_scoped(&scope)?;
            crate::sparse_views::preparation::ledger::inspect_scoped(&scope)?;
            assert_eq!(crate::sparse_views::preparation::publication::retention::inspection_counts(), constructed);
            assert_eq!(views::ledger::inspection_count() - headers_before, 1, "one full header/history decode");
            assert_eq!(ledger::inspection_count() - roots_before, 1, "one complete root metadata decode");
            assert_eq!(views::ledger::body_read_count() - bodies_before, 2, "both full reads independently inspect every block");
            drop(scope);
            tx.rollback()?;
        }
        for misuse in 0..6 {
            let tx = s.connection.transaction()?;
            let scope = crate::chunk_mount::read_scope(&tx, &key)?;
            match misuse {
                0 => { tx.execute("UPDATE sparse_retry_mode SET state_hash=state_hash", [])?; }
                1 => { tx.execute_batch("SAVEPOINT scope_misuse; UPDATE sparse_retry_mode SET state_hash=state_hash; ROLLBACK TO scope_misuse; RELEASE scope_misuse")?; }
                2 => { tx.execute_batch("CREATE TRIGGER scope_misuse AFTER UPDATE ON sparse_views BEGIN SELECT 1; END")?; }
                3 => { tx.execute_batch("CREATE TEMP TABLE scope_misuse(value INTEGER)")?; }
                4 => { let version:i64 = tx.pragma_query_value(None, "user_version", |r|r.get(0))?; tx.pragma_update(None, "user_version", version + 1)?; }
                _ => { let version:i64 = tx.pragma_query_value(Some("main"), "schema_version", |r|r.get(0))?; tx.pragma_update(Some("main"), "schema_version", version + 1)?; }
            }
            assert!(crate::sparse_views::preparation::publication::retention::load_scoped(&scope).is_err(), "stale anchor scope misuse={misuse}");
            assert!(views::ledger::inspect_scoped(&scope).is_err(), "stale source scope misuse={misuse}");
            assert!(crate::sparse_views::preparation::ledger::inspect_scoped(&scope).is_err(), "stale prep scope misuse={misuse}");
            drop(scope);
            if matches!(misuse, 0 | 1 | 3 | 5) {
                // Legitimate unchanged data/new schema cookie can be validated
                // again. The previous scope must not be silently refreshed.
                let fresh = crate::chunk_mount::read_scope(&tx, &key)?;
                views::ledger::inspect_scoped(&fresh)?;
            }
            tx.rollback()?;
            assert_eq!(native_snapshot(s), original);
        }
        {
            let tx = s.connection.transaction()?;
            let scope = crate::chunk_mount::read_scope(&tx, &key)?;
            // Deliberate test-only violation: an ended transaction is refused.
            // Raw COMMIT/BEGIN is forbidden in production scoped call paths.
            tx.execute_batch("COMMIT")?;
            assert!(crate::sparse_views::preparation::publication::retention::load_scoped(&scope).is_err());
            assert!(views::ledger::inspect_scoped(&scope).is_err());
            drop(scope);
            drop(tx);
            assert!(s.connection.is_autocommit());
            assert_eq!(native_snapshot(s), original);
        }

        for body_case in 0..4 {
            let tx = s.connection.transaction()?;
            let scope = crate::chunk_mount::read_scope(&tx, &key)?;
            views::ledger::inspect_scoped(&scope)?;
            let changed = match body_case {
                0 => tx.execute("DELETE FROM sparse_view_blocks WHERE block=(SELECT block FROM sparse_view_blocks LIMIT 1)", [])?,
                1 => tx.execute("UPDATE sparse_view_blocks SET data=zeroblob(length(data))", [])?,
                2 => {
                    let body = vec![239u8; 19];
                    tx.execute("INSERT INTO sparse_view_blocks(block,data) VALUES(?1,?2)", params![crate::sparse_views::digest(&body).as_slice(), body])?
                }
                _ => tx.execute("UPDATE sparse_view_blocks SET data=substr(data,1,length(data)-1)", [])?,
            };
            assert!(changed > 0);
            assert!(views::ledger::inspect_scoped(&scope).is_err(), "old scope body case={body_case}");
            drop(scope);
            // Header-only construction must not invent a source-byte proof.
            let fresh = crate::chunk_mount::read_scope(&tx, &key)?;
            assert!(fresh.source_metadata()?.is_some());
            assert!(views::ledger::inspect_scoped(&fresh).is_err(), "fresh full source body case={body_case}");
            assert!(crate::sparse_views::preparation::ledger::inspect_scoped(&fresh).is_err(), "fresh full prep body case={body_case}");
            drop(fresh);
            tx.rollback()?;
            assert_eq!(native_snapshot(s), original);
        }
        for anchor in anchors.values() {
            assert_eq!(anchor.bytes()?, serde_json::to_vec(anchor)?.len());
            let mut value = serde_json::to_value(anchor)?;
            value["canonical_len"] = serde_json::json!(1);
            assert!(serde_json::from_value::<crate::sparse_views::preparation::publication::retention::Anchor>(value).is_err());
        }
        // Every damage is introduced AFTER successful inspections in this same
        // transaction. No context may be reused by the later invocations.
        for case in 0..14 {
            let tx = s.connection.transaction()?;
            views::ledger::inspect(&tx)?;
            crate::sparse_views::preparation::ledger::inspect(&tx)?;
            match case {
                0 => { assert_eq!(tx.execute("UPDATE sparse_retry_mode SET state_hash=zeroblob(32)", [])?, 1); }
                1 => { assert_eq!(tx.execute("UPDATE sparse_scoped_mode SET state_hash=zeroblob(32)", [])?, 1); }
                2 => { assert!(tx.execute("UPDATE windows_mount_sessions SET state_hash=zeroblob(32)", [])? > 0); }
                3 => { assert!(tx.execute("UPDATE windows_open_events SET state_hash=zeroblob(32)", [])? > 0); }
                4 => { assert!(tx.execute("UPDATE sparse_view_blocks SET data=zeroblob(length(data))", [])? > 0); }
                5 | 6 | 10 | 11 => {
                    // Coherently reseal either target's local anchor and role.
                    // The independent original Close witness must still reject.
                    let target = *anchors.keys().nth(if case < 7 { case - 5 } else { case - 10 }).unwrap();
                    let mut changed = anchors.clone();
                    let mut raw = serde_json::to_value(changed.get(&target).unwrap())?;
                    if case < 7 { raw["native"]["close"] = serde_json::json!(hex(&[231; 16])); }
                    else { raw["publication"]["physical"]["checksum"] = serde_json::to_value([232u8;32])?; }
                    let anchor: crate::sparse_views::preparation::publication::retention::Anchor = serde_json::from_value(raw)?;
                    tx.execute("UPDATE sparse_retention_anchors SET record=?1,state_hash=?2 WHERE target=?3", params![serde_json::to_vec(&anchor)?, hash(b"s3drive-retention-anchor-v1\0", &anchor)?.as_slice(), target.as_slice()])?;
                    changed.insert(target, anchor);
                    let owner: Vec<u8> = tx.query_row("SELECT owner FROM sparse_retention_role", [], |r|r.get(0))?;
                    let owner: Id = owner.try_into().unwrap();
                    let sha = hash(b"s3drive-retention-role-v1\0", &(owner, changed.iter().collect::<Vec<_>>()))?;
                    tx.execute("UPDATE sparse_retention_role SET state_hash=?1", [sha.as_slice()])?;
                }
                7 | 8 => {
                    let mount = crate::chunk_mount::load(&tx)?.unwrap();
                    let mut raw = serde_json::to_value(mount)?;
                    if case == 7 { raw["terminals"] = serde_json::json!({}); }
                    else { raw["frontier"] = serde_json::json!(0); }
                    let mount: crate::chunk_mount::Binding = serde_json::from_value(raw)?;
                    tx.execute("UPDATE chunk_mount_role SET record=?1,state_hash=?2", params![serde_json::to_vec(&mount)?, hash(b"s3drive-chunk-mount-binding-v1\0", &mount)?.as_slice()])?;
                }
                12 | 13 => {
                    let (target, raw): (Vec<u8>, Vec<u8>) = tx.query_row("SELECT target,record FROM sparse_retention_anchors ORDER BY target LIMIT 1", [], |r|Ok((r.get(0)?,r.get(1)?)))?;
                    let noncanonical = if case == 12 {
                        let mut bytes = vec![b' ']; bytes.extend_from_slice(&raw); bytes
                    } else {
                        let value: serde_json::Value = serde_json::from_slice(&raw)?;
                        let mut bytes = b"{".to_vec();
                        for (index, (key, value)) in value.as_object().unwrap().iter().rev().enumerate() {
                            if index != 0 { bytes.push(b','); }
                            bytes.extend(serde_json::to_vec(key)?); bytes.push(b':');
                            bytes.extend(serde_json::to_vec(value)?);
                        }
                        bytes.push(b'}'); bytes
                    };
                    assert_ne!(noncanonical, raw);
                    assert_eq!(tx.execute("UPDATE sparse_retention_anchors SET record=?1 WHERE target=?2", params![noncanonical,target])?, 1);
                }
                _ => {
                    assert_eq!(tx.execute("INSERT INTO windows_mount_sessions SELECT ?1,volume_id,?2,owner_id,zeroblob(32) FROM windows_mount_sessions LIMIT 1", params![hex(&[77;16]),hex(&[78;16])])?, 1);
                }
            }
            if case == 4 {
                // ReadScope validates native/source metadata, not BLOB bodies;
                // the actual full scoped preparation inspection must read them.
                let fresh = crate::chunk_mount::read_scope(&tx, &key)?;
                assert!(crate::sparse_views::preparation::ledger::inspect_scoped(&fresh).is_err());
            } else {
                assert!(crate::chunk_mount::read_scope(&tx, &key).is_err(), "new scope case {case}");
            }
            assert!(views::ledger::inspect(&tx).is_err(), "source case {case}");
            assert!(crate::sparse_views::preparation::ledger::inspect(&tx).is_err(), "prep case {case}");
            tx.rollback()?;
            assert_eq!(native_snapshot(s), original);
            views::ledger::inspect(&s.connection)?;
            crate::sparse_views::preparation::ledger::inspect(&s.connection)?;
        }
        Ok(())
    }).await.unwrap();
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_retry_validation_work_isolation_context_is_fresh_in_both_native_modes() {
    for retry in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let n = setup_mode(&tmp.path().join("mode inspection freshness"), 16, retry).await;
        n.client
            .write(n.token.clone(), context(&n.view), [91; 32], 0, vec![9; 17])
            .await
            .unwrap();
        n.storage
            .cache()
            .owner
            .call(move |store| {
                let s = &mut store.state;
                let original = native_snapshot(s);
                for case in 0..3 {
                    let tx = s.connection.transaction()?;
                    // This is a legitimate dirty, not-yet-prepared intermediate
                    // state. Isolation reuse must not add a completion requirement.
                    views::ledger::inspect(&tx)?;
                    crate::sparse_views::preparation::ledger::inspect(&tx)?;
                    let changed = match case {
                        0 => {
                            tx.execute("UPDATE sparse_scoped_mode SET state_hash=zeroblob(32)", [])?
                        }
                        1 => tx.execute(
                            "UPDATE windows_mount_sessions SET state_hash=zeroblob(32)",
                            [],
                        )?,
                        _ => tx.execute(
                            "UPDATE sparse_view_blocks SET data=zeroblob(length(data))",
                            [],
                        )?,
                    };
                    assert!(changed > 0);
                    assert!(
                        views::ledger::inspect(&tx).is_err(),
                        "mode={retry}, source case={case}"
                    );
                    assert!(
                        crate::sparse_views::preparation::ledger::inspect(&tx).is_err(),
                        "mode={retry}, prep case={case}"
                    );
                    tx.rollback()?;
                    assert_eq!(native_snapshot(s), original);
                }
                Ok(())
            })
            .await
            .unwrap();
        n.storage.close().await.unwrap();
    }
}

async fn anchor_work(n: &Native) -> (usize, usize) {
    n.storage
        .cache()
        .owner
        .call(|_| Ok(crate::sparse_views::preparation::publication::retention::inspection_counts()))
        .await
        .unwrap()
}
#[tokio::test]
async fn native_retry_validation_work_source_commands_keep_pins_and_fresh_anchors() {
    for linked in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let n = setup_mode(&tmp.path().join("source commands"), 16, linked).await;
        let f = retry_cycle_mode(&n, fixture(), 1, linked).await;
        let local = n.storage.sparse_views(views::Options::default()).unwrap();
        let current = local.current_head(f.binding.clone()).await.unwrap();
        macro_rules! counted {
            ($loads:expr, $work:expr) => {{
                let before = anchor_work(&n).await;
                let value = $work;
                let after = anchor_work(&n).await;
                assert_eq!(after.0 - before.0, $loads);
                assert_eq!(after.1 - before.1, if linked { $loads } else { 0 });
                value
            }};
        }
        assert_eq!(
            counted!(1, local.view(current.id()).await.unwrap()),
            current
        );
        assert_eq!(
            counted!(1, local.head(f.binding.clone()).await.unwrap()),
            current
        );
        assert_eq!(
            counted!(1, local.current_head(f.binding.clone()).await.unwrap()),
            current
        );
        assert_eq!(
            counted!(1, local.heads().await.unwrap()),
            vec![current.clone()]
        );
        let expected = counted!(1, local.read(current.id(), 0, 17).await.unwrap());
        let guard = counted!(2, local.guard(current.id()).await.unwrap());
        assert_eq!(guard.view(), &current);
        let clone = guard.clone();
        drop(guard);
        assert_eq!(counted!(1, clone.read(0, 17).await.unwrap()), expected);
        let before = anchor_work(&n).await;
        drop(clone);
        // An owner barrier drains the final-drop release before reading counts.
        let after = anchor_work(&n).await;
        assert_eq!(after.0 - before.0, 2);
        let pin = counted!(2, local.pin(current.id()).await.unwrap());
        assert_eq!(
            counted!(1, local.read_pinned(pin.clone(), 0, 17).await.unwrap()),
            expected
        );
        if linked {
            let saved = n
                .storage
                .cache()
                .owner
                .call(|store| {
                    let before = native_snapshot(&store.state);
                    let hash: Vec<u8> = store.state.connection.query_row(
                        "SELECT state_hash FROM sparse_retention_anchors",
                        [],
                        |r| r.get(0),
                    )?;
                    assert_eq!(
                        store.state.connection.execute(
                            "UPDATE sparse_retention_anchors SET state_hash=zeroblob(32)",
                            []
                        )?,
                        1
                    );
                    Ok((before, hash))
                })
                .await
                .unwrap();
            assert!(local.read_pinned(pin.clone(), 0, 17).await.is_err());
            assert!(local.guard(current.id()).await.is_err());
            assert!(local.heads().await.is_err());
            assert!(local.view(current.id()).await.is_err());
            n.storage
                .cache()
                .owner
                .call(move |store| {
                    assert_eq!(
                        store.state.connection.execute(
                            "UPDATE sparse_retention_anchors SET state_hash=?1",
                            [saved.1]
                        )?,
                        1
                    );
                    assert_eq!(native_snapshot(&store.state), saved.0);
                    Ok(())
                })
                .await
                .unwrap();
            assert_eq!(
                counted!(1, local.read_pinned(pin.clone(), 0, 17).await.unwrap()),
                expected
            );
        }
        assert!(counted!(2, local.release_pin(pin.clone()).await.unwrap()));
        assert!(local.read_pinned(pin.clone(), 0, 17).await.is_err());
        assert!(!local.release_pin(pin).await.unwrap());
        n.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn native_retry_validation_work_proof_hash_reads_independent_persisted_digests() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("independent SHA256 current format");
    let n = setup_mode(&path, 16, true).await;
    retry_cycle(&n, fixture(), 1).await;
    n.storage
        .cache()
        .owner
        .call(|store| {
            let s = &mut store.state;
            let before = native_snapshot(s);
            let tx = s.connection.transaction()?;
            let anchors = crate::sparse_views::preparation::publication::retention::load(&tx)?;
            assert_eq!(anchors.len(), 1);
            for (target, anchor) in &anchors {
                let expected =
                    crate::proof_hash::reference_json(b"s3drive-retention-anchor-v1\0", anchor);
                let actual: Vec<u8> = tx.query_row(
                    "SELECT state_hash FROM sparse_retention_anchors WHERE target=?1",
                    [target.as_slice()],
                    |r| r.get(0),
                )?;
                assert_eq!(actual, expected);
                assert_eq!(
                    tx.execute(
                        "UPDATE sparse_retention_anchors SET state_hash=?1 WHERE target=?2",
                        params![expected.as_slice(), target.as_slice()]
                    )?,
                    1
                );
            }
            let owner: Vec<u8> =
                tx.query_row("SELECT owner FROM sparse_retention_role", [], |r| r.get(0))?;
            let owner: Id = owner.try_into().unwrap();
            let expected = crate::proof_hash::reference_json(
                b"s3drive-retention-role-v1\0",
                &(owner, anchors.iter().collect::<Vec<_>>()),
            );
            let actual: Vec<u8> =
                tx.query_row("SELECT state_hash FROM sparse_retention_role", [], |r| {
                    r.get(0)
                })?;
            assert_eq!(actual, expected);
            assert_eq!(
                tx.execute(
                    "UPDATE sparse_retention_role SET state_hash=?1",
                    [expected.as_slice()]
                )?,
                1
            );
            tx.commit()?;
            assert_eq!(native_snapshot(s), before);
            Ok(())
        })
        .await
        .unwrap();
    n.storage.close().await.unwrap();
    // Current format, not an old-format conversion: reopen exact oracle-written
    // persisted bytes through the ordinary full cold integrity validation.
    let mut state = State::open_managed(&path).unwrap();
    let anchors =
        crate::sparse_views::preparation::publication::retention::load(&state.connection).unwrap();
    assert_eq!(anchors.len(), 1);
    let heads = views::Journal::new(&mut state, views::Options::default())
        .unwrap()
        .heads()
        .unwrap();
    assert_eq!(heads.len(), 1);
    assert_eq!(heads[0].size(), 17);
}

#[tokio::test]
async fn native_retry_validation_work_metadata_absent_cache_preserves_full_source_reads() {
    for linked in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let n = setup_mode(&tmp.path().join("metadata absent cache"), 16, linked).await;
        retry_cycle_mode(&n, fixture(), 1, linked).await;
        let key = n.key.clone();
        n.storage
            .cache()
            .owner
            .call(move |store| {
                let tx = store.state.connection.transaction()?;
                let scope = crate::chunk_mount::read_scope(&tx, &key)?;
                // No bound open or retained native preparation remains after this
                // completed cycle. The constructor must not install partial data.
                assert!(scope.source_metadata()?.is_none());
                let before = views::ledger::inspection_count();
                let first = views::ledger::inspect_scoped(&scope)?;
                let second = views::ledger::inspect_scoped(&scope)?;
                assert!(first == second);
                assert_eq!(views::ledger::inspection_count() - before, 2);
                drop(scope);
                tx.rollback()?;
                Ok(())
            })
            .await
            .unwrap();
        n.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn native_retry_validation_work_info_identity_is_immutable_and_exact() {
    for linked in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let n = setup_mode(&tmp.path().join("immutable info"), 16, linked).await;
        let f = fixture();
        let context =
            Context::new(&n.view, f.base.root.encode().to_vec(), f.base.page.encode()).unwrap();
        let dirty = n
            .client
            .write(n.token.clone(), context, [2; 32], 0, vec![7; 17])
            .await
            .unwrap();
        let context =
            Context::new(&dirty, f.base.root.encode().to_vec(), f.base.page.encode()).unwrap();
        let info = n.client.prepare(n.token.clone(), context).await.unwrap();
        let expected = crate::proof_hash::reference_json(
            b"s3drive-mixed-preparation-input-v1\0",
            info.input(),
        );
        assert_eq!(info.id().unwrap(), expected);
        let before = crate::sparse_views::preparation::model::identity_work();
        for _ in 0..20 {
            assert_eq!(info.clone().id().unwrap(), expected);
        }
        let after = crate::sparse_views::preparation::model::identity_work();
        assert_eq!(
            after[0], before[0],
            "Info lookup must not rehash immutable input"
        );
        let mut progress = info.clone();
        progress.progress.cursor.offset += 1;
        assert_eq!(progress.id().unwrap(), expected);
        for field in 0..8 {
            let mut changed = info.input().clone();
            match field {
                0 => changed.operation[0] ^= 1,
                1 => changed.view[0] ^= 1,
                2 => changed.generation[0] ^= 1,
                3 => changed.base_root[0] ^= 1,
                4 => changed.base_page[0] ^= 1,
                5 => changed.native = None,
                6 => changed.retry = None,
                _ => changed.base_root.push(0),
            }
            let oracle = crate::proof_hash::reference_json(
                b"s3drive-mixed-preparation-input-v1\0",
                &changed,
            );
            let rebuilt = Info::new(changed, info.view().clone(), info.progress.clone()).unwrap();
            assert_eq!(rebuilt.id().unwrap(), oracle);
            assert_eq!(info.id().unwrap(), expected);
        }
        n.storage.close().await.unwrap();
    }
}

#[tokio::test]
async fn native_retry_validation_work_canonical_memo_keeps_live_proofs_fresh() {
    let tmp = tempfile::tempdir().unwrap();
    let n = setup_mode(&tmp.path().join("canonical values not proofs"), 16, true).await;
    retry_cycle(&n, fixture(), 1).await;
    n.storage
        .cache()
        .owner
        .call(|store| {
            let s = &mut store.state;
            let before = native_snapshot(s);
            let raw_len: i64 = s.connection.query_row(
                "SELECT length(record) FROM sparse_retention_anchors",
                [],
                |r| r.get(0),
            )?;
            eprintln!(
                "CANONICAL_MEMO raw_bytes={raw_len} cap_bytes={}",
                crate::pure_memo::MAX_RAW
            );
            assert!(
                raw_len <= crate::pure_memo::MAX_RAW as i64,
                "ordinary fixture must exercise real admission"
            );
            crate::pure_memo::clear_entry();
            let counts = crate::pure_memo::work();
            let proofs =
                crate::sparse_views::preparation::publication::retention::inspection_counts();
            let cold =
                crate::sparse_views::preparation::publication::retention::load(&s.connection)?;
            let warm =
                crate::sparse_views::preparation::publication::retention::load(&s.connection)?;
            assert!(cold == warm);
            assert_eq!(
                crate::sparse_views::preparation::publication::retention::inspection_counts().1
                    - proofs.1,
                2
            );
            let current = crate::pure_memo::work();
            assert_eq!([current[0] - counts[0], current[1] - counts[1]], [1, 1]);
            for case in 0..8 {
                let tx = s.connection.transaction()?;
                match case {
                    0 => {
                        tx.execute("UPDATE sparse_retry_mode SET state_hash=zeroblob(32)", [])?;
                    }
                    1 => {
                        tx.execute(
                            "UPDATE sparse_retention_role SET state_hash=zeroblob(32)",
                            [],
                        )?;
                    }
                    2 => {
                        tx.execute(
                            "UPDATE sparse_retention_anchors SET state_hash=zeroblob(32)",
                            [],
                        )?;
                    }
                    3 => {
                        let raw: Vec<u8> =
                            tx.query_row("SELECT record FROM sparse_retention_anchors", [], |r| {
                                r.get(0)
                            })?;
                        let mut noncanonical = vec![b' '];
                        noncanonical.extend(raw);
                        tx.execute(
                            "UPDATE sparse_retention_anchors SET record=?1",
                            [noncanonical],
                        )?;
                    }
                    4 | 5 => {
                        let mount = crate::chunk_mount::load(&tx)?.unwrap();
                        let mut value = serde_json::to_value(mount)?;
                        if case == 4 {
                            value["terminals"] = serde_json::json!({});
                        } else {
                            value["frontier"] = serde_json::json!(0);
                        }
                        let mount: crate::chunk_mount::Binding = serde_json::from_value(value)?;
                        tx.execute(
                            "UPDATE chunk_mount_role SET record=?1,state_hash=?2",
                            params![
                                serde_json::to_vec(&mount)?,
                                hash(b"s3drive-chunk-mount-binding-v1\0", &mount)?.as_slice()
                            ],
                        )?;
                    }
                    6 => {
                        assert!(
                            tx.execute("UPDATE remote_flushes SET receipt_hash=zeroblob(32)", [])?
                                > 0
                        );
                    }
                    _ => {
                        assert!(
                            tx.execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])?
                                > 0
                        );
                    }
                }
                let hits = crate::pure_memo::work()[1];
                assert!(
                    crate::sparse_views::preparation::publication::retention::load(&tx).is_err()
                );
                if case >= 4 {
                    assert!(
                        crate::pure_memo::work()[1] > hits,
                        "external proof case {case} must reach a warmed exact-byte hit"
                    );
                }
                tx.rollback()?;
                assert_eq!(native_snapshot(s), before);
                assert!(
                    crate::sparse_views::preparation::publication::retention::load(&s.connection)?
                        == cold
                );
            }
            Ok(())
        })
        .await
        .unwrap();
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_retry_validation_work_upload_selections_preserve_branches_and_fresh_checks() {
    for (linked, value) in [(false, 0), (false, 7), (true, 0), (true, 7)] {
        let tmp = tempfile::tempdir().unwrap();
        let n = setup_mode(
            &tmp.path().join("data selections not permission"),
            16,
            linked,
        )
        .await;
        let f = fixture();
        let context =
            Context::new(&n.view, f.base.root.encode().to_vec(), f.base.page.encode()).unwrap();
        let dirty = n
            .client
            .write(n.token.clone(), context, [2; 32], 0, vec![value; 17])
            .await
            .unwrap();
        let context =
            Context::new(&dirty, f.base.root.encode().to_vec(), f.base.page.encode()).unwrap();
        let info = n.client.prepare(n.token.clone(), context).await.unwrap();
        n.storage.cache().owner.call(move |store| {
            let s = &mut store.state;
            let plan = info.id()?;
            let info = pj(s).append(plan, info.cursor(), Candidate::Bytes(vec![value;17]))?;
            let info = crate::sparse_views::preparation::tests::fixture::finish(s, info);
            let data = crate::sparse_views::preparation::Position { level:-1, slot:0 };
            let root = crate::sparse_views::preparation::Position { level:0, slot:0 };
            let before = native_snapshot(s);
            let counters = native_work_here();
            let selected = pj(s).upload_selection(plan, data)?;
            assert_eq!(native_work_here()[7] - counters[7], 1);
            assert_eq!(selected.info.id()?, plan);
            assert_eq!(selected.transfer, pj(s).transfer(plan, data)?);
            assert!(selected.prior.is_none());
            let counters = native_work_here();
            let dependency = pj(s).dependency_selection(plan, root)?;
            assert_eq!(native_work_here()[7] - counters[7], 1);
            assert_eq!(dependency.info.id()?, plan);
            assert_eq!(dependency.children, pj(s).children(plan, root)?);
            match (dependency.content, pj(s).dependency(plan, root.level, root.slot)?) {
                (crate::sparse_views::preparation::Dependency::New { role:a, reference:b, bytes:c },
                 crate::sparse_views::preparation::Dependency::New { role:d, reference:e, bytes:f }) => {
                    assert_eq!(a,d); assert_eq!(b,e); assert_eq!(c,f);
                }
                _ => panic!("expected a new page dependency"),
            }
            assert!(matches!(j(s).begin_selection(plan)?, BeginSelection::Unchecked));
            assert_eq!(native_snapshot(s), before);
            j(s).check(plan, 0, None)?;
            let error = j(s).begin_selection(plan).err().expect("physical tree absent");
            assert_eq!(error.downcast_ref::<crate::sparse_views::preparation::TransferFailure>(), Some(&crate::sparse_views::preparation::TransferFailure::Incomplete));
            for position in [data, root] {
                let Some(staged) = pj(s).materialize(plan, position, crate::journal::Options::default())? else {
                    assert_eq!(value, 0);
                    assert_eq!(position, data);
                    assert!(pj(s).upload_selection(plan, position)?.transfer.is_none());
                    assert!(matches!(pj(s).dependency_selection(plan, position)?.content, crate::sparse_views::preparation::Dependency::Hole { offset:0, length:17 }));
                    continue;
                };
                let remote = crate::remote::Journal::new(s, crate::remote::Options::default())?.prepare(staged.generation().unwrap())?;
                assert_eq!(pj(s).upload_selection(plan, position)?.prior.unwrap().phase, crate::remote::Phase::Prepared);
                let permit = crate::remote::Journal::new(s, crate::remote::Options::default())?.claim(remote.operation, &remote.target)?;
                assert_eq!(pj(s).upload_selection(plan, position)?.prior.unwrap().phase, crate::remote::Phase::InFlight);
                let mut journal = crate::remote::Journal::new(s, crate::remote::Options::default())?;
                journal.verified(crate::remote::Verification {
                    operation: remote.operation, target: remote.target,
                    payload_sha256: remote.payload_sha256, size: remote.size,
                    revision: crate::remote::Revision::new("\"owned-model\"", None)?,
                })?;
                journal.acknowledge(remote.operation)?;
                drop(permit);
            }
            let selected = pj(s).upload_selection(plan, data)?;
            if value == 0 { assert!(selected.transfer.is_none()); assert!(selected.prior.is_none()); }
            else { assert_eq!(selected.prior.as_ref().unwrap().phase, crate::remote::Phase::Verified); }
            let before = native_snapshot(s);
            let counters = native_work_here();
            let BeginSelection::Fresh(tree) = j(s).begin_selection(plan)? else { panic!("fresh selection") };
            assert_eq!(native_work_here()[7] - counters[7], 1);
            assert_eq!(tree.plan().id()?, plan);
            assert_eq!(tree.root(), &info.root()?.unwrap());
            assert_eq!(native_snapshot(s), before);
            let operation = match info.retry_operation() {
                Some(token) => token.publish_operation()?,
                None => hex(&hash(b"selection publication", &plan)?[..16]),
            };
            let record = j(s).begin(plan, f.node.clone(), f.store.clone(), f.node.revision, operation)?;
            let BeginSelection::Existing(saved) = j(s).begin_selection(plan)? else { panic!("historical selection") };
            assert!(*saved == record);
            // The selection above must not authorize materialization after a
            // later SQL change, even a trigger unrelated to preparation tables.
            s.connection.execute_batch("CREATE TRIGGER selection_unrelated_trigger AFTER INSERT ON remote_intents BEGIN SELECT 1; END")?;
            let before = native_snapshot(s);
            assert!(pj(s).materialize(plan, root, crate::journal::Options::default()).is_err());
            assert_eq!(native_snapshot(s), before);
            s.connection.execute_batch("DROP TRIGGER selection_unrelated_trigger")?;
            Ok(())
        }).await.unwrap();
        n.storage.close().await.unwrap();
    }
}
