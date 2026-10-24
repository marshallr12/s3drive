//! Typed original-open provenance and explicit local effect settlement.
//! Retained facts never authorize a fresh provider request after restart.
use super::*;
use crate::{chunk_mount, namespace as ns, namespace_client as names};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    open: names::opens::OpenInput,
    owner: String,
    claim: ns::RemovalPublicationClaim,
    original: names::Input,
    original_receipt: ns::Receipt,
    event: names::Input,
    event_receipt: ns::Receipt,
}
impl fmt::Debug for Provenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SparseOwnership([retained original open])")
    }
}
impl Provenance {
    pub(super) fn same_epoch(&self, other: &Self) -> bool {
        self.open == other.open
            && self.owner == other.owner
            && self.claim.fence == other.claim.fence
            && self.original == other.original
            && self.original_receipt == other.original_receipt
    }
    pub fn original_open(&self) -> &names::opens::OpenInput {
        &self.open
    }
    pub fn claim(&self) -> &ns::RemovalPublicationClaim {
        &self.claim
    }
    pub fn event_sequence(&self) -> u64 {
        self.event_receipt.sequence
    }
    pub fn owner(&self) -> &str {
        &self.owner
    }
    pub(super) fn validate(&self, binding: &Binding) -> Result<()> {
        let base = binding.base()?;
        if !self.claim.fence.valid()
            || self.claim.fence.owner != self.owner
            || self.open.volume != base.volume()
            || self.open.node != base.node()
            || self.open.store != self.original.store
            || self.open.source != self.original.source
            || self.open.fingerprint != self.original.fingerprint
            || self.open.source != *binding.source()
            || self.open.fingerprint != binding.fingerprint()
            || !self.open.session_bound
            || self.open.store.id != hex(binding.store())
            || self.open.store.root != binding.root()
            || self.original.request.operation != self.claim.fence.intent
            || self.event.request.operation != self.claim.previous
            || self.original.request.volume != self.open.volume
            || self.event.request.volume != self.open.volume
            || self.original_receipt.node.id != self.open.node
            || self.original_receipt.sequence > self.event_receipt.sequence
        {
            return Err(Failure::Corrupt.into());
        }
        self.original
            .request
            .validate()
            .map_err(|_| Failure::Corrupt)?;
        self.event
            .request
            .validate()
            .map_err(|_| Failure::Corrupt)?;
        let origin = match &self.original.request.mutation {
            ns::Mutation::ArmRemoval { owner, .. }
            | ns::Mutation::PrepareRemoval { owner, .. }
            | ns::Mutation::CreateDeferredEntry { owner, .. }
            | ns::Mutation::CreateDeferredEmptyManifest { owner, .. } => owner,
            _ => return Err(Failure::Corrupt.into()),
        };
        if matches!(
            self.event.request.mutation,
            ns::Mutation::ArmRemoval { .. }
                | ns::Mutation::PrepareRemoval { .. }
                | ns::Mutation::CreateDeferredEntry { .. }
                | ns::Mutation::CreateDeferredEmptyManifest { .. }
        ) && self.event.request.operation != self.original.request.operation
        {
            return Err(Failure::Corrupt.into());
        }
        if origin != &self.owner {
            return Err(Failure::Corrupt.into());
        }
        let (fence, mode, node) = match &self.event.request.mutation {
            ns::Mutation::ArmRemoval { node, .. } => (
                &self.claim.fence,
                ns::RemovalPublicationMode::Pending,
                node.id.as_str(),
            ),
            ns::Mutation::PrepareRemoval { node, .. } => (
                &self.claim.fence,
                ns::RemovalPublicationMode::Deferred,
                node.id.as_str(),
            ),
            ns::Mutation::CreateDeferredEntry { .. }
            | ns::Mutation::CreateDeferredEmptyManifest { .. } => (
                &self.claim.fence,
                ns::RemovalPublicationMode::Deferred,
                self.event_receipt.node.id.as_str(),
            ),
            ns::Mutation::TransitionRemoval {
                node, fence, mode, ..
            } => (
                fence,
                match mode {
                    ns::RemovalMode::Pending => ns::RemovalPublicationMode::Pending,
                    ns::RemovalMode::Deferred => ns::RemovalPublicationMode::Deferred,
                },
                node.id.as_str(),
            ),
            ns::Mutation::CommitRemoval { node, fence, .. } => (
                fence,
                ns::RemovalPublicationMode::Committed,
                node.id.as_str(),
            ),
            ns::Mutation::MoveDeferredEntry { node, fence, .. } if node.id == self.open.node => (
                fence.as_ref(),
                ns::RemovalPublicationMode::Deferred,
                node.id.as_str(),
            ),
            mutation => {
                let (node, claim) = mutation.owned_destination().ok_or(Failure::Corrupt)?;
                (
                    &claim.fence,
                    ns::RemovalPublicationMode::Committed,
                    node.id.as_str(),
                )
            }
        };
        if fence != &self.claim.fence
            || mode != self.claim.mode
            || node != self.open.node
            || (self.event.request.operation == self.original.request.operation
                && self.event != self.original)
        {
            return Err(Failure::Corrupt.into());
        }
        let control = if self
            .event
            .request
            .mutation
            .owned_destination()
            .is_some_and(|(n, _)| n.id == self.open.node)
        {
            self.event
                .destination_control
                .as_deref()
                .cloned()
                .unwrap_or_else(|| self.event.control())
        } else {
            self.event.control()
        };
        if control != self.original.control() {
            return Err(Failure::Corrupt.into());
        }
        Ok(())
    }
    pub(super) fn historical(&self, db: &Connection, binding: &Binding) -> Result<()> {
        self.validate(binding)?;
        let open = names::opens::sparse_open(db, &self.open.id)?;
        if open.input != self.open || open.owner != self.owner || !open.active() {
            return Err(Failure::Corrupt.into());
        }
        for (input, receipt) in [
            (&self.original, &self.original_receipt),
            (&self.event, &self.event_receipt),
        ] {
            let saved = names::load(db, &input.request.operation)?.ok_or(Failure::Corrupt)?;
            if saved.input != *input || saved.outcome != Some(Ok(receipt.clone())) {
                return Err(Failure::Corrupt.into());
            }
        }
        Ok(())
    }
}
fn hex(v: &[u8]) -> String {
    v.iter().map(|b| format!("{b:02x}")).collect()
}
pub(super) fn capture(
    db: &Connection,
    key: &chunk_mount::Key,
    open: &str,
    claim: &ns::RemovalPublicationClaim,
) -> Result<Provenance> {
    let scope = chunk_mount::require(db, key)?;
    let open = names::opens::sparse_open(db, open)?;
    scope.open(&open.input)?;
    if open.owner != key.owner() || claim.fence.owner != key.owner() {
        return Err(Failure::Conflict.into());
    }
    let original = names::load(db, &claim.fence.intent)?.ok_or(Failure::Conflict)?;
    let event = names::load(db, &claim.previous)?.ok_or(Failure::Conflict)?;
    Ok(Provenance {
        open: open.input,
        owner: open.owner,
        claim: claim.clone(),
        original: original.input,
        original_receipt: original
            .outcome
            .ok_or(Failure::Conflict)?
            .map_err(|_| Failure::Conflict)?,
        event: event.input,
        event_receipt: event
            .outcome
            .ok_or(Failure::Conflict)?
            .map_err(|_| Failure::Conflict)?,
    })
}
pub(super) fn admit(
    db: &Connection,
    runtime: &str,
    key: &chunk_mount::Key,
    p: &Provenance,
    binding: &Binding,
) -> Result<()> {
    p.historical(db, binding)?;
    chunk_mount::require(db, key)?.open(&p.open)?;
    names::opens::sparse_current(db, runtime, &names::opens::sparse_open(db, &p.open.id)?)?;
    if names::pending_for_node(db, &p.open.volume, &p.open.node)?
        || names::removals::sparse_claim(db, &p.open.volume, &p.open.node)?.as_ref()
            != Some(&p.claim)
    {
        return Err(Failure::Conflict.into());
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Settlement {
    /// No object was ever handed to the staging journal.
    NoEffect { operation: Id },
    /// The exact saved authority request was terminally rejected. Objects and
    /// dirty/newer views remain retained; this is not a clean-source receipt.
    Rejected { operation: Id },
}
pub(super) fn require_unsettled(info: &Info) -> Result<()> {
    if info.settlement().is_some() {
        return Err(Failure::Conflict.into());
    }
    Ok(())
}
/// Reject marker laundering before an old marker is advanced. Legacy bytes stay unchanged.
pub(crate) fn require_pre47(db: &Connection) -> Result<()> {
    for (table, column, fields) in [
        ("sparse_preparations", "input", &["owned"][..]),
        ("sparse_preparations", "state", &["settlement"][..]),
        ("sparse_publications", "record", &["owned"][..]),
    ] {
        let mut q = db.prepare(&format!("SELECT CASE WHEN typeof({column})='blob' AND length({column}) BETWEEN 1 AND 65536 THEN {column} END FROM {table} LIMIT 257"))?;
        for raw in q.query_map([], |row| row.get::<_, Vec<u8>>(0))? {
            let raw = raw?;
            if raw.len() > 65536 {
                return Err(Failure::Corrupt.into());
            }
            let value: serde_json::Value = serde_json::from_slice(&raw)?;
            let value = if table == "sparse_publications" {
                &value["input"]
            } else {
                &value
            };
            if fields.iter().any(|field| value.get(*field).is_some()) {
                return Err(Failure::Corrupt.into());
            }
        }
    }
    Ok(())
}

pub(super) fn validate_snapshot(db: &Connection, s: &ledger::Snapshot) -> Result<()> {
    let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (plan, saved) in &s.plans {
        let info = saved.info(&s.views)?;
        if (info.ownership().is_some() || info.settlement().is_some()) && version < 47 {
            return Err(Failure::Corrupt.into());
        }
        if let Some(p) = info.ownership() {
            p.historical(db, info.view().binding())?;
        }
        if let Some(settlement) = info.settlement() {
            validate_settlement(db, s, *plan, settlement)?;
        }
    }
    Ok(())
}
fn validate_settlement(
    db: &Connection,
    s: &ledger::Snapshot,
    plan: Id,
    settlement: &Settlement,
) -> Result<()> {
    let root = publication::ledger::inspect(db)?;
    let operation = match settlement {
        Settlement::NoEffect { operation } | Settlement::Rejected { operation } => operation,
    };
    if *operation == [0; 32] {
        return Err(Failure::Corrupt.into());
    }
    match settlement {
        Settlement::Rejected { .. } => {
            let r = root.records.get(&plan).ok_or(Failure::Corrupt)?;
            if r.rejection().is_none() || r.receipt().is_some() || r.acknowledgment().is_some() {
                return Err(Failure::Corrupt.into());
            }
        }
        Settlement::NoEffect { .. } => {
            if root.records.contains_key(&plan) {
                return Err(Failure::Conflict.into());
            }
            let info = s.info(plan)?;
            if info.root()?.is_some() {
                for ((p, level, slot), _) in s.objects.range((plan, -1, 0)..=(plan, 2, u64::MAX)) {
                    if *p != plan {
                        break;
                    }
                    if transfers::transfer(
                        db,
                        s,
                        &info,
                        Position {
                            level: *level,
                            slot: *slot,
                        },
                    )?
                    .is_some_and(|t| t.generation().is_some())
                    {
                        return Err(Failure::Conflict.into());
                    }
                }
            }
        }
    }
    Ok(())
}
impl Journal<'_> {
    /// Terminal local resolution. Never discards bytes or makes a source clean.
    pub fn settle(&mut self, plan: Id, settlement: Settlement) -> Result<Info> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let before = ledger::inspect(&tx)?;
        let info = before.info(plan)?;
        if let Some(old) = info.settlement() {
            return if old == &settlement {
                Ok(info)
            } else {
                Err(Failure::Conflict.into())
            };
        }
        validate_settlement(&tx, &before, plan, &settlement)?;
        let mut after = before.clone();
        let mut progress = info.progress;
        progress.settlement = Some(settlement);
        after
            .plans
            .insert(plan, ledger::Saved::new(info.input, progress)?);
        journal::write_delta(&tx, &before, &after, "settle")?;
        let result = after.info(plan)?;
        tx.commit()?;
        journal::checkpoint("settle", "Committed");
        Ok(result)
    }
}

pub(super) fn fresh(
    db: &Connection,
    runtime: Option<&str>,
    native_runtime: &native::Runtime,
    info: &Info,
) -> Result<()> {
    native::fresh(db, runtime, native_runtime, info)?;
    fresh_ownership(db, runtime, info)
}
pub(super) fn fresh_scoped(
    scope: &crate::chunk_mount::ReadScope<'_>,
    runtime: Option<&str>,
    native_runtime: &native::Runtime,
    info: &Info,
) -> Result<()> {
    native::fresh_scoped(scope, runtime, native_runtime, info)?;
    fresh_ownership(scope.connection(), runtime, info)
}
fn fresh_ownership(db: &Connection, runtime: Option<&str>, info: &Info) -> Result<()> {
    require_unsettled(info)?;
    if let Some(p) = info.ownership() {
        p.historical(db, info.view().binding())?;
        if info.native_ownership().is_none() {
            names::opens::sparse_current(
                db,
                runtime.ok_or(Failure::Conflict)?,
                &names::opens::sparse_open(db, &p.open.id)?,
            )?;
        }
        if names::pending_for_node(db, &p.open.volume, &p.open.node)?
            || names::removals::sparse_claim(db, &p.open.volume, &p.open.node)?.as_ref()
                != Some(&p.claim)
        {
            return Err(Failure::Conflict.into());
        }
    }
    Ok(())
}
