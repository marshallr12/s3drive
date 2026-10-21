//! Owner-bearing record format, not an active native admission.
use super::*;
fn owned(n: u8) -> Input {
    let mut i = input(n);
    i.owner = Some(id(9));
    i
}
fn authority(path: &std::path::Path) -> State {
    let mut s = State::open_managed(path).unwrap();
    let v = s
        .namespace_initialize_with_policy(s3drive_core::names::PORTABLE)
        .unwrap();
    s.namespace_apply(&Apply {
        volume: v.id,
        operation: id(3),
        expected_sequence: 0,
        mutation: Mutation::RegisterStore {
            route: "a".repeat(64),
            root: "managed/".into(),
        },
    })
    .unwrap();
    s
}
#[test]
fn deferred_client_state_real_receipt_replays_restricted_without_source_body_or_owner_rebinding() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("client");
    let mut s = State::open_managed(&path).unwrap();
    let mut a = authority(&t.path().join("authority"));
    let mut i = owned(4);
    i.volume = a.namespace_volume().unwrap().id;
    let r = prepared(&mut s, &i);
    assert_eq!(r.format, 2);
    let request = r.request().unwrap().clone();
    let Mutation::CreateDeferredEmptyManifest { owner, .. } = &request.mutation else {
        panic!()
    };
    assert_eq!(Some(owner), i.owner.as_ref());
    let received = a.namespace_apply(&request).unwrap();
    let Some(ns::Artifact::PublishedRemoval { fence, .. }) = &received.artifact else {
        panic!()
    };
    assert_eq!(fence.intent, i.operation);
    assert_eq!(fence.owner, id(9));
    s.connection
        .execute("UPDATE staged_chunks SET data=randomblob(length(data))", [])
        .unwrap();
    drop(s);
    let mut recovery = RecoveryStore::open(&path).unwrap();
    assert_eq!(recovery.get(&i.operation).unwrap(), r);
    let done = recovery
        .acknowledge(&i.operation, &request, received.clone())
        .unwrap();
    assert_eq!(done.receipt(), Some(&received));
    drop(recovery);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(j(&mut s).get(&i.operation).unwrap(), done);
    assert_eq!(
        j(&mut s)
            .acknowledge(&i.operation, &request, received)
            .unwrap(),
        done
    );
}
#[test]
fn deferred_client_state_owner_and_fence_must_match_original_format_and_input() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("client")).unwrap();
    for owner in [ns::ROOT.into(), "f".repeat(31), "G".repeat(32)] {
        let mut i = owned(4);
        i.owner = Some(owner);
        let before = rows(&s);
        assert!(j(&mut s).begin(i).is_err());
        assert_eq!(rows(&s), before);
    }
    let i = owned(4);
    let r = prepared(&mut s, &i);
    let mut changed = i.clone();
    changed.owner = Some(id(10));
    assert!(j(&mut s).begin(changed.clone()).is_err());
    assert_eq!(i.body().unwrap(), changed.body().unwrap());
    assert_eq!(i.object_key().unwrap(), changed.object_key().unwrap());
    assert_ne!(i.stage_key().unwrap(), changed.stage_key().unwrap());
    let request = r.request().unwrap();
    let valid = receipt(&i, request);
    for case in 0..4 {
        let mut bad = valid.clone();
        match case {
            0 => {
                bad.artifact = Some(ns::Artifact::Published(bad.node.payload.clone().unwrap()));
            }
            1 => {
                let Some(ns::Artifact::PublishedRemoval { fence, .. }) = &mut bad.artifact else {
                    panic!()
                };
                fence.owner = id(10);
            }
            2 => {
                let Some(ns::Artifact::PublishedRemoval { fence, .. }) = &mut bad.artifact else {
                    panic!()
                };
                fence.intent = id(10);
            }
            _ => {
                bad.node.name = "another node".into();
            }
        }
        let before = rows(&s);
        assert!(j(&mut s).acknowledge(&i.operation, request, bad).is_err());
        assert_eq!(rows(&s), before);
    }
    assert!(j(&mut s).acknowledge(&i.operation, request, valid).is_ok());
    let mut wrong = r;
    wrong.format = 1;
    assert!(wrong.validate().is_err());
    let mut ordinary = wrong;
    ordinary.input.owner = None;
    ordinary.format = 2;
    assert!(ordinary.validate().is_err());
}
#[test]
fn deferred_client_state_self_consistent_owner_request_and_receipt_rewrite_cannot_replace_staging_witness()
 {
    for complete in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("client");
        let mut s = State::open_managed(&path).unwrap();
        let i = owned(4);
        let mut r = prepared(&mut s, &i);
        if complete {
            let q = r.request().unwrap().clone();
            r = j(&mut s)
                .acknowledge(&i.operation, &q, receipt(&i, &q))
                .unwrap();
        }
        r.input.owner = Some(id(10));
        let p = r.physical.as_ref().unwrap();
        r.request = Some(
            r.input
                .request(p.operation.clone(), p.etag.clone(), p.version.clone())
                .unwrap(),
        );
        if complete {
            r.receipt = Some(receipt(&r.input, r.request().unwrap()));
        }
        r.validate().unwrap();
        let mut map = BTreeMap::new();
        map.insert(i.operation.clone(), r.clone());
        s.connection
            .execute(
                "UPDATE manifest_creations SET record=?1,state_hash=?2",
                params![
                    serde_json::to_vec(&r).unwrap(),
                    record_hash(&r).unwrap().as_slice()
                ],
            )
            .unwrap();
        s.connection
            .execute(
                "UPDATE manifest_creation_role SET state_hash=?1",
                [projection(&map).unwrap().as_slice()],
            )
            .unwrap();
        let before = rows(&s);
        assert!(Journal::new(&mut s, Limits::default()).is_err());
        assert_eq!(rows(&s), before);
        drop(s);
        assert!(RecoveryStore::open(&path).is_err());
        assert!(State::open_managed(&path).is_err());
    }
}
#[test]
fn deferred_client_state_format1_bytes_and_stage_identity_survive42_to43_without_rewrite() {
    #[derive(Serialize)]
    struct Legacy<'a> {
        volume: &'a str,
        operation: &'a str,
        expected_sequence: u64,
        parent: &'a NodeGuard,
        name: &'a str,
        store: &'a Store,
        source: [u8; 32],
        fingerprint: &'a str,
    }
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("client");
    let mut s = State::open_managed(&path).unwrap();
    let i = input(4);
    let r = prepared(&mut s, &i);
    let q = r.request().unwrap().clone();
    let r = j(&mut s)
        .acknowledge(&i.operation, &q, receipt(&i, &q))
        .unwrap();
    assert_eq!(r.format, 1);
    assert!(
        serde_json::to_value(&r).unwrap()["input"]
            .get("owner")
            .is_none()
    );
    let old = Legacy {
        volume: &i.volume,
        operation: &i.operation,
        expected_sequence: i.expected_sequence,
        parent: &i.parent,
        name: &i.name,
        store: &i.store,
        source: i.source,
        fingerprint: &i.fingerprint,
    };
    assert_eq!(
        serde_json::to_vec(&old).unwrap(),
        serde_json::to_vec(&i).unwrap()
    );
    assert_eq!(
        hash(b"s3drive-empty-creation-stage-v1\0", &old).unwrap(),
        i.stage_key().unwrap()
    );
    let before = rows(&s);
    s.connection
        .pragma_update(None, "user_version", 42)
        .unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
    assert_eq!(rows(&s), before);
    assert_eq!(j(&mut s).get(&i.operation).unwrap(), r);
}
#[test]
fn deferred_client_state_old_marker_refuses_new_intent_and_format2_recovery_without_repair() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("client");
    let mut s = State::open_managed(&path).unwrap();
    j(&mut s).begin(input(4)).unwrap();
    s.connection
        .pragma_update(None, "user_version", 42)
        .unwrap();
    let before = rows(&s);
    assert!(j(&mut s).begin(owned(5)).is_err());
    assert_eq!(rows(&s), before);
    s.connection
        .pragma_update(None, "user_version", 43)
        .unwrap();
    prepared(&mut s, &owned(5));
    let before = rows(&s);
    s.connection
        .pragma_update(None, "user_version", 42)
        .unwrap();
    drop(s);
    assert!(RecoveryStore::open(&path).is_err());
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        42
    );
    let s = State {
        connection: db,
        _directory: None,
        incarnation: None,
        source_pin_witness: None,
    };
    assert_eq!(rows(&s), before);
}
#[test]
fn deferred_client_state_killed_owners_keep_exact_original_owner_at_all_eight_boundaries() {
    super::killed_creation_cases(true);
}

#[test]
fn deferred_client_state_actual_sqlite_full_rolls_back_owner_intent_then_exact_retry_succeeds() {
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
        let mut i = owned(n);
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
