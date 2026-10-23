//! Exact local predecessor edges. Never adopt an arbitrary current payload.
use super::super::ledger::Snapshot;
use super::ledger::Meta;
use super::*;

pub(super) fn ancestor(s: &Snapshot, older: Id, newer: Id) -> Result<bool> {
    let mut next = s.views.parent(newer)?;
    for _ in 0..super::super::super::HARD_VIEWS {
        let Some(id) = next else { return Ok(false) };
        if id == older {
            return Ok(true);
        }
        next = s.views.parent(id)?;
    }
    Err(Failure::Corrupt.into())
}
/// No receipt/payload substitution: selection uses the full exact published
/// payload, same frozen source, and actual immutable source-view ancestry.
pub(super) fn select(meta: &Meta, s: &Snapshot, plan: Id, node: &ns::Node) -> Result<Option<Id>> {
    let info = s.info(plan)?;
    let binding = info.view().binding();
    let base = binding.base()?;
    if node.id != base.node()
        || node.deleted != info.ownership().is_some_and(|p| p.claim().mode.detached())
        || node.kind != ns::Kind::File
    {
        return Err(Failure::Conflict.into());
    }
    if ns::payload_reference(&node.payload).as_deref() == Some(base.payload_reference()) {
        return Ok(None);
    }
    let mut found = None;
    for (p, r) in &meta.records {
        let Some(receipt) = r.receipt() else { continue };
        if receipt.node.payload == node.payload
            && match (r.input.owned.as_ref(), info.ownership()) {
                (None, None) => true,
                (Some(old), Some(new)) => {
                    old.same_epoch(new) && old.event_sequence() <= new.event_sequence()
                }
                _ => false,
            }
            && r.input.binding == *binding
            && r.input.original_root == info.original_root()?.encode()
            && receipt.sequence <= node.revision
            && ancestor(s, r.input.view, info.view().id())?
            && found.replace(*p).is_some()
        {
            return Err(Failure::Corrupt.into());
        }
    }
    found.map(Some).ok_or_else(|| Failure::Conflict.into())
}
/// Metadata-only recovery validates this graph without reading any source BLOB.
/// Strictly increasing receipt/request sequence edges preclude cycles.
pub(super) fn validate(meta: &Meta, r: &Record) -> Result<()> {
    let Some(p) = r.input.predecessor else {
        return Ok(());
    };
    let prior = meta.records.get(&p).ok_or(Failure::Corrupt)?;
    let receipt = prior.receipt().ok_or(Failure::Corrupt)?;
    if !match (&prior.input.owned, &r.input.owned) {
        (None, None) => true,
        (Some(old), Some(new)) => {
            old.same_epoch(new) && old.event_sequence() <= new.event_sequence()
        }
        _ => false,
    } || prior.input.binding != r.input.binding
        || prior.input.store != r.input.store
        || prior.input.original_root != r.input.original_root
        || prior.input.view == r.input.view
        || receipt.node.payload != r.input.base.payload
        || receipt.sequence > r.input.base.revision
        || receipt.sequence > r.input.reserve.expected_sequence
    {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
