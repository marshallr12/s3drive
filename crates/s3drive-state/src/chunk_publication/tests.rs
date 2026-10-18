//! Local synthetic physical acknowledgments plus real SQLite authority CAS.
//! No S3 requests, native activation, source settlement or provider acceptance.
use super::*;
use crate::{
    journal::{self, Base},
    remote,
};
use s3drive_core::manifest::{Reference, objects::RootDescriptor};
use std::path::PathBuf;
#[path = "sources_tests.rs"]
mod source_settlement;
fn uuid(n: u8) -> String {
    hex(&[n; 16])
}
fn bytes(s: &str) -> [u8; 16] {
    std::array::from_fn(|at| u8::from_str_radix(&s[at * 2..at * 2 + 2], 16).unwrap())
}
struct Fixture {
    _tmp: tempfile::TempDir,
    path: PathBuf,
    client: State,
    authority: State,
    input: chunked::Input,
    base: Node,
    store: Store,
    snapshot: GenerationId,
}
fn apply(s: &mut State, op: u8, mutation: Mutation) -> Receipt {
    let v = s.namespace_volume().unwrap();
    s.namespace_apply(&Apply {
        volume: v.id,
        operation: uuid(op),
        expected_sequence: v.sequence,
        mutation,
    })
    .unwrap()
}
impl Fixture {
    fn new(large: bool) -> Self {
        Self::with_policy(large, s3drive_core::names::PORTABLE)
    }
    fn with_policy(large: bool, policy: &str) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("client");
        let mut authority = State::open_managed(&tmp.path().join("authority")).unwrap();
        let volume = authority.namespace_initialize_with_policy(policy).unwrap();
        apply(
            &mut authority,
            3,
            Mutation::RegisterStore {
                route: "b".repeat(64),
                root: "managed/".into(),
            },
        );
        let base = apply(
            &mut authority,
            2,
            if policy == s3drive_core::names::PORTABLE {
                Mutation::CreateEntry {
                    parent: namespace::ROOT.into(),
                    parent_revision: 0,
                    store: Some(uuid(3)),
                    name: "café 日本語 🦀.txt".into(),
                    node_kind: Kind::EmptyFile,
                }
            } else {
                Mutation::Create {
                    parent: namespace::ROOT.into(),
                    name: "café 日本語 🦀.txt".into(),
                    node_kind: Kind::EmptyFile,
                }
            },
        )
        .node;
        let store = authority.namespace_store(&volume.id, &uuid(3)).unwrap();
        let mut client = State::open_managed(&path).unwrap();
        let input = chunked::Input {
            id: [7; 32],
            volume: bytes(&volume.id),
            node: [2; 16],
            store: [3; 16],
            generation: [4; 16],
            root: store.root.clone(),
            source: [8; 32],
            fingerprint: "a".repeat(64),
            parent: None,
            size: if large {
                128 * s3drive_core::manifest::CHUNK_BYTES + 64
            } else {
                64
            },
            expected_content: None,
        };
        chunked::Journal::new(&mut client, chunked::Options::default())
            .unwrap()
            .freeze(input.clone())
            .unwrap();
        let mut j = journal::Journal::new(&mut client, journal::Options::default()).unwrap();
        let snapshot = j
            .begin_operation(
                &input.staging_operation().unwrap(),
                &input.local_target().unwrap(),
                Base::Missing,
                None,
                input.size,
            )
            .unwrap();
        let tail = "binary\0 café 日本語 🦀".as_bytes();
        let zeros = input.size - tail.len() as u64;
        if zeros > 0 {
            j.append_zero_at(snapshot, 0, zeros).unwrap();
        }
        j.append_at(snapshot, zeros, tail).unwrap();
        j.seal(snapshot).unwrap();
        let mut j = chunked::Journal::new(&mut client, chunked::Options::default()).unwrap();
        let mut info = j.attach(input.id, snapshot).unwrap();
        while info.root().is_none() {
            info = j.step(input.id, info.cursor()).unwrap();
        }
        Self {
            _tmp: tmp,
            path,
            client,
            authority,
            input,
            base,
            store,
            snapshot,
        }
    }
    fn journal(&mut self) -> Journal<'_> {
        Journal::new(&mut self.client).unwrap()
    }
    fn begin(&mut self) -> Record {
        let seq = self.authority.namespace_volume().unwrap().sequence;
        let id = self.input.id;
        let base = self.base.clone();
        let store = self.store.clone();
        self.journal().begin(id, base, store, seq, uuid(5)).unwrap()
    }
    fn reserve(&mut self) -> Record {
        let r = self.begin();
        let receipt = self.authority.namespace_apply(r.input.reserve()).unwrap();
        let id = self.input.id;
        self.journal().reserved(id, receipt).unwrap()
    }
    fn complete(&mut self) -> Vec<GenerationId> {
        let p = chunked::Journal::new(&mut self.client, chunked::Options::default())
            .unwrap()
            .info(self.input.id)
            .unwrap();
        let mut out = Vec::new();
        for l in -1..=p.root_level() {
            for slot in 0..p.object_slots(l).unwrap() {
                let t = chunked::Journal::new(&mut self.client, chunked::Options::default())
                    .unwrap()
                    .materialize(self.input.id, l, slot, journal::Options::default())
                    .unwrap();
                if let Some(t) = t {
                    let mut r =
                        remote::Journal::new(&mut self.client, remote::Options::default()).unwrap();
                    let i = r.prepare(t.generation().unwrap()).unwrap();
                    let _permit = r.claim(i.operation, &i.target).unwrap();
                    r.verified(remote::Verification {
                        operation: i.operation,
                        target: i.target,
                        payload_sha256: i.payload_sha256,
                        size: i.size,
                        revision: remote::Revision::new("\"synthetic\"", None).unwrap(),
                    })
                    .unwrap();
                    r.acknowledge(i.operation).unwrap();
                    out.push(t.generation().unwrap());
                }
            }
        }
        out
    }
    fn prepare(&mut self) -> Record {
        self.reserve();
        self.complete();
        let id = self.input.id;
        let seq = self.authority.namespace_volume().unwrap().sequence;
        self.journal().prepare_publish(id, seq).unwrap()
    }
    fn reopen(&mut self) {
        // Move the owner out so its lock/connection close before opening again.
        let temp = State::open_managed(&self._tmp.path().join("swap")).unwrap();
        drop(std::mem::replace(&mut self.client, temp));
        self.client = State::open_managed(&self.path).unwrap();
    }
    fn source_pending(&mut self) {
        assert!(
            remote::Journal::new(&mut self.client, remote::Options::default())
                .unwrap()
                .flush_receipt(self.snapshot)
                .unwrap()
                .is_none()
        );
        assert!(
            journal::Journal::new(&mut self.client, journal::Options::default())
                .unwrap()
                .pending(None, 256)
                .unwrap()
                .iter()
                .any(|g| g.id == self.snapshot)
        );
    }
}
#[test]
fn exact_requests_survive_every_boundary_lost_replies_newer_root_and_dirty_head() {
    for boundary in 0..=4 {
        let mut f = Fixture::new(false);
        let plan = f.input.id;
        let begun = f.begin();
        let reserve_bytes = serde_json::to_vec(begun.input.reserve()).unwrap();
        if boundary == 0 {
            f.reopen();
        }
        let receipt = f.authority.namespace_apply(begun.input.reserve()).unwrap();
        if boundary == 1 {
            f.reopen();
        }
        let saved = f.journal().get(plan).unwrap();
        assert_eq!(
            serde_json::to_vec(saved.input.reserve()).unwrap(),
            reserve_bytes
        );
        assert_eq!(
            f.authority.namespace_apply(saved.input.reserve()).unwrap(),
            receipt
        );
        f.journal().reserved(plan, receipt).unwrap();
        f.complete();
        if boundary == 2 {
            f.reopen();
        }
        let seq = f.authority.namespace_volume().unwrap().sequence;
        let prepared = f.journal().prepare_publish(plan, seq).unwrap();
        let request = prepared.publication().unwrap().clone();
        let publish_bytes = serde_json::to_vec(&request).unwrap();
        let committed = f.authority.namespace_apply(&request).unwrap();
        if boundary == 3 {
            f.reopen();
        }
        // Newer root at the authority must not invalidate historical recovery.
        let mut newer = begun.input.reserve().clone();
        newer.operation = uuid(6);
        newer.expected_sequence = committed.sequence;
        if let Mutation::ReserveManifest {
            manifest,
            guard,
            expected_revision,
            ..
        } = &mut newer.mutation
        {
            guard.expected = namespace::payload_reference(&committed.node.payload);
            *expected_revision = committed.node.revision;
            let old = manifest.root().unwrap();
            **manifest = ManifestRoot::new(
                RootDescriptor::new(
                    *old.volume(),
                    *old.node(),
                    old.length(),
                    Reference::new(
                        *old.page().store(),
                        [6; 16],
                        old.page().ordinal(),
                        *old.page().sha256(),
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        }
        f.authority.namespace_apply(&newer).unwrap();
        apply(
            &mut f.authority,
            7,
            Mutation::Publish {
                reservation: uuid(6),
                upload_operation: "f".repeat(64),
                etag: "\"newer\"".into(),
                version: None,
            },
        );
        let saved = f.journal().get(plan).unwrap();
        assert_eq!(
            serde_json::to_vec(saved.publication().unwrap()).unwrap(),
            publish_bytes
        );
        assert!(f.journal().prepare_publish(plan, seq + 1).is_err());
        assert_eq!(
            f.authority
                .namespace_apply(saved.publication().unwrap())
                .unwrap(),
            committed
        );
        // A different local head remains pending after exact root receipt storage.
        let mut j = journal::Journal::new(&mut f.client, journal::Options::default()).unwrap();
        let dirty = j
            .begin(
                &f.input.local_target().unwrap(),
                Base::Missing,
                Some(f.snapshot),
                1,
            )
            .unwrap();
        j.append(dirty, b"x").unwrap();
        j.seal(dirty).unwrap();
        let r = f.journal().published(plan, committed.clone()).unwrap();
        assert_eq!(r.receipt(), Some(&committed));
        assert_ne!(r.root_generation(), Some(f.snapshot));
        if boundary == 4 {
            f.reopen();
        }
        assert_eq!(f.journal().get(plan).unwrap(), r);
        assert_eq!(
            journal::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .head(&f.input.local_target().unwrap())
                .unwrap()
                .unwrap()
                .id,
            dirty
        );
        f.source_pending();
    }
}
#[test]
fn reserve_conflict_precedes_upload_and_only_exact_conflict_is_terminal() {
    let mut f = Fixture::new(false);
    let plan = f.input.id;
    let seq = f.authority.namespace_volume().unwrap().sequence;
    let b = f.base.clone();
    let store = f.store.clone();
    let r = f
        .journal()
        .begin(plan, b, store, seq + 100, uuid(5))
        .unwrap();
    assert_eq!(
        f.authority.namespace_apply(r.input.reserve()),
        Err(namespace::Error::Conflict)
    );
    assert!(
        f.journal()
            .reject(plan, r.input.reserve.clone(), namespace::Error::Unavailable)
            .is_err()
    );
    let mut other = r.input.reserve.clone();
    other.expected_sequence = seq;
    assert!(
        f.journal()
            .reject(plan, other, namespace::Error::Conflict)
            .is_err()
    );
    let rejected = f
        .journal()
        .reject(plan, r.input.reserve.clone(), namespace::Error::Conflict)
        .unwrap();
    assert!(rejected.publication().is_none());
    f.reopen();
    assert_eq!(f.journal().get(plan).unwrap(), rejected);
    f.source_pending();
}
#[test]
fn rename_receipt_and_publication_content_race_retain_exact_requests() {
    let mut f = Fixture::new(false);
    let plan = f.input.id;
    let r = f.begin();
    apply(
        &mut f.authority,
        10,
        Mutation::Move {
            node: f.base.id.clone(),
            parent: namespace::ROOT.into(),
            name: "renamed 🦀".into(),
        },
    );
    let a = f.authority.namespace_apply(r.input.reserve()).unwrap();
    assert_ne!(a.node.name, r.input.base.name);
    f.journal().reserved(plan, a).unwrap();
    f.complete();
    let seq = f.authority.namespace_volume().unwrap().sequence;
    let p = f.journal().prepare_publish(plan, seq).unwrap();
    // Competing content publication consumes the same reservation first.
    apply(
        &mut f.authority,
        11,
        p.publication().unwrap().mutation.clone(),
    );
    assert_eq!(
        f.authority.namespace_apply(p.publication().unwrap()),
        Err(namespace::Error::Conflict)
    );
    let rejected = f
        .journal()
        .reject(
            plan,
            p.publication().unwrap().clone(),
            namespace::Error::Conflict,
        )
        .unwrap();
    assert!(rejected.root_generation().is_some());
    f.reopen();
    assert_eq!(f.journal().get(plan).unwrap(), rejected);
    f.source_pending();
}
#[test]
fn recursive_receipt_damage_blocks_prepare_without_replacing_intent() {
    let mut f = Fixture::new(true);
    let plan = f.input.id;
    let reserved = f.reserve();
    let objects = f.complete();
    let child = objects[0];
    f.client
        .connection
        .execute(
            "DELETE FROM write_settlements WHERE generation_id=?1",
            [child.value()],
        )
        .unwrap();
    f.client
        .connection
        .execute(
            "DELETE FROM remote_flushes WHERE generation_id=?1",
            [child.value()],
        )
        .unwrap();
    assert!(
        chunked::Journal::new(&mut f.client, chunked::Options::default())
            .unwrap()
            .require_dependencies(plan, 1, 0)
            .is_ok()
    );
    let seq = f.authority.namespace_volume().unwrap().sequence;
    assert!(f.journal().prepare_publish(plan, seq).is_err());
    assert_eq!(f.journal().get(plan).unwrap(), reserved);
    f.source_pending();
}
#[test]
fn completed_role_excludes_legacy_and_legacy_identity_excludes_new_role() {
    let mut f = Fixture::new(false);
    let plan = f.input.id;
    let p = f.prepare();
    let a = f
        .authority
        .namespace_apply(p.publication().unwrap())
        .unwrap();
    f.journal().published(plan, a).unwrap();
    f.reopen();
    assert!(crate::managed::Journal::new(&mut f.client, journal::Options::default()).is_err());
    assert!(crate::logical::Journal::new(&mut f.client, journal::Options::default()).is_err());
    assert!(crate::namespace_client::Journal::new(&mut f.client).is_err());
    assert!(crate::namespace_client::opens::Journal::new(&mut f.client).is_err());
    assert!(f.client.namespace_initialize().is_err());
    f.source_pending();
    let mut g = Fixture::new(false);
    let identity = crate::namespace_client::Journal::new(&mut g.client)
        .unwrap()
        .client_identity()
        .unwrap();
    assert!(Journal::new(&mut g.client).is_err());
    assert_eq!(
        crate::namespace_client::Journal::new(&mut g.client)
            .unwrap()
            .client_identity()
            .unwrap(),
        identity
    );
    let role: u32 = g
        .client
        .connection
        .query_row("SELECT count(*) FROM chunk_client_role", [], |r| r.get(0))
        .unwrap();
    assert_eq!(role, 0);
}
#[test]
fn post_write_trigger_damage_rolls_back_exact_record_plan_receipts_and_isolation() {
    for effect in [
        "SELECT RAISE(IGNORE)",
        "DELETE FROM chunk_publications",
        "UPDATE chunk_publications SET record=zeroblob(length(record))",
        "UPDATE chunk_plans SET manifest_hash=zeroblob(32)",
        "DELETE FROM write_settlements; DELETE FROM remote_flushes",
        "INSERT INTO retained_client_identity VALUES(1,zeroblob(32))",
        "UPDATE chunk_client_role SET admitted=admitted+1",
    ] {
        let mut f = Fixture::new(false);
        let plan = f.input.id;
        let p = f.prepare();
        let a = f
            .authority
            .namespace_apply(p.publication().unwrap())
            .unwrap();
        let timing = if effect == "SELECT RAISE(IGNORE)" {
            "BEFORE"
        } else {
            "AFTER"
        };
        f.client
            .connection
            .execute_batch(&format!(
                "CREATE TRIGGER damage {timing} UPDATE ON chunk_publications BEGIN {effect}; END"
            ))
            .unwrap();
        assert!(f.journal().published(plan, a.clone()).is_err(), "{effect}");
        assert_eq!(f.journal().get(plan).unwrap(), p, "{effect}");
        f.client
            .connection
            .execute_batch("DROP TRIGGER damage")
            .unwrap();
        assert_eq!(
            f.journal().published(plan, a.clone()).unwrap().receipt(),
            Some(&a)
        );
        f.source_pending();
    }
}
#[test]
fn ignored_insert_or_role_effect_cannot_acknowledge_admission() {
    for (table, effect) in [
        ("chunk_publications", "SELECT RAISE(IGNORE)"),
        ("chunk_publications", "DELETE FROM chunk_publications"),
        (
            "chunk_publications",
            "INSERT INTO retained_client_identity VALUES(1,zeroblob(32))",
        ),
        ("chunk_client_role", "SELECT RAISE(IGNORE)"),
        ("chunk_client_role", "DELETE FROM chunk_client_role"),
    ] {
        let mut f = Fixture::new(false);
        let seq = f.authority.namespace_volume().unwrap().sequence;
        let timing = if effect == "SELECT RAISE(IGNORE)" {
            "BEFORE"
        } else {
            "AFTER"
        };
        f.client
            .connection
            .execute_batch(&format!(
                "CREATE TRIGGER damage {timing} INSERT ON {table} BEGIN {effect}; END"
            ))
            .unwrap();
        let (plan, b, store) = (f.input.id, f.base.clone(), f.store.clone());
        assert!(
            f.journal().begin(plan, b, store, seq, uuid(5)).is_err(),
            "{table}: {effect}"
        );
        assert_eq!(
            f.client
                .connection
                .query_row("SELECT count(*) FROM chunk_client_role", [], |r| r
                    .get::<_, u32>(0))
                .unwrap(),
            0
        );
        f.client
            .connection
            .execute_batch("DROP TRIGGER damage")
            .unwrap();
        f.begin();
        f.source_pending();
    }
}
#[test]
fn lost_row_and_role_damage_fail_closed_across_reopen() {
    for effect in [
        "DELETE FROM chunk_publications",
        "DELETE FROM chunk_client_role",
        "UPDATE chunk_client_role SET state_hash=zeroblob(32)",
        "UPDATE chunk_publications SET state_hash=zeroblob(32)",
    ] {
        let mut f = Fixture::new(false);
        f.begin();
        f.client.connection.execute_batch(effect).unwrap();
        assert!(Journal::new(&mut f.client).is_err());
        assert!(crate::namespace_client::Journal::new(&mut f.client).is_err());
        let temp = State::open_managed(&f._tmp.path().join("swap")).unwrap();
        drop(std::mem::replace(&mut f.client, temp));
        assert!(State::open_managed(&f.path).is_err());
    }
}
#[test]
fn actual_sqlite_full_keeps_prepared_unknown_request_and_recovers() {
    let mut f = Fixture::new(false);
    let plan = f.input.id;
    let p = f.prepare();
    let a = f
        .authority
        .namespace_apply(p.publication().unwrap())
        .unwrap();
    f.client.connection.execute_batch("CREATE TABLE pressure(data BLOB); CREATE TRIGGER pressure_publication BEFORE UPDATE ON chunk_publications BEGIN INSERT INTO pressure VALUES(zeroblob(65536)); END").unwrap();
    let pages: i64 = f
        .client
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    f.client
        .connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    loop {
        match f
            .client
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
    let e = f.journal().published(plan, a.clone()).unwrap_err();
    assert_eq!(
        e.downcast_ref::<rusqlite::Error>()
            .and_then(|e| e.sqlite_error_code()),
        Some(rusqlite::ErrorCode::DiskFull)
    );
    assert_eq!(f.journal().get(plan).unwrap(), p);
    f.client
        .connection
        .pragma_update(None, "max_page_count", 1000000)
        .unwrap();
    f.client
        .connection
        .execute_batch("DROP TRIGGER pressure_publication")
        .unwrap();
    assert_eq!(
        f.journal().published(plan, a.clone()).unwrap().receipt(),
        Some(&a)
    );
    f.source_pending();
}

fn next_plan(f: &mut Fixture, generation: u8, base: &Node) -> chunked::Input {
    let mut i = f.input.clone();
    i.id = [generation; 32];
    i.generation = [generation; 16];
    i.parent = journal::Journal::new(&mut f.client, journal::Options::default())
        .unwrap()
        .head(&i.local_target().unwrap())
        .unwrap()
        .map(|g| g.id);
    i.expected_content = namespace::payload_reference(&base.payload)
        .map(|s| std::array::from_fn(|at| u8::from_str_radix(&s[at * 2..at * 2 + 2], 16).unwrap()));
    chunked::Journal::new(&mut f.client, chunked::Options::default())
        .unwrap()
        .freeze(i.clone())
        .unwrap();
    let mut j = journal::Journal::new(&mut f.client, journal::Options::default()).unwrap();
    let g = j
        .begin_operation(
            &i.staging_operation().unwrap(),
            &i.local_target().unwrap(),
            Base::Missing,
            i.parent,
            i.size,
        )
        .unwrap();
    j.append_zero_at(g, 0, i.size).unwrap();
    j.seal(g).unwrap();
    let mut j = chunked::Journal::new(&mut f.client, chunked::Options::default()).unwrap();
    let mut p = j.attach(i.id, g).unwrap();
    while p.root().is_none() {
        p = j.step(i.id, p.cursor()).unwrap();
    }
    i
}
#[test]
fn operation_ids_are_owned_across_both_columns_and_manifest_bases_are_supported() {
    let mut f = Fixture::new(false);
    let plan = f.input.id;
    let p = f.prepare();
    let a = f
        .authority
        .namespace_apply(p.publication().unwrap())
        .unwrap();
    f.journal().published(plan, a.clone()).unwrap();
    let first = a.clone();
    let i = next_plan(&mut f, 5, &a.node);
    let seq = f.authority.namespace_volume().unwrap().sequence;
    let store = f.store.clone();
    // Reserve5 would collide with the first row's publish5 despite the two
    // individual UNIQUE constraints. The entire role increment must roll back.
    assert!(
        f.journal()
            .begin(i.id, a.node.clone(), store, seq, uuid(6))
            .is_err()
    );
    assert_eq!(
        f.client
            .connection
            .query_row("SELECT admitted FROM chunk_client_role", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        1
    );
    let i = next_plan(&mut f, 8, &a.node);
    let store = f.store.clone();
    // Publish4 would collide with the first row's reserve4.
    assert!(
        f.journal()
            .begin(i.id, a.node.clone(), store.clone(), seq, uuid(4))
            .is_err()
    );
    let second = f
        .journal()
        .begin(i.id, a.node.clone(), store, seq, uuid(9))
        .unwrap();
    assert!(
        second
            .input
            .base
            .payload
            .as_ref()
            .unwrap()
            .reservation
            .manifest
            .is_some()
    );
    let a = f.authority.namespace_apply(second.input.reserve()).unwrap();
    f.journal().reserved(i.id, a).unwrap();
    f.reopen();
    assert_eq!(f.journal().get(plan).unwrap().receipt(), Some(&first));
}
#[test]
fn exact_policy_nonportable_names_and_distinct_route_fingerprints_survive_receipts() {
    let mut f = Fixture::with_policy(false, s3drive_core::names::EXACT);
    let plan = f.input.id;
    assert_ne!(f.store.route, f.input.fingerprint);
    let node = f.base.id.clone();
    f.base = apply(
        &mut f.authority,
        10,
        Mutation::Move {
            node,
            parent: namespace::ROOT.into(),
            name: "CON:a. ".into(),
        },
    )
    .node;
    let p = f.begin();
    let a = f.authority.namespace_apply(p.input.reserve()).unwrap();
    f.journal().reserved(plan, a).unwrap();
    f.complete();
    let node = f.base.id.clone();
    apply(
        &mut f.authority,
        11,
        Mutation::Move {
            node,
            parent: namespace::ROOT.into(),
            name: "a:b".into(),
        },
    );
    let seq = f.authority.namespace_volume().unwrap().sequence;
    let p = f.journal().prepare_publish(plan, seq).unwrap();
    let a = f
        .authority
        .namespace_apply(p.publication().unwrap())
        .unwrap();
    assert_eq!(a.node.name, "a:b");
    f.journal().published(plan, a.clone()).unwrap();
    f.reopen();
    assert_eq!(f.journal().get(plan).unwrap().receipt(), Some(&a));
}
#[test]
fn schema_twenty_six_migration_preserves_existing_plan_and_collision_rolls_back() {
    for collision in [false, true] {
        let f = Fixture::new(false);
        let plan = f.input.id;
        let path = f.path.clone();
        let before = chunked::inspect_info(&f.client.connection, plan).unwrap();
        f.client.connection.execute_batch("DROP TABLE chunk_source_settlements; DROP TABLE chunk_source_role; DROP TABLE chunk_publications; DROP TABLE chunk_client_role; PRAGMA user_version=26").unwrap();
        if collision {
            f.client.connection.execute_batch("CREATE TABLE chunk_publications(keep TEXT); INSERT INTO chunk_publications VALUES('untouched')").unwrap();
        }
        drop(f.client);
        if collision {
            assert!(State::open_managed(&path).is_err());
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value::<i32, _>(None, "user_version", |r| r.get(0))
                    .unwrap(),
                26
            );
            assert_eq!(
                db.query_row::<String, _, _>("SELECT keep FROM chunk_publications", [], |r| r
                    .get(0))
                    .unwrap(),
                "untouched"
            );
            assert_eq!(
                db.query_row::<u32, _, _>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='chunk_client_role'",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
                0
            );
        } else {
            let s = State::open_managed(&path).unwrap();
            assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
            assert_eq!(chunked::inspect_info(&s.connection, plan).unwrap(), before);
        }
    }
}
#[test]
fn authority_restart_preflight_refuses_new_client_tables_without_migration() {
    for table in [
        "chunk_client_role",
        "chunk_publications",
        "chunk_source_role",
        "chunk_source_settlements",
    ] {
        let f = Fixture::new(false);
        let path = f._tmp.path().join("authority");
        let v = f.authority.namespace_volume().unwrap();
        // Deliberately replace only the new table with arbitrary nonempty state.
        // The read-only authority identity preflight must reject it first.
        f.authority.connection.execute_batch(&format!("DROP TABLE {table}; CREATE TABLE {table}(sentinel TEXT); INSERT INTO {table} VALUES('preserve'); PRAGMA user_version=26")).unwrap();
        drop(f.authority);
        let before = std::fs::read(path.join("metadata.sqlite3")).unwrap();
        assert!(State::open_existing_authority(&path, &v.id, &v.name_policy).is_err());
        assert_eq!(
            std::fs::read(path.join("metadata.sqlite3")).unwrap(),
            before
        );
    }
}
#[test]
fn prepared_historical_recovery_does_not_reread_damaged_current_payload() {
    let mut f = Fixture::new(false);
    let plan = f.input.id;
    let p = f.prepare();
    let a = f
        .authority
        .namespace_apply(p.publication().unwrap())
        .unwrap();
    f.client
        .connection
        .execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])
        .unwrap();
    assert!(
        journal::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .read(f.snapshot, 0, 64)
            .is_err()
    );
    f.reopen();
    assert_eq!(f.journal().get(plan).unwrap(), p);
    let saved = f.journal().published(plan, a.clone()).unwrap();
    assert_eq!(saved.receipt(), Some(&a));
    f.source_pending();
}
#[tokio::test]
async fn shared_owner_rechecks_preexisting_legacy_handles_after_admission() {
    let f = Fixture::new(false);
    let plan = f.input.id;
    let seq = f.authority.namespace_volume().unwrap().sequence;
    drop(f.client);
    let storage = crate::worker::Storage::open(
        &f.path,
        crate::cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap();
    let legacy = storage.namespace();
    let client = storage.chunk_publications();
    let original = client
        .begin(plan, f.base.clone(), f.store.clone(), seq, uuid(5))
        .await
        .unwrap();
    assert!(legacy.client_identity().await.is_err());
    assert_eq!(client.clone().get(plan).await.unwrap(), original);
    assert_eq!(client.records().await.unwrap(), vec![original.clone()]);
    storage.close().await.unwrap();
    drop(client);
    drop(legacy);
    drop(storage);
    let mut s = State::open_managed(&f.path).unwrap();
    assert_eq!(Journal::new(&mut s).unwrap().get(plan).unwrap(), original);
    assert!(crate::namespace_client::Journal::new(&mut s).is_err());
}
