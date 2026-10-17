//! Owned-target replacement through real SDK HTTP and killed SQLite owners.
use super::pending_owner::{arm, prepare, request, stage};
use super::*;
use s3drive_authority::{NodeGuard, ROOT, RemovalClaim};
const SOURCE: &[u8] = b"replacement source\0\xfe";
const TARGET: &[u8] = b"displaced target\0\xff";
const LATE: &[u8] = b"late detached target\0\xfd";
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owned_replacement_sdk_recovers_both_modes_and_sources_retains_late_io_and_never_replays_puts()
 {
    pending_owner::run_children_retaining(
        &[
            "owned_replace_start",
            "owned_replace_recover",
            "owned_replace_finish",
        ],
        &[8, 12, 12],
        &[SOURCE, TARGET, LATE],
    )
    .await;
}
fn guard(n: &Node) -> NodeGuard {
    NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    }
}
fn parent() -> NodeGuard {
    NodeGuard {
        id: ROOT.into(),
        revision: 0,
    }
}
fn saved(root: &Path, n: u64) -> s3drive_state::namespace_client::Input {
    serde_json::from_slice(&std::fs::read(root.join(format!("owned-replace-{n}.json"))).unwrap())
        .unwrap()
}
pub(super) async fn step(
    mode: &str,
    p: &Publisher,
    w: &WireClient,
    root: &Path,
    volume: &str,
    loss: &AtomicU8,
) {
    for case in 0..4 {
        let n = 100 + case * 10;
        let t = n + 1;
        let source_deferred = case >= 2;
        let target_deferred = case % 2 == 0;
        if mode == "owned_replace_start" {
            for (idn, bytes) in [(n, SOURCE), (t, TARGET)] {
                change(
                    w,
                    idn,
                    Mutation::CreateEntry {
                        parent: ROOT.into(),
                        parent_revision: 0,
                        store: Some(id(200)),
                        name: format!("owned replacement café 日本語 🦀 {idn}"),
                        node_kind: Kind::EmptyFile,
                    },
                )
                .await;
                p.bind_edit_file(&id(idn)).await.unwrap();
                stage(p, w, idn, 30000 + idn, bytes).await;
                prepare(p, w, idn, 30000 + idn, 40000 + idn, false).await;
                p.flush_edit(&id(30000 + idn)).await.unwrap();
            }
            let retained = p.read(&id(t)).await.unwrap();
            if source_deferred {
                arm(p, w, n, 10000 + n, true).await;
            }
            arm(p, w, t, 10000 + t, target_deferred).await;
            // Dirty target bytes remain pinned while replacement is UNKNOWN.
            stage(p, w, t, 60000 + t, LATE).await;
            let s = p
                .storage()
                .logical()
                .file(volume.into(), id(n))
                .await
                .unwrap()
                .base;
            let target = p
                .storage()
                .logical()
                .file(volume.into(), id(t))
                .await
                .unwrap()
                .base;
            let old = p.owned_removal(&id(t)).await.unwrap().unwrap();
            let claim = Some(Box::new(RemovalClaim {
                fence: old.fence,
                previous: old.event,
                mode: old.mode,
            }));
            let mutation = if source_deferred {
                let old = p.owned_removal(&id(n)).await.unwrap().unwrap();
                Mutation::MoveDeferredEntry {
                    node: guard(&s),
                    source_parent: parent(),
                    parent: parent(),
                    name: target.name.clone(),
                    destination: Some(guard(&target)),
                    fence: Box::new(old.fence),
                    previous: old.event.into(),
                    destination_removal: claim,
                }
            } else {
                Mutation::MoveEntry {
                    node: guard(&s),
                    source_parent: parent(),
                    parent: parent(),
                    name: target.name.clone(),
                    destination: Some(guard(&target)),
                    destination_removal: claim,
                }
            };
            let i = p
                .namespace_change_input(request(w, 20000 + n, mutation).await, &[s, target])
                .unwrap();
            std::fs::write(
                root.join(format!("owned-replace-{n}.json")),
                serde_json::to_vec(&i).unwrap(),
            )
            .unwrap();
            loss.store(9, Ordering::SeqCst);
            assert!(p.change_namespace(i).await.is_err());
            assert_eq!(loss.load(Ordering::SeqCst), 0);
            assert_eq!(retained.bytes(), TARGET);
            assert!(p.owned_removal(&id(t)).await.unwrap().is_some());
        } else if mode == "owned_replace_recover" {
            p.change_namespace(saved(root, n)).await.unwrap();
            assert!(p.owned_removal(&id(t)).await.unwrap().is_none());
            assert_eq!(p.read(&id(n)).await.unwrap().bytes(), SOURCE);
            assert!(p.read(&id(t)).await.is_err());
            let file = p
                .storage()
                .logical()
                .file(volume.into(), id(t))
                .await
                .unwrap();
            assert!(file.base.deleted);
            let edit = p.storage().logical().get(id(60000 + t)).await.unwrap();
            assert_eq!(file.head, edit.generation);
            p.stage_edit(
                &id(60000 + t),
                request(
                    w,
                    70000 + t,
                    Mutation::ReserveDetachedContent {
                        node: id(t),
                        expected_revision: file.base.revision,
                        store: id(200),
                        size: edit.input.size,
                        sha256: edit.input.sha256,
                        expected_payload: s3drive_authority::payload_reference(&file.base.payload),
                    },
                )
                .await,
                id(80000 + t),
            )
            .await
            .unwrap();
            if case == 3 {
                loss.store(2, Ordering::SeqCst);
                assert!(p.flush_edit(&id(60000 + t)).await.is_err());
                assert_eq!(loss.load(Ordering::SeqCst), 0);
            } else {
                p.flush_edit(&id(60000 + t)).await.unwrap();
            }
            assert_eq!(p.read(&id(n)).await.unwrap().bytes(), SOURCE);
        } else {
            assert_eq!(mode, "owned_replace_finish");
            p.flush_edit(&id(60000 + t)).await.unwrap();
            let before = node(w, volume, n).await;
            let target = node(w, volume, t).await;
            assert!(target.deleted);
            assert_eq!(
                target.payload.as_ref().unwrap().reservation.sha256,
                sha(LATE)
            );
            let i = saved(root, n);
            let claim = i.request.mutation.owned_destination().unwrap().1;
            let old_close = p.namespace_change_input(
                request(
                    w,
                    90000 + t,
                    Mutation::CancelRemoval {
                        node: id(t),
                        fence: claim.fence.clone(),
                    },
                )
                .await,
                std::slice::from_ref(&target),
            );
            assert!(
                old_close.is_err(),
                "deleted target cannot invent a new close by pathname"
            );
            if source_deferred {
                pending_owner::close(p, w, n, 90000 + n, false).await;
                arm(p, w, n, 91000 + n, true).await;
                let successor = p.owned_removal(&id(n)).await.unwrap().unwrap();
                p.change_namespace(i).await.unwrap();
                assert_eq!(p.owned_removal(&id(n)).await.unwrap().unwrap(), successor);
                pending_owner::close(p, w, n, 92000 + n, false).await;
            } else {
                p.change_namespace(i).await.unwrap();
                assert_eq!(node(w, volume, n).await, before);
            }
            assert!(p.owned_removal(&id(t)).await.unwrap().is_none());
            assert_eq!(p.read(&id(n)).await.unwrap().bytes(), SOURCE);
            for edit in [30000 + n, 30000 + t, 60000 + t] {
                p.flush_edit(&id(edit)).await.unwrap();
            }
        }
    }
    if mode != "owned_replace_finish" {
        checkpoint("FENCED_CHECKPOINT");
        std::future::pending::<()>().await;
    }
}
