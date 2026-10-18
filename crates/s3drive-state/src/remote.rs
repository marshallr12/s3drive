//! Durable single-PUT attempt records. No network, provider authorization,
//! upload executor or payload reclamation is performed by this module.
mod acknowledgments;
use crate::{
    Result, State,
    journal::{self, Base, GenerationId, Target},
};
pub(crate) use acknowledgments::validate_all as validate_acknowledgments;
pub(crate) use acknowledgments::{CheckedFlushes, checked_flushes};
pub use acknowledgments::{FlushReceipt, Settlement};
use rusqlite::{Connection, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::fmt;

pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const MAX_RECORDS: u32 = 65_536;
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub max_records: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self { max_records: 4096 }
    }
}
impl Options {
    fn validate(self) -> Result<Self> {
        if self.max_records == 0 || self.max_records > MAX_RECORDS {
            return Err(Failure::InvalidInput.into());
        }
        Ok(self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    InvalidInput,
    InvalidState,
    TooLarge,
    QuotaExceeded,
    StaleGeneration,
    TargetBlocked,
    BindingMismatch,
    AlreadyDispatched,
    EvidenceMismatch,
    Corrupt,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
    Self::InvalidInput=>"invalid remote intent input",Self::InvalidState=>"remote intent transition is not allowed",
    Self::TooLarge=>"remote single-PUT journal bound exceeded; local bytes were preserved",
    Self::QuotaExceeded=>"remote intent record quota is full; existing records were preserved",
    Self::StaleGeneration=>"a newer local generation must be selected before preparing or claiming",
    Self::TargetBlocked=>"another unresolved remote intent blocks this target",
    Self::BindingMismatch=>"remote credential/configuration binding differs; explicit resolution is required",
    Self::AlreadyDispatched=>"send permission was already issued; observe the remote outcome instead of replaying",
    Self::EvidenceMismatch=>"remote observation does not match the exact recorded intent",
    Self::Corrupt=>"remote intent integrity check failed; data was preserved",
})
    }
}
impl std::error::Error for Failure {}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationId([u8; 32]);
impl OperationId {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
impl fmt::Debug for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RemoteOperation([redacted])")
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Guard {
    Missing,
    Match(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Prepared,
    InFlight,
    Uncertain,
    Verified,
    Rejected,
    Cancelled,
}
impl Phase {
    fn text(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::InFlight => "in-flight",
            Self::Uncertain => "uncertain",
            Self::Verified => "verified",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
        }
    }
    fn parse(v: &str) -> Result<Self> {
        match v {
            "prepared" => Ok(Self::Prepared),
            "in-flight" => Ok(Self::InFlight),
            "uncertain" => Ok(Self::Uncertain),
            "verified" => Ok(Self::Verified),
            "rejected" => Ok(Self::Rejected),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(Failure::Corrupt.into()),
        }
    }
    fn blocks(self) -> bool {
        !matches!(self, Self::Verified | Self::Cancelled)
    }
    fn retained(self) -> bool {
        !matches!(self, Self::Prepared | Self::Cancelled)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    None,
    Interrupted,
    Transport,
    Timeout,
    Response,
    Observation,
}
impl Reason {
    fn text(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Interrupted => "interrupted",
            Self::Transport => "transport",
            Self::Timeout => "timeout",
            Self::Response => "response",
            Self::Observation => "observation",
        }
    }
    fn parse(v: &str) -> Result<Self> {
        match v {
            "none" => Ok(Self::None),
            "interrupted" => Ok(Self::Interrupted),
            "transport" => Ok(Self::Transport),
            "timeout" => Ok(Self::Timeout),
            "response" => Ok(Self::Response),
            "observation" => Ok(Self::Observation),
            _ => Err(Failure::Corrupt.into()),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observation {
    None,
    Absent,
    Other,
}
impl Observation {
    fn text(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Absent => "absent",
            Self::Other => "other",
        }
    }
    fn parse(v: &str) -> Result<Self> {
        match v {
            "none" => Ok(Self::None),
            "absent" => Ok(Self::Absent),
            "other" => Ok(Self::Other),
            _ => Err(Failure::Corrupt.into()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revision {
    etag: String,
    version: Option<String>,
}
impl Revision {
    pub fn new(etag: &str, version: Option<&str>) -> Result<Self> {
        let version = version.filter(|v| *v != "null");
        if !valid_etag(etag)
            || version
                .is_some_and(|v| v.is_empty() || v.len() > 8192 || v.chars().any(char::is_control))
        {
            return Err(Failure::InvalidInput.into());
        }
        Ok(Self {
            etag: etag.into(),
            version: version.map(Into::into),
        })
    }
    pub fn etag(&self) -> &str {
        &self.etag
    }
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
}
#[derive(Clone, Debug)]
pub struct Info {
    pub operation: OperationId,
    pub generation: GenerationId,
    pub target: Target,
    pub manifest_hash: [u8; 32],
    pub payload_sha256: [u8; 32],
    pub size: u64,
    pub guard: Guard,
    pub anchor: Option<OperationId>,
    pub phase: Phase,
    pub reason: Reason,
    pub observation: Observation,
    pub rejection_status: Option<u16>,
    pub revision: Option<Revision>,
    anchor_receipt: Option<[u8; 32]>,
    intent_hash: [u8; 32],
    state_hash: [u8; 32],
}
/// Not Clone: a successful prepared -> in-flight commit may issue this only
/// once. The future transport must consume it and never retry an uncertain PUT.
pub struct SendPermit {
    intent: Info,
}
impl SendPermit {
    pub fn intent(&self) -> &Info {
        &self.intent
    }
}
impl fmt::Debug for SendPermit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SendPermit([redacted])")
    }
}
/// Caller-supplied observation, not an authorization token. A network layer
/// must establish exact key/binding/operation metadata and complete body hash.
#[derive(Clone, Debug)]
pub struct Verification {
    pub operation: OperationId,
    pub target: Target,
    pub payload_sha256: [u8; 32],
    pub size: u64,
    pub revision: Revision,
}

pub struct Journal<'a> {
    state: &'a mut State,
    options: Options,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State, options: Options) -> Result<Self> {
        Ok(Self {
            state,
            options: options.validate()?,
        })
    }
    pub fn info(&self, id: OperationId) -> Result<Info> {
        load_operation(&self.state.connection, id)
    }
    pub fn for_generation(&self, id: GenerationId) -> Result<Option<Info>> {
        let exists: bool = self.state.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM remote_intents WHERE generation_id=?1)",
            [id.value()],
            |r| r.get(0),
        )?;
        exists.then(|| load(&self.state.connection, id)).transpose()
    }
    pub fn list(&self, after: Option<GenerationId>, limit: u16) -> Result<Vec<Info>> {
        if !(1..=1000).contains(&limit) {
            return Err(Failure::InvalidInput.into());
        }
        let mut q=self.state.connection.prepare("SELECT generation_id FROM remote_intents WHERE generation_id>?1 ORDER BY generation_id LIMIT ?2")?;
        let ids = q
            .query_map(params![after.map_or(0, GenerationId::value), limit], |r| {
                r.get::<_, i64>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.into_iter()
            .map(|id| load(&self.state.connection, GenerationId::from_database(id)?))
            .collect()
    }
    pub fn prepare(&mut self, generation: GenerationId) -> Result<Info> {
        self.prepare_inner(generation, |_| Ok(()))
    }
    fn prepare_inner(
        &mut self,
        generation: GenerationId,
        mut hook: impl FnMut(PreparePoint) -> Result<()>,
    ) -> Result<Info> {
        self.state.verify_staging_storage()?;
        if let Some(info) = self.for_generation(generation)? {
            return Ok(info);
        }
        let (generation_data, payload) = payload(self.state, generation)?;
        let mut operation = [0; 32];
        getrandom::fill(&mut operation).map_err(|_| Failure::InvalidInput)?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let history = preparation_history(&tx, &generation_data)?;
        if history.count >= self.options.max_records {
            return Err(Failure::QuotaExceeded.into());
        }
        let selected = preparation_base(&tx, &generation_data, history)?;
        let guard = selected.guard;
        let anchor_id = selected.anchor.map(OperationId);
        let anchor_receipt = selected.anchor_receipt;
        let mut info = Info {
            operation: OperationId(operation),
            generation,
            target: generation_data.target,
            manifest_hash: generation_data.manifest_hash.ok_or(Failure::Corrupt)?,
            payload_sha256: payload,
            size: generation_data.size,
            guard,
            anchor: anchor_id,
            phase: Phase::Prepared,
            reason: Reason::None,
            observation: Observation::None,
            rejection_status: None,
            revision: None,
            anchor_receipt,
            intent_hash: [0; 32],
            state_hash: [0; 32],
        };
        info.intent_hash = intent_hash(&info);
        info.state_hash = state_hash(&info);
        tx.execute("INSERT INTO remote_intents VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'prepared','none','none',NULL,NULL,NULL,?13)",params![generation.value(),info.operation.0.as_slice(),info.target.source().as_slice(),info.target.fingerprint(),info.target.object_key(),info.manifest_hash.as_slice(),info.payload_sha256.as_slice(),info.size as i64,guard_tag(&info.guard),info.anchor.map(|id|id.0.to_vec()),info.anchor_receipt.map(|v|v.to_vec()),info.intent_hash.as_slice(),info.state_hash.as_slice()])?;
        hook(PreparePoint::Stored)?;
        tx.commit()?;
        hook(PreparePoint::Committed)?;
        Ok(info)
    }
    pub fn claim(&mut self, id: OperationId, binding: &Target) -> Result<SendPermit> {
        self.claim_inner(id, binding, |_| Ok(()))
    }
    fn claim_inner(
        &mut self,
        id: OperationId,
        binding: &Target,
        mut hook: impl FnMut(ClaimPoint) -> Result<()>,
    ) -> Result<SendPermit> {
        self.state.verify_staging_storage()?;
        let old = self.info(id)?;
        if old.phase != Phase::Prepared {
            return Err(Failure::AlreadyDispatched.into());
        }
        if old.target != *binding {
            return Err(Failure::BindingMismatch.into());
        }
        let (_, hash) = payload(self.state, old.generation)?;
        if hash != old.payload_sha256 {
            return Err(Failure::Corrupt.into());
        }
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        scan(&tx, |_| Ok(()))?;
        acknowledgments::validate_all(&tx)?;
        let mut info = load_operation(&tx, id)?;
        if info.phase != Phase::Prepared {
            return Err(Failure::AlreadyDispatched.into());
        }
        if info.intent_hash != old.intent_hash {
            return Err(Failure::Corrupt.into());
        }
        if journal::current_head(&tx, &info.target)? != Some(info.generation) {
            return Err(Failure::StaleGeneration.into());
        }
        tx.execute(
            "INSERT INTO retained_remote_intents VALUES (?1)",
            [id.0.as_slice()],
        )?;
        hook(ClaimPoint::Retained)?;
        info.phase = Phase::InFlight;
        update(&tx, &mut info)?;
        hook(ClaimPoint::Updated)?;
        tx.commit()?;
        hook(ClaimPoint::Committed)?;
        Ok(SendPermit { intent: info })
    }
    pub fn cancel_prepared(&mut self, id: OperationId) -> Result<Info> {
        self.change(id, |info| {
            match info.phase {
                Phase::Prepared => info.phase = Phase::Cancelled,
                Phase::Cancelled => {}
                _ => return Err(Failure::AlreadyDispatched.into()),
            }
            Ok(())
        })
    }
    pub fn uncertain(&mut self, id: OperationId, reason: Reason) -> Result<Info> {
        if reason == Reason::None {
            return Err(Failure::InvalidInput.into());
        }
        self.change(id, |info| {
            match info.phase {
                Phase::InFlight => {
                    info.phase = Phase::Uncertain;
                    info.reason = reason;
                }
                Phase::Uncertain | Phase::Verified | Phase::Rejected => {}
                _ => return Err(Failure::InvalidState.into()),
            }
            Ok(())
        })
    }
    pub fn observe_unmatched(&mut self, id: OperationId, observation: Observation) -> Result<Info> {
        if observation == Observation::None {
            return Err(Failure::InvalidInput.into());
        }
        self.change(id, |info| {
            if !matches!(info.phase, Phase::InFlight | Phase::Uncertain) {
                return Err(Failure::InvalidState.into());
            }
            if info.phase == Phase::InFlight {
                info.phase = Phase::Uncertain;
                info.reason = Reason::Observation;
            }
            info.observation = observation;
            Ok(())
        })
    }
    pub fn rejected(&mut self, id: OperationId, status: u16) -> Result<Info> {
        if !matches!(status, 409 | 412) {
            return Err(Failure::InvalidInput.into());
        }
        self.change(id, |info| {
            if info.phase == Phase::Rejected && info.rejection_status == Some(status) {
                return Ok(());
            }
            if !matches!(info.phase, Phase::InFlight | Phase::Uncertain) {
                return Err(Failure::InvalidState.into());
            }
            info.phase = Phase::Rejected;
            info.reason = Reason::None;
            info.observation = Observation::None;
            info.rejection_status = Some(status);
            Ok(())
        })
    }
    pub fn verified(&mut self, proof: Verification) -> Result<Info> {
        self.verified_inner(proof, |_| Ok(()))
    }
    fn verified_inner(
        &mut self,
        proof: Verification,
        mut hook: impl FnMut(VerifyPoint) -> Result<()>,
    ) -> Result<Info> {
        self.state.verify_staging_storage()?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut info = load_operation(&tx, proof.operation)?;
        if proof.target != info.target
            || proof.size != info.size
            || proof.payload_sha256 != info.payload_sha256
        {
            return Err(Failure::EvidenceMismatch.into());
        }
        if info.phase == Phase::Verified {
            if info.revision != Some(proof.revision) {
                return Err(Failure::EvidenceMismatch.into());
            }
            return Ok(info);
        }
        if !matches!(info.phase, Phase::InFlight | Phase::Uncertain) {
            return Err(Failure::InvalidState.into());
        }
        info.phase = Phase::Verified;
        info.reason = Reason::None;
        info.observation = Observation::None;
        info.revision = Some(proof.revision);
        update(&tx, &mut info)?;
        hook(VerifyPoint::Updated)?;
        tx.commit()?;
        hook(VerifyPoint::Committed)?;
        Ok(info)
    }
    fn change(
        &mut self,
        id: OperationId,
        change: impl FnOnce(&mut Info) -> Result<()>,
    ) -> Result<Info> {
        self.state.verify_staging_storage()?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut info = load_operation(&tx, id)?;
        change(&mut info)?;
        update(&tx, &mut info)?;
        tx.commit()?;
        Ok(info)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreparePoint {
    Stored,
    Committed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClaimPoint {
    Retained,
    Updated,
    Committed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VerifyPoint {
    Updated,
    Committed,
}
pub(crate) fn recover_interrupted(state: &mut State) -> Result<()> {
    let tx = state
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    // Validate globally before granting any future permission. Stream rows;
    // never materialize an unbounded history or trust a mutable target index.
    scan(&tx, |_| Ok(()))?;
    acknowledgments::validate_all(&tx)?;
    let ids = {
        let mut q=tx.prepare("SELECT generation_id FROM remote_intents WHERE phase='in-flight' ORDER BY generation_id")?;
        q.query_map([], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for id in ids {
        let mut info = load(&tx, GenerationId::from_database(id)?)?;
        info.phase = Phase::Uncertain;
        info.reason = Reason::Interrupted;
        update(&tx, &mut info)?;
    }
    tx.commit()?;
    Ok(())
}
/// Read-only extraction shared by single PUT and offline multipart planning.
/// Keeping history and base selection separate preserves the legacy quota/error
/// order. Neither helper grants target ownership or permission to send.
pub(crate) struct PreparationHistory {
    pub count: u32,
    anchor: Option<Info>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedBase {
    pub guard: Guard,
    pub anchor: Option<[u8; 32]>,
    pub anchor_receipt: Option<[u8; 32]>,
}
pub(crate) fn preparation_history(
    connection: &Connection,
    generation_data: &journal::Generation,
) -> Result<PreparationHistory> {
    let mut count = 0;
    let mut anchor = None;
    scan(connection, |info| {
        count += 1;
        if same_locator(&info.target, &generation_data.target) {
            if info.phase.blocks() {
                return Err(Failure::TargetBlocked.into());
            }
            if info.target != generation_data.target {
                return Err(Failure::BindingMismatch.into());
            }
            if info.phase == Phase::Verified
                && anchor
                    .as_ref()
                    .is_none_or(|old: &Info| info.generation > old.generation)
            {
                anchor = Some(info.clone());
            }
        }
        Ok(())
    })?;
    Ok(PreparationHistory { count, anchor })
}
pub(crate) fn preparation_base(
    connection: &Connection,
    generation_data: &journal::Generation,
    history: PreparationHistory,
) -> Result<PreparedBase> {
    let anchor = history.anchor;
    // Another credential generation's local bytes need explicit rebinding,
    // including when they have not yet reached the remote journal.
    let mut heads = connection.prepare("SELECT generation_id FROM staged_heads NOT INDEXED")?;
    let mut rows = heads.query([])?;
    while let Some(row) = rows.next()? {
        let other = journal::load(connection, GenerationId::from_database(row.get(0)?)?)?;
        if same_locator(&other.target, &generation_data.target)
            && other.target != generation_data.target
        {
            return Err(Failure::BindingMismatch.into());
        }
    }
    drop(rows);
    drop(heads);
    if journal::current_head(connection, &generation_data.target)? != Some(generation_data.id) {
        return Err(Failure::StaleGeneration.into());
    }
    let current = journal::load(connection, generation_data.id)?;
    if current.manifest_hash != generation_data.manifest_hash {
        return Err(Failure::Corrupt.into());
    }
    let (guard, anchor_id, anchor_receipt) = if let Some(anchor) = anchor {
        if anchor.generation >= generation_data.id
            || !ancestor(connection, generation_data, anchor.generation)?
        {
            return Err(Failure::Corrupt.into());
        }
        (
            Guard::Match(
                anchor
                    .revision
                    .as_ref()
                    .ok_or(Failure::Corrupt)?
                    .etag
                    .clone(),
            ),
            Some(anchor.operation.0),
            Some(anchor.state_hash),
        )
    } else {
        let guard = match &generation_data.base {
            Base::Missing => Guard::Missing,
            Base::Existing {
                etag: Some(tag), ..
            } if valid_etag(tag) => Guard::Match(tag.clone()),
            _ => return Err(Failure::InvalidInput.into()),
        };
        (guard, None, None)
    };
    Ok(PreparedBase {
        guard,
        anchor: anchor_id,
        anchor_receipt,
    })
}
/// Revalidate the original selection; never choose a newer guard on resume.
pub(crate) fn validate_prepared_base(
    connection: &Connection,
    generation: &journal::Generation,
    base: &PreparedBase,
) -> Result<()> {
    match (base.anchor, base.anchor_receipt) {
        (Some(operation), Some(receipt)) => {
            let anchor = load_operation(connection, OperationId(operation))?;
            if anchor.phase != Phase::Verified
                || anchor.target != generation.target
                || anchor.generation >= generation.id
                || anchor.state_hash != receipt
                || !ancestor(connection, generation, anchor.generation)?
                || base.guard
                    != Guard::Match(
                        anchor
                            .revision
                            .as_ref()
                            .ok_or(Failure::Corrupt)?
                            .etag
                            .clone(),
                    )
            {
                return Err(Failure::Corrupt.into());
            }
        }
        (None, None) => {
            let expected = match &generation.base {
                Base::Missing => Guard::Missing,
                Base::Existing {
                    etag: Some(tag), ..
                } if valid_etag(tag) => Guard::Match(tag.clone()),
                _ => return Err(Failure::Corrupt.into()),
            };
            if base.guard != expected {
                return Err(Failure::Corrupt.into());
            }
        }
        _ => return Err(Failure::Corrupt.into()),
    }
    Ok(())
}

fn payload(state: &mut State, id: GenerationId) -> Result<(journal::Generation, [u8; 32])> {
    let j = journal::Journal::new(state, journal::Options::default())?;
    let info = j.info(id)?;
    if info.phase != journal::Phase::Sealed {
        return Err(Failure::InvalidState.into());
    }
    if info.size > MAX_BODY_BYTES as u64 {
        return Err(Failure::TooLarge.into());
    }
    let bytes = j.read(id, 0, info.size as usize)?;
    Ok((info, Sha256::digest(bytes).into()))
}
fn ancestor(
    connection: &Connection,
    child: &journal::Generation,
    wanted: GenerationId,
) -> Result<bool> {
    let mut parent = child.parent;
    let mut count = 0;
    while let Some(id) = parent {
        count += 1;
        if count > MAX_RECORDS {
            return Err(Failure::Corrupt.into());
        }
        let generation_data = journal::load(connection, id)?;
        journal::validate_manifest(connection, &generation_data, false)?;
        if generation_data.target != child.target || generation_data.base != child.base {
            return Err(Failure::Corrupt.into());
        }
        if id == wanted {
            return Ok(true);
        }
        parent = generation_data.parent;
    }
    Ok(false)
}
fn same_locator(a: &Target, b: &Target) -> bool {
    a.fingerprint() == b.fingerprint() && a.object_key() == b.object_key()
}
fn scan(connection: &Connection, mut visit: impl FnMut(&Info) -> Result<()>) -> Result<()> {
    let orphan:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM retained_remote_intents r LEFT JOIN remote_intents i ON i.operation_id=r.operation_id WHERE i.operation_id IS NULL)",[],|r|r.get(0))?;
    if orphan {
        return Err(Failure::Corrupt.into());
    }
    let mut q = connection.prepare("SELECT generation_id FROM remote_intents NOT INDEXED")?;
    let mut rows = q.query([])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        if count > MAX_RECORDS {
            return Err(Failure::Corrupt.into());
        }
        let info = load(connection, GenerationId::from_database(row.get(0)?)?)?;
        visit(&info)?;
    }
    Ok(())
}
fn load_operation(connection: &Connection, id: OperationId) -> Result<Info> {
    let generation: i64 = connection.query_row(
        "SELECT generation_id FROM remote_intents WHERE operation_id=?1",
        [id.0.as_slice()],
        |r| r.get(0),
    )?;
    let info = load(connection, GenerationId::from_database(generation)?)?;
    if info.operation != id {
        return Err(Failure::Corrupt.into());
    }
    Ok(info)
}
pub(crate) fn load(connection: &Connection, generation: GenerationId) -> Result<Info> {
    let info = raw_load(connection, generation)?;
    if let Some(anchor) = info.anchor {
        let id: i64 = connection.query_row(
            "SELECT generation_id FROM remote_intents WHERE operation_id=?1",
            [anchor.0.as_slice()],
            |r| r.get(0),
        )?;
        let base = raw_load(connection, GenerationId::from_database(id)?)?;
        if base.operation != anchor
            || base.phase != Phase::Verified
            || base.generation >= generation
            || base.target != info.target
            || Some(base.state_hash) != info.anchor_receipt
            || info.guard
                != Guard::Match(base.revision.as_ref().ok_or(Failure::Corrupt)?.etag.clone())
        {
            return Err(Failure::Corrupt.into());
        }
    }
    Ok(info)
}
fn raw_load(connection: &Connection, generation: GenerationId) -> Result<Info> {
    let shape:bool=connection.query_row("SELECT coalesce(length(operation_id)=32 AND length(source_id)=32 AND length(config_fingerprint)=64 AND length(CAST(object_key AS BLOB)) BETWEEN 1 AND 1024 AND length(manifest_hash)=32 AND length(payload_sha256)=32 AND length(intent_hash)=32 AND length(state_hash)=32 AND (guard_etag IS NULL OR length(CAST(guard_etag AS BLOB)) BETWEEN 1 AND 8192) AND (remote_etag IS NULL OR length(CAST(remote_etag AS BLOB)) BETWEEN 1 AND 8192) AND (remote_version IS NULL OR length(CAST(remote_version AS BLOB)) BETWEEN 1 AND 8192) AND (anchor_operation IS NULL OR length(anchor_operation)=32) AND (anchor_receipt IS NULL OR length(anchor_receipt)=32) AND length(phase) BETWEEN 1 AND 12 AND length(reason) BETWEEN 1 AND 12 AND length(observation) BETWEEN 1 AND 8,0) FROM remote_intents WHERE generation_id=?1",[generation.value()],|r|r.get(0))?;
    if !shape {
        return Err(Failure::Corrupt.into());
    }
    let row=connection.query_row("SELECT operation_id,source_id,config_fingerprint,object_key,manifest_hash,payload_sha256,size_bytes,guard_etag,anchor_operation,anchor_receipt,intent_hash,phase,reason,observation,rejection_status,remote_etag,remote_version,state_hash FROM remote_intents WHERE generation_id=?1",[generation.value()],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,Vec<u8>>(4)?,r.get::<_,Vec<u8>>(5)?,r.get::<_,i64>(6)?,r.get::<_,Option<String>>(7)?,r.get::<_,Option<Vec<u8>>>(8)?,r.get::<_,Option<Vec<u8>>>(9)?,r.get::<_,Vec<u8>>(10)?,r.get::<_,String>(11)?,r.get::<_,String>(12)?,r.get::<_,String>(13)?,r.get::<_,Option<u16>>(14)?,r.get::<_,Option<String>>(15)?,r.get::<_,Option<String>>(16)?,r.get::<_,Vec<u8>>(17)?)))?;
    let (
        op,
        source,
        fingerprint,
        key,
        manifest,
        payload,
        size,
        guard,
        anchor,
        anchor_receipt,
        ih,
        phase,
        reason,
        observation,
        status,
        tag,
        version,
        sh,
    ) = row;
    let target = Target::new(digest(source)?, &fingerprint, &key)?;
    if version.as_deref() == Some("null") {
        return Err(Failure::Corrupt.into());
    }
    let revision = match tag {
        Some(tag) => Some(Revision::new(&tag, version.as_deref())?),
        None if version.is_none() => None,
        _ => return Err(Failure::Corrupt.into()),
    };
    let info = Info {
        operation: OperationId(digest(op)?),
        generation,
        target,
        manifest_hash: digest(manifest)?,
        payload_sha256: digest(payload)?,
        size: u64::try_from(size).map_err(|_| Failure::Corrupt)?,
        guard: guard.map_or(Guard::Missing, Guard::Match),
        anchor: anchor.map(digest).transpose()?.map(OperationId),
        anchor_receipt: anchor_receipt.map(digest).transpose()?,
        intent_hash: digest(ih)?,
        phase: Phase::parse(&phase)?,
        reason: Reason::parse(&reason)?,
        observation: Observation::parse(&observation)?,
        rejection_status: status,
        revision,
        state_hash: digest(sh)?,
    };
    let retained: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM retained_remote_intents WHERE operation_id=?1)",
        [info.operation.0.as_slice()],
        |r| r.get(0),
    )?;
    let generation_data = journal::load(connection, generation)?;
    journal::validate_manifest(connection, &generation_data, false)?;
    if info.size > MAX_BODY_BYTES as u64
        || generation_data.phase != journal::Phase::Sealed
        || info.target != generation_data.target
        || info.size != generation_data.size
        || Some(info.manifest_hash) != generation_data.manifest_hash
        || retained != info.phase.retained()
        || info.intent_hash != intent_hash(&info)
        || info.state_hash != state_hash(&info)
        || info.anchor.is_some() != info.anchor_receipt.is_some()
        || guard_tag(&info.guard).is_some_and(|tag| !valid_etag(tag))
        || !valid_state(&info)
    {
        return Err(Failure::Corrupt.into());
    }
    Ok(info)
}
fn valid_state(i: &Info) -> bool {
    match i.phase {
        Phase::Prepared | Phase::InFlight | Phase::Cancelled => {
            i.reason == Reason::None
                && i.observation == Observation::None
                && i.rejection_status.is_none()
                && i.revision.is_none()
        }
        Phase::Uncertain => {
            i.reason != Reason::None && i.rejection_status.is_none() && i.revision.is_none()
        }
        Phase::Rejected => {
            i.reason == Reason::None
                && i.observation == Observation::None
                && matches!(i.rejection_status, Some(409 | 412))
                && i.revision.is_none()
        }
        Phase::Verified => {
            i.reason == Reason::None
                && i.observation == Observation::None
                && i.rejection_status.is_none()
                && i.revision.is_some()
        }
    }
}
fn update(connection: &Connection, info: &mut Info) -> Result<()> {
    let old = info.state_hash;
    info.state_hash = state_hash(info);
    let changed=connection.execute("UPDATE remote_intents SET phase=?1,reason=?2,observation=?3,rejection_status=?4,remote_etag=?5,remote_version=?6,state_hash=?7 WHERE operation_id=?8 AND state_hash=?9",params![info.phase.text(),info.reason.text(),info.observation.text(),info.rejection_status,info.revision.as_ref().map(|r|r.etag.as_str()),info.revision.as_ref().and_then(|r|r.version.as_deref()),info.state_hash.as_slice(),info.operation.0.as_slice(),old.as_slice()])?;
    if changed != 1 {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
fn digest(v: Vec<u8>) -> Result<[u8; 32]> {
    v.try_into().map_err(|_| Failure::Corrupt.into())
}
fn guard_tag(g: &Guard) -> Option<&str> {
    match g {
        Guard::Missing => None,
        Guard::Match(tag) => Some(tag),
    }
}
fn valid_etag(v: &str) -> bool {
    (3..=8192).contains(&v.len())
        && v.starts_with('"')
        && v.ends_with('"')
        && v[1..v.len() - 1]
            .bytes()
            .all(|b| b.is_ascii() && !b.is_ascii_control() && b != b'"')
}
fn field(hash: &mut Sha256, value: Option<&[u8]>) {
    hash.update([u8::from(value.is_some())]);
    if let Some(v) = value {
        hash.update((v.len() as u64).to_be_bytes());
        hash.update(v);
    }
}
fn intent_hash(i: &Info) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"s3drive-remote-intent-v1\0");
    h.update(i.operation.0);
    h.update(i.generation.value().to_be_bytes());
    h.update(i.target.source());
    field(&mut h, Some(i.target.fingerprint().as_bytes()));
    field(&mut h, Some(i.target.object_key().as_bytes()));
    h.update(i.manifest_hash);
    h.update(i.payload_sha256);
    h.update(i.size.to_be_bytes());
    field(&mut h, guard_tag(&i.guard).map(str::as_bytes));
    field(&mut h, i.anchor.as_ref().map(|id| id.0.as_slice()));
    field(
        &mut h,
        i.anchor_receipt.as_ref().map(|hash| hash.as_slice()),
    );
    h.finalize().into()
}
fn state_hash(i: &Info) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"s3drive-remote-state-v1\0");
    h.update(i.intent_hash);
    field(&mut h, Some(i.phase.text().as_bytes()));
    field(&mut h, Some(i.reason.text().as_bytes()));
    field(&mut h, Some(i.observation.text().as_bytes()));
    field(
        &mut h,
        i.rejection_status
            .as_ref()
            .map(|v| v.to_be_bytes())
            .as_ref()
            .map(|v| v.as_slice()),
    );
    field(&mut h, i.revision.as_ref().map(|r| r.etag.as_bytes()));
    field(
        &mut h,
        i.revision
            .as_ref()
            .and_then(|r| r.version.as_ref())
            .map(|v| v.as_bytes()),
    );
    h.finalize().into()
}

#[cfg(test)]
mod tests;
