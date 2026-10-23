//! Actual state transactions; physical receipts and bulk quota rows are modeled.
use super::*;

fn setup_open(path: &Path) -> (State, State, chunk_mount::Key, Input) {
    let (mut s, a, key, mut i) = setup(path);
    opens::Journal::for_chunks(&mut s, &key)
        .unwrap()
        .start_session(&id(70), &i.volume)
        .unwrap();
    i.open = Some(crate::manifest_creation::CreationOpen {
        id: id(71),
        session: id(70),
        delete_on_close: true,
        posix: false,
    });
    (s, a, key, i)
}
fn cj<'a>(s: &'a mut State, key: &chunk_mount::Key) -> Journal<'a> {
    Journal::for_chunks(s, Limits::default(), key).unwrap()
}
fn prepare(s: &mut State, key: &chunk_mount::Key, i: &Input) -> Record {
    cj(s, key).begin(i.clone()).unwrap();
    let g = staged(s, i);
    j(s).link(&i.operation, g).unwrap();
    verified(s, g);
    j(s).prepare_request(&i.operation).unwrap()
}
fn acknowledge(s: &mut State, a: &mut State, key: &chunk_mount::Key, i: &Input) -> Record {
    let r = prepare(s, key, i);
    let q = r.request().unwrap();
    let result = a.namespace_apply(q).unwrap();
    j(s).acknowledge(&i.operation, q, result).unwrap()
}
fn get_open(s: &mut State, key: &chunk_mount::Key) -> opens::OpenRecord {
    opens::Journal::for_chunks(s, key)
        .unwrap()
        .get(&id(71))
        .unwrap()
}

#[test]
fn prospective_admission_is_atomic_inactive_exact_and_requires_original_capability_and_session() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, _a, key, i) = setup_open(t.path());
    let before = catalog(&s);
    assert!(j(&mut s).begin(i.clone()).is_err());
    assert_eq!(catalog(&s), before);
    let original = cj(&mut s, &key).begin(i.clone()).unwrap();
    assert_eq!(original.format, 3);
    let open = get_open(&mut s, &key);
    assert!(!open.active());
    assert_eq!(open.input.creation.as_deref(), Some(i.operation.as_str()));
    assert_eq!(open.owner, key.owner());
    assert!(open.input.admission.is_none());
    assert_eq!(admission::reserved(&s.connection, &[]).unwrap(), 3);
    assert!(
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .activate(&id(71), &id(72))
            .is_err()
    );
    let exact = catalog(&s);
    assert_eq!(cj(&mut s, &key).begin(i.clone()).unwrap(), original);
    for mutation in 0..6 {
        let mut changed = i.clone();
        match mutation {
            0 => changed.open.as_mut().unwrap().id = id(73),
            1 => changed.open.as_mut().unwrap().session = id(74),
            2 => changed.open.as_mut().unwrap().posix = true,
            3 => changed.owner = Some(id(75)),
            4 => changed.source = [42; 32],
            _ => changed.store.root = "foreign/".into(),
        }
        assert!(cj(&mut s, &key).begin(changed).is_err());
        assert_eq!(catalog(&s), exact);
    }
    let mut legacy = i.clone();
    legacy.open = None;
    assert_eq!(legacy.body().unwrap(), i.body().unwrap());
    assert_ne!(legacy.stage_key().unwrap(), i.stage_key().unwrap());
    assert!(!serde_json::to_string(&legacy).unwrap().contains("\"open\""));
    drop(s);
    let mut s = State::open_managed(&t.path().join("client")).unwrap();
    assert_eq!(j(&mut s).get(&i.operation).unwrap(), original);
    let before = catalog(&s);
    assert!(cj(&mut s, &key).begin(i.clone()).is_err());
    assert!(
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .prepare(open.input)
            .is_err()
    );
    opens::Journal::for_chunks(&mut s, &key)
        .unwrap()
        .start_session(&id(80), &i.volume)
        .unwrap();
    let before_recovery = catalog(&s);
    assert!(
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .prepare_recovery(&id(80), &i.volume, &i.operation, None)
            .is_err()
    );
    assert!(
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .retire(&id(71))
            .is_err()
    );
    assert_eq!(catalog(&s), before_recovery);
    assert_ne!(before, before_recovery); // only the explicitly started new session
}

#[test]
fn prospective_import_converts_credit_once_and_activation_keeps_original_fence() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, mut a, key, i) = setup_open(t.path());
    acknowledge(&mut s, &mut a, &key, &i);
    assert_eq!(admission::reserved(&s.connection, &[]).unwrap(), 3);
    let imported = import(&mut s, &key, &i.operation).unwrap();
    assert_eq!(
        admission::reserved(&s.connection, std::slice::from_ref(&imported)).unwrap(),
        0
    );
    assert_eq!(import(&mut s, &key, &i.operation).unwrap(), imported);
    assert!(
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .activate(&id(71), &id(72))
            .unwrap()
            .active()
    );
    drop(s);
    let mut s = State::open_managed(&t.path().join("client")).unwrap();
    assert!(get_open(&mut s, &key).active()); // historical activation survives restart
    let old = get_open(&mut s, &key).input;
    assert!(
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .prepare(old)
            .is_err()
    );
}

#[test]
fn prospective_success_before_import_and_rejection_have_distinct_abandonment_barriers() {
    for rejected in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let (mut s, mut a, key, i) = setup_open(t.path());
        let r = prepare(&mut s, &key, &i);
        if rejected {
            j(&mut s)
                .reject(&i.operation, r.request().unwrap(), ns::Error::AlreadyExists)
                .unwrap();
            assert_eq!(admission::reserved(&s.connection, &[]).unwrap(), 0);
        } else {
            let receipt = a.namespace_apply(r.request().unwrap()).unwrap();
            // Remote commit without local receipt is still an unknown creator.
            assert!(import(&mut s, &key, &i.operation).is_err());
            j(&mut s)
                .acknowledge(&i.operation, r.request().unwrap(), receipt)
                .unwrap();
        }
        drop(s);
        let mut s = State::open_managed(&t.path().join("client")).unwrap();
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .start_session(&id(80), &i.volume)
            .unwrap();
        let result = opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .prepare_recovery(&id(80), &i.volume, &i.operation, None);
        if rejected {
            let recovery = result.unwrap();
            opens::Journal::for_chunks(&mut s, &key)
                .unwrap()
                .finish_recovery(&id(80), &recovery.input.id)
                .unwrap();
            opens::Journal::for_chunks(&mut s, &key)
                .unwrap()
                .retire(&id(71))
                .unwrap();
            assert!(!get_open(&mut s, &key).active());
        } else {
            assert!(result.is_err());
            import(&mut s, &key, &i.operation).unwrap();
            // Even after import, an obsolete session cannot activate a handle.
            assert!(
                opens::Journal::for_chunks(&mut s, &key)
                    .unwrap()
                    .activate(&id(71), &id(72))
                    .is_err()
            );
            assert!(
                opens::Journal::for_chunks(&mut s, &key)
                    .unwrap()
                    .prepare_recovery(&id(80), &i.volume, &i.operation, None)
                    .is_err()
            );
        }
    }
}

#[test]
fn prospective_precommit_errors_roll_back_both_roles_and_lost_commit_reply_retains_both() {
    for boundary in [
        "AdmissionCreatorStored",
        "AdmissionOpenStored",
        "AdmissionStored",
        "AdmissionCommitted",
    ] {
        let t = tempfile::tempdir().unwrap();
        let (mut s, _a, key, i) = setup_open(t.path());
        let before = catalog(&s);
        let error = cj(&mut s, &key).begin_inner(i.clone(), |at| {
            if at == boundary {
                Err("modeled lost response".into())
            } else {
                Ok(())
            }
        });
        assert!(error.is_err());
        if boundary == "AdmissionCommitted" {
            assert_eq!(cj(&mut s, &key).begin(i.clone()).unwrap().input(), &i);
            assert!(!get_open(&mut s, &key).active());
        } else {
            assert_eq!(catalog(&s), before);
            cj(&mut s, &key).begin(i.clone()).unwrap();
        }
    }
}

#[test]
fn prospective_namespace_quota_is_reserved_before_upload_and_cannot_be_stolen_by_other_work() {
    for count in [4093, 4094] {
        let t = tempfile::tempdir().unwrap();
        let (mut s, mut a, key, i) = setup_open(t.path());
        model_directories(&mut s, &i, count);
        let before = catalog(&s);
        let admitted = cj(&mut s, &key).begin(i.clone());
        if count == 4094 {
            assert!(admitted.is_err());
            assert_eq!(catalog(&s), before);
            continue;
        }
        admitted.unwrap();
        let request = Apply {
            volume: i.volume.clone(),
            operation: id(90),
            expected_sequence: 1,
            mutation: Mutation::CreateEntry {
                parent: ns::ROOT.into(),
                parent_revision: 0,
                name: "unrelated".into(),
                node_kind: ns::Kind::Directory,
                store: None,
            },
        };
        let before = catalog(&s);
        assert!(ns(&mut s, &key).begin(ns_input(&i, &request)).is_err());
        assert_eq!(catalog(&s), before);
        acknowledge(&mut s, &mut a, &key, &i);
        import(&mut s, &key, &i.operation).unwrap();
        let owned = ns(&mut s, &key)
            .owned_removal(&i.volume, &i.operation)
            .unwrap()
            .unwrap();
        let node = a.namespace_node(&i.volume, &i.operation).unwrap();
        let parent = a.namespace_node(&i.volume, ns::ROOT).unwrap();
        change(
            &mut s,
            &mut a,
            &key,
            &i,
            5,
            Mutation::TransitionRemoval {
                node: ns::NodeGuard {
                    id: node.id,
                    revision: node.revision,
                },
                parent: ns::NodeGuard {
                    id: parent.id,
                    revision: parent.revision,
                },
                fence: owned.fence.clone(),
                expected: ns::RemovalMode::Deferred,
                mode: ns::RemovalMode::Pending,
            },
        );
        let node = a
            .namespace_pending_removal(&i.volume, &i.operation)
            .unwrap()
            .unwrap()
            .node;
        let parent = a.namespace_node(&i.volume, ns::ROOT).unwrap();
        change(
            &mut s,
            &mut a,
            &key,
            &i,
            6,
            Mutation::CommitRemoval {
                node: ns::NodeGuard {
                    id: node.id,
                    revision: node.revision,
                },
                parent: ns::NodeGuard {
                    id: parent.id,
                    revision: parent.revision,
                },
                fence: owned.fence,
            },
        );
        assert_eq!(
            s.connection
                .query_row("SELECT count(*) FROM namespace_intents", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            4096
        );
    }
}

#[test]
fn prospective_missing_or_rewritten_anchor_and_old_marker_never_launder_new_origin() {
    for fault in 0..4 {
        let t = tempfile::tempdir().unwrap();
        let (mut s, _a, key, i) = setup_open(t.path());
        cj(&mut s, &key).begin(i.clone()).unwrap();
        match fault {
            0 => {
                s.connection
                    .execute("DELETE FROM retained_windows_opens", [])
                    .unwrap();
            }
            1 => {
                s.connection
                    .execute(
                        "UPDATE windows_open_journal SET state_hash=zeroblob(32)",
                        [],
                    )
                    .unwrap();
            }
            2 => {
                s.connection
                    .execute(
                        "UPDATE manifest_creation_role SET state_hash=zeroblob(32)",
                        [],
                    )
                    .unwrap();
            }
            _ => crate::test_schema::demote(&s.connection, 44).unwrap(),
        }
        let before = catalog(&s);
        assert!(Journal::new(&mut s, Limits::default()).is_err());
        assert_eq!(catalog(&s), before);
        drop(s);
        assert!(State::open_managed(&t.path().join("client")).is_err());
        let raw = rusqlite::Connection::open(t.path().join("client/metadata.sqlite3")).unwrap();
        assert_eq!(catalog_db(&raw), before);
    }
}

#[test]
fn prospective_valid44_upgrade_preserves_legacy_rows_and_failure_rolls_back_marker() {
    for damaged in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let (mut s, mut a, key, i) = setup(t.path());
        ack(&mut s, &mut a, &i);
        import(&mut s, &key, &i.operation).unwrap();
        crate::test_schema::demote(&s.connection, 44).unwrap();
        if damaged {
            s.connection
                .execute(
                    "UPDATE manifest_creation_role SET state_hash=zeroblob(32)",
                    [],
                )
                .unwrap();
        }
        let before = catalog(&s);
        drop(s);
        let result = State::open_managed(&t.path().join("client"));
        if damaged {
            assert!(result.is_err());
            let raw = rusqlite::Connection::open(t.path().join("client/metadata.sqlite3")).unwrap();
            assert_eq!(
                raw.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                    .unwrap(),
                44
            );
            assert_eq!(catalog_db(&raw), before);
        } else {
            let s = result.unwrap();
            assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
            assert_legacy_catalog(catalog(&s), before);
        }
    }
}

#[test]
fn prospective_open_row_and_callback_budgets_refuse_admission_independently() {
    for (count, eventful) in [(256, false), (227, true)] {
        let t = tempfile::tempdir().unwrap();
        let (mut s, _a, key, i) = setup_open(t.path());
        let tx = s.connection.transaction().unwrap();
        opens::creation::quota_fixture(&tx, &i, key.owner(), count, eventful).unwrap();
        tx.commit().unwrap();
        let before = catalog(&s);
        assert!(cj(&mut s, &key).begin(i.clone()).is_err());
        assert_eq!(catalog(&s), before);
        assert!(inspect(&s.connection).unwrap().is_empty());
    }
}

#[test]
fn prospective_actual_sqlite_full_rolls_back_creator_open_anchor_and_exact_retry() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, _a, key, i) = setup_open(t.path());
    pressure(&mut s);
    let mut failed = None;
    for n in 4..20 {
        let mut i = i.clone();
        i.operation = id(n);
        i.name = format!("quota 日本語 {n}");
        i.open.as_mut().unwrap().id = id(n + 70);
        let before = catalog(&s);
        match cj(&mut s, &key).begin(i.clone()) {
            Ok(_) => {}
            Err(error) => {
                assert!(crate::cache::is_disk_full(error.as_ref()), "{error}");
                assert_eq!(catalog(&s), before);
                failed = Some(i);
                break;
            }
        }
    }
    let i = failed.expect("sixteen prospective creators must eventually need a new page");
    grow(&mut s);
    assert_eq!(cj(&mut s, &key).begin(i.clone()).unwrap().input(), &i);
    assert!(
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .get(&i.open.unwrap().id)
            .is_ok()
    );
}

#[test]
fn prospective_joint_creator_open_and_anchor_rewrite_still_fails_original_staging_witness() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, mut a, key, i) = setup_open(t.path());
    let mut creator = acknowledge(&mut s, &mut a, &key, &i);
    let mut open = get_open(&mut s, &key).input;
    creator.input.open.as_mut().unwrap().delete_on_close = false;
    open.delete_on_close = false;
    let mut rows = inspect(&s.connection).unwrap();
    write(&s.connection, &mut rows, creator, false).unwrap();
    let row_hash = hash(
        b"s3drive-open-row-v1\0",
        &(&open, key.owner(), 0usize, &Option::<Vec<u8>>::None),
    )
    .unwrap();
    let anchor = hash(b"s3drive-open-input-v1\0", &(&open, key.owner())).unwrap();
    s.connection
        .execute(
            "UPDATE windows_open_journal SET input=?1,state_hash=?2 WHERE open_id=?3",
            params![
                serde_json::to_vec(&open).unwrap(),
                row_hash.as_slice(),
                open.id
            ],
        )
        .unwrap();
    s.connection
        .execute(
            "UPDATE retained_windows_opens SET input_hash=?1 WHERE open_id=?2",
            params![anchor.as_slice(), open.id],
        )
        .unwrap();
    let before = catalog(&s);
    assert!(Journal::new(&mut s, Limits::default()).is_err());
    assert!(import(&mut s, &key, &i.operation).is_err());
    assert_eq!(catalog(&s), before);
    drop(s);
    assert!(State::open_managed(&t.path().join("client")).is_err());
}

#[test]
fn prospective_unknown_creator_cannot_be_laundered_through_forged_no_effect_recovery() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, _a, key, i) = setup_open(t.path());
    cj(&mut s, &key).begin(i.clone()).unwrap();
    drop(s);
    let mut s = State::open_managed(&t.path().join("client")).unwrap();
    opens::Journal::for_chunks(&mut s, &key)
        .unwrap()
        .start_session(&id(80), &i.volume)
        .unwrap();
    let open_hash: Vec<u8> = s
        .connection
        .query_row(
            "SELECT state_hash FROM windows_open_journal WHERE open_id=?",
            [id(71)],
            |r| r.get(0),
        )
        .unwrap();
    let input = opens::session::RecoveryInput {
        id: id(81),
        session: id(80),
        volume: i.volume.clone(),
        node: i.operation.clone(),
        opens: vec![opens::session::OpenProof {
            id: id(71),
            hash: open_hash,
        }],
        cancel: None,
    };
    let seal = hash(
        b"s3drive-session-recovery-v1\0",
        &(&input, &Option::<serde_json::Value>::None),
    )
    .unwrap();
    s.connection
        .execute(
            "INSERT INTO windows_session_recoveries VALUES(?1,?2,?3,?4,NULL,?5)",
            params![
                input.id,
                input.volume,
                input.node,
                serde_json::to_vec(&input).unwrap(),
                seal.as_slice()
            ],
        )
        .unwrap();
    s.connection
        .execute(
            "INSERT INTO recovered_windows_opens VALUES(?1,?2)",
            params![id(71), input.id],
        )
        .unwrap();
    let before = catalog(&s);
    assert!(opens::Journal::for_chunks(&mut s, &key).is_err());
    assert_eq!(catalog(&s), before);
    drop(s);
    assert!(State::open_managed(&t.path().join("client")).is_err());
}

#[tokio::test]
async fn prospective_crash_worker() {
    use std::io::{Read, Write};
    let Ok(root) = std::env::var("S3DRIVE_PROSPECTIVE_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let i: Input =
        serde_json::from_slice(&std::fs::read(root.join("claim.json")).unwrap()).unwrap();
    let storage = crate::worker::Storage::open(
        &root.join("client"),
        crate::cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap();
    let client = storage
        .chunk_mount(chunk_mount::Input {
            volume: i.volume.clone(),
            controls: vec![names::Control {
                store: i.store.clone(),
                source: i.source,
                fingerprint: i.fingerprint.clone(),
            }],
        })
        .await
        .unwrap();
    client
        .opens()
        .start_session(id(70), i.volume.clone())
        .await
        .unwrap();
    println!("PROSPECTIVE_READY");
    std::io::stdout().flush().unwrap();
    std::io::stdin().read_exact(&mut [0u8; 1]).unwrap();
    client
        .creations(Limits::default())
        .unwrap()
        .begin(i)
        .await
        .unwrap();
    panic!("expected stop checkpoint not reached");
}

#[test]
fn prospective_killed_public_workers_preserve_neither_or_both_admitted_roles() {
    use std::{
        io::{BufRead, BufReader, Write},
        process::{Command, Stdio},
        time::Duration,
    };
    struct Reap(std::process::Child);
    impl Drop for Reap {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for boundary in [
        "AdmissionCreatorStored",
        "AdmissionOpenStored",
        "AdmissionStored",
        "AdmissionCommitted",
    ] {
        let t = tempfile::tempdir().unwrap();
        let (s, _a, key, mut i) = setup(t.path());
        i.open = Some(crate::manifest_creation::CreationOpen {
            id: id(71),
            session: id(70),
            delete_on_close: true,
            posix: false,
        });
        std::fs::write(t.path().join("claim.json"), serde_json::to_vec(&i).unwrap()).unwrap();
        drop(s);
        let mut child=Reap(Command::new(std::env::current_exe().unwrap()).args(["--exact","manifest_creation::journal_impl::tests::creator_import::prospective::prospective_crash_worker","--nocapture"]).env("S3DRIVE_PROSPECTIVE_ROOT",t.path()).env("S3DRIVE_EMPTY_CREATION_STOP",boundary).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap());
        let stdout = child.0.stdout.take().unwrap();
        let (send, recv) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if line.contains("PROSPECTIVE_READY")
                    || line.contains("S3DRIVE_EMPTY_CREATION_BOUNDARY")
                {
                    let _ = send.send(line);
                }
            }
        });
        assert!(
            recv.recv_timeout(Duration::from_secs(15))
                .unwrap()
                .contains("PROSPECTIVE_READY")
        );
        let raw = rusqlite::Connection::open_with_flags(
            t.path().join("client/metadata.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let before = catalog_db(&raw);
        drop(raw);
        child.0.stdin.as_mut().unwrap().write_all(b"1").unwrap();
        assert!(
            recv.recv_timeout(Duration::from_secs(15))
                .unwrap()
                .contains(boundary)
        );
        child.0.kill().unwrap();
        assert!(!child.0.wait().unwrap().success());
        reader.join().unwrap();
        let mut s = State::open_managed(&t.path().join("client")).unwrap();
        if boundary == "AdmissionCommitted" {
            assert_eq!(j(&mut s).get(&i.operation).unwrap().input(), &i);
            assert!(!get_open(&mut s, &key).active());
            assert_eq!(admission::reserved(&s.connection, &[]).unwrap(), 3);
        } else {
            assert_eq!(catalog(&s), before, "{boundary}");
            assert!(inspect(&s.connection).unwrap().is_empty());
        }
        assert!(cj(&mut s, &key).begin(i).is_err()); // original child session is obsolete
    }
}

#[test]
fn prospective_namespace_operation_collisions_cannot_consume_or_steal_the_reservation() {
    for creator_first in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let (mut s, _a, key, i) = setup_open(t.path());
        let q = Apply {
            volume: i.volume.clone(),
            operation: i.operation.clone(),
            expected_sequence: 1,
            mutation: Mutation::CreateEntry {
                parent: ns::ROOT.into(),
                parent_revision: 0,
                name: "collision".into(),
                node_kind: ns::Kind::Directory,
                store: None,
            },
        };
        if creator_first {
            cj(&mut s, &key).begin(i.clone()).unwrap();
            let before = catalog(&s);
            assert!(ns(&mut s, &key).begin(ns_input(&i, &q)).is_err());
            assert_eq!(catalog(&s), before);
        } else {
            ns(&mut s, &key).begin(ns_input(&i, &q)).unwrap();
            let before = catalog(&s);
            assert!(cj(&mut s, &key).begin(i).is_err());
            assert_eq!(catalog(&s), before);
        }
    }
}

#[test]
fn prospective_inactive_open_cannot_adopt_cancellation_or_a_successor_fence() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, mut a, key, i) = setup_open(t.path());
    acknowledge(&mut s, &mut a, &key, &i);
    import(&mut s, &key, &i.operation).unwrap();
    let fence = ns(&mut s, &key)
        .owned_removal(&i.volume, &i.operation)
        .unwrap()
        .unwrap()
        .fence;
    change(
        &mut s,
        &mut a,
        &key,
        &i,
        5,
        Mutation::CancelRemoval {
            node: i.operation.clone(),
            fence,
        },
    );
    let before = catalog(&s);
    assert!(
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .activate(&id(71), &id(72))
            .is_err()
    );
    assert_eq!(catalog(&s), before);
    let node = a.namespace_node(&i.volume, &i.operation).unwrap();
    let parent = a.namespace_node(&i.volume, ns::ROOT).unwrap();
    change(
        &mut s,
        &mut a,
        &key,
        &i,
        6,
        Mutation::PrepareRemoval {
            node: ns::NodeGuard {
                id: node.id,
                revision: node.revision,
            },
            parent: ns::NodeGuard {
                id: parent.id,
                revision: parent.revision,
            },
            owner: key.owner().into(),
        },
    );
    let before = catalog(&s);
    assert!(
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .activate(&id(71), &id(72))
            .is_err()
    );
    assert_eq!(catalog(&s), before);
    assert!(!get_open(&mut s, &key).active());
}

#[tokio::test]
async fn prospective_concurrent_public_worker_admissions_cannot_share_an_open_identity() {
    let t = tempfile::tempdir().unwrap();
    let (s, _a, key, mut left) = setup(t.path());
    left.open = Some(crate::manifest_creation::CreationOpen {
        id: id(71),
        session: id(70),
        delete_on_close: true,
        posix: false,
    });
    drop(s);
    let storage = crate::worker::Storage::open(
        &t.path().join("client"),
        crate::cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap();
    let chunks = storage
        .chunk_mount(chunk_mount::Input {
            volume: left.volume.clone(),
            controls: vec![names::Control {
                store: left.store.clone(),
                source: left.source,
                fingerprint: left.fingerprint.clone(),
            }],
        })
        .await
        .unwrap();
    chunks
        .opens()
        .start_session(id(70), left.volume.clone())
        .await
        .unwrap();
    let creators = chunks.creations(Limits::default()).unwrap();
    assert!(creators.same_storage(&storage));
    let mut right = left.clone();
    right.operation = id(5);
    right.name = "rival 日本語".into();
    let (one, two) = tokio::join!(creators.begin(left.clone()), creators.begin(right.clone()));
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    let winner = one.or(two).unwrap();
    let all = creators.records().await.unwrap();
    assert_eq!(all, vec![winner.clone()]);
    let opens = chunks.opens().records().await.unwrap();
    assert_eq!(opens.len(), 1);
    assert_eq!(
        opens[0].input.creation.as_deref(),
        Some(winner.input().operation.as_str())
    );
    assert_eq!(opens[0].owner, key.owner());
    assert!(!opens[0].active());
    storage.close().await.unwrap();
}

#[test]
fn prospective_conversion_keeps_its_credit_while_unrelated_closing_result_is_unknown() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, mut a, key, one) = setup_open(t.path());
    acknowledge(&mut s, &mut a, &key, &one);
    import(&mut s, &key, &one.operation).unwrap();
    let mut two = one.clone();
    two.operation = id(7);
    two.name = "second 日本語".into();
    two.open.as_mut().unwrap().id = id(73);
    two.expected_sequence = a.namespace_volume().unwrap().sequence;
    two.parent.revision = a.namespace_node(&one.volume, ns::ROOT).unwrap().revision;
    acknowledge(&mut s, &mut a, &key, &two); // retained success; still no namespace import
    model_directories(&mut s, &one, 4090);
    let owned = ns(&mut s, &key)
        .owned_removal(&one.volume, &one.operation)
        .unwrap()
        .unwrap();
    let node = a.namespace_node(&one.volume, &one.operation).unwrap();
    let parent = a.namespace_node(&one.volume, ns::ROOT).unwrap();
    let request = Apply {
        volume: one.volume.clone(),
        operation: id(5),
        expected_sequence: a.namespace_volume().unwrap().sequence,
        mutation: Mutation::TransitionRemoval {
            node: ns::NodeGuard {
                id: node.id.clone(),
                revision: node.revision,
            },
            parent: ns::NodeGuard {
                id: parent.id,
                revision: parent.revision,
            },
            fence: owned.fence,
            expected: ns::RemovalMode::Deferred,
            mode: ns::RemovalMode::Pending,
        },
    };
    let mut input = ns_input(&one, &request);
    input.entries = vec![names::Entry::from_node(&node).unwrap()];
    ns(&mut s, &key).begin(input).unwrap(); // owns a reserved promotion credit
    assert!(names::check_creation_capacity(&s.connection).is_err());
    let imported = import(&mut s, &key, &two.operation).unwrap();
    assert_eq!(import(&mut s, &key, &two.operation).unwrap(), imported);
    let receipt = a.namespace_apply(&request).unwrap();
    ns(&mut s, &key)
        .acknowledge(&request.operation, Ok(receipt))
        .unwrap();
    names::check_creation_capacity(&s.connection).unwrap();
    assert!(
        ns(&mut s, &key)
            .owned_removal(&two.volume, &two.operation)
            .unwrap()
            .is_some()
    );
}
