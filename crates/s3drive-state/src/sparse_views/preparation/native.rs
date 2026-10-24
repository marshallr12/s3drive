//! Original native source-epoch identity and exact Close finalization.
//! Local intent only. This capability never authorizes a provider operation.
use super::*;
use crate::namespace_client::opens::{self, Action, OpenRecord};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

const MAX_CONTEXT_BYTES: usize = 16 * 1024 * 1024;
const MAX_NATIVE_OPENS: usize = 256;

/// Immutable original-open witness, independent of removal ownership.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    open: String,
    session: String,
    owner: String,
    input_hash: Id,
    epoch: Id,
    operation: Id,
}
impl fmt::Debug for Provenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NativeSparseProvenance([private])")
    }
}
impl Provenance {
    pub fn open(&self) -> &str {
        &self.open
    }
    pub fn epoch(&self) -> Id {
        self.epoch
    }
    fn capture(record: &OpenRecord, binding: &Binding, operation: Id) -> Result<Self> {
        Ok(Self {
            open: record.input.id.clone(),
            session: record.input.session.clone(),
            owner: record.owner.clone(),
            input_hash: hash(b"s3drive-native-sparse-open-v1\0", &record.input)?,
            epoch: binding.epoch()?,
            operation,
        })
    }
    fn historical(&self, db: &Connection, binding: &Binding) -> Result<OpenRecord> {
        let record = opens::sparse_open(db, &self.open)?;
        self.historical_record(&record, binding)?;
        Ok(record)
    }
    // A fully loaded original record from this immutable validation command.
    // This only avoids repeated replay of its identical event chain; all
    // provenance comparisons below remain mandatory for every source/plan.
    fn historical_record(&self, record: &OpenRecord, binding: &Binding) -> Result<()> {
        let base = binding.base()?;
        if record.input.id != self.open || !record.active() || !record.input.controlled || !record.input.session_bound
            || record.input.session != self.session || record.owner != self.owner
            || record.input.volume != base.volume() || record.input.node != base.node()
            || record.input.source != *binding.source()
            || record.input.fingerprint != binding.fingerprint()
            || record.input.store.id != hex(binding.store()) || record.input.store.root != binding.root()
            || hash(b"s3drive-native-sparse-open-v1\0", &record.input)? != self.input_hash
            || self.epoch != binding.epoch()? || self.operation == [0; 32]
            || !record.events.iter().any(|e| matches!(&e.action,
                Action::OwnSparseEpoch { binding: b, operation, .. } if b.as_ref() == binding && *operation == self.operation))
        { return Err(Failure::Corrupt.into()); }
        Ok(())
    }
}

/// Structurally checked original-root intent. The SDK must obtain it from its
/// authenticated edited reader and independently reauthorize all later sends.
#[derive(Clone)]
pub struct Context {
    binding: Binding,
    view: Id,
    root: Vec<u8>,
    page: Vec<u8>,
}
impl fmt::Debug for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NativeSparseContext([untrusted intent])")
    }
}
impl Context {
    pub fn new(view: &View, root: Vec<u8>, page: Vec<u8>) -> Result<Self> {
        let binding = view.binding().clone();
        binding.validate()?;
        let base = binding.base()?;
        let descriptor = RootDescriptor::decode(&root)?;
        if page.is_empty()
            || page.len() > manifest::CHUNK_BYTES as usize
            || hex(descriptor.volume()) != base.volume()
            || hex(descriptor.node()) != base.node()
            || descriptor.length() != binding.base_size()
            || descriptor.page().store() != binding.store()
        {
            return Err(Failure::Conflict.into());
        }
        proof::scope(&descriptor.verify_page(&page)?, binding.store())?;
        Ok(Self {
            binding,
            view: view.id(),
            root,
            page,
        })
    }
    fn bytes(&self) -> usize {
        self.root.len() + self.page.len()
    }
}

#[derive(Default)]
struct CloseSignal {
    observed: AtomicBool,
    callback: OnceLock<String>,
}
impl CloseSignal {
    fn load(&self, order: Ordering) -> bool {
        self.observed.load(order)
    }
    fn store(&self, value: bool, order: Ordering) {
        self.observed.store(value, order);
    }
}

/// Owned by one exclusive State, never persisted or recreated for an old runtime.
#[derive(Default)]
pub(crate) struct Runtime {
    tokens: BTreeMap<String, Arc<CloseSignal>>,
    contexts: BTreeMap<Id, Context>,
}
impl Runtime {
    fn token(&mut self, db: &Connection, open: &str) -> Result<Arc<CloseSignal>> {
        if let Some(token) = self.tokens.get(open) {
            return Ok(token.clone());
        }
        if self.tokens.len() >= MAX_NATIVE_OPENS {
            self.prune_retired(db)?;
        }
        if self.tokens.len() >= MAX_NATIVE_OPENS {
            return Err(Failure::Quota.into());
        }
        let token = Arc::new(CloseSignal::default());
        self.tokens.insert(open.into(), token.clone());
        Ok(token)
    }
    pub(super) fn prune_retired(&mut self, db: &Connection) -> Result<()> {
        let mut retired = Vec::new();
        for open in self.tokens.keys() {
            if opens::sparse_retired(db, open)? {
                retired.push(open.clone());
            }
        }
        for open in retired {
            self.tokens.remove(&open);
        }
        Ok(())
    }
    fn closing(&self, open: &str) -> Result<bool> {
        Ok(self
            .tokens
            .get(open)
            .ok_or(Failure::Conflict)?
            .load(Ordering::Acquire))
    }
    fn remember(&mut self, context: Context) -> Result<()> {
        let target = context.binding.target()?;
        let total: usize = self
            .contexts
            .iter()
            .filter(|(id, _)| **id != target)
            .map(|(_, c)| c.bytes())
            .sum();
        if total
            .checked_add(context.bytes())
            .is_none_or(|n| n > MAX_CONTEXT_BYTES)
            || (!self.contexts.contains_key(&target) && self.contexts.len() >= HARD_FILES as usize)
        {
            return Err(Failure::Quota.into());
        }
        self.contexts.insert(target, context);
        Ok(())
    }
}

/// Reject new wire forms before advancing an older marker. No old row is rewritten.
pub(crate) fn require_pre48(db: &Connection) -> Result<()> {
    let mut q = db.prepare("SELECT CASE WHEN typeof(input)='blob' AND length(input) BETWEEN 1 AND 65536 THEN input END FROM sparse_preparations LIMIT 257")?;
    for (index, raw) in q.query_map([], |r| r.get::<_, Vec<u8>>(0))?.enumerate() {
        if index >= 256 {
            return Err(Failure::Corrupt.into());
        }
        let value: serde_json::Value = serde_json::from_slice(&raw?)?;
        if value.get("native").is_some() {
            return Err(Failure::Corrupt.into());
        }
    }
    let mut q = db.prepare("SELECT CASE WHEN typeof(action)='blob' AND length(action) BETWEEN 1 AND 16384 THEN action END FROM windows_open_events LIMIT 4097")?;
    for (index, raw) in q.query_map([], |r| r.get::<_, Vec<u8>>(0))?.enumerate() {
        if index >= 4096 {
            return Err(Failure::Corrupt.into());
        }
        let value: serde_json::Value = serde_json::from_slice(&raw?)?;
        if matches!(
            value.get("type").and_then(|v| v.as_str()),
            Some("bind-sparse" | "complete-sparse" | "own-sparse-epoch" | "freeze-sparse")
        ) {
            return Err(Failure::Corrupt.into());
        }
    }
    Ok(())
}

pub(super) fn validate_snapshot(db: &Connection, snapshot: &ledger::Snapshot) -> Result<()> {
    let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let mut originals = BTreeMap::new();
    for saved in snapshot.plans.values() {
        let info = saved.info(&snapshot.views)?;
        if let Some(p) = info.native_ownership() {
            if version < 48 {
                return Err(Failure::Corrupt.into());
            }
            if !originals.contains_key(&p.open) {
                originals.insert(p.open.clone(), opens::sparse_open(db, &p.open)?);
            }
            let record = originals.get(&p.open).ok_or(Failure::Corrupt)?;
            p.historical_record(record, info.view().binding())?;
            if info
                .ownership()
                .is_some_and(|owned| owned.original_open() != &record.input)
            {
                return Err(Failure::Corrupt.into());
            }
        }
    }
    Ok(())
}

pub(super) fn fresh(
    db: &Connection,
    incarnation: Option<&str>,
    runtime: &Runtime,
    info: &Info,
) -> Result<()> {
    metadata::validate(db)?;
    let Some(p) = info.native_ownership() else {
        if epoch_owner(db, info.view().binding())?.is_some()
            || bound_node(db, info.view().binding())?
        {
            return Err(Failure::Conflict.into());
        }
        return Ok(());
    };
    let record = p.historical(db, info.view().binding())?;
    opens::sparse_original_session(db, incarnation.ok_or(Failure::Conflict)?, &record)?;
    let closing = runtime.closing(&p.open)?;
    if let Some(close) = record.close() {
        if !closing || record.completed(&close.id)
            || !record.events.iter().any(|e| matches!(&e.action,
                Action::FreezeSparse { callback, plan } if callback == &close.id && info.id().ok() == Some(*plan)))
        { return Err(Failure::Conflict.into()); }
    } else if closing {
        return Err(Failure::Conflict.into());
    }
    Ok(())
}

/// One derivation for admission and independent metadata verification. Scoped
/// retry identity includes the exact original open witness and source view.
fn plan_identity(
    db: &Connection,
    native: &Provenance,
    binding: &Binding,
    view: Id,
) -> Result<(Id, [u8; 16], Option<retry::Operation>)> {
    let nonce = hash(b"s3drive-native-sparse-plan-v1\0", &(native, view))?;
    if crate::chunk_mount::retry_enabled(db)? {
        if !retry::enabled(db)? {
            return Err(Failure::Corrupt.into());
        }
        let token = retry::Operation::new(binding, nonce)?;
        Ok((token.id()?, token.generation()?, Some(token)))
    } else {
        if retry::enabled(db)? {
            return Err(Failure::Isolated.into());
        }
        Ok((
            nonce,
            hash(b"s3drive-native-sparse-generation-v1\0", &nonce)?[..16]
                .try_into()
                .map_err(|_| Failure::Corrupt)?,
            None,
        ))
    }
}

fn epoch_owner(db: &Connection, binding: &Binding) -> Result<Option<(Provenance, OpenRecord)>> {
    let epoch = binding.epoch()?;
    let mut result = None;
    for record in opens::sparse_records(db)? {
        for event in &record.events {
            if let Action::OwnSparseEpoch {
                binding: b,
                operation,
                ..
            } = &event.action
                && b.epoch()? == epoch
            {
                if b.as_ref() != binding || result.is_some() {
                    return Err(Failure::Corrupt.into());
                }
                result = Some((
                    Provenance::capture(&record, binding, *operation)?,
                    record.clone(),
                ));
            }
        }
    }
    Ok(result)
}

fn epoch_context(record: &OpenRecord, binding: &Binding) -> Result<(Vec<u8>, Id)> {
    record
        .events
        .iter()
        .find_map(|e| match &e.action {
            Action::OwnSparseEpoch {
                binding: b,
                base_root,
                base_page,
                ..
            } if b.as_ref() == binding => Some((base_root.clone(), *base_page)),
            _ => None,
        })
        .ok_or_else(|| Failure::Corrupt.into())
}
// Actual SQLite allocation failure at a transaction boundary. Test-only and
// connection-local: no triggers, global environment changes, or guard bypass.
#[cfg(test)]
pub(crate) fn full_probe(db: &Connection) -> Result<()> {
    let enabled: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='native_sparse_full_probe')", [], |r| r.get(0))?;
    if enabled {
        let pages: i64 = db.pragma_query_value(None, "page_count", |r| r.get(0))?;
        let size: i64 = db.pragma_query_value(None, "page_size", |r| r.get(0))?;
        db.execute(
            "INSERT INTO native_sparse_full_probe VALUES(zeroblob(?))",
            [(pages + 1) * size],
        )?;
    }
    Ok(())
}

pub(super) fn admit(
    db: &Connection,
    incarnation: Option<&str>,
    runtime: &Runtime,
    view: &View,
    supplied: Option<&Provenance>,
    finalizing: bool,
) -> Result<()> {
    let owner = epoch_owner(db, view.binding())?;
    match (owner, supplied) {
        (None, None) if !finalizing && !bound_node(db, view.binding())? => Ok(()),
        (Some((p, record)), Some(supplied)) if &p == supplied => {
            opens::sparse_current(db, incarnation.ok_or(Failure::Conflict)?, &record)?;
            if runtime.closing(&p.open)? != finalizing {
                return Err(Failure::Conflict.into());
            }
            Ok(())
        }
        _ => Err(Failure::Conflict.into()),
    }
}

/// Original file-object token. Clones share Close observation, not a new open.
#[derive(Clone)]
pub struct Token {
    input: opens::OpenInput,
    key: crate::chunk_mount::Key,
    closing: Arc<CloseSignal>,
}
impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NativeSparseToken([private])")
    }
}
impl Token {
    pub fn input(&self) -> &opens::OpenInput {
        &self.input
    }
    /// Observed fact only; not persistence, completion, or finalization permission.
    pub fn observe_close(&self) {
        self.closing.store(true, Ordering::Release);
    }
    /// Retain the exact callback identity even if enqueue, commit, or the
    /// separate observed-fact write fails. Call synchronously at adapter entry.
    pub fn observe_close_as(&self, callback: &str) -> Result<()> {
        self.observe_close();
        if callback.len() != 32
            || !callback
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            || callback.bytes().all(|b| b == b'0')
            || self.closing.callback.get_or_init(|| callback.to_owned()) != callback
        {
            return Err(Failure::Conflict.into());
        }
        Ok(())
    }
    fn validate(&self, db: &Connection, runtime: &Runtime) -> Result<OpenRecord> {
        let record = opens::sparse_open(db, &self.input.id)?;
        if record.input != self.input
            || !runtime
                .tokens
                .get(&self.input.id)
                .is_some_and(|t| Arc::ptr_eq(t, &self.closing))
        {
            return Err(Failure::Conflict.into());
        }
        crate::chunk_mount::require(db, &self.key)?.open(&record.input)?;
        Ok(record)
    }
}

mod mutations;
pub(in crate::sparse_views) use mutations::{Mutation, before_mutation, checkpoint};

mod client;
pub use client::Client;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
mod close;
pub use close::Close;

fn is_bound(record: &OpenRecord) -> bool {
    record
        .events
        .iter()
        .any(|event| matches!(event.action, Action::BindSparse))
}
mod metadata;
#[cfg(test)]
pub(super) use metadata::epoch_source_visits;
pub(crate) use metadata::validate as validate_metadata;
pub(in crate::sparse_views) use metadata::validate_retirement;

pub(crate) fn reserved_events<'a>(
    db: &Connection,
    records: impl Iterator<Item = &'a OpenRecord>,
) -> Result<usize> {
    let bound = records
        .filter(|r| is_bound(r) && r.close().is_none_or(|e| !r.completed(&e.id)))
        .collect::<Vec<_>>();
    if bound.is_empty() {
        return Ok(0);
    }
    let plans = metadata::headers(db)?;
    let mut slots = 0;
    for record in bound {
        let frozen = record
            .events
            .iter()
            .filter_map(|e| match e.action {
                Action::FreezeSparse { plan, .. } => Some(plan),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        slots += usize::from(record.close().is_none()); // one final head
        slots += usize::from(
            !record
                .events
                .iter()
                .any(|e| matches!(e.action, Action::CompleteSparse { .. })),
        );
        slots += plans
            .iter()
            .filter(|(id, p)| {
                p.input
                    .native
                    .as_ref()
                    .is_some_and(|p| p.open == record.input.id)
                    && !frozen.contains(*id)
            })
            .count();
    }
    Ok(slots)
}
fn bound_node(db: &Connection, binding: &Binding) -> Result<bool> {
    let base = binding.base()?;
    Ok(opens::sparse_records(db)?
        .iter()
        .any(|r| is_bound(r) && r.input.volume == base.volume() && r.input.node == base.node()))
}

fn frozen_hash(record: &OpenRecord) -> Result<Id> {
    let frozen = record
        .events
        .iter()
        .filter_map(|event| match event.action {
            Action::FreezeSparse { plan, .. } => Some(plan),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    hash(b"s3drive-native-sparse-completion-plans-v1\0", &frozen)
}

/// Session abandonment is not native Close completion. Only a clean source and
/// settled whole-node roots can retire the old mutation fence; unknown dirty
/// epochs remain retained even when no preparation was admitted before a crash.
pub(crate) fn recovery_ready(db: &Connection, record: &OpenRecord) -> Result<bool> {
    if !is_bound(record) {
        return Ok(false);
    }
    metadata::validate(db)?;
    publication::namespace_pending(db, &record.input.volume, &record.input.node)?;
    let source = super::super::ledger::inspect_metadata(db)?;
    for file in source.files.values() {
        let base = file.binding.base()?;
        if base.volume() == record.input.volume && base.node() == record.input.node {
            if !source
                .records
                .get(&file.head)
                .ok_or(Failure::Corrupt)?
                .record
                .clean()
            {
                return Err(Failure::Conflict.into());
            }
            return Ok(true);
        }
    }
    if record
        .events
        .iter()
        .any(|e| matches!(e.action, Action::OwnSparseEpoch { .. }))
    {
        return Err(Failure::Corrupt.into());
    }
    Ok(true)
}

impl Provenance {
    pub(in crate::sparse_views) fn validate_terminal(
        &self,
        db: &Connection,
        original: &opens::OpenInput,
        owner: &str,
        binding: &Binding,
        first_operation: Id,
    ) -> Result<()> {
        let mount = crate::chunk_mount::load(db)?.ok_or(Failure::Corrupt)?;
        mount.open(original)?;
        let base = binding.base()?;
        if original.volume != base.volume()
            || original.node != base.node()
            || original.source != *binding.source()
            || original.fingerprint != binding.fingerprint()
            || original.store.id != hex(binding.store())
            || original.store.root != binding.root()
        {
            return Err(Failure::Corrupt.into());
        }
        if self.open != original.id
            || self.session != original.session
            || self.owner != owner
            || owner != mount.owner
            || !original.controlled
            || !original.session_bound
            || original.admission.is_some()
            || original.creation.is_some()
            || original.delete_on_close
            || self.input_hash != hash(b"s3drive-native-sparse-open-v1\0", original)?
            || self.epoch != binding.epoch()?
            || self.operation != first_operation
        {
            return Err(Failure::Corrupt.into());
        }
        crate::chunk_mount::admission::retained(
            db,
            crate::chunk_mount::AdmissionDomain::Open,
            &original.id,
        )?;
        crate::chunk_mount::admission::retained(
            db,
            crate::chunk_mount::AdmissionDomain::Session,
            &original.session,
        )?;
        Ok(())
    }
}
pub(in crate::sparse_views) fn verify_terminal_plan(db: &Connection, info: &Info) -> Result<()> {
    let native = info.native_ownership().ok_or(Failure::Corrupt)?;
    let (operation, generation, retry) =
        plan_identity(db, native, info.view().binding(), info.view().id())?;
    if info.operation() != operation
        || info.input.generation != generation
        || info.input.retry != retry
    {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
impl Runtime {
    pub(in crate::sparse_views) fn forget_retired(&mut self, records: &[opens::OpenRecord]) {
        for record in records {
            self.tokens.remove(&record.input.id);
        }
    }
}
