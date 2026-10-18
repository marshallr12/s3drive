use super::*;
use crate::State;
use s3drive_core::manifest::{Entry, Page, Reference};
fn id(n: u8) -> String {
    format!("{n:02x}").repeat(16)
}
fn apply(s: &mut State, op: u8, m: Mutation) -> Receipt {
    let v = s.namespace_volume().unwrap();
    s.namespace_apply(&Apply {
        volume: v.id,
        operation: id(op),
        expected_sequence: v.sequence,
        mutation: m,
    })
    .unwrap()
}
fn setup(s: &mut State) -> Node {
    s.namespace_initialize_with_policy(s3drive_core::names::PORTABLE)
        .unwrap();
    apply(
        s,
        3,
        Mutation::RegisterStore {
            route: "a".repeat(64),
            root: "managed/".into(),
        },
    );
    apply(
        s,
        2,
        Mutation::CreateEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            store: Some(id(3)),
            name: "café 日本語 🦀".into(),
            node_kind: Kind::EmptyFile,
        },
    )
    .node
}
fn request(s: &State, n: &Node, op: u8, guard: ContentGuard) -> Apply {
    let v = s.namespace_volume().unwrap();
    let page = Page::new(
        bytes(&v.id).unwrap(),
        bytes(&n.id).unwrap(),
        0,
        0,
        vec![Entry::Hole {
            length: 9 * manifest::CHUNK_BYTES,
        }],
    )
    .unwrap();
    let root = RootDescriptor::new(
        bytes(&v.id).unwrap(),
        bytes(&n.id).unwrap(),
        page.length(),
        Reference::new([3; 16], [op; 16], 1 << 56, page.digest()).unwrap(),
    )
    .unwrap();
    Apply {
        volume: v.id,
        operation: id(op),
        expected_sequence: v.sequence,
        mutation: Mutation::ReserveManifest {
            node: n.id.clone(),
            expected_revision: n.revision,
            store: id(3),
            size: page.encode().len() as u64,
            sha256: Sha256::digest(page.encode())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            manifest: Box::new(ManifestRoot::new(root).unwrap()),
            guard,
        },
    }
}
fn normal(n: &Node) -> ContentGuard {
    ContentGuard {
        expected: payload_reference(&n.payload),
        detached: false,
        removal: None,
    }
}
fn reservation(s: &mut State, r: &Apply) -> Reservation {
    let Some(Artifact::Reservation(r)) = s.namespace_apply(r).unwrap().artifact else {
        panic!("reservation")
    };
    r
}
fn publish(s: &mut State, r: &Reservation, op: u8) -> Apply {
    Apply {
        volume: r.volume.clone(),
        operation: id(op),
        expected_sequence: s.namespace_volume().unwrap().sequence,
        mutation: Mutation::Publish {
            reservation: r.id.clone(),
            upload_operation: "d".repeat(64),
            etag: "\"opaque\"".into(),
            version: None,
        },
    }
}
#[test]
fn independent_manifest_payload_preserves_both_hash_domains_and_logical_length() {
    let raw = include_bytes!("../fixtures/manifest-payload.json");
    let p: Payload = serde_json::from_slice(raw).unwrap();
    assert!(p.valid());
    assert_eq!(serde_json::to_vec(&p).unwrap(), raw);
    assert_eq!(
        payload_reference(&Some(p.clone())).unwrap(),
        include_str!("../fixtures/manifest-reference.sha256").trim()
    );
    let r = &p.reservation;
    assert_eq!(r.logical_size().unwrap(), 128 * manifest::CHUNK_BYTES + 16);
    assert_eq!(r.size, 226);
    let root = r.manifest.as_ref().unwrap().root().unwrap();
    let page = include_bytes!("../../../chunked/fixtures/root.bin");
    root.verify_page(page).unwrap();
    let rawsha: String = Sha256::digest(page)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(r.sha256, rawsha);
    assert_ne!(
        root.page().sha256().as_slice(),
        Sha256::digest(page).as_slice()
    );
    let mut legacy = p.clone();
    legacy.reservation.manifest = None;
    assert!(!legacy.valid());
    let old_domain: String =
        Sha256::digest([b"s3drive-content-reference-v1\0".as_slice(), raw.as_slice()].concat())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
    assert_ne!(payload_reference(&Some(p)).unwrap(), old_domain);
}
#[test]
fn strict_manifest_encoding_context_and_geometry_refuse_ambiguous_identities() {
    let raw = include_bytes!("../fixtures/manifest-payload.json");
    let p: Payload = serde_json::from_slice(raw).unwrap();
    let m = p.reservation.manifest.as_ref().unwrap();
    for field in 0..9 {
        let mut p = p.clone();
        let r = &mut p.reservation;
        match field {
            0 => r.volume = id(8),
            1 => r.node = id(8),
            2 => r.store.id = id(8),
            3 => r.id = id(8),
            4 => r.key.push('x'),
            5 => r.size = 0,
            6 => r.size = 10433,
            7 => r.guard = None,
            _ => r.store.root = "elsewhere/".into(),
        }
        assert!(!p.valid(), "field {field}");
        assert!(p.reservation.logical_size().is_err());
    }
    for raw in ["", "00", &"00".repeat(121), &m.descriptor.to_uppercase()] {
        let mut bad = m.clone();
        bad.descriptor = raw.into();
        assert!(bad.root().is_err());
    }
    let mut bad = m.clone();
    bad.format = "s3dman02".into();
    assert!(bad.root().is_err());
    let r = m.root().unwrap();
    for ordinal in [0, 1 << 56, (2 << 56) | 1, u64::MAX] {
        let r = RootDescriptor::new(
            *r.volume(),
            *r.node(),
            r.length(),
            Reference::new(
                *r.page().store(),
                *r.page().generation(),
                ordinal,
                *r.page().sha256(),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(ManifestRoot::new(r).is_err());
    }
    let huge = RootDescriptor::new(
        *r.volume(),
        *r.node(),
        MAX_MANIFEST_LENGTH + 1,
        r.page().clone(),
    )
    .unwrap();
    assert!(ManifestRoot::new(huge).is_err());
    let mut value = serde_json::to_value(m).unwrap();
    value["extra"] = true.into();
    assert!(serde_json::from_value::<ManifestRoot>(value).is_err());
}
#[test]
fn root_reservations_replay_exactly_survive_rename_and_race_original_content() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let mut s = State::open_managed(&path).unwrap();
    let n = setup(&mut s);
    let a = request(&s, &n, 4, normal(&n));
    let b = request(&s, &n, 5, normal(&n));
    let ra = reservation(&mut s, &a);
    let rb = reservation(&mut s, &b);
    assert_ne!(ra.manifest, rb.manifest);
    assert_eq!(ra.logical_size().unwrap(), 9 * manifest::CHUNK_BYTES);
    let original = s.namespace_operation(&a.volume, &a.operation).unwrap();
    assert!(original.as_ref().unwrap().is_ok());
    let mut changed = a.clone();
    changed.expected_sequence += 1;
    assert_eq!(s.namespace_apply(&changed), Err(Error::OperationMismatch));
    apply(
        &mut s,
        6,
        Mutation::Move {
            node: n.id.clone(),
            parent: ROOT.into(),
            name: "renamed 🦀".into(),
        },
    );
    let pa = publish(&mut s, &ra, 7);
    let receipt = s.namespace_apply(&pa).unwrap();
    assert_eq!(receipt.node.name, "renamed 🦀");
    assert_eq!(receipt.node.payload.as_ref().unwrap().reservation, ra);
    assert_eq!(
        s.namespace_apply(&a).unwrap().artifact,
        Some(Artifact::Reservation(ra.clone()))
    );
    let pb = publish(&mut s, &rb, 8);
    assert_eq!(s.namespace_apply(&pb), Err(Error::Conflict));
    let old = receipt.clone();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.namespace_apply(&pa), Ok(old));
    assert_eq!(s.namespace_apply(&pb), Err(Error::Conflict));
    assert_eq!(
        s.namespace_operation(&a.volume, &a.operation).unwrap(),
        original
    );
    let n = s.namespace_node(&a.volume, &n.id).unwrap();
    let c = request(&s, &n, 9, normal(&n));
    let rc = reservation(&mut s, &c);
    let pc = publish(&mut s, &rc, 10);
    let newer = s.namespace_apply(&pc).unwrap();
    assert_ne!(
        payload_reference(&receipt.node.payload),
        payload_reference(&newer.node.payload)
    );
    assert_eq!(s.namespace_apply(&pa).unwrap(), receipt);
}
#[test]
fn invalid_apply_root_context_is_rejected_before_any_operation_is_recorded() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("authority")).unwrap();
    let n = setup(&mut s);
    let before = s.namespace_volume().unwrap();
    let mut r = request(&s, &n, 4, normal(&n));
    r.operation = id(8);
    assert_eq!(s.namespace_apply(&r), Err(Error::Invalid));
    assert_eq!(s.namespace_operation(&before.id, &r.operation), Ok(None));
    assert_eq!(s.namespace_volume().unwrap(), before);
    let mut r = request(&s, &n, 4, normal(&n));
    if let Mutation::ReserveManifest { guard, .. } = &mut r.mutation {
        guard.detached = true;
        guard.removal = Some(RemovalFence {
            owner: id(7),
            intent: id(8),
        });
    }
    assert!(r.validate().is_err());
}

#[test]
fn deleted_or_rewritten_manifest_effects_never_return_a_false_receipt() {
    for fault in 0..7 {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("authority")).unwrap();
        let n = setup(&mut s);
        let reserve = request(&s, &n, 4, normal(&n));
        let operation = if fault < 2 {
            reserve.clone()
        } else {
            let r = reservation(&mut s, &reserve);
            publish(&mut s, &r, 5)
        };
        let before = s.namespace_volume().unwrap();
        let node_before = s.namespace_node(&before.id, &n.id).unwrap();
        let trigger=match fault {
            0=>"CREATE TRIGGER damage AFTER INSERT ON namespace_uploads BEGIN DELETE FROM namespace_uploads WHERE reservation_id=NEW.reservation_id; END;".to_string(),
            1=>"CREATE TRIGGER damage AFTER INSERT ON namespace_operations BEGIN DELETE FROM namespace_uploads; END;".to_string(),
            2=>"CREATE TRIGGER damage AFTER UPDATE OF publication ON namespace_uploads BEGIN UPDATE namespace_uploads SET publication=NULL WHERE reservation_id=NEW.reservation_id; END;".to_string(),
            3=>"CREATE TRIGGER damage AFTER INSERT ON namespace_operations BEGIN UPDATE namespace_nodes SET payload=NULL WHERE payload IS NOT NULL; END;".to_string(),
            4=>"CREATE TRIGGER damage AFTER UPDATE OF payload ON namespace_nodes BEGIN UPDATE namespace_nodes SET revision=OLD.revision WHERE node_id=NEW.node_id; END;".to_string(),
            5=>"CREATE TRIGGER damage AFTER UPDATE OF payload ON namespace_nodes BEGIN UPDATE namespace_nodes SET deleted=1 WHERE node_id=NEW.node_id; END;".to_string(),
            _=>"CREATE TRIGGER damage AFTER INSERT ON namespace_operations BEGIN UPDATE namespace_volume SET sequence=sequence-1; END;".to_string(),
        };
        s.connection.execute_batch(&trigger).unwrap();
        assert_eq!(
            s.namespace_apply(&operation),
            Err(Error::Internal),
            "fault {fault}"
        );
        assert_eq!(s.namespace_volume().unwrap(), before);
        assert_eq!(s.namespace_node(&before.id, &n.id).unwrap(), node_before);
        assert_eq!(
            s.namespace_operation(&before.id, &operation.operation),
            Ok(None)
        );
        s.connection.execute_batch("DROP TRIGGER damage").unwrap();
        assert!(s.namespace_apply(&operation).is_ok());
    }
}

#[test]
fn manifest_publication_requires_exact_active_removal_fence_in_both_modes() {
    for deferred in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("authority")).unwrap();
        let n = setup(&mut s);
        let parent = s
            .namespace_node(&s.namespace_volume().unwrap().id, ROOT)
            .unwrap();
        let ng = |n: &Node| NodeGuard {
            id: n.id.clone(),
            revision: n.revision,
        };
        let a = apply(
            &mut s,
            11,
            if deferred {
                Mutation::PrepareRemoval {
                    node: ng(&n),
                    parent: ng(&parent),
                    owner: id(12),
                }
            } else {
                Mutation::ArmRemoval {
                    node: ng(&n),
                    parent: ng(&parent),
                    owner: id(12),
                }
            },
        );
        let Some(Artifact::Removal(fence)) = a.artifact else {
            panic!()
        };
        let denied = request(&s, &a.node, 4, normal(&a.node));
        assert_eq!(s.namespace_apply(&denied), Err(Error::DeletePending));
        let mut owner = normal(&a.node);
        owner.removal = Some(fence.clone());
        let r = request(&s, &a.node, 5, owner.clone());
        let reserved = reservation(&mut s, &r);
        let p = publish(&mut s, &reserved, 6);
        let published = s.namespace_apply(&p).unwrap();
        assert!(
            published
                .node
                .payload
                .as_ref()
                .unwrap()
                .reservation
                .manifest
                .is_some()
        );
        let mut owner2 = owner;
        owner2.expected = payload_reference(
            &s.namespace_pending_removal(&r.volume, &n.id)
                .unwrap()
                .unwrap()
                .node
                .payload,
        );
        let now = s
            .namespace_pending_removal(&r.volume, &n.id)
            .unwrap()
            .unwrap()
            .node;
        let r2 = request(&s, &now, 7, owner2);
        let reserved2 = reservation(&mut s, &r2);
        apply(
            &mut s,
            8,
            Mutation::CancelRemoval {
                node: n.id.clone(),
                fence,
            },
        );
        let p2 = publish(&mut s, &reserved2, 9);
        assert_eq!(s.namespace_apply(&p2), Err(Error::Conflict));
        assert_eq!(s.namespace_apply(&p), Ok(published));
    }
}
#[test]
fn detached_manifest_publication_preserves_tombstone_and_reused_name() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("authority")).unwrap();
    let n = setup(&mut s);
    let pending = request(&s, &n, 4, normal(&n));
    let pending = reservation(&mut s, &pending);
    apply(&mut s, 5, Mutation::Remove { node: n.id.clone() });
    let reused = apply(
        &mut s,
        6,
        Mutation::CreateEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            store: Some(id(3)),
            name: n.name.clone(),
            node_kind: Kind::EmptyFile,
        },
    )
    .node;
    let p = publish(&mut s, &pending, 7);
    assert_eq!(s.namespace_apply(&p), Err(Error::NotFound));
    let tombstone = s.namespace_node(&pending.volume, &n.id).unwrap();
    assert!(tombstone.deleted);
    let mut guard = normal(&tombstone);
    guard.detached = true;
    let r = request(&s, &tombstone, 8, guard);
    let r = reservation(&mut s, &r);
    let p = publish(&mut s, &r, 9);
    let result = s.namespace_apply(&p).unwrap();
    assert!(result.node.deleted);
    assert!(result.node.payload.unwrap().reservation.manifest.is_some());
    assert_eq!(s.namespace_node(&r.volume, &reused.id).unwrap(), reused);
    assert_eq!(
        s.namespace_list(&r.volume, ROOT).unwrap().children,
        vec![reused]
    );
}
#[test]
fn legacy_manifest_and_legacy_replacements_keep_original_receipts_and_hashes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("authority")).unwrap();
    let n = setup(&mut s);
    let first = apply(
        &mut s,
        10,
        Mutation::ReserveContent {
            node: n.id.clone(),
            expected_revision: n.revision,
            store: id(3),
            size: 5,
            sha256: "a".repeat(64),
            expected_payload: None,
        },
    );
    let Some(Artifact::Reservation(old)) = first.artifact else {
        panic!()
    };
    let p = publish(&mut s, &old, 11);
    let old_result = s.namespace_apply(&p).unwrap();
    let old_json = serde_json::to_vec(&old_result).unwrap();
    let old_reference = payload_reference(&old_result.node.payload);
    let r = request(&s, &old_result.node, 12, normal(&old_result.node));
    let r = reservation(&mut s, &r);
    let p2 = publish(&mut s, &r, 13);
    let chunked = s.namespace_apply(&p2).unwrap();
    assert_ne!(payload_reference(&chunked.node.payload), old_reference);
    let restored = apply(
        &mut s,
        14,
        Mutation::ReserveContent {
            node: n.id.clone(),
            expected_revision: chunked.node.revision,
            store: id(3),
            size: 5,
            sha256: "a".repeat(64),
            expected_payload: payload_reference(&chunked.node.payload),
        },
    );
    let Some(Artifact::Reservation(r)) = restored.artifact else {
        panic!()
    };
    assert!(r.manifest.is_none());
    let p3 = publish(&mut s, &r, 15);
    let result = s.namespace_apply(&p3).unwrap();
    assert!(result.node.payload.unwrap().reservation.manifest.is_none());
    assert_eq!(
        serde_json::to_vec(&s.namespace_apply(&p).unwrap()).unwrap(),
        old_json
    );
    assert_eq!(payload_reference(&old_result.node.payload), old_reference);
}
#[test]
fn schema_twenty_five_marker_upgrade_preserves_exact_legacy_request_and_payload_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let mut s = State::open_managed(&path).unwrap();
    let n = setup(&mut s);
    let r = apply(
        &mut s,
        10,
        Mutation::ReserveContent {
            node: n.id.clone(),
            expected_revision: n.revision,
            store: id(3),
            size: 5,
            sha256: "a".repeat(64),
            expected_payload: None,
        },
    );
    let Some(Artifact::Reservation(r)) = r.artifact else {
        panic!()
    };
    let p = publish(&mut s, &r, 11);
    let published = s.namespace_apply(&p).unwrap();
    let rows = |s: &State| -> Vec<(Vec<u8>, Vec<u8>)> {
        s.connection
            .prepare("SELECT request,outcome FROM namespace_operations ORDER BY operation_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    let before = rows(&s);
    let body: Vec<u8> = s
        .connection
        .query_row(
            "SELECT descriptor FROM namespace_uploads WHERE reservation_id=?1",
            [&r.id],
            |r| r.get(0),
        )
        .unwrap();
    s.connection
        .execute_batch("DROP TABLE chunk_source_settlements; DROP TABLE chunk_source_role; DROP TABLE chunk_publications; DROP TABLE chunk_client_role")
        .unwrap();
    s.connection
        .pragma_update(None, "user_version", 25)
        .unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
    assert_eq!(rows(&s), before);
    let after: Vec<u8> = s
        .connection
        .query_row(
            "SELECT descriptor FROM namespace_uploads WHERE reservation_id=?1",
            [&r.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(body, after);
    assert_eq!(s.namespace_apply(&p), Ok(published));
}

#[test]
fn malformed_or_rebound_historical_root_results_fail_without_rewriting_evidence() {
    for fault in 0..3 {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("authority")).unwrap();
        let n = setup(&mut s);
        let request = request(&s, &n, 4, normal(&n));
        let r = reservation(&mut s, &request);
        let raw: Vec<u8> = s
            .connection
            .query_row(
                "SELECT outcome FROM namespace_operations WHERE operation_id=?1",
                [&r.id],
                |r| r.get(0),
            )
            .unwrap();
        let mut changed: std::result::Result<Receipt, Error> =
            serde_json::from_slice(&raw).unwrap();
        let Some(Artifact::Reservation(r)) = &mut changed.as_mut().unwrap().artifact else {
            panic!()
        };
        match fault {
            0 => r.manifest = None,
            1 => r.sha256 = "f".repeat(64),
            _ => r.manifest.as_mut().unwrap().format = "unknown".into(),
        };
        let damaged = serde_json::to_vec(&changed).unwrap();
        s.connection
            .execute(
                "UPDATE namespace_operations SET outcome=?1 WHERE operation_id=?2",
                params![damaged, request.operation],
            )
            .unwrap();
        assert_eq!(
            s.namespace_operation(&request.volume, &request.operation),
            Err(Error::Internal)
        );
        assert_eq!(s.namespace_apply(&request), Err(Error::Internal));
        let after: Vec<u8> = s
            .connection
            .query_row(
                "SELECT outcome FROM namespace_operations WHERE operation_id=?1",
                [&request.operation],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(after, damaged);
    }
}
#[test]
fn legacy_local_binding_and_managed_input_refuse_manifest_before_staging() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("authority")).unwrap();
    let n = setup(&mut s);
    let request = request(&s, &n, 4, normal(&n));
    let r = reservation(&mut s, &request);
    let p = publish(&mut s, &r, 5);
    let published = s.namespace_apply(&p).unwrap();
    let binding = crate::logical::Binding {
        volume: r.volume.clone(),
        node: published.node,
        store: r.store.clone(),
        source: [9; 32],
        fingerprint: "a".repeat(64),
    };
    assert!(binding.target().is_err());
    let input = crate::managed::Input {
        reserve: request,
        publish_operation: p.operation,
        store: r.store,
        source: [9; 32],
        fingerprint: "a".repeat(64),
    };
    assert!(input.reservation().is_err());
}

#[test]
fn real_sqlite_full_preserves_root_reservation_or_publication_for_exact_retry() {
    for publishing in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("authority")).unwrap();
        let n = setup(&mut s);
        let r = request(&s, &n, 4, normal(&n));
        let r = if publishing {
            let r = reservation(&mut s, &r);
            publish(&mut s, &r, 5)
        } else {
            r
        };
        let before = s.namespace_volume().unwrap();
        let node_before = s.namespace_node(&before.id, &n.id).unwrap();
        s.connection
            .execute_batch("CREATE TABLE pressure(data BLOB)")
            .unwrap();
        let event = if publishing {
            "BEFORE UPDATE OF publication"
        } else {
            "BEFORE INSERT"
        };
        s.connection.execute_batch(&format!("CREATE TRIGGER pressure_root {event} ON namespace_uploads BEGIN INSERT INTO pressure VALUES(zeroblob(65536)); END;")).unwrap();
        let pages: i64 = s
            .connection
            .pragma_query_value(None, "page_count", |r| r.get(0))
            .unwrap();
        s.connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        loop {
            match s
                .connection
                .execute("INSERT INTO pressure VALUES(zeroblob(4096))", [])
            {
                Ok(_) => (),
                Err(e) => {
                    assert_eq!(e.sqlite_error_code(), Some(rusqlite::ErrorCode::DiskFull));
                    break;
                }
            }
        }
        assert_eq!(s.namespace_apply(&r), Err(Error::Internal));
        assert_eq!(s.namespace_volume().unwrap(), before);
        assert_eq!(s.namespace_node(&before.id, &n.id).unwrap(), node_before);
        assert_eq!(s.namespace_operation(&before.id, &r.operation), Ok(None));
        s.connection
            .pragma_update(None, "max_page_count", 1000000)
            .unwrap();
        s.connection
            .execute_batch("DROP TRIGGER pressure_root")
            .unwrap();
        assert!(s.namespace_apply(&r).is_ok());
    }
}

#[test]
fn manifest_publish_replay_is_bound_to_original_proof_guard_and_request_sequence() {
    for fault in 0..4 {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("authority")).unwrap();
        let n = setup(&mut s);
        let request = request(&s, &n, 4, normal(&n));
        let r = reservation(&mut s, &request);
        let p = publish(&mut s, &r, 5);
        let original = s.namespace_apply(&p).unwrap();
        let second = self::request(&s, &original.node, 6, normal(&original.node));
        let r2 = reservation(&mut s, &second);
        let p2 = publish(&mut s, &r2, 7);
        let other = s.namespace_apply(&p2).unwrap();
        assert_eq!(
            s.namespace_apply(&p),
            Ok(original.clone()),
            "historical reply must survive a newer publication"
        );
        let mut changed = original;
        match fault {
            0 => changed = other,
            1 => changed.node.deleted = !changed.node.deleted,
            2 => {
                changed.sequence = p.expected_sequence;
                changed.node.revision = changed.sequence;
            }
            _ => {
                let Some(Artifact::Published(payload)) = &mut changed.artifact else {
                    panic!()
                };
                payload.upload_operation = "e".repeat(64);
                changed.node.payload = Some(payload.clone());
            }
        }
        let damaged = serde_json::to_vec(&Ok::<_, Error>(changed)).unwrap();
        s.connection
            .execute(
                "UPDATE namespace_operations SET outcome=?1 WHERE operation_id=?2",
                params![damaged, p.operation],
            )
            .unwrap();
        assert_eq!(
            s.namespace_operation(&p.volume, &p.operation),
            Err(Error::Internal),
            "fault {fault}"
        );
        assert_eq!(s.namespace_apply(&p), Err(Error::Internal), "fault {fault}");
        let after: Vec<u8> = s
            .connection
            .query_row(
                "SELECT outcome FROM namespace_operations WHERE operation_id=?1",
                [&p.operation],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(after, damaged);
    }
}
