use super::*;
fn id(n: u64) -> String {
    format!("{n:032x}")
}
fn binding() -> Binding {
    Binding {
        volume: id(999),
        node: Node {
            id: id(10),
            parent: Some(namespace::ROOT.into()),
            name: "資料 café 🦀".into(),
            kind: Kind::EmptyFile,
            revision: 1,
            deleted: false,
            placement: None,
            payload: None,
        },
        store: Store {
            id: id(200),
            route: "b".repeat(64),
            root: "writable/.s3drive/".into(),
        },
        source: [1; 32],
        fingerprint: "a".repeat(64),
    }
}
fn open(root: &std::path::Path) -> State {
    State::open_managed(root).unwrap()
}
fn stage(s: &mut State, n: u64, parent: Option<GenerationId>, bytes: &[u8]) -> Record {
    let b = binding();
    let mut l = Journal::new(s, journal::Options::default()).unwrap();
    l.bind(b.clone()).unwrap();
    let input = Input::from_bytes(id(n), &b, parent, bytes);
    let r = l.begin(input).unwrap();
    let g = r.generation.unwrap();
    let mut j = journal::Journal::new(s, journal::Options::default()).unwrap();
    if j.info(g).unwrap().phase == journal::Phase::Building {
        for (i, c) in bytes.chunks(journal::CHUNK_BYTES).enumerate() {
            j.append_at(g, (i * journal::CHUNK_BYTES) as u64, c)
                .unwrap();
        }
    }
    Journal::new(s, journal::Options::default())
        .unwrap()
        .seal(&id(n))
        .unwrap()
}
#[test]
fn logical_generations_replay_conflict_and_reopen_without_physical_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("state");
    let mut s = open(&root);
    let a = stage(&mut s, 1, None, b"abc\0\xff");
    let first = a.generation.unwrap();
    let b = stage(&mut s, 2, Some(first), b"new contents");
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    assert_eq!(l.begin(a.input.clone()).unwrap().generation, Some(first));
    assert!(
        l.begin(Input::from_bytes(id(3), &binding(), Some(first), b"stale"))
            .is_err()
    );
    let mut changed = a.input.clone();
    changed.sha256 = "f".repeat(64);
    assert!(l.begin(changed).is_err());
    let mut wrong = binding();
    wrong.source = [2; 32];
    assert!(l.bind(wrong).is_err());
    assert_eq!(l.pending(None, 10).unwrap().len(), 2);
    drop(s);
    let mut s = open(&root);
    let f = Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .file(&id(999), &id(10))
        .unwrap();
    assert_eq!(f.head, b.generation);
    assert!(f.settled.is_none());
    assert_eq!(f.base.revision, 1);
    let target = f.binding.target().unwrap();
    assert_ne!(target.source(), &f.binding.source);
    assert_ne!(target.fingerprint(), f.binding.fingerprint);
    assert_eq!(
        journal::Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .read(first, 0, 100)
            .unwrap(),
        b"abc\0\xff"
    );
}
#[test]
fn seal_rejects_wrong_bytes_and_rolls_back_head_and_retention() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = open(&tmp.path().join("state"));
    let b = binding();
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    l.bind(b.clone()).unwrap();
    let r = l
        .begin(Input::from_bytes(id(1), &b, None, b"correct"))
        .unwrap();
    let g = r.generation.unwrap();
    journal::Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .append(g, b"corrupt")
        .unwrap();
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    assert!(l.seal(&id(1)).is_err());
    assert!(l.file(&b.volume, &b.node.id).unwrap().head.is_none());
    assert_eq!(
        journal::load(&s.connection, g).unwrap().phase,
        journal::Phase::Building
    );
    let retained: i64 = s
        .connection
        .query_row("SELECT count(*) FROM retained_staged_chunks", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(retained, 0);
}
#[test]
fn logical_only_state_cannot_become_authority_and_corruption_is_not_a_miss() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = open(&tmp.path().join("state"));
    Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .bind(binding())
        .unwrap();
    assert!(s.namespace_initialize().is_err());
    let a = stage(&mut s, 1, None, b"bytes");
    s.connection
        .execute(
            "UPDATE logical_edits SET state_hash=zeroblob(32) WHERE operation_id=?",
            [&a.input.id],
        )
        .unwrap();
    assert!(
        Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .pending(None, 10)
            .is_err()
    );
}

fn publication(r: &Record, base: u64) -> managed::Input {
    let b = binding();
    managed::Input {
        reserve: namespace::Apply {
            volume: b.volume,
            operation: id(1000 + r.input.id.parse::<u64>().unwrap_or(1)),
            expected_sequence: base.max(2),
            mutation: namespace::Mutation::Reserve {
                node: b.node.id,
                expected_revision: base,
                store: b.store.id.clone(),
                size: r.input.size,
                sha256: r.input.sha256.clone(),
            },
        },
        publish_operation: id(5000 + r.input.id.parse::<u64>().unwrap_or(1)),
        store: b.store,
        source: b.source,
        fingerprint: b.fingerprint,
    }
}
fn physical_commit(s: &mut State, r: &Record, long: bool) -> managed::Record {
    let input = r.publication.as_ref().unwrap().clone();
    let bytes = journal::Journal::new(s, journal::Options::default())
        .unwrap()
        .read(r.generation.unwrap(), 0, r.input.size as usize)
        .unwrap();
    managed::Journal::new(s, journal::Options::default())
        .unwrap()
        .begin(input.clone())
        .unwrap();
    let mut j = journal::Journal::new(s, journal::Options::default()).unwrap();
    let g = j
        .begin_operation(
            &input.stage_key().unwrap(),
            &input.target().unwrap(),
            journal::Base::Missing,
            None,
            r.input.size,
        )
        .unwrap();
    j.append(g, &bytes).unwrap();
    j.seal(g).unwrap();
    let mut m = managed::Journal::new(s, journal::Options::default()).unwrap();
    m.link(input.id(), g).unwrap();
    m.reserved(input.id(), input.reservation().unwrap())
        .unwrap();
    let mut remote = remote::Journal::new(s, remote::Options::default()).unwrap();
    let send = remote.prepare(g).unwrap();
    let _permit = remote.claim(send.operation, &send.target).unwrap();
    remote
        .verified(crate::remote::Verification {
            operation: send.operation,
            target: send.target,
            payload_sha256: send.payload_sha256,
            size: send.size,
            revision: crate::remote::Revision::new(
                &format!(
                    "\"{}\"",
                    if long {
                        "x".repeat(1000)
                    } else {
                        "etag".into()
                    }
                ),
                None,
            )
            .unwrap(),
        })
        .unwrap();
    remote.acknowledge(send.operation).unwrap();
    let mut m = managed::Journal::new(s, journal::Options::default()).unwrap();
    let prepared = m.prepare(input.id(), 3).unwrap();
    let namespace::Mutation::Publish {
        upload_operation,
        etag,
        version,
        ..
    } = &prepared.publication.unwrap().mutation
    else {
        panic!()
    };
    let payload = namespace::Payload {
        reservation: input.reservation().unwrap(),
        upload_operation: upload_operation.clone(),
        etag: etag.clone(),
        version: version.clone(),
    };
    let receipt = Receipt {
        sequence: 4,
        node: Node {
            kind: Kind::File,
            revision: 4,
            payload: Some(payload.clone()),
            ..binding().node
        },
        artifact: Some(namespace::Artifact::Published(payload)),
    };
    m.acknowledge(input.id(), receipt).unwrap()
}
#[test]
fn logical_settlement_needs_both_receipts_and_never_clears_a_newer_head() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = open(&tmp.path().join("state"));
    let a = stage(&mut s, 1, None, b"first");
    let ga = a.generation.unwrap();
    let b = stage(&mut s, 2, Some(ga), b"newer dirty bytes");
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    let prepared = l.prepare(&a.input.id, publication(&a, 1)).unwrap();
    assert!(l.acknowledge(&a.input.id).is_err());
    let mut wrong = publication(&a, 1);
    wrong.reserve.expected_sequence += 1;
    assert!(l.prepare(&a.input.id, wrong).is_err());
    physical_commit(&mut s, &prepared, false);
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    l.acknowledge(&a.input.id).unwrap();
    l.acknowledge(&a.input.id).unwrap();
    let f = l.file(&id(999), &id(10)).unwrap();
    assert_eq!(f.head, b.generation);
    assert_eq!(f.settled, Some(ga));
    assert_eq!(f.base.revision, 4);
    assert_eq!(
        l.pending(None, 10)
            .unwrap()
            .iter()
            .map(|r| r.input.id.clone())
            .collect::<Vec<_>>(),
        vec![b.input.id.clone()]
    );
    assert!(l.prepare(&b.input.id, publication(&b, 1)).is_err());
    l.prepare(&b.input.id, publication(&b, 4)).unwrap();
    // Even settled/covered rows must be checked during pending enumeration.
    s.connection
        .execute(
            "UPDATE logical_edits SET receipt=NULL WHERE operation_id=?",
            [a.input.id],
        )
        .unwrap();
    assert!(
        Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .pending(None, 10)
            .is_err()
    );
}
#[test]
fn logical_disk_full_rolls_back_ack_after_physical_and_metadata_success() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = open(&tmp.path().join("state"));
    let a = stage(&mut s, 1, None, b"retained data");
    let prepared = Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .prepare(&a.input.id, publication(&a, 1))
        .unwrap();
    physical_commit(&mut s, &prepared, true);
    s.connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let pages: i64 = s
        .connection
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let error = Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .acknowledge(&a.input.id)
        .unwrap_err();
    assert!(error.to_string().to_lowercase().contains("full"), "{error}");
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    assert!(l.get(&a.input.id).unwrap().receipt.is_none());
    assert!(l.file(&id(999), &id(10)).unwrap().settled.is_none());
    assert_eq!(l.pending(None, 10).unwrap().len(), 1);
    s.connection
        .pragma_update(None, "max_page_count", pages + 100)
        .unwrap();
    Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .acknowledge(&a.input.id)
        .unwrap();
}
#[test]
fn logical_migration_preserves_schema9_data_and_rejects_unknown_shapes_atomically() {
    for damaged in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("state");
        let mut s = open(&root);
        let target = Target::new([5; 32], &"f".repeat(64), "existing/dirty").unwrap();
        let mut j = journal::Journal::new(&mut s, journal::Options::default()).unwrap();
        let g = j.begin(&target, journal::Base::Missing, None, 3).unwrap();
        j.append(g, b"old").unwrap();
        j.seal(g).unwrap();
        s.connection
            .execute_batch(
                "DROP TABLE namespace_removals; DROP TABLE namespace_intents; DROP INDEX namespace_portable_names; ALTER TABLE namespace_nodes DROP COLUMN name_key; ALTER TABLE namespace_nodes DROP COLUMN placement; DROP TABLE logical_publication_links; DROP TABLE logical_edits; DROP TABLE logical_files; PRAGMA user_version=9",
            )
            .unwrap();
        if damaged {
            s.connection.execute_batch("CREATE TABLE logical_edits(keep TEXT); INSERT INTO logical_edits VALUES('untouched')").unwrap();
        }
        drop(s);
        if damaged {
            assert!(State::open_managed(&root).is_err());
            let db = rusqlite::Connection::open(root.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value::<i32, _>(None, "user_version", |r| r.get(0))
                    .unwrap(),
                9
            );
            assert_eq!(
                db.query_row("SELECT keep FROM logical_edits", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "untouched"
            );
            assert!(db.prepare("SELECT * FROM logical_files").is_err());
        } else {
            let mut s = open(&root);
            assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
            assert_eq!(
                journal::Journal::new(&mut s, journal::Options::default())
                    .unwrap()
                    .read(g, 0, 3)
                    .unwrap(),
                b"old"
            );
        }
    }
}
#[test]
fn logical_crash_child() {
    let Ok(point) = std::env::var("S3DRIVE_LOGICAL_KILL_POINT") else {
        return;
    };
    let root = std::path::PathBuf::from(std::env::var_os("S3DRIVE_LOGICAL_KILL_ROOT").unwrap());
    let mut s = open(&root);
    let b = binding();
    let mut hook = |p: Point| -> Result<()> {
        if format!("{p:?}") == point {
            use std::io::Write;
            println!("LOGICAL_READY");
            std::io::stdout().flush().unwrap();
            loop {
                std::thread::park();
            }
        }
        Ok(())
    };
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    l.bind(b.clone()).unwrap();
    let a = l
        .begin_inner(
            Input::from_bytes(id(1), &b, None, b"crash bytes"),
            &mut hook,
        )
        .unwrap();
    journal::Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .append(a.generation.unwrap(), b"crash bytes")
        .unwrap();
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    let a = l.seal_inner(&id(1), &mut hook).unwrap();
    let a = l
        .prepare_inner(&id(1), publication(&a, 1), &mut hook)
        .unwrap();
    physical_commit(&mut s, &a, false);
    Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .acknowledge_inner(&id(1), &mut hook)
        .unwrap();
    panic!("kill point was not reached")
}
#[test]
fn killed_owner_recovers_nine_logical_boundaries_without_losing_bytes_or_ids() {
    use std::{
        io::{BufRead, BufReader},
        process::{Command, Stdio},
        sync::mpsc,
        time::Duration,
    };
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for point in [
        Point::InputPersisted,
        Point::BuilderCreated,
        Point::BuilderLinked,
        Point::SealPending,
        Point::Sealed,
        Point::PublicationPending,
        Point::PublicationPrepared,
        Point::SettlementPending,
        Point::Settled,
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("state");
        let mut child = Child(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "logical::tests::logical_crash_child",
                    "--nocapture",
                ])
                .env("S3DRIVE_LOGICAL_KILL_POINT", format!("{point:?}"))
                .env("S3DRIVE_LOGICAL_KILL_ROOT", &root)
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let stdout = child.0.stdout.take().unwrap();
        let (send, receive) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if line.unwrap() == "LOGICAL_READY" {
                    let _ = send.send(());
                    break;
                }
            }
        });
        let ready = receive.recv_timeout(Duration::from_secs(20));
        let _ = child.0.kill();
        child.0.wait().unwrap();
        reader.join().unwrap();
        assert!(ready.is_ok(), "{point:?}: {ready:?}");
        let mut s = open(&root);
        let a = stage(&mut s, 1, None, b"crash bytes");
        let count: i64 = s
            .connection
            .query_row("SELECT count(*) FROM logical_edits", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        if matches!(point, Point::SettlementPending | Point::Settled) {
            let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
            assert_eq!(
                l.get(&id(1)).unwrap().receipt.is_some(),
                point == Point::Settled
            );
            l.acknowledge(&id(1)).unwrap();
            assert!(l.pending(None, 10).unwrap().is_empty());
        } else {
            assert!(a.receipt.is_none());
        }
        assert_eq!(
            journal::Journal::new(&mut s, journal::Options::default())
                .unwrap()
                .read(a.generation.unwrap(), 0, 100)
                .unwrap(),
            b"crash bytes"
        );
    }
}

#[test]
fn schema9_authority_migration_preserves_volume_and_cannot_host_local_edits() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("authority");
    let mut s = open(&root);
    let volume = s.namespace_initialize().unwrap();
    s.connection
        .execute_batch("DROP TABLE namespace_removals; DROP TABLE namespace_intents; DROP INDEX namespace_portable_names; ALTER TABLE namespace_nodes DROP COLUMN name_key; ALTER TABLE namespace_nodes DROP COLUMN placement; DROP TABLE logical_publication_links; DROP TABLE logical_edits; DROP TABLE logical_files; PRAGMA user_version=9")
        .unwrap();
    drop(s);
    let mut s = open(&root);
    assert_eq!(s.namespace_initialize().unwrap(), volume);
    assert!(Journal::new(&mut s, journal::Options::default()).is_err());
}
#[test]
fn quota_and_concurrent_builders_preserve_complete_edit_intents_and_losing_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = open(&tmp.path().join("state"));
    let b = binding();
    let options = journal::Options {
        max_bytes: 4,
        max_generations: 3,
        ..journal::Options::default()
    };
    let mut l = Journal::new(&mut s, options).unwrap();
    l.bind(b.clone()).unwrap();
    let a = l.begin(Input::from_bytes(id(1), &b, None, b"abc")).unwrap();
    let other = l.begin(Input::from_bytes(id(2), &b, None, b"xyz")).unwrap();
    let mut j = journal::Journal::new(&mut s, options).unwrap();
    j.append(a.generation.unwrap(), b"abc").unwrap();
    assert!(j.append(other.generation.unwrap(), b"xyz").is_err());
    Journal::new(&mut s, options).unwrap().seal(&id(1)).unwrap();
    let bigger = journal::Options {
        max_bytes: 10,
        ..options
    };
    journal::Journal::new(&mut s, bigger)
        .unwrap()
        .append(other.generation.unwrap(), b"xyz")
        .unwrap();
    let mut l = Journal::new(&mut s, bigger).unwrap();
    assert!(l.seal(&id(2)).is_err());
    assert_eq!(l.pending(None, 10).unwrap().len(), 2);
    assert_eq!(
        journal::Journal::new(&mut s, bigger)
            .unwrap()
            .read_draft(other.generation.unwrap(), 0, 3)
            .unwrap(),
        b"xyz"
    );
    assert!(serde_json::from_str::<GenerationId>("-1").is_err());
}
#[test]
fn ignored_settlement_update_rolls_back_both_local_markers() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = open(&tmp.path().join("state"));
    let a = stage(&mut s, 1, None, b"retained");
    let a = Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .prepare(&a.input.id, publication(&a, 1))
        .unwrap();
    physical_commit(&mut s, &a, false);
    s.connection.execute_batch("CREATE TRIGGER refuse_logical_settlement BEFORE UPDATE ON logical_files BEGIN SELECT RAISE(IGNORE); END").unwrap();
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    assert!(l.acknowledge(&id(1)).is_err());
    assert!(l.get(&id(1)).unwrap().receipt.is_none());
    assert!(l.file(&id(999), &id(10)).unwrap().settled.is_none());
}

#[test]
fn publication_ids_are_exclusively_bound_to_one_logical_edit() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = open(&tmp.path().join("state"));
    let a = stage(&mut s, 1, None, b"same bytes");
    let b = stage(&mut s, 2, a.generation, b"same bytes");
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    let input = publication(&a, 1);
    l.prepare(&a.input.id, input.clone()).unwrap();
    assert!(l.prepare(&b.input.id, input.clone()).is_err());
    let mut second = publication(&b, 1);
    second.publish_operation = input.reserve.operation.clone();
    assert!(l.prepare(&b.input.id, second).is_err());
    assert!(l.get(&b.input.id).unwrap().publication.is_none());
    assert!(l.get(&a.input.id).unwrap().publication.is_some());
}

#[test]
fn verified_observation_preserves_identity_and_rejects_dirty_external_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("state");
    let mut s = open(&root);
    let original = binding();
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    l.bind(original.clone()).unwrap();
    let mut renamed = original.clone();
    renamed.node.name = "renamed 🦀".into();
    renamed.node.revision = 2;
    let f = l.observe(renamed.clone()).unwrap();
    assert_eq!(f.binding, original);
    assert_eq!(f.base, renamed.node);
    // Simulate an old schema-10 file, preserving its original hash encoding.
    s.connection.execute_batch("DROP TABLE namespace_removals; DROP TABLE namespace_intents; DROP INDEX namespace_portable_names; ALTER TABLE namespace_nodes DROP COLUMN name_key; ALTER TABLE namespace_nodes DROP COLUMN placement; UPDATE logical_files SET observed_base=NULL; ALTER TABLE logical_files DROP COLUMN observed_base; PRAGMA user_version=10").unwrap();
    let row = FileRow {
        binding: original.clone(),
        settled: None,
        publication: None,
        observed: None,
    };
    s.connection
        .execute(
            "UPDATE logical_files SET state_hash=?",
            [hash(&row).unwrap()],
        )
        .unwrap();
    drop(s);
    let mut s = open(&root);
    let staged = stage(&mut s, 1, None, b"dirty bytes");
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    let f = l.observe(renamed.clone()).unwrap();
    assert_eq!(f.head, staged.generation);
    let mut remote = renamed.clone();
    remote.node.kind = Kind::File;
    remote.node.payload = Some(namespace::Payload {
        reservation: namespace::Reservation {
            id: id(1000),
            volume: original.volume.clone(),
            node: original.node.id.clone(),
            expected_revision: 2,
            store: original.store.clone(),
            key: format!(
                "{}v1/{}/{}/{}",
                original.store.root,
                original.volume,
                original.node.id,
                id(1000)
            ),
            size: 0,
            sha256: hex(&Sha256::digest([])),
            guard: None,
        },
        upload_operation: "e".repeat(64),
        etag: "\"remote\"".into(),
        version: None,
    });
    remote.node.revision = 3;
    assert!(l.observe(remote.clone()).is_err());
    assert_eq!(
        l.file(&original.volume, &original.node.id).unwrap().head,
        staged.generation
    );
    // A separate clean identity adopts the same verified remote base.
    let mut clean = original.clone();
    clean.node.id = id(11);
    l.bind(clean.clone()).unwrap();
    remote.node.id = id(11);
    remote.node.payload.as_mut().unwrap().reservation.node = id(11);
    remote.node.payload.as_mut().unwrap().reservation.key = format!(
        "{}v1/{}/{}/{}",
        original.store.root,
        original.volume,
        id(11),
        id(1000)
    );
    assert_eq!(l.observe(remote.clone()).unwrap().base, remote.node);
    drop(s);
    let mut s = open(&root);
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    assert_eq!(l.file(&original.volume, &id(11)).unwrap().base, remote.node);
    assert_eq!(
        l.file(&original.volume, &id(10)).unwrap().head,
        staged.generation
    );
}

#[test]
fn schema10_settled_file_and_legacy_receipts_keep_their_hashes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("state");
    let mut s = open(&root);
    let a = stage(&mut s, 1, None, b"legacy committed bytes");
    let r = Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .prepare(&a.input.id, publication(&a, 1))
        .unwrap();
    physical_commit(&mut s, &r, false);
    let r = Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .acknowledge(&a.input.id)
        .unwrap();
    let reservation = r.publication.as_ref().unwrap().reservation().unwrap();
    let encoded = serde_json::to_vec(&reservation).unwrap();
    assert!(!String::from_utf8_lossy(&encoded).contains("guard"));
    let checksum: Vec<u8> = s
        .connection
        .query_row("SELECT state_hash FROM logical_files", [], |r| r.get(0))
        .unwrap();
    s.connection
        .execute_batch(
            "DROP TABLE namespace_removals; DROP TABLE namespace_intents; DROP INDEX namespace_portable_names; ALTER TABLE namespace_nodes DROP COLUMN name_key; ALTER TABLE namespace_nodes DROP COLUMN placement; ALTER TABLE logical_files DROP COLUMN observed_base; PRAGMA user_version=10",
        )
        .unwrap();
    drop(s);
    let mut s = open(&root);
    let mut l = Journal::new(&mut s, journal::Options::default()).unwrap();
    let file = l.file(&id(999), &id(10)).unwrap();
    assert_eq!(file.head, r.generation);
    assert_eq!(file.head, file.settled);
    assert_eq!(file.base, r.receipt.unwrap().node);
    assert!(l.pending(None, 10).unwrap().is_empty());
    let after: Vec<u8> = s
        .connection
        .query_row("SELECT state_hash FROM logical_files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(checksum, after);
}
