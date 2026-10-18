use super::removals::{apply, arm, close, owned};
use super::*;
use crate::{managed, remote};
const BYTES: &[u8] = b"dirty bytes remain intact\0\xff";
fn input(
    f: &mut Fixture,
    n: &Node,
    edit: &logical::Record,
    fence: Option<namespace::RemovalFence>,
) -> managed::Input {
    let mutation = match fence {
        Some(fence) => Mutation::ReservePendingContent {
            node: n.id.clone(),
            expected_revision: n.revision,
            store: f.store.id.clone(),
            size: edit.input.size,
            sha256: edit.input.sha256.clone(),
            expected_payload: namespace::payload_reference(&n.payload),
            fence,
        },
        None => Mutation::ReserveContent {
            node: n.id.clone(),
            expected_revision: n.revision,
            store: f.store.id.clone(),
            size: edit.input.size,
            sha256: edit.input.sha256.clone(),
            expected_payload: namespace::payload_reference(&n.payload),
        },
    };
    let request = f.request(mutation);
    managed::Input {
        publish_operation: id(f.op + 100_000),
        reserve: request,
        store: f.store.clone(),
        source: [1; 32],
        fingerprint: "b".repeat(64),
    }
}
fn prepare(
    f: &mut Fixture,
    edit: &logical::Record,
    input: &managed::Input,
) -> journal::GenerationId {
    logical::Journal::new(&mut f.client, journal::Options::default())
        .unwrap()
        .prepare(&edit.input.id, input.clone())
        .unwrap();
    managed::Journal::new(&mut f.client, journal::Options::default())
        .unwrap()
        .begin(input.clone())
        .unwrap();
    let mut j = journal::Journal::new(&mut f.client, journal::Options::default()).unwrap();
    let g = j
        .begin_operation(
            &input.stage_key().unwrap(),
            &input.target().unwrap(),
            journal::Base::Missing,
            None,
            edit.input.size,
        )
        .unwrap();
    j.append(g, BYTES).unwrap();
    j.seal(g).unwrap();
    managed::Journal::new(&mut f.client, journal::Options::default())
        .unwrap()
        .link(input.id(), g)
        .unwrap();
    g
}
fn rejected(f: &mut Fixture, g: journal::GenerationId) -> managed::Rejection {
    let mut j = remote::Journal::new(&mut f.client, remote::Options::default()).unwrap();
    let info = j.prepare(g).unwrap();
    let _permit = j.claim(info.operation, &info.target).unwrap();
    j.rejected(info.operation, 412).unwrap();
    managed::Rejection::Physical {
        operation: *info.operation.as_bytes(),
        status: 412,
    }
}
#[test]
fn pending_owner_admission_requires_acknowledged_exact_fence_for_both_journals() {
    let mut f = Fixture::new();
    let n = f.create("pending 資料 🦀");
    let edit = f.stage(&n, 500, BYTES);
    let arm = arm(&mut f, &n);
    Journal::new(&mut f.client)
        .unwrap()
        .begin(arm.clone())
        .unwrap();
    let r = f.authority.namespace_apply(&arm.request).unwrap();
    let Some(Artifact::Removal(fence)) = r.artifact.clone() else {
        panic!()
    };
    let good = input(&mut f, &r.node, &edit, Some(fence));
    assert!(
        managed::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .begin(good.clone())
            .is_err()
    );
    f.ack(&arm, r.clone()).unwrap();
    assert!(
        managed::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .begin(good.clone())
            .is_err(),
        "raw physical staging cannot bypass the frozen logical edit barrier"
    );
    let ordinary = input(&mut f, &r.node, &edit, None);
    let mut wrong_owner = good.clone();
    if let Mutation::ReservePendingContent { fence, .. } = &mut wrong_owner.reserve.mutation {
        fence.owner = id(987);
    }
    let mut wrong_intent = good.clone();
    if let Mutation::ReservePendingContent { fence, .. } = &mut wrong_intent.reserve.mutation {
        fence.intent = id(986);
    }
    let mut wrong_source = good.clone();
    wrong_source.source = [9; 32];
    let mut wrong_fingerprint = good.clone();
    wrong_fingerprint.fingerprint = "c".repeat(64);
    let mut wrong_store = good.clone();
    wrong_store.store.route = "d".repeat(64);
    for bad in [
        ordinary,
        wrong_owner,
        wrong_intent,
        wrong_source,
        wrong_fingerprint,
        wrong_store,
    ] {
        assert!(
            logical::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .prepare(&edit.input.id, bad.clone())
                .is_err()
        );
        assert!(
            managed::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .begin(bad)
                .is_err()
        );
    }
    let mut foreign = State::open_managed(&f.tmp.path().join("foreign")).unwrap();
    assert!(
        managed::Journal::new(&mut foreign, journal::Options::default())
            .unwrap()
            .begin(good.clone())
            .is_err()
    );
    assert_eq!(
        f.authority.namespace_apply(&good.reserve).unwrap().artifact,
        Some(Artifact::Reservation(good.reservation().unwrap()))
    );
    prepare(&mut f, &edit, &good);
    assert_eq!(
        logical::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .get(&edit.input.id)
            .unwrap()
            .publication,
        Some(good)
    );
}
#[test]
fn only_explicit_retained_terminal_conflicts_release_closing_and_never_settle_bytes() {
    for physical in [false, true] {
        let mut f = Fixture::new();
        let n = f.create("retained conflict");
        let edit = f.stage(&n, 500, BYTES);
        let arm = arm(&mut f, &n);
        let r = apply(&mut f, &arm);
        let fence = owned(&mut f, &n).unwrap().fence;
        let mut publication = input(&mut f, &r.node, &edit, Some(fence.clone()));
        if !physical {
            publication.reserve.expected_sequence += 1000;
        }
        let g = prepare(&mut f, &edit, &publication);
        let rejection = if physical {
            rejected(&mut f, g)
        } else {
            assert_eq!(
                f.authority.namespace_apply(&publication.reserve),
                Err(namespace::Error::Conflict)
            );
            managed::Rejection::Authority {
                request: Box::new(publication.reserve.clone()),
                error: namespace::Error::Conflict,
            }
        };
        let closing = close(&mut f, &r.node, &fence, false);
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .begin(closing.clone())
                .is_err()
        );
        assert!(
            logical::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .retain_conflict(&edit.input.id)
                .is_err()
        );
        managed::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .reject(publication.id(), rejection.clone())
            .unwrap();
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .begin(closing.clone())
                .is_err(),
            "retention must be explicit"
        );
        let conflict = logical::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .retain_conflict(&edit.input.id)
            .unwrap();
        assert_eq!(conflict.rejection, rejection);
        f.reopen_client();
        assert_eq!(
            logical::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .retained_conflict(&edit.input.id)
                .unwrap(),
            Some(conflict.clone())
        );
        assert_eq!(
            logical::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .retain_conflict(&edit.input.id)
                .unwrap(),
            conflict
        );
        Journal::new(&mut f.client)
            .unwrap()
            .begin(closing.clone())
            .unwrap();
        let cancel = f.authority.namespace_apply(&closing.request).unwrap();
        f.ack(&closing, cancel).unwrap();
        assert_eq!(f.file(&n).head, edit.generation);
        assert!(f.file(&n).settled.is_none());
        assert_eq!(
            journal::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .read(edit.generation.unwrap(), 0, 100)
                .unwrap(),
            BYTES
        );
        assert!(
            managed::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .prepare(publication.id(), 0)
                .is_err()
        );
        if physical {
            let mut remote =
                remote::Journal::new(&mut f.client, remote::Options::default()).unwrap();
            let info = remote.for_generation(g).unwrap().unwrap();
            assert!(
                remote
                    .claim(info.operation, &publication.target().unwrap())
                    .is_err()
            );
        }
    }
}
#[test]
fn unknown_or_changed_evidence_never_becomes_a_retained_conflict() {
    let mut f = Fixture::new();
    let n = f.create("unknown evidence");
    let edit = f.stage(&n, 500, BYTES);
    let arm = arm(&mut f, &n);
    let r = apply(&mut f, &arm);
    let fence = owned(&mut f, &n).unwrap().fence;
    let publication = input(&mut f, &r.node, &edit, Some(fence));
    let g = prepare(&mut f, &edit, &publication);
    let mut remote = remote::Journal::new(&mut f.client, remote::Options::default()).unwrap();
    let info = remote.prepare(g).unwrap();
    let _permit = remote.claim(info.operation, &info.target).unwrap();
    remote
        .uncertain(info.operation, remote::Reason::Timeout)
        .unwrap();
    let fake = managed::Rejection::Physical {
        operation: *info.operation.as_bytes(),
        status: 412,
    };
    assert!(
        managed::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .reject(publication.id(), fake)
            .is_err()
    );
    for error in [
        namespace::Error::DeletePending,
        namespace::Error::Unavailable,
        namespace::Error::Internal,
        namespace::Error::OperationMismatch,
        namespace::Error::Limit,
    ] {
        assert!(
            managed::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .reject(
                    publication.id(),
                    managed::Rejection::Authority {
                        request: Box::new(publication.reserve.clone()),
                        error
                    }
                )
                .is_err()
        );
    }
    let mut changed = publication.reserve.clone();
    changed.expected_sequence += 1;
    assert!(
        managed::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .reject(
                publication.id(),
                managed::Rejection::Authority {
                    request: Box::new(changed),
                    error: namespace::Error::Conflict
                }
            )
            .is_err()
    );
    assert!(
        logical::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .retain_conflict(&edit.input.id)
            .is_err()
    );
    assert_eq!(f.file(&n).head, edit.generation);
    assert!(f.file(&n).settled.is_none());
}
#[test]
fn ignored_retention_writes_and_corrupted_evidence_fail_closed_without_closing_admission() {
    for table in ["managed_rejections", "logical_conflicts"] {
        for after in [false, true] {
            let mut f = Fixture::new();
            let n = f.create("atomic retained conflict");
            let edit = f.stage(&n, 500, BYTES);
            let arm = arm(&mut f, &n);
            let r = apply(&mut f, &arm);
            let fence = owned(&mut f, &n).unwrap().fence;
            let publication = input(&mut f, &r.node, &edit, Some(fence.clone()));
            let g = prepare(&mut f, &edit, &publication);
            let rejection = rejected(&mut f, g);
            if table == "logical_conflicts" {
                managed::Journal::new(&mut f.client, journal::Options::default())
                    .unwrap()
                    .reject(publication.id(), rejection.clone())
                    .unwrap();
            }
            let trigger = if after {
                format!(
                    "CREATE TRIGGER fail AFTER INSERT ON {table} BEGIN DELETE FROM {table}; END"
                )
            } else {
                format!(
                    "CREATE TRIGGER fail BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE); END"
                )
            };
            f.client.connection.execute_batch(&trigger).unwrap();
            if table == "managed_rejections" {
                assert!(
                    managed::Journal::new(&mut f.client, journal::Options::default())
                        .unwrap()
                        .reject(publication.id(), rejection.clone())
                        .is_err()
                );
            } else {
                assert!(
                    logical::Journal::new(&mut f.client, journal::Options::default())
                        .unwrap()
                        .retain_conflict(&edit.input.id)
                        .is_err()
                );
            }
            let closing = close(&mut f, &r.node, &fence, false);
            assert!(
                Journal::new(&mut f.client)
                    .unwrap()
                    .begin(closing.clone())
                    .is_err()
            );
            f.client
                .connection
                .execute_batch("DROP TRIGGER fail")
                .unwrap();
            managed::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .reject(publication.id(), rejection)
                .unwrap();
            logical::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .retain_conflict(&edit.input.id)
                .unwrap();
            f.client
                .connection
                .execute(&format!("UPDATE {table} SET state_hash=zeroblob(32)"), [])
                .unwrap();
            assert!(
                logical::Journal::new(&mut f.client, journal::Options::default())
                    .unwrap()
                    .retained_conflict(&edit.input.id)
                    .is_err()
            );
            assert!(Journal::new(&mut f.client).unwrap().begin(closing).is_err());
            assert_eq!(
                journal::Journal::new(&mut f.client, journal::Options::default())
                    .unwrap()
                    .read(edit.generation.unwrap(), 0, 100)
                    .unwrap(),
                BYTES
            );
        }
    }
}
#[test]
fn actual_disk_full_rolls_back_each_retention_boundary_and_exact_retry_recovers() {
    for table in ["managed_rejections", "logical_conflicts"] {
        let mut f = Fixture::new();
        let n = f.create("full disk conflict");
        let edit = f.stage(&n, 500, BYTES);
        let arm = arm(&mut f, &n);
        let r = apply(&mut f, &arm);
        let fence = owned(&mut f, &n).unwrap().fence;
        let publication = input(&mut f, &r.node, &edit, Some(fence));
        let g = prepare(&mut f, &edit, &publication);
        let rejection = rejected(&mut f, g);
        if table == "logical_conflicts" {
            managed::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .reject(publication.id(), rejection.clone())
                .unwrap();
        }
        f.client.connection.execute_batch(&format!("CREATE TABLE full_disk_filler(data BLOB); CREATE TRIGGER fail BEFORE INSERT ON {table} BEGIN INSERT INTO full_disk_filler VALUES(zeroblob(1048576)); END; PRAGMA wal_checkpoint(TRUNCATE)")).unwrap();
        let pages: i64 = f
            .client
            .connection
            .pragma_query_value(None, "page_count", |r| r.get(0))
            .unwrap();
        f.client
            .connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        let error = if table == "managed_rejections" {
            managed::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .reject(publication.id(), rejection.clone())
                .unwrap_err()
        } else {
            logical::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .retain_conflict(&edit.input.id)
                .unwrap_err()
        };
        assert!(
            error.to_string().contains("full"),
            "expected SQLite full, got {error}"
        );
        assert!(
            logical::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .retained_conflict(&edit.input.id)
                .unwrap()
                .is_none()
        );
        f.client
            .connection
            .pragma_update(None, "max_page_count", pages + 1024)
            .unwrap();
        f.client
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        f.reopen_client();
        managed::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .reject(publication.id(), rejection)
            .unwrap();
        logical::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .retain_conflict(&edit.input.id)
            .unwrap();
        assert_eq!(f.file(&n).head, edit.generation);
        assert!(f.file(&n).settled.is_none());
    }
}
#[test]
fn schema_fourteen_migration_preserves_old_managed_hashes_and_refuses_conflicting_shapes() {
    for conflict in [false, true] {
        let mut f = Fixture::new();
        let n = f.create("schema fourteen bytes");
        let edit = f.stage(&n, 500, BYTES);
        let publication = input(&mut f, &n, &edit, None);
        prepare(&mut f, &edit, &publication);
        let original: Vec<u8> = f
            .client
            .connection
            .query_row("SELECT state_hash FROM managed_publications", [], |r| {
                r.get(0)
            })
            .unwrap();
        f.client.connection.execute_batch("DROP TABLE chunk_publications; DROP TABLE chunk_client_role; DROP TABLE chunk_objects; DROP TABLE chunk_plans; DROP TABLE multipart_parts; DROP TABLE multipart_plans; DROP TABLE recovered_windows_opens; DROP TABLE windows_session_recoveries; DROP TABLE windows_mount_sessions; DROP TABLE retired_windows_opens; DROP TABLE windows_open_events; DROP TABLE retained_windows_opens; DROP TABLE windows_open_journal; DROP TABLE namespace_removal_events; ALTER TABLE namespace_removals DROP COLUMN mode; ALTER TABLE namespace_removals DROP COLUMN event_id; DROP TABLE logical_conflicts; DROP TABLE managed_rejections; PRAGMA user_version=14").unwrap();
        if conflict {
            f.client.connection.execute_batch("CREATE TABLE managed_rejections(keep TEXT); INSERT INTO managed_rejections VALUES('preserved')").unwrap();
        }
        let path = f.tmp.path().join("client");
        let placeholder = State::open_managed(&f.tmp.path().join("placeholder")).unwrap();
        drop(std::mem::replace(&mut f.client, placeholder));
        if conflict {
            assert!(State::open_managed(&path).is_err());
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                    .unwrap(),
                14
            );
            assert_eq!(
                db.query_row::<String, _, _>("SELECT keep FROM managed_rejections", [], |r| r
                    .get(0))
                    .unwrap(),
                "preserved"
            );
        } else {
            f.client = State::open_managed(&path).unwrap();
            let actual: Vec<u8> = f
                .client
                .connection
                .query_row("SELECT state_hash FROM managed_publications", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(original, actual);
            assert_eq!(
                managed::Journal::new(&mut f.client, journal::Options::default())
                    .unwrap()
                    .get(publication.id())
                    .unwrap()
                    .input,
                publication
            );
            assert_eq!(f.file(&n).head, edit.generation);
        }
    }
}
