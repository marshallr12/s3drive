//! Exact-snapshot historical acknowledgment on the private SQLite owner.
//! No generic flush, ancestor coverage, payload deletion or fresh media proof.
use super::{Record, hash, isolation, raw_load, validate_record};
use crate::{Result, State, journal, namespace::Receipt, worker::Owner};
use journal::{GenerationId, Phase, Target};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

/// Bounds for the added lineage walk, independent of file byte length. The
/// existing publication/tree validators have separate costs. Exceeding either
/// preserves all history and returns Limit; it never implies file cleanliness.
pub const MAX_LINEAGE: usize = 4096;
pub const MAX_LINEAGE_EXTENTS: u64 = 1_000_000;
const MAX_BYTES: usize = 4096;
const PROOF_DOMAIN: &[u8] = b"s3drive-chunk-source-settlement-v1\0";
const ROLE_DOMAIN: &[u8] = b"s3drive-chunk-source-role-v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    NotCommitted,
    Corrupt,
    Limit,
    MixedSemantics,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotCommitted => "chunk source publication is not committed; data preserved",
            Self::Corrupt => "chunk source settlement integrity failure; data preserved",
            Self::Limit => "chunk source metadata limit exceeded; data preserved",
            Self::MixedSemantics => "chunk source has unsupported physical intent; data preserved",
        })
    }
}
impl std::error::Error for Failure {}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    format: u32,
    plan: [u8; 32],
    snapshot: GenerationId,
    manifest: [u8; 32],
    publication: [u8; 32],
    root_generation: GenerationId,
    root_checksum: [u8; 32],
}
/// Historical proof for exactly one frozen snapshot. Does not settle ancestors
/// or newer heads, prove current payload health, or permit payload reclamation.
#[derive(Clone, PartialEq, Eq)]
pub struct SourceReceipt {
    proof: Proof,
    target: Target,
    authority: Receipt,
}
impl std::fmt::Debug for SourceReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChunkSourceReceipt([private])")
    }
}
impl SourceReceipt {
    pub fn plan(&self) -> &[u8; 32] {
        &self.proof.plan
    }
    pub fn snapshot(&self) -> GenerationId {
        self.proof.snapshot
    }
    pub fn manifest(&self) -> &[u8; 32] {
        &self.proof.manifest
    }
    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn root_generation(&self) -> GenerationId {
        self.proof.root_generation
    }
    pub fn authority_receipt(&self) -> &Receipt {
        &self.authority
    }
}
/// Exact snapshot status and its unfiltered publication history. A matching
/// sealed head says nothing about building drafts or other pending operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceStatus {
    publication: Record,
    settlement: Option<SourceReceipt>,
    current_head: GenerationId,
}
impl SourceStatus {
    pub fn publication(&self) -> &Record {
        &self.publication
    }
    pub fn settlement(&self) -> Option<&SourceReceipt> {
        self.settlement.as_ref()
    }
    pub fn current_head(&self) -> GenerationId {
        self.current_head
    }
}

pub(super) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT singleton,settled,state_hash FROM chunk_source_role LIMIT 0")?;
    db.prepare(
        "SELECT plan_id,snapshot_id,record,state_hash FROM chunk_source_settlements LIMIT 0",
    )?;
    Ok(())
}
fn snapshot(db: &Connection, r: &Record) -> Result<journal::Generation> {
    let g = journal::load(db, r.input.snapshot)?;
    if g.target != r.input.plan.local_target()?
        || g.phase != Phase::Sealed
        || g.size != r.input.plan.size
        || journal::validate_manifest(db, &g, false)? != r.input.snapshot_manifest
    {
        return Err(Failure::Corrupt.into());
    }
    let mixed: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM remote_intents WHERE generation_id=?1) OR EXISTS(SELECT 1 FROM remote_flushes WHERE generation_id=?1) OR EXISTS(SELECT 1 FROM write_settlements WHERE generation_id=?1)",
        [g.id.value()], |row| row.get(0))?;
    if mixed {
        return Err(Failure::MixedSemantics.into());
    }
    Ok(g)
}
fn expected(db: &Connection, r: &Record) -> Result<SourceReceipt> {
    validate_record(db, r)?;
    let authority = r.receipt.clone().ok_or(Failure::NotCommitted)?;
    if r.rejection.is_some() {
        return Err(Failure::NotCommitted.into());
    }
    let root = r.root.as_ref().ok_or(Failure::Corrupt)?;
    let g = snapshot(db, r)?;
    Ok(SourceReceipt {
        proof: Proof {
            format: 1,
            plan: r.input.plan.id,
            snapshot: g.id,
            manifest: r.input.snapshot_manifest,
            publication: hash(b"s3drive-chunk-publication-v1\0", r)?,
            root_generation: root.generation,
            root_checksum: root.checksum,
        },
        target: g.target,
        authority,
    })
}
fn lineage(db: &Connection, r: &Record) -> Result<GenerationId> {
    let source = snapshot(db, r)?;
    let head = journal::current_head(db, &source.target)?.ok_or(Failure::Corrupt)?;
    let mut cursor = head;
    let mut extents = 0u64;
    for _ in 0..MAX_LINEAGE {
        let g = journal::load(db, cursor)?;
        if g.target != source.target || g.base != source.base || g.phase != Phase::Sealed {
            return Err(Failure::Corrupt.into());
        }
        extents = extents
            .checked_add(u64::from(g.extent_count()))
            .ok_or(Failure::Limit)?;
        if extents > MAX_LINEAGE_EXTENTS {
            return Err(Failure::Limit.into());
        }
        journal::validate_manifest(db, &g, false)?;
        if cursor == source.id {
            return Ok(head);
        }
        cursor = g
            .parent
            .filter(|id| *id >= source.id)
            .ok_or(Failure::Corrupt)?;
    }
    Err(Failure::Limit.into())
}
fn load(db: &Connection, plan: [u8; 32]) -> Result<Option<Proof>> {
    let row = db.query_row("SELECT snapshot_id,length(record),substr(record,1,4096),length(state_hash),substr(state_hash,1,32) FROM chunk_source_settlements WHERE plan_id=?1", [plan.as_slice()], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,Vec<u8>>(2)?,r.get::<_,i64>(3)?,r.get::<_,Vec<u8>>(4)?))).optional()?;
    let Some((snapshot, len, raw, hlen, checksum)) = row else {
        return Ok(None);
    };
    if !(1..=MAX_BYTES as i64).contains(&len) || hlen != 32 {
        return Err(Failure::Corrupt.into());
    }
    let p: Proof = serde_json::from_slice(&raw).map_err(|_| Failure::Corrupt)?;
    if p.format != 1
        || p.plan != plan
        || p.snapshot.value() != snapshot
        || hash(PROOF_DOMAIN, &p)?.as_slice() != checksum
    {
        return Err(Failure::Corrupt.into());
    }
    Ok(Some(p))
}
/// No call to parent's validate_all here: the parent invokes this helper after
/// its own complete validation, including when its tables are empty.
pub(super) fn validate_all(db: &Connection) -> Result<()> {
    let role = db.query_row("SELECT settled,length(state_hash),substr(state_hash,1,32) FROM chunk_source_role WHERE singleton=1", [], |r| Ok((r.get::<_,u32>(0)?,r.get::<_,i64>(1)?,r.get::<_,Vec<u8>>(2)?))).optional()?;
    let roles: u32 = db.query_row("SELECT count(*) FROM chunk_source_role", [], |r| r.get(0))?;
    let ids = {
        let mut q = db.prepare(
            "SELECT length(plan_id),substr(plan_id,1,32) FROM chunk_source_settlements LIMIT 257",
        )?;
        q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    if role.is_none() && roles == 0 && ids.is_empty() {
        return Ok(());
    }
    let (count, len, checksum) = role.ok_or(Failure::Corrupt)?;
    if roles != 1
        || count == 0
        || count > super::MAX_RECORDS
        || count as usize != ids.len()
        || len != 32
        || hash(ROLE_DOMAIN, &count)?.as_slice() != checksum
    {
        return Err(Failure::Corrupt.into());
    }
    isolation(db)?;
    for (len, id) in ids {
        if len != 32 {
            return Err(Failure::Corrupt.into());
        }
        let plan: [u8; 32] = id.try_into().map_err(|_| Failure::Corrupt)?;
        let p = load(db, plan)?.ok_or(Failure::Corrupt)?;
        let r = raw_load(db, plan)?.ok_or(Failure::Corrupt)?;
        if p != expected(db, &r)?.proof {
            return Err(Failure::Corrupt.into());
        }
    }
    Ok(())
}

pub struct Journal<'a> {
    state: &'a mut State,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Point {
    Role,
    Row,
    Committed,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State) -> Result<Self> {
        state.verify_staging_storage()?;
        isolation(&state.connection)?;
        super::validate_all(&state.connection)?;
        Ok(Self { state })
    }
    pub fn source_status(&mut self, plan: [u8; 32]) -> Result<SourceStatus> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        super::validate_all(&tx)?;
        let r = raw_load(&tx, plan)?.ok_or(super::Failure::Missing)?;
        let current_head = lineage(&tx, &r)?;
        let settlement = load(&tx, plan)?
            .map(|p| -> Result<SourceReceipt> {
                let e = expected(&tx, &r)?;
                if e.proof != p {
                    return Err(Failure::Corrupt.into());
                }
                Ok(e)
            })
            .transpose()?;
        tx.commit()?;
        Ok(SourceStatus {
            publication: r,
            settlement,
            current_head,
        })
    }
    pub fn settle_source(&mut self, plan: [u8; 32]) -> Result<SourceReceipt> {
        self.settle_inner(plan, |_| Ok(()))
    }
    pub(super) fn settle_inner(
        &mut self,
        plan: [u8; 32],
        mut hook: impl FnMut(Point) -> Result<()>,
    ) -> Result<SourceReceipt> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        super::validate_all(&tx)?;
        let r = raw_load(&tx, plan)?.ok_or(super::Failure::Missing)?;
        let receipt = expected(&tx, &r)?;
        lineage(&tx, &r)?;
        if let Some(old) = load(&tx, plan)? {
            if old != receipt.proof {
                return Err(Failure::Corrupt.into());
            }
        } else {
            let count: u32 =
                tx.query_row("SELECT count(*) FROM chunk_source_settlements", [], |r| {
                    r.get(0)
                })?;
            if count >= super::MAX_RECORDS {
                return Err(Failure::Limit.into());
            }
            let count = count + 1;
            if tx.execute("INSERT INTO chunk_source_role VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET settled=excluded.settled,state_hash=excluded.state_hash", params![count,hash(ROLE_DOMAIN,&count)?.as_slice()])? != 1 { return Err(Failure::Corrupt.into()); }
            hook(Point::Role)?;
            let bytes = serde_json::to_vec(&receipt.proof)?;
            if bytes.len() > MAX_BYTES {
                return Err(Failure::Limit.into());
            }
            if tx.execute(
                "INSERT INTO chunk_source_settlements VALUES(?1,?2,?3,?4)",
                params![
                    plan.as_slice(),
                    receipt.proof.snapshot.value(),
                    bytes,
                    hash(PROOF_DOMAIN, &receipt.proof)?.as_slice()
                ],
            )? != 1
            {
                return Err(Failure::Corrupt.into());
            }
            hook(Point::Row)?;
        }
        // FINAL IMMUTABLE SECTION: exact effects, complete recursive physical
        // receipts and authority proof, source lineage and isolation. No further
        // writes, hooks, callbacks or awaits are permitted before commit.
        super::validate_all(&tx)?;
        let saved = raw_load(&tx, plan)?.ok_or(Failure::Corrupt)?;
        if saved != r
            || load(&tx, plan)?.as_ref() != Some(&receipt.proof)
            || expected(&tx, &saved)? != receipt
        {
            return Err(Failure::Corrupt.into());
        }
        lineage(&tx, &saved)?;
        isolation(&tx)?;
        tx.commit()?;
        hook(Point::Committed)?;
        Ok(receipt)
    }
}

#[derive(Clone)]
pub struct Client {
    owner: Owner,
}
impl Client {
    pub(super) fn new(owner: Owner) -> Self {
        Self { owner }
    }
    pub async fn source_status(&self, plan: [u8; 32]) -> Result<SourceStatus> {
        self.owner
            .call(move |s| Journal::new(&mut s.state)?.source_status(plan))
            .await
    }
    pub async fn settle_source(&self, plan: [u8; 32]) -> Result<SourceReceipt> {
        self.owner
            .call(move |s| Journal::new(&mut s.state)?.settle_source(plan))
            .await
    }
}
