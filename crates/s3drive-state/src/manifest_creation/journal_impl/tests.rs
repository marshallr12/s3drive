use super::*;
use crate::{
    journal::{Base, GenerationId},
    namespace::{Apply, Mutation, NodeGuard, Store},
    remote,
};
fn id(n: u8) -> String {
    format!("{n:02x}").repeat(16)
}
fn input(n: u8) -> Input {
    Input {
        volume: id(1),
        operation: id(n),
        expected_sequence: 1,
        parent: NodeGuard {
            id: ns::ROOT.into(),
            revision: 0,
        },
        name: "café 日本語 🦀 %2F.bin".into(),
        store: Store {
            id: id(3),
            route: "a".repeat(64),
            root: "managed/".into(),
        },
        source: [9; 32],
        fingerprint: "a".repeat(64),
    }
}
fn j(s: &mut State) -> Journal<'_> {
    Journal::new(s, Limits::default()).unwrap()
}
fn staged(s: &mut State, i: &Input) -> GenerationId {
    let mut q = journal::Journal::new(s, journal::Options::default()).unwrap();
    let g = q
        .begin_operation(
            &i.stage_key().unwrap(),
            &i.target().unwrap(),
            Base::Missing,
            None,
            64,
        )
        .unwrap();
    q.append(g, &i.body().unwrap()).unwrap();
    q.seal(g).unwrap();
    g
}
fn verified(s: &mut State, g: GenerationId) {
    let mut q = remote::Journal::new(s, remote::Options::default()).unwrap();
    let info = q.prepare(g).unwrap();
    q.claim(info.operation, &info.target).unwrap();
    q.verified(remote::Verification {
        operation: info.operation,
        target: info.target,
        payload_sha256: info.payload_sha256,
        size: 64,
        revision: remote::Revision::new("\"model-only\"", Some("model-version")).unwrap(),
    })
    .unwrap();
}
fn prepared(s: &mut State, i: &Input) -> Record {
    j(s).begin(i.clone()).unwrap();
    let g = staged(s, i);
    j(s).link(&i.operation, g).unwrap();
    verified(s, g);
    j(s).prepare_request(&i.operation).unwrap()
}
fn receipt(i: &Input, r: &Apply) -> ns::Receipt {
    let p = i.payload(r, 3).unwrap();
    ns::Receipt {
        sequence: 3,
        node: ns::Node {
            id: i.operation.clone(),
            parent: Some(i.parent.id.clone()),
            name: i.name.clone(),
            kind: ns::Kind::File,
            revision: 3,
            deleted: false,
            payload: Some(p.clone()),
            placement: Some(i.store.clone()),
        },
        artifact: Some(ns::Artifact::Published(p)),
    }
}
fn rows(s: &State) -> Vec<Vec<String>> {
    ["SELECT quote(operation)||quote(record)||quote(state_hash) FROM manifest_creations ORDER BY operation",
     "SELECT quote(singleton)||quote(records)||quote(state_hash) FROM manifest_creation_role",
     "SELECT quote(source_id)||quote(config_fingerprint) FROM source_bindings ORDER BY source_id"]
    .iter().map(|sql|s.connection.prepare(sql).unwrap().query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap()).collect()
}
#[test]
fn canonical_intent_matches_independent_parent_page_and_never_changes_admitted_binding() {
    let i = input(2);
    assert_eq!(
        i.body().unwrap(),
        include_bytes!("../../../../s3drive-core/src/manifest/objects/fixtures/empty-page.bin")
    );
    assert_eq!(
        i.object_key().unwrap(),
        format!(
            "managed/v2/{}/{}/{}/{}/page/0100000000000000",
            id(1),
            id(2),
            id(3),
            id(2)
        )
    );
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let r = j(&mut s).begin(i.clone()).unwrap();
    let before = rows(&s);
    assert_eq!(j(&mut s).begin(i.clone()).unwrap(), r);
    for which in 0..8 {
        let mut other = i.clone();
        match which {
            0 => other.volume = id(8),
            1 => other.name = "different".into(),
            2 => other.parent.id = id(8),
            3 => other.parent.revision = 1,
            4 => other.expected_sequence = 2,
            5 => other.store.root = "other/".into(),
            6 => other.source = [8; 32],
            _ => other.fingerprint = "b".repeat(64),
        }
        assert!(j(&mut s).begin(other).is_err());
        assert_eq!(rows(&s), before);
    }
}
#[test]
fn narrowed_quotas_preserve_known_replay_and_fresh_admission_is_atomic() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let i = input(4);
    let a = j(&mut s).begin(i.clone()).unwrap();
    j(&mut s).begin(input(5)).unwrap();
    let before = rows(&s);
    let mut q = Journal::new(
        &mut s,
        Limits {
            records: 1,
            metadata_bytes: RECORD_BYTES,
        },
    )
    .unwrap();
    assert_eq!(q.begin(i).unwrap(), a);
    assert!(q.begin(input(6)).is_err());
    assert_eq!(rows(&s), before);
    assert!(
        j(&mut s)
            .begin_inner(input(7), |_| Err("lost before commit".into()))
            .is_err()
    );
    assert_eq!(rows(&s), before);
}
#[test]
fn exact_verified_request_and_real_authority_receipt_survive_damaged_source_and_restart() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("client");
    let mut s = State::open_managed(&path).unwrap();
    let mut authority = State::open_managed(&t.path().join("authority")).unwrap();
    let v = authority
        .namespace_initialize_with_policy(s3drive_core::names::PORTABLE)
        .unwrap();
    authority
        .namespace_apply(&Apply {
            volume: v.id.clone(),
            operation: id(3),
            expected_sequence: 0,
            mutation: Mutation::RegisterStore {
                route: "a".repeat(64),
                root: "managed/".into(),
            },
        })
        .unwrap();
    let mut i = input(4);
    i.volume = v.id;
    let saved = prepared(&mut s, &i);
    let request = saved.request().unwrap().clone();
    let r = authority.namespace_apply(&request).unwrap();
    s.connection
        .execute("UPDATE staged_chunks SET data=randomblob(length(data))", [])
        .unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(j(&mut s).prepare_request(&i.operation).unwrap(), saved);
    let done = j(&mut s)
        .acknowledge(&i.operation, &request, r.clone())
        .unwrap();
    assert_eq!(done.receipt(), Some(&r));
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(j(&mut s).get(&i.operation).unwrap(), done);
    assert_eq!(
        j(&mut s).acknowledge(&i.operation, &request, r).unwrap(),
        done
    );
}
#[test]
fn incomplete_or_wrong_physical_proofs_and_forged_metadata_never_authorize_progress() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let i = input(4);
    j(&mut s).begin(i.clone()).unwrap();
    assert!(j(&mut s).prepare_request(&i.operation).is_err());
    let g = staged(&mut s, &i);
    j(&mut s).link(&i.operation, g).unwrap();
    assert!(j(&mut s).prepare_request(&i.operation).is_err());
    verified(&mut s, g);
    let saved = j(&mut s).prepare_request(&i.operation).unwrap();
    let request = saved.request().unwrap();
    let before = rows(&s);
    for which in 0..6 {
        let mut r = receipt(&i, request);
        match which {
            0 => r.node.name = "other".into(),
            1 => r.node.deleted = true,
            2 => r.sequence = 1,
            3 => r.node.payload.as_mut().unwrap().etag = "\"wrong\"".into(),
            4 => r.artifact = None,
            _ => r.node.placement = None,
        };
        assert!(j(&mut s).acknowledge(&i.operation, request, r).is_err());
        assert_eq!(rows(&s), before);
    }
    for error in [
        ns::Error::Internal,
        ns::Error::Unavailable,
        ns::Error::Limit,
        ns::Error::DeletePending,
    ] {
        assert!(j(&mut s).reject(&i.operation, request, error).is_err());
        assert_eq!(rows(&s), before);
    }
    let rejected = j(&mut s)
        .reject(&i.operation, request, ns::Error::AlreadyExists)
        .unwrap();
    assert_eq!(
        j(&mut s)
            .reject(&i.operation, request, ns::Error::AlreadyExists)
            .unwrap(),
        rejected
    );
    assert!(
        j(&mut s)
            .acknowledge(&i.operation, request, receipt(&i, request))
            .is_err()
    );
}
#[test]
fn role_is_reciprocal_and_triggers_or_corrupt_membership_never_repair_rows() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("s");
    let mut s = State::open_managed(&path).unwrap();
    j(&mut s).begin(input(4)).unwrap();
    assert!(crate::managed::Journal::new(&mut s, journal::Options::default()).is_err());
    assert!(crate::namespace_client::Journal::new(&mut s).is_err());
    assert!(s.namespace_initialize().is_err());
    let before = rows(&s);
    s.connection.execute_batch("CREATE TRIGGER forbidden AFTER INSERT ON manifest_creations BEGIN DELETE FROM manifest_creation_role; END").unwrap();
    assert!(Journal::new(&mut s, Limits::default()).is_err());
    assert_eq!(rows(&s), before);
    s.connection
        .execute_batch("DROP TRIGGER forbidden; DELETE FROM manifest_creations")
        .unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
}
#[test]
fn additive_schema39_migration_preserves_old_blobs_and_rejects_collisions_without_marker_change() {
    for collision in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("s");
        let mut s = State::open_managed(&path).unwrap();
        s.register_source(&[7; 32], &"c".repeat(64)).unwrap();
        crate::test_schema::without_creation(&s.connection);
        s.connection
            .pragma_update(None, "user_version", 39)
            .unwrap();
        if collision {
            s.connection
                .execute_batch("CREATE TABLE manifest_creations(wrong BLOB)")
                .unwrap();
        }
        drop(s);
        if collision {
            assert!(State::open_managed(&path).is_err());
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
                    .unwrap(),
                39
            );
        } else {
            let s = State::open_managed(&path).unwrap();
            assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
            assert_eq!(
                s.connection
                    .query_row(
                        "SELECT config_fingerprint FROM source_bindings WHERE source_id=?1",
                        [&[7u8; 32][..]],
                        |r| r.get::<_, String>(0)
                    )
                    .unwrap(),
                "c".repeat(64)
            );
        }
    }
}

#[test]
fn actual_sqlite_full_rolls_back_new_membership_source_and_intent_then_exact_retry_succeeds() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("full")).unwrap();
    s.connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA page_size=512; VACUUM; PRAGMA journal_mode=WAL; CREATE TABLE capacity_probe(data BLOB)").unwrap();
    let original: i64 = s
        .connection
        .pragma_query_value(None, "max_page_count", |r| r.get(0))
        .unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let direct = s
        .connection
        .execute("INSERT INTO capacity_probe VALUES(zeroblob(1048576))", [])
        .unwrap_err();
    assert!(crate::cache::is_disk_full(&direct));
    let mut failed = None;
    for n in 4..120 {
        let mut i = input(n);
        i.name = "x".repeat(230);
        let before = rows(&s);
        match Journal::new(
            &mut s,
            Limits {
                records: HARD_RECORDS,
                metadata_bytes: HARD_RECORDS * RECORD_BYTES,
            },
        )
        .unwrap()
        .begin(i.clone())
        {
            Ok(_) => {}
            Err(e) => {
                assert!(crate::cache::is_disk_full(e.as_ref()), "{e}");
                assert_eq!(rows(&s), before);
                failed = Some(i);
                break;
            }
        }
    }
    let i = failed.expect("intent admission did not reach actual SQLITE_FULL");
    s.connection
        .pragma_update(None, "max_page_count", original)
        .unwrap();
    let r = Journal::new(
        &mut s,
        Limits {
            records: HARD_RECORDS,
            metadata_bytes: HARD_RECORDS * RECORD_BYTES,
        },
    )
    .unwrap()
    .begin(i.clone())
    .unwrap();
    assert_eq!(r.input(), &i);
    assert_eq!(j(&mut s).get(&i.operation).unwrap(), r);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_owner_races_and_a_lost_committed_reply_recover_exact_intent_without_another_owner() {
    use std::time::Duration;
    tokio::time::timeout(Duration::from_secs(15), async {
        let t = tempfile::tempdir().unwrap();
        let storage = crate::worker::Storage::open(
            &t.path().join("s"),
            crate::cache::Options::default(),
            journal::Options::default(),
        )
        .await
        .unwrap();
        let c = storage.manifest_creations(Limits::default()).unwrap();
        assert!(c.same_storage(&storage));
        let i = input(4);
        let (a, b) = tokio::join!(c.begin(i.clone()), c.begin(i.clone()));
        assert_eq!(a.unwrap(), b.unwrap());
        let owner = c.owner.clone();
        let next = input(5);
        let saved = next.clone();
        let (committed, observed) = tokio::sync::oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            owner
                .call(move |store| {
                    let r = Journal::new(&mut store.state, Limits::default())?.begin(next)?;
                    let _ = committed.send(());
                    let _ = blocked.recv_timeout(Duration::from_secs(5));
                    Ok(r)
                })
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), observed)
            .await
            .unwrap()
            .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        release.send(()).unwrap();
        let recovered = c.get(saved.operation.clone()).await.unwrap();
        assert_eq!(recovered.input(), &saved);
        assert_eq!(c.begin(saved.clone()).await.unwrap(), recovered);
        let mut rebound = saved;
        rebound.name = "wrong".into();
        assert!(c.begin(rebound).await.is_err());
        storage.close().await.unwrap();
    })
    .await
    .expect("empty creation worker race/lost-reply test timed out");
}

#[test]
fn creation_crash_worker() {
    let Ok(path) = std::env::var("S3DRIVE_EMPTY_CREATION_PATH") else {
        return;
    };
    let action = std::env::var("S3DRIVE_EMPTY_CREATION_ACTION").unwrap();
    let mut s = State::open_managed(std::path::Path::new(&path)).unwrap();
    let i = input(4);
    match action.as_str() {
        "admit" => {
            j(&mut s).begin(i).unwrap();
        }
        "link" => {
            let g = GenerationId::try_from(
                std::env::var("S3DRIVE_EMPTY_CREATION_GENERATION")
                    .unwrap()
                    .parse::<i64>()
                    .unwrap(),
            )
            .unwrap();
            j(&mut s).link(&i.operation, g).unwrap();
        }
        "request" => {
            j(&mut s).prepare_request(&i.operation).unwrap();
        }
        "ack" => {
            let r = j(&mut s).get(&i.operation).unwrap();
            let request = r.request().unwrap();
            j(&mut s)
                .acknowledge(&i.operation, request, receipt(&i, request))
                .unwrap();
        }
        _ => panic!("unknown child action"),
    }
}
#[test]
fn killed_real_owners_preserve_old_or_exact_committed_admission_link_request_and_receipt() {
    use std::{
        io::{BufRead, BufReader},
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
    for action in ["admit", "link", "request", "ack"] {
        for committed in [false, true] {
            let t = tempfile::tempdir().unwrap();
            let path = t.path().join("s");
            let mut s = State::open_managed(&path).unwrap();
            let i = input(4);
            let mut g = None;
            if action != "admit" {
                j(&mut s).begin(i.clone()).unwrap();
                let staged = staged(&mut s, &i);
                g = Some(staged);
                if action != "link" {
                    j(&mut s).link(&i.operation, staged).unwrap();
                    verified(&mut s, staged);
                }
                if action == "ack" {
                    j(&mut s).prepare_request(&i.operation).unwrap();
                }
            }
            let before = rows(&s);
            drop(s);
            let boundary = match (action, committed) {
                ("admit", false) => "AdmissionStored",
                ("admit", true) => "AdmissionCommitted",
                (_, false) => "TransitionStored",
                (_, true) => "TransitionCommitted",
            };
            let mut child = Reap(
                Command::new(std::env::current_exe().unwrap())
                    .arg("--exact")
                    .arg("manifest_creation::journal_impl::tests::creation_crash_worker")
                    .arg("--nocapture")
                    .env("S3DRIVE_EMPTY_CREATION_PATH", &path)
                    .env("S3DRIVE_EMPTY_CREATION_ACTION", action)
                    .env("S3DRIVE_EMPTY_CREATION_STOP", boundary)
                    .env(
                        "S3DRIVE_EMPTY_CREATION_GENERATION",
                        g.map(|g| g.value()).unwrap_or(1).to_string(),
                    )
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let stdout = child.0.stdout.take().unwrap();
            let (send, recv) = std::sync::mpsc::channel();
            let reader = std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if line.contains("S3DRIVE_EMPTY_CREATION_BOUNDARY") {
                        let _ = send.send(line);
                        return;
                    }
                }
            });
            let marker = recv
                .recv_timeout(Duration::from_secs(10))
                .unwrap_or_else(|e| {
                    panic!("{action}/{boundary}: no child transaction marker: {e}")
                });
            assert!(marker.contains(boundary));
            child.0.kill().unwrap();
            let status = child.0.wait().unwrap();
            assert!(!status.success());
            reader.join().unwrap();
            let mut s = State::open_managed(&path).unwrap();
            if !committed {
                assert_eq!(rows(&s), before);
            }
            if action == "admit" && !committed {
                assert!(j(&mut s).get(&i.operation).is_err());
                continue;
            }
            let r = j(&mut s).get(&i.operation).unwrap();
            assert_eq!(r.input(), &i);
            assert_eq!(
                r.generation().is_some(),
                action != "admit" && (action != "link" || committed)
            );
            assert_eq!(
                r.request().is_some(),
                action == "ack" || (action == "request" && committed)
            );
            assert_eq!(r.receipt().is_some(), action == "ack" && committed);
            if let Some(request) = r.request() {
                assert_eq!(
                    *request,
                    i.request(
                        r.physical.as_ref().unwrap().operation.clone(),
                        "\"model-only\"".into(),
                        Some("model-version".into())
                    )
                    .unwrap()
                );
            }
        }
    }
}

#[test]
fn self_consistent_changed_request_proof_and_membership_or_oversized_rows_are_rejected_without_repair()
 {
    for which in 0..5 {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("s");
        let mut s = State::open_managed(&path).unwrap();
        let i = input(4);
        let mut r = prepared(&mut s, &i);
        if which < 3 {
            match which {
                0 => {
                    r.input.name = "rewritten".into();
                    if let ns::Mutation::CreateEmptyManifest { name, .. } =
                        &mut r.request.as_mut().unwrap().mutation
                    {
                        *name = "rewritten".into();
                    }
                }
                1 => {
                    if let ns::Mutation::CreateEmptyManifest { etag, .. } =
                        &mut r.request.as_mut().unwrap().mutation
                    {
                        *etag = "\"forged\"".into();
                    }
                }
                _ => r.physical.as_mut().unwrap().payload = [0; 32],
            }
            let raw = serde_json::to_vec(&r).unwrap();
            let mut map = BTreeMap::new();
            map.insert(i.operation.clone(), r.clone());
            s.connection
                .execute(
                    "UPDATE manifest_creations SET record=?1,state_hash=?2",
                    params![raw, record_hash(&r).unwrap().as_slice()],
                )
                .unwrap();
            s.connection
                .execute(
                    "UPDATE manifest_creation_role SET state_hash=?1",
                    [projection(&map).unwrap().as_slice()],
                )
                .unwrap();
        } else {
            s.connection
                .execute_batch("PRAGMA ignore_check_constraints=ON")
                .unwrap();
            if which == 3 {
                s.connection
                    .execute("UPDATE manifest_creations SET record=zeroblob(2097152)", [])
                    .unwrap();
            } else {
                s.connection
                    .execute(
                        "INSERT INTO manifest_creation_role VALUES(2,1,zeroblob(32))",
                        [],
                    )
                    .unwrap();
            }
        }
        let before = rows(&s);
        assert!(Journal::new(&mut s, Limits::default()).is_err());
        assert_eq!(rows(&s), before);
        drop(s);
        assert!(State::open_managed(&path).is_err());
    }
}

#[test]
fn foreign_legacy_identity_and_noncanonical_staged_body_never_enter_creation_role() {
    let t = tempfile::tempdir().unwrap();
    let mut legacy = State::open_managed(&t.path().join("legacy")).unwrap();
    crate::namespace_client::Journal::new(&mut legacy)
        .unwrap()
        .client_identity()
        .unwrap();
    assert!(Journal::new(&mut legacy, Limits::default()).is_err());
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let i = input(4);
    j(&mut s).begin(i.clone()).unwrap();
    let mut q = journal::Journal::new(&mut s, journal::Options::default()).unwrap();
    let g = q
        .begin_operation(
            &i.stage_key().unwrap(),
            &i.target().unwrap(),
            Base::Missing,
            None,
            64,
        )
        .unwrap();
    q.append(g, &[7; 64]).unwrap();
    q.seal(g).unwrap();
    let before = rows(&s);
    assert!(j(&mut s).link(&i.operation, g).is_err());
    assert_eq!(rows(&s), before);
    assert!(j(&mut s).get(&i.operation).unwrap().generation().is_none());
}

#[test]
fn creation_and_sparse_intents_share_one_owner_in_both_admission_orders_without_adopting_legacy_roles()
 {
    use crate::sparse_views;
    for creation_first in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("s");
        let mut s = State::open_managed(&path).unwrap();
        let i = input(4);
        if creation_first {
            j(&mut s).begin(i.clone()).unwrap();
        }
        let base = s3drive_core::manifest::base::Descriptor::from_hex(
            &id(1),
            &id(8),
            &id(7),
            &"b".repeat(64),
        )
        .unwrap();
        let binding = sparse_views::Binding::new(
            base,
            [3; 16],
            [9; 32],
            "a".repeat(64),
            "managed/".into(),
            0,
        )
        .unwrap();
        let mut sparse =
            sparse_views::Journal::new(&mut s, sparse_views::Options::default()).unwrap();
        let view = sparse.begin([70; 32], binding, 0).unwrap();
        let edited = sparse
            .write([71; 32], view.id(), 0, b"retained dirty data")
            .unwrap();
        if !creation_first {
            j(&mut s).begin(i.clone()).unwrap();
        }
        let before = rows(&s);
        assert_eq!(j(&mut s).get(&i.operation).unwrap().input(), &i);
        assert_eq!(
            sparse_views::Journal::new(&mut s, sparse_views::Options::default())
                .unwrap()
                .view(edited.id())
                .unwrap(),
            edited
        );
        assert_eq!(rows(&s), before);
        assert!(crate::namespace_client::Journal::new(&mut s).is_err());
        drop(s);
        let mut s = State::open_managed(&path).unwrap();
        assert_eq!(j(&mut s).get(&i.operation).unwrap().input(), &i);
        assert_eq!(
            sparse_views::Journal::new(&mut s, sparse_views::Options::default())
                .unwrap()
                .view(edited.id())
                .unwrap(),
            edited
        );
    }
}

#[test]
fn downgraded_or_future_schema_markers_never_adopt_or_repair_new_creation_records() {
    for version in [39, crate::SCHEMA_VERSION + 1] {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("s");
        let mut s = State::open_managed(&path).unwrap();
        j(&mut s).begin(input(4)).unwrap();
        let before = rows(&s);
        s.connection
            .pragma_update(None, "user_version", version)
            .unwrap();
        assert!(Journal::new(&mut s, Limits::default()).is_err());
        assert_eq!(rows(&s), before);
        drop(s);
        assert!(State::open_managed(&path).is_err());
        let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
                .unwrap(),
            version
        );
        let state = State {
            connection: db,
            _directory: None,
            incarnation: None,
            source_pin_witness: None,
        };
        assert_eq!(rows(&state), before);
    }
}

#[test]
fn restricted_creation_recovery_finishes_saved_metadata_without_opening_corrupt_sparse_data() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("s");
    let mut s = State::open_managed(&path).unwrap();
    let i = input(4);
    let saved = prepared(&mut s, &i);
    let request = saved.request().unwrap().clone();
    let base =
        s3drive_core::manifest::base::Descriptor::from_hex(&id(1), &id(8), &id(7), &"b".repeat(64))
            .unwrap();
    let binding = crate::sparse_views::Binding::new(
        base,
        [3; 16],
        [9; 32],
        "a".repeat(64),
        "managed/".into(),
        0,
    )
    .unwrap();
    let mut sparse =
        crate::sparse_views::Journal::new(&mut s, crate::sparse_views::Options::default()).unwrap();
    let view = sparse.begin([70; 32], binding, 0).unwrap();
    sparse
        .write([71; 32], view.id(), 0, b"retained dirty bytes")
        .unwrap();
    s.connection
        .execute(
            "UPDATE sparse_view_blocks SET data=randomblob(length(data))",
            [],
        )
        .unwrap();
    let data: Vec<u8> = s
        .connection
        .query_row("SELECT data FROM sparse_view_blocks", [], |r| r.get(0))
        .unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
    let mut recovery = RecoveryStore::open(&path).unwrap();
    assert_eq!(recovery.get(&i.operation).unwrap(), saved);
    let done = recovery
        .acknowledge(&i.operation, &request, receipt(&i, &request))
        .unwrap();
    assert!(done.receipt().is_some());
    assert_eq!(recovery.records().unwrap(), vec![done.clone()]);
    drop(recovery);
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("SELECT data FROM sparse_view_blocks", [], |r| r
            .get::<_, Vec<u8>>(0))
            .unwrap(),
        data
    );
    drop(db);
    assert_eq!(
        RecoveryStore::open(&path)
            .unwrap()
            .get(&i.operation)
            .unwrap(),
        done
    );
}

#[test]
fn restricted_creation_recovery_never_creates_migrates_or_prepares_and_refuses_late_triggers() {
    let t = tempfile::tempdir().unwrap();
    let missing = t.path().join("missing");
    assert!(RecoveryStore::open(&missing).is_err());
    assert!(!missing.exists());
    let path = t.path().join("s");
    let mut s = State::open_managed(&path).unwrap();
    drop(s);
    assert!(RecoveryStore::open(&path).is_err());
    s = State::open_managed(&path).unwrap();
    let i = input(4);
    j(&mut s).begin(i.clone()).unwrap();
    drop(s);
    let mut recovery = RecoveryStore::open(&path).unwrap();
    let r = recovery.get(&i.operation).unwrap();
    assert!(r.request().is_none());
    let fake = i
        .request("a".repeat(64), "\"model-only\"".into(), None)
        .unwrap();
    assert!(
        recovery
            .acknowledge(&i.operation, &fake, receipt(&i, &fake))
            .is_err()
    );
    assert_eq!(recovery.get(&i.operation).unwrap(), r);
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    db.execute_batch("CREATE TRIGGER no_recovery AFTER UPDATE ON manifest_creations BEGIN DELETE FROM manifest_creation_role; END").unwrap();
    assert!(recovery.get(&i.operation).is_err());
    drop(recovery);
    assert!(RecoveryStore::open(&path).is_err());
    db.execute_batch("DROP TRIGGER no_recovery").unwrap();
    db.pragma_update(None, "user_version", 39).unwrap();
    drop(db);
    assert!(RecoveryStore::open(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        39
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restricted_creation_worker_reopens_exact_requests_and_releases_the_private_lock_on_close()
{
    use std::time::Duration;
    tokio::time::timeout(Duration::from_secs(15), async {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("s");
        let mut s = State::open_managed(&path).unwrap();
        let i = input(4);
        let saved = prepared(&mut s, &i);
        drop(s);
        let client = RecoveryClient::open(&path).await.unwrap();
        assert!(RecoveryStore::open(&path).is_err());
        assert_eq!(client.get(i.operation.clone()).await.unwrap(), saved);
        let request = saved.request().unwrap().clone();
        let done = client
            .acknowledge(i.operation.clone(), request.clone(), receipt(&i, &request))
            .await
            .unwrap();
        assert_eq!(client.records().await.unwrap(), vec![done.clone()]);
        client.close().await.unwrap();
        assert!(client.get(i.operation.clone()).await.is_err());
        assert_eq!(
            RecoveryStore::open(&path)
                .unwrap()
                .get(&i.operation)
                .unwrap(),
            done
        );
    })
    .await
    .expect("restricted creation worker test timed out");
}

#[test]
fn rewritten_request_and_physical_snapshot_cannot_replace_the_independent_verified_upload() {
    for which in 0..3 {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut state = State::open_managed(&path).unwrap();
        let input = input(4);
        let mut record = prepared(&mut state, &input);
        let ns::Mutation::CreateEmptyManifest {
            etag,
            upload_operation,
            version,
            ..
        } = &mut record.request.as_mut().unwrap().mutation
        else {
            panic!()
        };
        let proof = record.physical.as_mut().unwrap();
        match which {
            0 => {
                *etag = "\"rewritten\"".into();
                proof.etag = etag.clone();
            }
            1 => {
                *upload_operation = "b".repeat(64);
                proof.operation = upload_operation.clone();
            }
            _ => {
                *version = Some("rewritten-version".into());
                proof.version = version.clone();
            }
        }
        // Recompute both local checksums: a self-consistent duplicate snapshot
        // is still not the original, independently retained upload proof.
        let raw = serde_json::to_vec(&record).unwrap();
        let mut map = BTreeMap::new();
        map.insert(input.operation.clone(), record.clone());
        state
            .connection
            .execute(
                "UPDATE manifest_creations SET record=?1,state_hash=?2",
                params![raw, record_hash(&record).unwrap().as_slice()],
            )
            .unwrap();
        state
            .connection
            .execute(
                "UPDATE manifest_creation_role SET state_hash=?1",
                [projection(&map).unwrap().as_slice()],
            )
            .unwrap();
        let before = rows(&state);
        assert!(
            Journal::new(&mut state, Limits::default()).is_err(),
            "rewritten physical field {which} was admitted"
        );
        assert_eq!(rows(&state), before);
        drop(state);
        assert!(RecoveryStore::open(&path).is_err());
        assert!(State::open_managed(&path).is_err());
    }
}
