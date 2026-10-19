//! Isolated client root-publication journal. Separate exact source settlement;
//! detached/removal publication, legacy migration or native activation.
use crate::{
    Result, State, chunked,
    journal::GenerationId,
    namespace::{
        self, Apply, Artifact, ContentGuard, Kind, ManifestRoot, Mutation, Node, Payload, Receipt,
        Reservation, Store,
    },
    worker::Owner,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
const MAX_RECORDS: u32 = 256;
const MAX_BYTES: usize = 65536;
pub mod sources;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Invalid,
    Conflict,
    Corrupt,
    Isolated,
    Quota,
    Missing,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid chunk root publication",
            Self::Conflict => "chunk root publication conflicts; data preserved",
            Self::Corrupt => "chunk root publication integrity failure; data preserved",
            Self::Isolated => "chunk publication requires dedicated client state; data preserved",
            Self::Quota => "chunk root publication quota exceeded; data preserved",
            Self::Missing => "unknown chunk root publication",
        })
    }
}
impl std::error::Error for Failure {}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn hash<T: Serialize>(domain: &[u8], value: &T) -> Result<[u8; 32]> {
    let mut h = Sha256::new();
    h.update(domain);
    h.update(serde_json::to_vec(value)?);
    Ok(h.finalize().into())
}
fn id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
// Match the authority's common exact-name shape; portable admission belongs
// to its registered policy, not this historical receipt journal.
fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value != "."
        && value != ".."
        && !value
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control())
}
// Includes retained/orphaned records, not just active operations. Shared
// namespace removal tables are authority state and are excluded as well.
const EXCLUDED: &[&str] = &[
    "manifest_creation_role",
    "manifest_creations",
    "sparse_scoped_mode",
    "source_read_pin_session",
    "source_read_pins",
    "sparse_publication_role",
    "sparse_publications",
    "sparse_publication_checks",
    "sparse_preparations",
    "sparse_preparation_objects",
    "sparse_preparation_blobs",
    "sparse_view_role",
    "sparse_view_files",
    "sparse_views",
    "sparse_view_operations",
    "sparse_view_blocks",
    "managed_publications",
    "managed_rejections",
    "logical_files",
    "logical_edits",
    "logical_publication_links",
    "logical_conflicts",
    "namespace_intents",
    "client_identity",
    "retained_client_identity",
    "windows_open_journal",
    "retained_windows_opens",
    "windows_open_events",
    "retired_windows_opens",
    "windows_mount_sessions",
    "windows_session_recoveries",
    "recovered_windows_opens",
    "namespace_volume",
    "namespace_nodes",
    "namespace_operations",
    "namespace_stores",
    "namespace_uploads",
    "namespace_removals",
    "namespace_removal_events",
];
fn isolation(db: &Connection) -> Result<()> {
    for table in EXCLUDED {
        let used: bool =
            db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| {
                r.get(0)
            })?;
        if used {
            return Err(Failure::Isolated.into());
        }
    }
    Ok(())
}
/// Reciprocal constructor guard; never put this in generic staging validation.
pub(crate) fn require_legacy(db: &Connection) -> Result<()> {
    crate::chunk_mount::require_legacy(db)?;
    crate::manifest_creation::require_legacy(db)?;
    crate::sparse_views::require_legacy(db)?;
    let used: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM chunk_client_role) OR EXISTS(SELECT 1 FROM chunk_publications) OR EXISTS(SELECT 1 FROM chunk_source_role) OR EXISTS(SELECT 1 FROM chunk_source_settlements)", [], |r| r.get(0))?;
    if used {
        return Err(Failure::Isolated.into());
    }
    Ok(())
}
pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    sources::check_schema(db)?;
    db.prepare("SELECT singleton,admitted,state_hash FROM chunk_client_role LIMIT 0")?;
    db.prepare(
        "SELECT plan_id,reserve_id,publish_id,record,state_hash FROM chunk_publications LIMIT 0",
    )?;
    Ok(())
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    plan: chunked::Input,
    snapshot: GenerationId,
    snapshot_manifest: [u8; 32],
    base: Node,
    store: Store,
    reserve: Apply,
    publish_operation: String,
}
impl std::fmt::Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChunkPublicationInput([private])")
    }
}
impl Input {
    pub fn plan(&self) -> &chunked::Input {
        &self.plan
    }
    pub fn snapshot(&self) -> GenerationId {
        self.snapshot
    }
    pub fn snapshot_manifest(&self) -> &[u8; 32] {
        &self.snapshot_manifest
    }
    pub fn base(&self) -> &Node {
        &self.base
    }
    pub fn store(&self) -> &Store {
        &self.store
    }
    pub fn reserve(&self) -> &Apply {
        &self.reserve
    }
    pub fn publish_operation(&self) -> &str {
        &self.publish_operation
    }
    pub fn reservation(&self) -> Result<Reservation> {
        self.reserve.validate().map_err(|_| Failure::Invalid)?;
        let Mutation::ReserveManifest {
            node,
            expected_revision,
            store,
            size,
            sha256,
            manifest,
            guard,
        } = &self.reserve.mutation
        else {
            return Err(Failure::Invalid.into());
        };
        let root = manifest.root().map_err(|_| Failure::Invalid)?;
        let key = s3drive_core::manifest::objects::Namespace::new(
            self.store.root.clone(),
            self.plan.volume,
            self.plan.node,
            self.plan.store,
        )?
        .root_key(&root)?;
        let r = Reservation {
            id: self.reserve.operation.clone(),
            volume: self.reserve.volume.clone(),
            node: node.clone(),
            expected_revision: *expected_revision,
            store: self.store.clone(),
            key: key.as_str().into(),
            size: *size,
            sha256: sha256.clone(),
            manifest: Some((**manifest).clone()),
            guard: Some(guard.clone()),
        };
        let b = &self.base;
        if !r.valid()
            || self.reserve.operation != hex(&self.plan.generation)
            || self.reserve.volume != hex(&self.plan.volume)
            || node != &hex(&self.plan.node)
            || store != &self.store.id
            || store != &hex(&self.plan.store)
            || self.store.root != self.plan.root
            || !id(&self.publish_operation)
            || self.publish_operation == namespace::ROOT
            || self.publish_operation == self.reserve.operation
            || guard.detached
            || guard.removal.is_some()
            || guard.expected != self.plan.expected_content.as_ref().map(|h| hex(h))
            || b.id != *node
            || b.deleted
            || b.kind == Kind::Directory
            || b.revision != *expected_revision
            || b.revision > self.reserve.expected_sequence
            || b.parent.as_ref().is_none_or(|p| !id(p))
            || !name(&b.name)
            || namespace::payload_reference(&b.payload) != guard.expected
            || b.placement.as_ref().is_some_and(|s| s != &self.store)
            || (b.kind == Kind::File) != b.payload.is_some()
            || b.payload.as_ref().is_some_and(|p| {
                !p.valid()
                    || p.reservation.volume != r.volume
                    || p.reservation.node != r.node
                    || p.reservation.store != r.store
            })
        {
            return Err(Failure::Invalid.into());
        }
        Ok(r)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RootProof {
    generation: GenerationId,
    operation: [u8; 32],
    checksum: [u8; 32],
    etag: String,
    version: Option<String>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rejection {
    request: Apply,
    error: namespace::Error,
}
impl Rejection {
    pub fn request(&self) -> &Apply {
        &self.request
    }
    pub fn error(&self) -> namespace::Error {
        self.error
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    format: u32,
    input: Input,
    reservation: Option<Receipt>,
    root: Option<RootProof>,
    publication: Option<Apply>,
    receipt: Option<Receipt>,
    rejection: Option<Rejection>,
}
impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkRootPublication")
            .field("reserved", &self.reservation.is_some())
            .field("prepared", &self.publication.is_some())
            .field("committed", &self.receipt.is_some())
            .field("rejected", &self.rejection.is_some())
            .finish_non_exhaustive()
    }
}
impl Record {
    pub fn input(&self) -> &Input {
        &self.input
    }
    pub fn reservation(&self) -> Option<&Receipt> {
        self.reservation.as_ref()
    }
    pub fn publication(&self) -> Option<&Apply> {
        self.publication.as_ref()
    }
    /// Exact historical root commit only, never a source/file flush receipt.
    pub fn receipt(&self) -> Option<&Receipt> {
        self.receipt.as_ref()
    }
    pub fn rejection(&self) -> Option<&Rejection> {
        self.rejection.as_ref()
    }
    pub fn root_generation(&self) -> Option<GenerationId> {
        self.root.as_ref().map(|r| r.generation)
    }
}
fn receipt_node(n: &Node, r: &Reservation, sequence: u64) -> bool {
    n.id == r.node
        && !n.deleted
        && n.kind != Kind::Directory
        && n.revision <= sequence
        && sequence < i64::MAX as u64
        && n.parent.as_ref().is_some_and(|p| id(p))
        && name(&n.name)
        && n.placement.as_ref().is_none_or(|s| s == &r.store)
        && (n.kind == Kind::File) == n.payload.is_some()
        && n.payload.as_ref().is_none_or(|p| {
            p.valid()
                && p.reservation.volume == r.volume
                && p.reservation.node == r.node
                && p.reservation.store == r.store
        })
}
fn validate_record(db: &Connection, r: &Record) -> Result<()> {
    if r.format != 1 {
        return Err(Failure::Corrupt.into());
    }
    let expected = r.input.reservation()?;
    let p = chunked::inspect_info(db, r.input.plan.id)?;
    if p.input() != &r.input.plan
        || p.generation() != Some(r.input.snapshot)
        || p.snapshot_manifest() != Some(r.input.snapshot_manifest)
        || p.root()
            != Some(
                &expected
                    .manifest
                    .as_ref()
                    .ok_or(Failure::Corrupt)?
                    .root()
                    .map_err(|_| Failure::Corrupt)?,
            )
    {
        return Err(Failure::Corrupt.into());
    }
    let transfer = chunked::inspect_transfer(db, &p, p.root_level(), 0)?.ok_or(Failure::Corrupt)?;
    if transfer.target().object_key() != expected.key
        || transfer.size() != expected.size
        || hex(transfer.payload_sha256()) != expected.sha256
    {
        return Err(Failure::Corrupt.into());
    }
    if let Some(a) = &r.reservation
        && (a.sequence <= r.input.reserve.expected_sequence
            || !receipt_node(&a.node, &expected, a.sequence)
            || a.artifact != Some(Artifact::Reservation(expected.clone()))
            || namespace::payload_reference(&a.node.payload)
                != expected.guard.as_ref().ok_or(Failure::Corrupt)?.expected)
    {
        return Err(Failure::Corrupt.into());
    }
    if r.publication.is_some() != r.root.is_some()
        || (r.root.is_some() && r.reservation.is_none())
        || (r.receipt.is_some() && r.publication.is_none())
    {
        return Err(Failure::Corrupt.into());
    }
    if let Some(root) = &r.root {
        // Fresh complete historical proof inside the final immutable validation
        // section. Never acknowledge the original logical snapshot with this.
        let tree = chunked::inspect_tree_receipt(db, r.input.plan.id)?;
        if proof(&tree)? != *root {
            return Err(Failure::Corrupt.into());
        }
        let request = r.publication.as_ref().ok_or(Failure::Corrupt)?;
        request.validate().map_err(|_| Failure::Corrupt)?;
        if request.volume != expected.volume
            || request.operation != r.input.publish_operation
            || request.expected_sequence < r.reservation.as_ref().ok_or(Failure::Corrupt)?.sequence
            || request.mutation != publish_mutation(&expected, root)
        {
            return Err(Failure::Corrupt.into());
        }
        if let Some(a) = &r.receipt {
            let payload = Payload {
                reservation: expected.clone(),
                upload_operation: hex(&root.operation),
                etag: root.etag.clone(),
                version: root.version.clone(),
            };
            if !payload.valid()
                || !receipt_node(&a.node, &expected, a.sequence)
                || a.sequence <= request.expected_sequence
                || a.node.revision != a.sequence
                || a.node.kind != Kind::File
                || a.node.payload.as_ref() != Some(&payload)
                || a.artifact != Some(Artifact::Published(payload))
            {
                return Err(Failure::Corrupt.into());
            }
        }
    }
    if let Some(rejected) = &r.rejection
        && (rejected.error != namespace::Error::Conflict
            || r.receipt.is_some()
            || match &r.publication {
                Some(p) => rejected.request != *p,
                None => rejected.request != r.input.reserve || r.reservation.is_some(),
            })
    {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
fn proof(tree: &chunked::TreeReceipt) -> Result<RootProof> {
    let a = tree.root_receipt();
    let i = a.info();
    let rev = i.revision.as_ref().ok_or(Failure::Corrupt)?;
    Ok(RootProof {
        generation: i.generation,
        operation: *i.operation.as_bytes(),
        checksum: *a.checksum(),
        etag: rev.etag().into(),
        version: rev.version().map(Into::into),
    })
}
fn publish_mutation(reservation: &Reservation, proof: &RootProof) -> Mutation {
    Mutation::Publish {
        reservation: reservation.id.clone(),
        upload_operation: hex(&proof.operation),
        etag: proof.etag.clone(),
        version: proof.version.clone(),
    }
}
fn raw_load(db: &Connection, plan: [u8; 32]) -> Result<Option<Record>> {
    let row = db.query_row("SELECT reserve_id,publish_id,length(record),substr(record,1,65536),length(state_hash),substr(state_hash,1,32) FROM chunk_publications WHERE plan_id=?1", [plan.as_slice()], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,Vec<u8>>(3)?,r.get::<_,i64>(4)?,r.get::<_,Vec<u8>>(5)?))).optional()?;
    let Some((reserve, publish, len, bytes, hlen, checksum)) = row else {
        return Ok(None);
    };
    if len == 0 || len > MAX_BYTES as i64 || hlen != 32 {
        return Err(Failure::Corrupt.into());
    }
    let r: Record = serde_json::from_slice(&bytes).map_err(|_| Failure::Corrupt)?;
    if r.input.plan.id != plan
        || r.input.reserve.operation != reserve
        || r.input.publish_operation != publish
        || hash(b"s3drive-chunk-publication-v1\0", &r)?.as_slice() != checksum
    {
        return Err(Failure::Corrupt.into());
    }
    Ok(Some(r))
}
pub(crate) fn validate_all(db: &Connection) -> Result<()> {
    validate_publications(db)?;
    sources::validate_all(db)
}
fn validate_publications(db: &Connection) -> Result<()> {
    let row = db.query_row("SELECT admitted,length(state_hash),substr(state_hash,1,32) FROM chunk_client_role WHERE singleton=1", [], |r| Ok((r.get::<_,u32>(0)?,r.get::<_,i64>(1)?,r.get::<_,Vec<u8>>(2)?))).optional()?;
    let mut q = db
        .prepare("SELECT length(plan_id),substr(plan_id,1,32) FROM chunk_publications LIMIT 257")?;
    let ids = q
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let role_rows: u32 =
        db.query_row("SELECT count(*) FROM chunk_client_role", [], |r| r.get(0))?;
    if row.is_none() && role_rows == 0 && ids.is_empty() {
        return Ok(());
    }
    let (admitted, len, checksum) = row.ok_or(Failure::Corrupt)?;
    if role_rows != 1
        || admitted == 0
        || admitted > MAX_RECORDS
        || admitted as usize != ids.len()
        || len != 32
        || hash(b"s3drive-chunk-client-role-v1\0", &admitted)?.as_slice() != checksum
    {
        return Err(Failure::Corrupt.into());
    }
    isolation(db)?;
    let mut operations = BTreeSet::new();
    for (len, id) in ids {
        if len != 32 {
            return Err(Failure::Corrupt.into());
        }
        let r =
            raw_load(db, id.try_into().map_err(|_| Failure::Corrupt)?)?.ok_or(Failure::Corrupt)?;
        if !operations.insert(r.input.reserve.operation.clone())
            || !operations.insert(r.input.publish_operation.clone())
        {
            return Err(Failure::Corrupt.into());
        }
        validate_record(db, &r)?;
    }
    Ok(())
}

pub struct Journal<'a> {
    state: &'a mut State,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State) -> Result<Self> {
        state.verify_staging_storage()?;
        isolation(&state.connection)?;
        validate_all(&state.connection)?;
        Ok(Self { state })
    }
    pub fn get(&self, plan: [u8; 32]) -> Result<Record> {
        validate_all(&self.state.connection)?;
        raw_load(&self.state.connection, plan)?.ok_or_else(|| Failure::Missing.into())
    }
    /// Bounded retained history for restart discovery, including unknown and
    /// terminal outcomes. Callers must not infer settlement from this list.
    pub fn records(&self) -> Result<Vec<Record>> {
        let db = &self.state.connection;
        validate_all(db)?;
        let mut q =
            db.prepare("SELECT plan_id FROM chunk_publications ORDER BY plan_id LIMIT 256")?;
        let ids = q
            .query_map([], |r| r.get::<_, Vec<u8>>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.into_iter()
            .map(|id| {
                raw_load(db, id.try_into().map_err(|_| Failure::Corrupt)?)?
                    .ok_or_else(|| Failure::Corrupt.into())
            })
            .collect()
    }
    /// Caller supplies authenticated metadata; this offline journal does not
    /// authorize it. The controller must bind the store to its policy-bound SDK.
    pub fn begin(
        &mut self,
        plan: [u8; 32],
        base: Node,
        store: Store,
        reserve_sequence: u64,
        publish_operation: String,
    ) -> Result<Record> {
        let p = chunked::inspect_info(&self.state.connection, plan)?;
        let root = p.root().cloned().ok_or(Failure::Invalid)?;
        let t = chunked::inspect_transfer(&self.state.connection, &p, p.root_level(), 0)?
            .ok_or(Failure::Corrupt)?;
        let input = Input {
            plan: p.input().clone(),
            snapshot: p.generation().ok_or(Failure::Invalid)?,
            snapshot_manifest: p.snapshot_manifest().ok_or(Failure::Invalid)?,
            base: base.clone(),
            store: store.clone(),
            reserve: Apply {
                volume: hex(&p.input().volume),
                operation: hex(&p.input().generation),
                expected_sequence: reserve_sequence,
                mutation: Mutation::ReserveManifest {
                    node: base.id,
                    expected_revision: base.revision,
                    store: store.id,
                    size: t.size(),
                    sha256: hex(t.payload_sha256()),
                    manifest: Box::new(ManifestRoot::new(root).map_err(|_| Failure::Invalid)?),
                    guard: ContentGuard {
                        expected: p.input().expected_content.as_ref().map(|h| hex(h)),
                        detached: false,
                        removal: None,
                    },
                },
            },
            publish_operation,
        };
        input.reservation()?;
        if let Some(r) = raw_load(&self.state.connection, plan)? {
            self.get(plan)?;
            if r.input != input {
                return Err(Failure::Conflict.into());
            }
            return Ok(r);
        }
        let r = Record {
            format: 1,
            input,
            reservation: None,
            root: None,
            publication: None,
            receipt: None,
            rejection: None,
        };
        self.save(&r, true)?;
        Ok(r)
    }
    pub fn reserved(&mut self, plan: [u8; 32], receipt: Receipt) -> Result<Record> {
        let mut r = self.get(plan)?;
        if r.reservation.as_ref() == Some(&receipt) {
            return Ok(r);
        }
        if r.reservation.is_some() || r.rejection.is_some() {
            return Err(Failure::Conflict.into());
        }
        r.reservation = Some(receipt);
        self.save(&r, false)?;
        Ok(r)
    }
    /// Freeze once. An existing prepared request cannot be rebased to a newer
    /// sequence, including after an unknown response or after restart.
    pub fn prepare_publish(&mut self, plan: [u8; 32], sequence: u64) -> Result<Record> {
        let mut r = self.get(plan)?;
        if let Some(p) = &r.publication {
            if p.expected_sequence == sequence {
                return Ok(r);
            }
            return Err(Failure::Conflict.into());
        }
        if r.reservation.is_none() || r.rejection.is_some() {
            return Err(Failure::Conflict.into());
        }
        let root = proof(&chunked::inspect_tree_receipt(
            &self.state.connection,
            plan,
        )?)?;
        r.publication = Some(Apply {
            volume: r.input.reserve.volume.clone(),
            operation: r.input.publish_operation.clone(),
            expected_sequence: sequence,
            mutation: publish_mutation(&r.input.reservation()?, &root),
        });
        r.root = Some(root);
        self.save(&r, false)?;
        Ok(r)
    }
    pub fn published(&mut self, plan: [u8; 32], receipt: Receipt) -> Result<Record> {
        let mut r = self.get(plan)?;
        if r.receipt.as_ref() == Some(&receipt) {
            return Ok(r);
        }
        if r.receipt.is_some() || r.rejection.is_some() {
            return Err(Failure::Conflict.into());
        }
        r.receipt = Some(receipt);
        self.save(&r, false)?;
        Ok(r)
    }
    /// Only an exact authority Conflict is terminal here. Transport uncertainty,
    /// policy refusal and physical object failures retain the original intent.
    pub fn reject(
        &mut self,
        plan: [u8; 32],
        request: Apply,
        error: namespace::Error,
    ) -> Result<Record> {
        let mut r = self.get(plan)?;
        let rejected = Rejection { request, error };
        if r.rejection.as_ref() == Some(&rejected) {
            return Ok(r);
        }
        if r.rejection.is_some() || r.receipt.is_some() {
            return Err(Failure::Conflict.into());
        }
        r.rejection = Some(rejected);
        self.save(&r, false)?;
        Ok(r)
    }
    fn save(&mut self, r: &Record, insert: bool) -> Result<()> {
        let bytes = serde_json::to_vec(r)?;
        if bytes.len() > MAX_BYTES {
            return Err(Failure::Quota.into());
        }
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_all(&tx)?;
        isolation(&tx)?;
        if insert {
            let count: u32 =
                tx.query_row("SELECT count(*) FROM chunk_publications", [], |r| r.get(0))?;
            if count >= MAX_RECORDS {
                return Err(Failure::Quota.into());
            }
            let admitted = count + 1;
            let changed = tx.execute("INSERT INTO chunk_client_role VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET admitted=excluded.admitted,state_hash=excluded.state_hash", params![admitted,hash(b"s3drive-chunk-client-role-v1\0", &admitted)?.as_slice()])?;
            if changed != 1 {
                return Err(Failure::Corrupt.into());
            }
            let changed = tx.execute(
                "INSERT INTO chunk_publications VALUES(?1,?2,?3,?4,?5)",
                params![
                    r.input.plan.id.as_slice(),
                    r.input.reserve.operation,
                    r.input.publish_operation,
                    bytes,
                    hash(b"s3drive-chunk-publication-v1\0", r)?.as_slice()
                ],
            )?;
            if changed != 1 {
                return Err(Failure::Corrupt.into());
            }
        } else {
            let changed = tx.execute(
                "UPDATE chunk_publications SET record=?2,state_hash=?3 WHERE plan_id=?1",
                params![
                    r.input.plan.id.as_slice(),
                    bytes,
                    hash(b"s3drive-chunk-publication-v1\0", r)?.as_slice()
                ],
            )?;
            if changed != 1 {
                return Err(Failure::Corrupt.into());
            }
        }
        // FINAL IMMUTABLE SECTION: reload every effect and current plan/graph,
        // isolation and receipt forest. No further writes/callbacks before commit.
        if raw_load(&tx, r.input.plan.id)?.as_ref() != Some(r) {
            return Err(Failure::Corrupt.into());
        }
        validate_all(&tx)?;
        tx.commit()?;
        Ok(())
    }
}
#[derive(Clone)]
pub struct Client {
    owner: Owner,
}
impl Client {
    /// Same private owner; separate historical source status, never generic flush.
    pub fn sources(&self) -> sources::Client {
        sources::Client::new(self.owner.clone())
    }
    pub(crate) fn new(owner: Owner) -> Self {
        Self { owner }
    }
    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Journal<'_>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.owner
            .call(move |s| f(&mut Journal::new(&mut s.state)?))
            .await
    }
    pub async fn get(&self, plan: [u8; 32]) -> Result<Record> {
        self.call(move |j| j.get(plan)).await
    }
    pub async fn records(&self) -> Result<Vec<Record>> {
        self.call(|j| j.records()).await
    }
    pub async fn begin(
        &self,
        plan: [u8; 32],
        base: Node,
        store: Store,
        sequence: u64,
        publish: String,
    ) -> Result<Record> {
        self.call(move |j| j.begin(plan, base, store, sequence, publish))
            .await
    }
    pub async fn reserved(&self, plan: [u8; 32], receipt: Receipt) -> Result<Record> {
        self.call(move |j| j.reserved(plan, receipt)).await
    }
    pub async fn prepare_publish(&self, plan: [u8; 32], sequence: u64) -> Result<Record> {
        self.call(move |j| j.prepare_publish(plan, sequence)).await
    }
    pub async fn published(&self, plan: [u8; 32], receipt: Receipt) -> Result<Record> {
        self.call(move |j| j.published(plan, receipt)).await
    }
    pub async fn reject(
        &self,
        plan: [u8; 32],
        request: Apply,
        error: namespace::Error,
    ) -> Result<Record> {
        self.call(move |j| j.reject(plan, request, error)).await
    }
}
#[cfg(test)]
mod tests;
