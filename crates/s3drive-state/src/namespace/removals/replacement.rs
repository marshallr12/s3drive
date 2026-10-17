//! The old destination closes by stable ID in the same transaction as rename.
use super::*;

pub(in crate::namespace) fn guarded(
    db: &Connection,
    g: &NodeGuard,
    claim: &RemovalClaim,
) -> Result<Node> {
    let n = super::guarded(db, g)?;
    let tag = pending(db, &g.id)?.ok_or(Error::Conflict)?;
    if tag.fence != claim.fence || tag.event != claim.previous || tag.mode != claim.mode {
        return Err(Error::Conflict);
    }
    Ok(n)
}
pub(in crate::namespace) fn close(
    db: &Connection,
    r: &Apply,
    id: &str,
    claim: &RemovalClaim,
    sequence: u64,
) -> Result<()> {
    release(db, id, &claim.fence)?;
    events::insert(db, r, id, &claim.fence, "committed", sequence)
}
/// Project the displaced-node side of the one exact two-node receipt. This is
/// historical evidence, not a second operation or a fabricated callback.
pub(super) fn proof(
    db: &Connection,
    event: &events::Event,
    r: &Apply,
    receipt: &Receipt,
) -> Result<Receipt> {
    let (target, claim) = r.mutation.owned_destination().ok_or(Error::Internal)?;
    let (source, parent, name, displaced) = match (&r.mutation, &receipt.artifact) {
        (
            Mutation::MoveEntry {
                node, parent, name, ..
            },
            Some(Artifact::Namespace {
                displaced: Some(old),
            }),
        ) => (node, parent, name, old),
        (
            Mutation::MoveDeferredEntry {
                node,
                parent,
                name,
                fence,
                ..
            },
            Some(Artifact::RemovalMove {
                fence: actual,
                displaced: Some(old),
            }),
        ) if fence.as_ref() == actual => (node, parent, name, old),
        _ => return Err(Error::Internal),
    };
    if r.validate().is_err()
        || r.volume != volume(db)?.id
        || r.operation != event.operation
        || target.id != event.node
        || claim.fence != event.fence
        || event.mode != "committed"
        || event.sequence != receipt.sequence
        || receipt.sequence <= r.expected_sequence
        || receipt.node.id != source.id
        || receipt.node.deleted
        || receipt.node.revision != receipt.sequence
        || receipt.node.parent.as_ref() != Some(&parent.id)
        || receipt.node.name != *name
        || displaced.id != target.id
        || !displaced.deleted
        || displaced.revision != receipt.sequence
        || displaced.parent.as_ref() != Some(&parent.id)
        || names::collision_key(&displaced.name) != names::collision_key(name)
        || (displaced.kind == Kind::Directory) != (receipt.node.kind == Kind::Directory)
        || [displaced, &receipt.node].iter().any(|n| {
            n.payload.as_ref().is_some_and(|p| {
                !p.valid() || p.reservation.node != n.id || p.reservation.volume != r.volume
            })
        })
    {
        return Err(Error::Internal);
    }
    Ok(Receipt {
        sequence: receipt.sequence,
        node: displaced.clone(),
        artifact: Some(Artifact::Removal(claim.fence.clone())),
    })
}
