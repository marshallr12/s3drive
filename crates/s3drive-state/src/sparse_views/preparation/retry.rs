//! Opt-in retry closure and revalidated retirement evidence. Never deletes data.
use super::*;
mod migration;
use crate::journal::{Base, GenerationId, Target};

/// Persist the complete token before an uncertain begin. A new epoch/nonce is a
/// new operation, not a retry. Distinct domains separate source and plan tokens.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    epoch: Id,
    nonce: Id,
}
impl fmt::Debug for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PreparationRetry([private])")
    }
}
impl Operation {
    pub fn new(binding: &Binding, nonce: Id) -> Result<Self> {
        if nonce == [0; 32] {
            return Err(Failure::Invalid.into());
        }
        Ok(Self {
            epoch: binding.epoch()?,
            nonce,
        })
    }
    pub fn epoch(&self) -> Id {
        self.epoch
    }
    pub fn id(&self) -> Result<Id> {
        if self.epoch == [0; 32] || self.nonce == [0; 32] {
            return Err(Failure::Invalid.into());
        }
        hash(b"s3drive-preparation-retry-v1\0", self)
    }
    pub fn generation(&self) -> Result<[u8; 16]> {
        let id = hash(b"s3drive-preparation-generation-v1\0", &self.id()?)?;
        Ok(id[..16].try_into().map_err(|_| Failure::Invalid)?)
    }
    pub fn publish_operation(&self) -> Result<String> {
        let id = hash(b"s3drive-preparation-publication-v1\0", &self.id()?)?;
        Ok(id[..16].iter().map(|b| format!("{b:02x}")).collect())
    }
    pub(super) fn matches(&self, input: &Input, binding: &Binding) -> Result<()> {
        if self.epoch != binding.epoch()?
            || self.id()? != input.operation
            || self.generation()? != input.generation
        {
            return Err(Failure::Corrupt.into());
        }
        Ok(())
    }
}
fn mode_hash() -> Result<Id> {
    hash(b"s3drive-retry-mode-v1\0", &1u8)
}
/// Called only inside atomic fresh mounted-owner admission, after its role is
/// installed. There is no existing-owner conversion or data rewrite here.
pub(crate) fn install_fresh(db: &Connection) -> Result<()> {
    db.execute(
        "INSERT INTO sparse_scoped_mode VALUES(1,1,?1)",
        [super::super::scoped::mode_hash()?.as_slice()],
    )?;
    db.execute(
        "INSERT INTO sparse_retry_mode VALUES(1,1,?1)",
        [mode_hash()?.as_slice()],
    )?;
    if !enabled(db)? {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
pub(crate) fn enabled(db: &Connection) -> Result<bool> {
    enabled_in(db, None)
}
pub(crate) fn enabled_scoped(scope: &crate::chunk_mount::ReadScope<'_>) -> Result<bool> {
    enabled_in(scope.connection(), Some(scope))
}
fn enabled_in(db: &Connection, scope: Option<&crate::chunk_mount::ReadScope<'_>>) -> Result<bool> {
    let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='sparse_retry_mode')", [], |r| r.get(0))?;
    if !exists {
        let v: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        return if v < 49 {
            Ok(false)
        } else {
            Err(Failure::Corrupt.into())
        };
    }
    let mut q = db.prepare("SELECT singleton,format,state_hash FROM sparse_retry_mode LIMIT 2")?;
    let mut rows = q.query([])?;
    let Some(row) = rows.next()? else {
        return Ok(false);
    };
    let (id, format, sha): (i64, i64, Vec<u8>) = (row.get(0)?, row.get(1)?, row.get(2)?);
    let v: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if id != 1
        || format != 1
        || sha != mode_hash()?
        || rows.next()?.is_some()
        || v < 49
        || !match scope {
            Some(scope) => super::super::scoped::enabled_scoped(scope)?,
            None => super::super::scoped::enabled(db)?,
        }
    {
        return Err(Failure::Corrupt.into());
    }
    Ok(true)
}
pub(super) fn validate_snapshot(db: &Connection, s: &ledger::Snapshot) -> Result<()> {
    validate_snapshot_in(s, enabled(db)?)
}
pub(super) fn validate_snapshot_scoped(
    scope: &crate::chunk_mount::ReadScope<'_>,
    s: &ledger::Snapshot,
) -> Result<()> {
    validate_snapshot_in(s, enabled_scoped(scope)?)
}
fn validate_snapshot_in(s: &ledger::Snapshot, on: bool) -> Result<()> {
    if s.plans.values().any(|p| p.input.retry.is_some()) && !on {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
fn current(s: &ledger::Snapshot, info: &Info) -> Result<()> {
    let t = info.input.retry.ok_or(Failure::ScopedRequired)?;
    t.matches(&info.input, info.view.binding())?;
    let f = s
        .views
        .files
        .get(&info.view.binding().target()?)
        .ok_or(Failure::Corrupt)?;
    if f.binding.epoch()? != t.epoch {
        return Err(Failure::EpochClosed.into());
    }
    Ok(())
}
pub(super) fn admit(
    db: &Connection,
    s: &ledger::Snapshot,
    input: &Input,
    view: &View,
) -> Result<()> {
    match (enabled(db)?, input.retry) {
        (false, None) => Ok(()),
        (false, Some(_)) => Err(Failure::ScopedDisabled.into()),
        (true, None) => Err(Failure::ScopedRequired.into()),
        (true, Some(t)) => {
            t.matches(input, view.binding())?;
            if s.views
                .files
                .get(&view.binding().target()?)
                .ok_or(Failure::Corrupt)?
                .binding
                .epoch()?
                != t.epoch
            {
                return Err(Failure::EpochClosed.into());
            }
            Ok(())
        }
    }
}
pub(super) fn admit_root(
    db: &Connection,
    s: &ledger::Snapshot,
    info: &Info,
    operation: &str,
) -> Result<()> {
    if !enabled(db)? {
        return Ok(());
    }
    current(s, info)?;
    if operation
        != info
            .input
            .retry
            .ok_or(Failure::ScopedRequired)?
            .publish_operation()?
    {
        return Err(Failure::Conflict.into());
    }
    Ok(())
}
/// Unknown generic staging IDs cannot revive a closed preparation. The only
/// fresh admissible ID is derived from an exact retained current-epoch plan.
pub(crate) fn admit_staging(
    db: &Connection,
    operation: Option<Id>,
    target: &Target,
    base: &Base,
    parent: Option<GenerationId>,
    size: u64,
) -> Result<()> {
    if !enabled(db)? {
        return Ok(());
    }
    let operation = operation.ok_or(Failure::ScopedRequired)?;
    if *base != Base::Missing || parent.is_some() {
        return Err(Failure::Conflict.into());
    }
    let s = ledger::inspect(db)?;
    for (plan, level, slot) in s.objects.keys() {
        let i = s.info(*plan)?;
        if i.root()?.is_none() || i.input.retry.is_none() {
            continue;
        }
        if let Some(t) = transfers::transfer(
            db,
            &s,
            &i,
            Position {
                level: *level,
                slot: *slot,
            },
        )? && t.operation == operation
        {
            current(&s, &i)?;
            if t.target() != target || t.size() != size {
                return Err(Failure::Conflict.into());
            }
            return Ok(());
        }
    }
    Err(Failure::ScopedRequired.into())
}
pub(crate) fn admit_remote(db: &Connection, generation: GenerationId) -> Result<()> {
    if !enabled(db)? {
        return Ok(());
    }
    let s = ledger::inspect(db)?;
    for (plan, level, slot) in s.objects.keys() {
        let i = s.info(*plan)?;
        if i.root()?.is_none() || i.input.retry.is_none() {
            continue;
        }
        if let Some(t) = transfers::transfer(
            db,
            &s,
            &i,
            Position {
                level: *level,
                slot: *slot,
            },
        )? && t.generation() == Some(generation)
        {
            return current(&s, &i);
        }
    }
    Err(Failure::ScopedRequired.into())
}
impl Journal<'_> {
    /// Permanent explicit opt-in. No defaults, quotas, records or readers change.
    pub fn enable_retry_safety(&mut self) -> Result<()> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let before = ledger::inspect(&tx)?;
        if tx.query_row("SELECT EXISTS(SELECT 1 FROM chunk_mount_role)", [], |r| {
            r.get::<_, bool>(0)
        })? {
            return Err(Failure::Isolated.into());
        }
        if enabled(&tx)? {
            return Ok(());
        }
        for table in [
            "sparse_preparations",
            "staged_generations",
            "remote_intents",
            "sparse_publications",
        ] {
            if tx.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| {
                r.get::<_, bool>(0)
            })? {
                return Err(Failure::Isolated.into());
            }
        }
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger')",
            [],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(Failure::Corrupt.into());
        }
        if !super::super::scoped::enabled(&tx)? {
            tx.execute(
                "INSERT INTO sparse_scoped_mode VALUES(1,1,?1)",
                [super::super::scoped::mode_hash()?.as_slice()],
            )?;
        }
        tx.execute(
            "INSERT INTO sparse_retry_mode VALUES(1,1,?1)",
            [mode_hash()?.as_slice()],
        )?;
        journal::checkpoint("retry_enable", "Stored");
        if !enabled(&tx)? || ledger::inspect(&tx)? != before {
            return Err(Failure::Corrupt.into());
        }
        journal::checkpoint("retry_enable", "Validated");
        tx.commit()?;
        journal::checkpoint("retry_enable", "Committed");
        Ok(())
    }
    pub fn begin_scoped(&mut self, token: Operation, request: Request) -> Result<Info> {
        journal::request_bounds(&request)?;
        // Never silently replace a caller's persisted identities.
        if request.operation != token.id()? || request.generation != token.generation()? {
            return Err(Failure::Conflict.into());
        }
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (info, changed) = journal::begin_in_transaction(
            &tx,
            self.options,
            self.state.incarnation.as_deref(),
            &self.state.native_sparse,
            request,
            journal::Admission {
                retry: Some(token),
                ..Default::default()
            },
        )?;
        if changed {
            tx.commit()?;
            journal::checkpoint("begin", "Committed");
        }
        Ok(info)
    }
}
impl Client {
    pub async fn enable_retry_safety(&self) -> Result<()> {
        self.call(|j| j.enable_retry_safety()).await
    }
    pub async fn begin_scoped(&self, token: Operation, request: Request) -> Result<Info> {
        self.call(move |j| j.begin_scoped(token, request)).await
    }
}

/// Immutable observational proof, not a cut-edge permission or a flush receipt.
/// No deserializer/public fields: verification always reloads the original
/// evidence in one SQLite snapshot. A successful proof never relaxes validators.
#[derive(Clone, PartialEq, Eq)]
pub struct Checkpoint {
    plan: Id,
    anchor: Id,
    closed_epoch: Id,
    terminal_plans: Vec<Id>,
    blocked_plans: Vec<Id>,
    protected_views: Vec<Id>,
    protected_generations: Vec<GenerationId>,
    evidence: Id,
}
impl fmt::Debug for Checkpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RetryCheckpoint([historical; no deletion permission])")
    }
}
impl Checkpoint {
    pub fn plan(&self) -> Id {
        self.plan
    }
    pub fn anchor(&self) -> Id {
        self.anchor
    }
    pub fn closed_epoch(&self) -> Id {
        self.closed_epoch
    }
    pub fn terminal_plans(&self) -> &[Id] {
        &self.terminal_plans
    }
    pub fn blocked_plans(&self) -> &[Id] {
        &self.blocked_plans
    }
    pub fn protected_views(&self) -> &[Id] {
        &self.protected_views
    }
    pub fn protected_generations(&self) -> &[GenerationId] {
        &self.protected_generations
    }
    /// Necessary local retirement conditions only. Actual retirement is absent.
    pub fn unblocked(&self) -> bool {
        self.blocked_plans.is_empty()
            && self.protected_views.is_empty()
            && self.protected_generations.is_empty()
    }
}
pub(in crate::sparse_views::preparation) fn checkpoint(
    db: &Connection,
    plan: Id,
    physical_readers: &[GenerationId],
) -> Result<Checkpoint> {
    if !enabled(db)? {
        return Err(Failure::ScopedDisabled.into());
    }
    let s = ledger::inspect(db)?;
    let meta = publication::ledger::inspect(db)?;
    publication::ledger::validate_source_snapshot(db, &meta, &s)?;
    let info = s.info(plan)?;
    let token = info.input.retry.ok_or(Failure::ScopedRequired)?;
    let record = meta.records.get(&plan).ok_or(Failure::NotFound)?;
    let ack = record.acknowledgment().ok_or(Failure::Conflict)?;
    let anchor = ack.clean_view().ok_or(Failure::Conflict)?;
    let file = s
        .views
        .files
        .get(&info.view.binding().target()?)
        .ok_or(Failure::Corrupt)?;
    let clean = s.views.record(anchor)?;
    // A historical acknowledgment with a newer dirty head is NOT a clean cut.
    if file.head != anchor
        || clean.request.binding != file.binding
        || clean.parent != Some(info.view.id())
        || file.binding.epoch()? == token.epoch
    {
        return Err(Failure::Conflict.into());
    }
    let native_terminal = if crate::chunk_mount::retry_enabled(db)? {
        let base = info.view.binding().base()?;
        crate::namespace_client::opens::native_candidates(db, base.volume(), base.node()).is_ok()
    } else {
        false
    };
    let mut terminal_plans = Vec::new();
    let mut blocked_plans = Vec::new();
    for (id, saved) in &s.plans {
        let i = saved.info(&s.views)?;
        if i.view.binding().target()? != info.view.binding().target()?
            || i.view.binding().epoch()? != token.epoch
        {
            continue;
        }
        // Native/owned history still carries independent open/epoch dependency
        // edges. Do not certify their retirement with a source-only proof.
        let terminal = i.input.retry.is_some()
            && i.ownership().is_none()
            && (i.native_ownership().is_none() || native_terminal)
            && meta
                .records
                .get(id)
                .is_some_and(|r| r.receipt().is_some() && r.acknowledgment().is_some());
        if terminal {
            // Whole-tree proof includes every dependency and verified physical
            // receipt. A prepared/claimed/unknown upload never counts as terminal.
            transfers::inspect_tree(db, *id)?;
            terminal_plans.push(*id);
        } else {
            blocked_plans.push(*id);
        }
    }
    let protected_views = s
        .views
        .read_pins
        .pins
        .values()
        .filter_map(|pin| {
            // All pins on this target remain protective, including the clean anchor.
            match s.views.record(pin.view_id()) {
                Ok(v) if v.request.binding.target().ok() == info.view.binding().target().ok() => {
                    Some(Ok(pin.view_id()))
                }
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let mut protected_generations = Vec::new();
    for (id, saved) in &s.plans {
        let i = saved.info(&s.views)?;
        if i.view.binding().target()? != info.view.binding().target()? || i.root()?.is_none() {
            continue;
        }
        for (_, level, slot) in s.objects.keys().filter(|(p, _, _)| p == id) {
            if let Some(t) = transfers::transfer(
                db,
                &s,
                &i,
                Position {
                    level: *level,
                    slot: *slot,
                },
            )? && let Some(generation) = t.generation()
                && physical_readers.contains(&generation)
                && !protected_generations.contains(&generation)
            {
                protected_generations.push(generation);
            }
        }
    }
    if let Some(anchor) = s.views.anchors.get(&info.view.binding().target()?) {
        let generation = anchor.generation();
        if physical_readers.contains(&generation) && !protected_generations.contains(&generation) {
            protected_generations.push(generation);
        }
    }
    protected_generations.sort_by_key(|g| g.value());
    // Exact retained graph/body hashes and root records bind the proof; neither
    // a copied row hash nor a self-consistent rewritten anchor is accepted alone.
    let evidence = hash(
        b"s3drive-retry-checkpoint-v1\0",
        &(
            plan,
            anchor,
            &file.raw,
            &terminal_plans,
            &blocked_plans,
            &protected_views,
            protected_generations
                .iter()
                .map(|g| g.value())
                .collect::<Vec<_>>(),
            s.views.records.values().map(|v| &v.raw).collect::<Vec<_>>(),
            s.plans
                .values()
                .map(|p| (&p.input_raw, &p.state_raw))
                .collect::<Vec<_>>(),
            meta.records.values().collect::<Vec<_>>(),
        ),
    )?;
    Ok(Checkpoint {
        plan,
        anchor,
        closed_epoch: token.epoch,
        terminal_plans,
        blocked_plans,
        protected_views,
        protected_generations,
        evidence,
    })
}
impl Journal<'_> {
    pub fn checkpoint(&mut self, plan: Id) -> Result<Checkpoint> {
        let physical_readers = self.state.staging_readers.active();
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let result = checkpoint(&tx, plan, &physical_readers)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn verify_checkpoint(&mut self, proof: &Checkpoint) -> Result<()> {
        if self.checkpoint(proof.plan)? != *proof {
            return Err(Failure::Conflict.into());
        }
        Ok(())
    }
}
impl Client {
    pub async fn checkpoint(&self, plan: Id) -> Result<Checkpoint> {
        self.call(move |j| j.checkpoint(plan)).await
    }
    pub async fn verify_checkpoint(&self, proof: Checkpoint) -> Result<()> {
        self.call(move |j| j.verify_checkpoint(&proof)).await
    }
}

/// Check before source registration, so rejected raw staging cannot mutate even
/// the binding registry. Repeat fresh admission inside the write transaction.
pub(crate) fn preflight_staging(
    db: &Connection,
    operation: Option<Id>,
    target: &Target,
    base: &Base,
    parent: Option<GenerationId>,
    size: u64,
) -> Result<()> {
    if !enabled(db)? {
        return Ok(());
    }
    if let Some(op) = operation {
        use rusqlite::OptionalExtension;
        let id: Option<i64> = db
            .query_row(
                "SELECT generation_id FROM staged_operations WHERE operation_key=?1",
                [op.as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = id {
            let g = crate::journal::load(db, GenerationId::from_database(id)?)?;
            if g.target != *target || g.base != *base || g.parent != parent || g.size != size {
                return Err(Failure::Conflict.into());
            }
            return Ok(());
        }
    }
    admit_staging(db, operation, target, base, parent, size)
}
