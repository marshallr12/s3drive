use super::*;
fn id(n: u64) -> String {
    format!("{n:032x}")
}
fn input(n: u64) -> Input {
    Input {
        reserve: Apply {
            volume: id(999),
            operation: id(n),
            expected_sequence: 2,
            mutation: Mutation::Reserve {
                node: id(10),
                expected_revision: 1,
                store: id(200),
                size: 3,
                sha256: hex(&Sha256::digest(b"abc")),
            },
        },
        publish_operation: id(n + 1000),
        store: Store {
            id: id(200),
            route: "b".repeat(64),
            root: "writable/.s3drive/".into(),
        },
        source: [1; 32],
        fingerprint: "a".repeat(64),
    }
}
fn staged(s: &mut State, n: u64) -> Record {
    let input = input(n);
    let mut m = Journal::new(s, journal::Options::default()).unwrap();
    m.begin(input.clone()).unwrap();
    let mut j = journal::Journal::new(s, journal::Options::default()).unwrap();
    let generation = j
        .begin_operation(
            &input.stage_key().unwrap(),
            &input.target().unwrap(),
            journal::Base::Missing,
            None,
            3,
        )
        .unwrap();
    j.append(generation, b"abc").unwrap();
    j.seal(generation).unwrap();
    Journal::new(s, journal::Options::default())
        .unwrap()
        .recover_stage(input.id())
        .unwrap()
}
fn verified(s: &mut State, n: u64, long_tag: bool) -> Record {
    let r = staged(s, n);
    let mut m = Journal::new(s, journal::Options::default()).unwrap();
    m.reserved(&id(n), r.input.reservation().unwrap()).unwrap();
    let mut j = remote::Journal::new(s, remote::Options::default()).unwrap();
    let intent = j.prepare(r.generation.unwrap()).unwrap();
    let _permit = j.claim(intent.operation, &intent.target).unwrap();
    j.verified(remote::Verification {
        operation: intent.operation,
        target: intent.target,
        payload_sha256: intent.payload_sha256,
        size: intent.size,
        revision: remote::Revision::new(
            &format!(
                "\"{}\"",
                if long_tag {
                    "x".repeat(1000)
                } else {
                    "tag".into()
                }
            ),
            None,
        )
        .unwrap(),
    })
    .unwrap();
    j.acknowledge(intent.operation).unwrap();
    Journal::new(s, journal::Options::default())
        .unwrap()
        .prepare(&id(n), 3)
        .unwrap()
}
fn metadata(r: &Record) -> Receipt {
    let Mutation::Publish {
        upload_operation,
        etag,
        version,
        ..
    } = &r.publication.as_ref().unwrap().mutation
    else {
        panic!()
    };
    let payload = Payload {
        reservation: r.reservation.clone().unwrap(),
        upload_operation: upload_operation.clone(),
        etag: etag.clone(),
        version: version.clone(),
    };
    Receipt {
        sequence: 4,
        node: crate::namespace::Node {
            id: id(10),
            parent: Some(crate::namespace::ROOT.into()),
            name: "file".into(),
            kind: Kind::File,
            revision: 4,
            deleted: false,
            placement: None,
            payload: Some(payload.clone()),
        },
        artifact: Some(Artifact::Published(payload)),
    }
}
#[test]
fn sealed_unlinked_recovery_binding_and_corruption_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("client");
    let mut s = State::open_managed(&path).unwrap();
    let r = staged(&mut s, 1000);
    assert!(r.generation.is_some());
    let mut wrong = r.input.clone();
    wrong.publish_operation = id(5555);
    assert!(
        Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .begin(wrong)
            .is_err()
    );
    assert!(
        Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .prepare(&id(1000), 3)
            .is_err()
    );
    let generation = r.generation.unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(
        Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .get(&id(1000))
            .unwrap()
            .generation,
        Some(generation)
    );
    s.connection
        .execute(
            "UPDATE managed_publications SET publication_request=x'00' WHERE operation_id=?",
            [id(1000)],
        )
        .unwrap();
    assert!(
        Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .pending(None, 10)
            .is_err()
    );
    assert_eq!(
        journal::Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .read(generation, 0, 3)
            .unwrap(),
        b"abc"
    );
}
#[test]
fn acknowledgment_is_exact_and_settled_rows_cannot_hide_pending() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&dir.path().join("client")).unwrap();
    let r = verified(&mut s, 1000, false);
    let receipt = metadata(&r);
    let mut wrong = receipt.clone();
    wrong.node.id = id(11);
    assert!(
        Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .acknowledge(&id(1000), wrong)
            .is_err()
    );
    let mut m = Journal::new(&mut s, journal::Options::default()).unwrap();
    let prepared = m.prepare(&id(1000), 999).unwrap();
    assert_eq!(
        prepared.publication, r.publication,
        "unknown commits cannot be rebound to a newer guard"
    );
    m.acknowledge(&id(1000), receipt.clone()).unwrap();
    m.acknowledge(&id(1000), receipt).unwrap();
    staged(&mut s, 1001);
    let pending = Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .pending(None, 1)
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].input.id(), id(1001));
    s.connection
        .execute(
            "UPDATE managed_publications SET receipt=x'00' WHERE operation_id=?",
            [id(1000)],
        )
        .unwrap();
    assert!(
        Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .pending(None, 1)
            .is_err()
    );
}
#[test]
fn full_disk_during_logical_ack_preserves_verified_blob_and_pending_intent() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&dir.path().join("client")).unwrap();
    let r = verified(&mut s, 1000, true);
    let receipt = metadata(&r);
    s.connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let result = Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .acknowledge(&id(1000), receipt.clone());
    let error = result.unwrap_err();
    assert!(
        crate::cache::is_disk_full(error.as_ref()),
        "must reach SQLITE_FULL, got {error}"
    );
    let pending = Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .get(&id(1000))
        .unwrap();
    assert!(pending.receipt.is_none());
    assert!(pending.publication.is_some());
    assert!(
        remote::Journal::new(&mut s, remote::Options::default())
            .unwrap()
            .flush_receipt(r.generation.unwrap())
            .unwrap()
            .is_some()
    );
    assert_eq!(
        journal::Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .read(r.generation.unwrap(), 0, 3)
            .unwrap(),
        b"abc"
    );
    s.connection
        .pragma_update(None, "max_page_count", pages + 100)
        .unwrap();
    Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .acknowledge(&id(1000), receipt)
        .unwrap();
}
#[test]
fn schema_eight_upgrade_keeps_namespace_receipts_and_client_manifests() {
    let dir = tempfile::tempdir().unwrap();
    for authority in [false, true] {
        let path = dir
            .path()
            .join(if authority { "authority" } else { "client" });
        let mut s = State::open_managed(&path).unwrap();
        let original = if authority {
            let v = s.namespace_initialize().unwrap();
            Some(
                s.namespace_apply(&Apply {
                    volume: v.id,
                    operation: id(10),
                    expected_sequence: 0,
                    mutation: Mutation::Create {
                        parent: crate::namespace::ROOT.into(),
                        name: "café".into(),
                        node_kind: Kind::EmptyFile,
                    },
                })
                .unwrap(),
            )
        } else {
            None
        };
        let generation = if !authority {
            let mut j = journal::Journal::new(&mut s, journal::Options::default()).unwrap();
            let g = j
                .begin(
                    &input(1000).target().unwrap(),
                    journal::Base::Missing,
                    None,
                    3,
                )
                .unwrap();
            j.append(g, b"abc").unwrap();
            j.seal(g).unwrap();
            Some(g)
        } else {
            None
        };
        s.connection.execute_batch(&crate::test_schema::legacy_sql("DROP TABLE chunk_source_settlements; DROP TABLE chunk_source_role; DROP TABLE chunk_publications; DROP TABLE chunk_client_role; DROP TABLE chunk_objects; DROP TABLE chunk_plans; DROP TABLE multipart_parts; DROP TABLE multipart_plans; DROP TABLE recovered_windows_opens; DROP TABLE windows_session_recoveries; DROP TABLE windows_mount_sessions; DROP TABLE retired_windows_opens; DROP TABLE windows_open_events; DROP TABLE retained_windows_opens; DROP TABLE windows_open_journal; DROP TABLE namespace_removal_events; ALTER TABLE namespace_removals DROP COLUMN mode; ALTER TABLE namespace_removals DROP COLUMN event_id; DROP TABLE logical_conflicts; DROP TABLE managed_rejections; DROP TABLE retained_client_identity; DROP TABLE client_identity; DROP TABLE namespace_removals; DROP TABLE namespace_intents; DROP INDEX namespace_portable_names; ALTER TABLE namespace_nodes DROP COLUMN name_key; ALTER TABLE namespace_nodes DROP COLUMN placement; DROP TABLE logical_publication_links; DROP TABLE logical_edits; DROP TABLE logical_files; DROP TABLE managed_publications;DROP TABLE namespace_uploads;DROP TABLE namespace_stores;ALTER TABLE namespace_nodes DROP COLUMN payload;PRAGMA user_version=8")).unwrap();
        drop(s);
        let mut s = State::open_managed(&path).unwrap();
        assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
        if let Some(original) = original {
            let v = s.namespace_initialize().unwrap();
            assert_eq!(
                s.namespace_operation(&v.id, &id(10)).unwrap(),
                Some(Ok(original))
            );
        }
        if let Some(g) = generation {
            assert_eq!(
                journal::Journal::new(&mut s, journal::Options::default())
                    .unwrap()
                    .read(g, 0, 3)
                    .unwrap(),
                b"abc"
            );
        }
    }
}

#[test]
fn an_ignored_acknowledgment_update_cannot_return_success() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&dir.path().join("client")).unwrap();
    let r = verified(&mut s, 1000, false);
    let receipt = metadata(&r);
    s.connection.execute_batch("CREATE TRIGGER ignore_managed_ack BEFORE UPDATE ON managed_publications WHEN NEW.receipt IS NOT NULL BEGIN SELECT RAISE(IGNORE); END").unwrap();
    assert!(
        Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .acknowledge(&id(1000), receipt)
            .is_err()
    );
    assert!(
        Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .get(&id(1000))
            .unwrap()
            .receipt
            .is_none()
    );
}
