//! Durable offline multipart commitments. No initiation, send/completion permit,
//! upload ID, network request, target admission or reclamation is implemented.
//! Ready records describe expected immutable bytes, not present media freshness.
use crate::{Result, State, journal, remote, worker::Owner};
use journal::{GenerationId, Target};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use s3drive_core::multipart::{Limits, Part, Plan};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

const MAX_PLANS: u32 = 4096;
const MAX_RESERVED_PARTS: u32 = 1_000_000;
const MAX_SCAN_EXTENTS: u32 = 65_536;

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub max_plans: u32,
    pub max_reserved_parts: u32,
    /// A metadata-work admission bound; reads still validate the whole manifest.
    pub max_scan_extents: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_plans: 64,
            max_reserved_parts: 100_000,
            max_scan_extents: 4096,
        }
    }
}
impl Options {
    fn validate(self) -> Result<Self> {
        if !(1..=MAX_PLANS).contains(&self.max_plans)
            || !(1..=MAX_RESERVED_PARTS).contains(&self.max_reserved_parts)
            || !(1..=MAX_SCAN_EXTENTS).contains(&self.max_scan_extents)
        {
            return Err(Failure::InvalidInput.into());
        }
        Ok(self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    InvalidInput,
    Conflict,
    QuotaExceeded,
    Corrupt,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "invalid offline multipart planning input",
            Self::Conflict => {
                "multipart planning identity conflicts with frozen input; data preserved"
            }
            Self::QuotaExceeded => {
                "multipart planning resource quota exceeded; existing data preserved"
            }
            Self::Corrupt => "multipart planning integrity check failed; data preserved",
        })
    }
}
impl std::error::Error for Failure {}

/// Supply and retain this key before calling freeze/prepare. A lost reply does
/// not authorize replacing the key or abandoning a prior plan.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Id([u8; 32]);
impl Id {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MultipartId([redacted])")
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Frozen,
    Ready,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartHash {
    pub part: Part,
    pub sha256: [u8; 32],
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitRow {
    object: u64,
    minimum: u64,
    maximum: u64,
    count: u16,
}
impl From<Limits> for LimitRow {
    fn from(v: Limits) -> Self {
        Self {
            object: v.max_object_bytes,
            minimum: v.min_nonfinal_part_bytes,
            maximum: v.max_part_bytes,
            count: v.max_parts,
        }
    }
}
impl LimitRow {
    fn limits(&self) -> Limits {
        Limits {
            max_object_bytes: self.object,
            min_nonfinal_part_bytes: self.minimum,
            max_part_bytes: self.maximum,
            max_parts: self.count,
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    format: u8,
    id: Id,
    generation: GenerationId,
    source: [u8; 32],
    fingerprint: String,
    key: String,
    manifest: [u8; 32],
    size: u64,
    preferred: u64,
    limits: LimitRow,
    part_bytes: u64,
    part_count: u16,
    guard: Option<String>,
    anchor: Option<[u8; 32]>,
    anchor_receipt: Option<[u8; 32]>,
}
impl Input {
    fn target(&self) -> Result<Target> {
        Target::new(self.source, &self.fingerprint, &self.key)
    }
    fn geometry(&self) -> Result<Plan> {
        let p = Plan::new(self.size, self.preferred, self.limits.limits())
            .map_err(|_| Failure::Corrupt)?;
        if self.format != 1
            || p.part_bytes() != self.part_bytes
            || p.part_count() != self.part_count
        {
            return Err(Failure::Corrupt.into());
        }
        Ok(p)
    }
    fn selected(&self) -> remote::PreparedBase {
        remote::PreparedBase {
            guard: self
                .guard
                .clone()
                .map_or(remote::Guard::Missing, remote::Guard::Match),
            anchor: self.anchor,
            anchor_receipt: self.anchor_receipt,
        }
    }
    fn hash(&self) -> Result<[u8; 32]> {
        let mut h = Sha256::new();
        h.update(b"s3drive-multipart-input-v1\0");
        h.update(serde_json::to_vec(self)?);
        Ok(h.finalize().into())
    }
}
#[derive(Clone)]
pub struct Info {
    input: Input,
    input_hash: [u8; 32],
    target: Target,
    geometry: Plan,
    selected: remote::PreparedBase,
    phase: Phase,
    payload: Option<[u8; 32]>,
    parts: Vec<PartHash>,
    plan_hash: Option<[u8; 32]>,
}
impl fmt::Debug for Info {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OfflineMultipartPlan")
            .field("generation", &self.input.generation)
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}
impl Info {
    pub fn id(&self) -> Id {
        self.input.id
    }
    pub fn generation(&self) -> GenerationId {
        self.input.generation
    }
    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn geometry(&self) -> Plan {
        self.geometry
    }
    pub fn guard(&self) -> &remote::Guard {
        &self.selected.guard
    }
    pub fn anchor(&self) -> Option<&[u8; 32]> {
        self.selected.anchor.as_ref()
    }
    pub fn anchor_receipt(&self) -> Option<&[u8; 32]> {
        self.selected.anchor_receipt.as_ref()
    }
    pub fn input_hash(&self) -> &[u8; 32] {
        &self.input_hash
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn payload_sha256(&self) -> Option<&[u8; 32]> {
        self.payload.as_ref()
    }
    pub fn parts(&self) -> &[PartHash] {
        &self.parts
    }
    /// Offline commitment digest, not an upload, flush or authority receipt.
    pub fn plan_hash(&self) -> Option<&[u8; 32]> {
        self.plan_hash.as_ref()
    }
}
fn digest(v: Vec<u8>) -> Result<[u8; 32]> {
    v.try_into().map_err(|_| Failure::Corrupt.into())
}
fn commitment(input: &[u8; 32], payload: &[u8; 32], parts: &[PartHash]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"s3drive-multipart-plan-v1\0");
    h.update(input);
    h.update(payload);
    h.update((parts.len() as u16).to_be_bytes());
    for p in parts {
        h.update(p.part.number.to_be_bytes());
        h.update(p.part.offset.to_be_bytes());
        h.update(p.part.length.to_be_bytes());
        h.update(p.sha256);
    }
    h.finalize().into()
}

/// Metadata/commitment validation. The sole false-manifest caller immediately
/// performs read_data on the same exclusive owner command, before hashing.
fn load(db: &Connection, id: Id, manifest: bool) -> Result<Info> {
    let shape: bool = db.query_row("SELECT length(plan_id)=32 AND length(input) BETWEEN 1 AND 32768 AND length(input_hash)=32 AND part_count BETWEEN 1 AND 10000 AND length(phase) BETWEEN 5 AND 6 AND (payload_sha256 IS NULL OR length(payload_sha256)=32) AND (plan_hash IS NULL OR length(plan_hash)=32) FROM multipart_plans WHERE plan_id=?1", [id.0.as_slice()], |r| r.get(0))?;
    if !shape {
        return Err(Failure::Corrupt.into());
    }
    let (generation, raw, ih, count, phase, payload, ph) = db.query_row("SELECT generation_id,input,input_hash,part_count,phase,payload_sha256,plan_hash FROM multipart_plans WHERE plan_id=?1", [id.0.as_slice()], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,Vec<u8>>(2)?,r.get::<_,u16>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<Vec<u8>>>(5)?,r.get::<_,Option<Vec<u8>>>(6)?)))?;
    let input: Input = serde_json::from_slice(&raw).map_err(|_| Failure::Corrupt)?;
    let input_hash = digest(ih)?;
    let target = input.target()?;
    let geometry = input.geometry()?;
    if input.id != id
        || input.generation.value() != generation
        || input.part_count != count
        || input.hash()? != input_hash
    {
        return Err(Failure::Corrupt.into());
    }
    let g = journal::load(db, input.generation)?;
    if g.phase != journal::Phase::Sealed
        || g.target != target
        || g.size != input.size
        || g.manifest_hash != Some(input.manifest)
    {
        return Err(Failure::Corrupt.into());
    }
    if manifest {
        journal::validate_manifest(db, &g, false)?;
    }
    let selected = input.selected();
    remote::validate_prepared_base(db, &g, &selected)?;
    let payload = payload.map(digest).transpose()?;
    let plan_hash = ph.map(digest).transpose()?;
    let mut parts = Vec::new();
    let mut q = db.prepare("SELECT part_number,offset_bytes,length_bytes,CASE WHEN typeof(sha256)='blob' AND length(sha256)=32 THEN sha256 ELSE NULL END FROM multipart_parts WHERE plan_id=?1 ORDER BY part_number LIMIT 10001")?;
    let mut rows = q.query([id.0.as_slice()])?;
    while let Some(row) = rows.next()? {
        let number: u16 = row.get(0)?;
        let part = geometry.part(number).ok_or(Failure::Corrupt)?;
        if usize::from(number) != parts.len() + 1
            || row.get::<_, i64>(1)? != part.offset as i64
            || row.get::<_, i64>(2)? != part.length as i64
        {
            return Err(Failure::Corrupt.into());
        }
        parts.push(PartHash {
            part,
            sha256: digest(row.get(3)?)?,
        });
    }
    let phase = match (phase.as_str(), payload, plan_hash) {
        ("frozen", None, None) if parts.is_empty() => Phase::Frozen,
        ("ready", Some(payload), Some(hash))
            if parts.len() == usize::from(count)
                && commitment(&input_hash, &payload, &parts) == hash =>
        {
            Phase::Ready
        }
        _ => return Err(Failure::Corrupt.into()),
    };
    Ok(Info {
        input,
        input_hash,
        target,
        geometry,
        selected,
        phase,
        payload,
        parts,
        plan_hash,
    })
}
pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT plan_id,generation_id,input,input_hash,part_count,phase,payload_sha256,plan_hash FROM multipart_plans LIMIT 0")?;
    db.prepare(
        "SELECT plan_id,part_number,offset_bytes,length_bytes,sha256 FROM multipart_parts LIMIT 0",
    )?;
    Ok(())
}
/// Reopen/admission validates retained metadata without claiming payload freshness.
pub(crate) fn validate_all(db: &Connection) -> Result<(u32, u32)> {
    let (count, reserved): (u32, u32) = db.query_row(
        "SELECT count(*),coalesce(sum(part_count),0) FROM multipart_plans",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if count > MAX_PLANS || reserved > MAX_RESERVED_PARTS {
        return Err(Failure::Corrupt.into());
    }
    let orphan: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM multipart_parts p LEFT JOIN multipart_plans i ON p.plan_id=i.plan_id WHERE i.plan_id IS NULL)", [], |r| r.get(0))?;
    if orphan {
        return Err(Failure::Corrupt.into());
    }
    let ids = db
        .prepare("SELECT CASE WHEN typeof(plan_id)='blob' AND length(plan_id)=32 THEN plan_id ELSE NULL END FROM multipart_plans LIMIT 4097")?
        .query_map([], |r| r.get::<_, Vec<u8>>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in ids {
        load(db, Id(digest(id)?), true)?;
    }
    Ok((count, reserved))
}

pub struct Journal<'a> {
    state: &'a mut State,
    options: Options,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    Frozen,
    FreezeCommitted,
    Parts,
    Ready,
    ReadyCommitted,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State, options: Options) -> Result<Self> {
        Ok(Self {
            state,
            options: options.validate()?,
        })
    }
    pub fn info(&self, id: Id) -> Result<Info> {
        load(&self.state.connection, id, true)
    }
    pub fn for_generation(&self, generation: GenerationId) -> Result<Option<Info>> {
        let id: Option<Vec<u8>> = self
            .state
            .connection
            .query_row(
                "SELECT CASE WHEN typeof(plan_id)='blob' AND length(plan_id)=32 THEN plan_id ELSE NULL END FROM multipart_plans WHERE generation_id=?1",
                [generation.value()],
                |r| r.get(0),
            )
            .optional()?;
        id.map(|v| self.info(Id(digest(v)?))).transpose()
    }
    pub fn freeze(
        &mut self,
        id: Id,
        generation: GenerationId,
        preferred: u64,
        limits: Limits,
    ) -> Result<Info> {
        self.freeze_inner(id, generation, preferred, limits, |_| Ok(()))
    }
    fn freeze_inner(
        &mut self,
        id: Id,
        generation: GenerationId,
        preferred: u64,
        limits: Limits,
        mut hook: impl FnMut(Point) -> Result<()>,
    ) -> Result<Info> {
        self.state.verify_staging_storage()?;
        let exists: bool = self.state.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM multipart_plans WHERE plan_id=?1)",
            [id.0.as_slice()],
            |r| r.get(0),
        )?;
        if exists {
            let old = self.info(id)?;
            if old.generation() != generation
                || old.input.preferred != preferred
                || old.input.limits != limits.into()
            {
                return Err(Failure::Conflict.into());
            }
            return Ok(old);
        }
        if self.for_generation(generation)?.is_some() {
            return Err(Failure::Conflict.into());
        }
        let g = journal::load(&self.state.connection, generation)?;
        if g.phase != journal::Phase::Sealed {
            return Err(Failure::InvalidInput.into());
        }
        let geometry = Plan::new(g.size, preferred, limits).map_err(|_| Failure::InvalidInput)?;
        if g.extent_count() > self.options.max_scan_extents {
            return Err(Failure::QuotaExceeded.into());
        }
        journal::validate_manifest(&self.state.connection, &g, false)?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let history = remote::preparation_history(&tx, &g)?;
        let (plans, parts) = validate_all(&tx)?;
        if plans >= self.options.max_plans
            || parts
                .checked_add(u32::from(geometry.part_count()))
                .is_none_or(|n| n > self.options.max_reserved_parts)
        {
            return Err(Failure::QuotaExceeded.into());
        }
        let selected = remote::preparation_base(&tx, &g, history)?;
        let input = Input {
            format: 1,
            id,
            generation,
            source: *g.target.source(),
            fingerprint: g.target.fingerprint().into(),
            key: g.target.object_key().into(),
            manifest: g.manifest_hash.ok_or(Failure::Corrupt)?,
            size: g.size,
            preferred,
            limits: limits.into(),
            part_bytes: geometry.part_bytes(),
            part_count: geometry.part_count(),
            guard: match selected.guard {
                remote::Guard::Missing => None,
                remote::Guard::Match(s) => Some(s),
            },
            anchor: selected.anchor,
            anchor_receipt: selected.anchor_receipt,
        };
        let raw = serde_json::to_vec(&input)?;
        if raw.len() > 32768 {
            return Err(Failure::InvalidInput.into());
        }
        let inserted = tx.execute(
            "INSERT INTO multipart_plans VALUES (?1,?2,?3,?4,?5,'frozen',NULL,NULL)",
            params![
                id.0.as_slice(),
                generation.value(),
                raw,
                input.hash()?.as_slice(),
                geometry.part_count()
            ],
        )?;
        if inserted != 1 {
            return Err(Failure::Corrupt.into());
        }
        let saved = load(&tx, id, true)?;
        if saved.input != input || saved.phase != Phase::Frozen {
            return Err(Failure::Corrupt.into());
        }
        hook(Point::Frozen)?;
        tx.commit()?;
        hook(Point::FreezeCommitted)?;
        self.info(id)
    }
    fn step(&self, mut work: HashWork) -> Result<HashWork> {
        self.state.verify_staging_storage()?;
        let current = load(&self.state.connection, work.input.id, false)?;
        if current.input != work.input || current.input_hash != work.input_hash {
            return Err(Failure::Conflict.into());
        }
        let g = journal::load(&self.state.connection, work.input.generation)?;
        if g.extent_count() > self.options.max_scan_extents {
            return Err(Failure::QuotaExceeded.into());
        }
        let length = (work.input.size - work.offset).min(journal::CHUNK_BYTES as u64) as usize;
        if length == 0 {
            return Err(Failure::InvalidInput.into());
        }
        let bytes = journal::read_data(
            &self.state.connection,
            work.input.generation,
            work.offset,
            length,
            false,
        )?;
        if bytes.len() != length {
            return Err(Failure::Corrupt.into());
        }
        work.consume(&bytes)?;
        Ok(work)
    }
    fn finish(
        &mut self,
        work: HashWork,
        mut hook: impl FnMut(Point) -> Result<()>,
    ) -> Result<Info> {
        self.state.verify_staging_storage()?;
        if work.offset != work.input.size
            || work.parts.len() != usize::from(work.geometry.part_count())
            || work.part_filled != 0
        {
            return Err(Failure::InvalidInput.into());
        }
        let payload: [u8; 32] = work.whole.finalize().into();
        let hash = commitment(&work.input_hash, &payload, &work.parts);
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_all(&tx)?;
        let current = load(&tx, work.input.id, true)?;
        if current.input != work.input || current.input_hash != work.input_hash {
            return Err(Failure::Conflict.into());
        }
        if current.phase == Phase::Ready {
            if current.payload != Some(payload)
                || current.parts != work.parts
                || current.plan_hash != Some(hash)
            {
                return Err(Failure::Corrupt.into());
            }
            return Ok(current);
        }
        for p in &work.parts {
            let inserted = tx.execute(
                "INSERT INTO multipart_parts VALUES (?1,?2,?3,?4,?5)",
                params![
                    work.input.id.0.as_slice(),
                    p.part.number,
                    p.part.offset as i64,
                    p.part.length as i64,
                    p.sha256.as_slice()
                ],
            )?;
            if inserted != 1 {
                return Err(Failure::Corrupt.into());
            }
        }
        hook(Point::Parts)?;
        if tx.execute("UPDATE multipart_plans SET phase='ready',payload_sha256=?1,plan_hash=?2 WHERE plan_id=?3 AND phase='frozen' AND input_hash=?4", params![payload.as_slice(),hash.as_slice(),work.input.id.0.as_slice(),work.input_hash.as_slice()])? != 1 { return Err(Failure::Corrupt.into()); }
        hook(Point::Ready)?;
        // Validate effects inside this same transaction: ignored writes or
        // triggers must not leave a durable Ready row with missing/changed parts.
        let saved = load(&tx, work.input.id, true)?;
        if saved.input != work.input
            || saved.payload != Some(payload)
            || saved.parts != work.parts
            || saved.plan_hash != Some(hash)
        {
            return Err(Failure::Corrupt.into());
        }
        tx.commit()?;
        hook(Point::ReadyCommitted)?;
        self.info(work.input.id)
    }
}

/// Only this module can construct/advance/finalize the transient hash state.
/// Restart from zero after losing it; no dependency-specific hasher is persisted.
struct HashWork {
    input: Input,
    input_hash: [u8; 32],
    geometry: Plan,
    offset: u64,
    whole: Sha256,
    part_hash: Sha256,
    part_filled: u64,
    parts: Vec<PartHash>,
}
impl HashWork {
    fn new(info: Info) -> Self {
        Self {
            input: info.input,
            input_hash: info.input_hash,
            geometry: info.geometry,
            offset: 0,
            whole: Sha256::new(),
            part_hash: Sha256::new(),
            part_filled: 0,
            parts: Vec::new(),
        }
    }
    fn consume(&mut self, bytes: &[u8]) -> Result<()> {
        self.whole.update(bytes);
        let mut bytes = bytes;
        while !bytes.is_empty() {
            let number = u16::try_from(self.parts.len() + 1).map_err(|_| Failure::Corrupt)?;
            let part = self.geometry.part(number).ok_or(Failure::Corrupt)?;
            let take = (part.length - self.part_filled).min(bytes.len() as u64) as usize;
            self.part_hash.update(&bytes[..take]);
            self.part_filled += take as u64;
            self.offset += take as u64;
            bytes = &bytes[take..];
            if self.part_filled == part.length {
                self.parts.push(PartHash {
                    part,
                    sha256: std::mem::take(&mut self.part_hash).finalize().into(),
                });
                self.part_filled = 0;
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Planner {
    owner: Owner,
    options: Options,
}
impl Planner {
    pub(crate) fn new(owner: Owner, options: Options) -> Result<Self> {
        Ok(Self {
            owner,
            options: options.validate()?,
        })
    }
    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Journal<'_>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let options = self.options;
        self.owner
            .call(move |store| f(&mut Journal::new(&mut store.state, options)?))
            .await
    }
    pub async fn info(&self, id: Id) -> Result<Info> {
        self.call(move |j| j.info(id)).await
    }
    pub async fn for_generation(&self, generation: GenerationId) -> Result<Option<Info>> {
        self.call(move |j| j.for_generation(generation)).await
    }
    pub async fn freeze(
        &self,
        id: Id,
        generation: GenerationId,
        preferred: u64,
        limits: Limits,
    ) -> Result<Info> {
        self.call(move |j| j.freeze(id, generation, preferred, limits))
            .await
    }
    /// Freeze durably before hashing. One owner command reads/hashes <=1MiB;
    /// other commands may run between chunks. Each chunk revalidates its bytes.
    /// Repeated preparation rehashes even Ready plans; info() is historical
    /// metadata lookup. Future sends must still use verified staged reads.
    pub async fn prepare(
        &self,
        id: Id,
        generation: GenerationId,
        preferred: u64,
        limits: Limits,
    ) -> Result<Info> {
        let mut work = HashWork::new(self.freeze(id, generation, preferred, limits).await?);
        while work.offset < work.input.size {
            work = self.call(move |j| j.step(work)).await?;
            tokio::task::yield_now().await;
        }
        self.call(move |j| j.finish(work, |_| Ok(()))).await
    }
}

#[cfg(test)]
mod tests;
