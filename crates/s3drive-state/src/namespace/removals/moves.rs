//! Same-fence identity changes, proved by a bounded immutable event chain.
use super::*;

pub(in crate::namespace) fn guarded(
    db: &Connection,
    g: &NodeGuard,
    fence: &RemovalFence,
    previous: &str,
) -> Result<Node> {
    if g.id == ROOT {
        return Err(Error::Root);
    }
    let n = super::guarded(db, g)?;
    let tag = pending(db, &g.id)?.ok_or(Error::Conflict)?;
    if tag.fence != *fence || tag.event != previous {
        return Err(Error::Conflict);
    }
    if tag.mode != RemovalMode::Deferred {
        return Err(Error::DeletePending);
    }
    Ok(n)
}
pub(in crate::namespace) fn mutate(
    db: &Connection,
    r: &Apply,
    sequence: i64,
) -> Result<(String, Artifact)> {
    let Mutation::MoveDeferredEntry {
        node: g,
        fence,
        previous,
        ..
    } = &r.mutation
    else {
        return Err(Error::Invalid);
    };
    guarded(db, g, fence, previous)?;
    let (id, displaced) = entries::mutate(db, r, sequence)?;
    let rows = sql(db.execute(
        "UPDATE namespace_removals SET event_id=?1 WHERE node_id=?2 AND intent_id=?3 AND owner_id=?4 AND mode='deferred' AND event_id=?5",
        params![r.operation,id,fence.intent,fence.owner,previous],
    ))?;
    if rows != 1
        || raw_pending(db, &id)?
            != Some(Tag {
                fence: fence.as_ref().clone(),
                mode: RemovalMode::Deferred,
                event: r.operation.clone(),
            })
    {
        return Err(Error::Internal);
    }
    events::insert(db, r, &id, fence, "deferred", sequence as u64)?;
    Ok((
        id,
        Artifact::RemovalMove {
            fence: fence.as_ref().clone(),
            displaced,
        },
    ))
}
/// Shape proof only; `chain` proves the preceding identity and mode without
/// recursive lookups. A forged/cyclic predecessor can never recurse or be adopted.
pub(super) fn proof(
    db: &Connection,
    event: &events::Event,
    r: &Apply,
    receipt: &Receipt,
) -> Result<()> {
    let Mutation::MoveDeferredEntry {
        node: g,
        parent,
        name,
        destination,
        fence,
        ..
    } = &r.mutation
    else {
        return Err(Error::Internal);
    };
    let Some(Artifact::RemovalMove {
        fence: actual,
        displaced,
    }) = &receipt.artifact
    else {
        return Err(Error::Internal);
    };
    if r.validate().is_err()
        || r.volume != volume(db)?.id
        || r.operation != event.operation
        || g.id != event.node
        || receipt.node.id != g.id
        || actual != fence.as_ref()
        || fence.as_ref() != &event.fence
        || event.mode != "deferred"
        || receipt.node.deleted
        || receipt.node.parent.as_ref() != Some(&parent.id)
        || receipt.node.name != *name
        || receipt.sequence != event.sequence
        || receipt.node.revision != receipt.sequence
        || receipt.sequence <= r.expected_sequence
        || receipt.node.payload.as_ref().is_some_and(|p| {
            !p.valid() || p.reservation.node != g.id || p.reservation.volume != r.volume
        })
    {
        return Err(Error::Internal);
    }
    match (destination.as_ref().filter(|d| d.id != g.id), displaced) {
        (None, None) => (),
        (Some(d), Some(n))
            if n.id == d.id
                && n.deleted
                && n.revision == receipt.sequence
                && n.parent.as_ref() == Some(&parent.id)
                && names::collision_key(&n.name) == names::collision_key(name)
                && (n.kind == Kind::Directory) == (receipt.node.kind == Kind::Directory)
                && n.payload.as_ref().is_none_or(|p| {
                    p.valid() && p.reservation.node == n.id && p.reservation.volume == r.volume
                }) => {}
        _ => return Err(Error::Internal),
    }
    Ok(())
}
pub(super) fn chain(db: &Connection, last: &events::Event) -> Result<Receipt> {
    let (initial, mut mode) = origin(db, &last.node, &last.fence)?;
    let mut stmt = sql(db.prepare("SELECT node_id,operation_id,intent_id,owner_id,mode,sequence FROM namespace_removal_events WHERE node_id=?1 AND intent_id=?2 AND owner_id=?3 AND sequence<=?4 ORDER BY sequence LIMIT 8193"))?;
    let rows = sql(sql(stmt.query_map(
        params![
            last.node,
            last.fence.intent,
            last.fence.owner,
            last.sequence as i64
        ],
        events::read,
    ))?
    .collect::<rusqlite::Result<Vec<_>>>())?;
    if rows.is_empty() || rows.len() > MAX_OPERATIONS as usize {
        return Err(Error::Internal);
    }
    let first = &rows[0];
    if first.operation != last.fence.intent
        || first.mode != mode.text()
        || first.sequence != initial.sequence
        || events::proof(db, first)? != initial
    {
        return Err(Error::Internal);
    }
    let mut before = initial;
    let mut previous = first.operation.as_str();
    for event in rows.iter().skip(1) {
        if event.sequence <= before.sequence {
            return Err(Error::Internal);
        }
        let receipt = events::proof(db, event)?;
        let request: Apply =
            serde_json::from_slice(&stored(db, &event.operation)?.ok_or(Error::Internal)?.0)
                .map_err(|_| Error::Internal)?;
        let mut preimage = receipt.node.clone();
        preimage.deleted = false;
        let guard = if let Some((target, claim)) = request
            .mutation
            .owned_destination()
            .filter(|(n, _)| n.id == event.node)
        {
            if claim.previous != previous || claim.mode != mode {
                return Err(Error::Internal);
            }
            Some(target)
        } else {
            match &request.mutation {
                Mutation::MoveDeferredEntry {
                    node,
                    source_parent,
                    previous: predecessor,
                    ..
                } => {
                    if mode != RemovalMode::Deferred
                        || predecessor.as_ref() != previous
                        || before.node.parent.as_ref() != Some(&source_parent.id)
                    {
                        return Err(Error::Internal);
                    }
                    preimage.parent = before.node.parent.clone();
                    preimage.name = before.node.name.clone();
                    Some(node)
                }
                Mutation::TransitionRemoval {
                    node,
                    expected,
                    mode: next,
                    ..
                } => {
                    if mode != *expected {
                        return Err(Error::Internal);
                    }
                    mode = *next;
                    Some(node)
                }
                Mutation::CommitRemoval { node, .. } if mode == RemovalMode::Pending => Some(node),
                Mutation::CancelRemoval { .. } => None,
                _ => return Err(Error::Internal),
            }
        };
        if let Some(g) = guard {
            preimage.revision = g.revision;
            verify_revision(&preimage, &before.node, &last.fence)?;
        } else {
            // Cancellation has no revision guard. Its immutable receipt still
            // must retain identity and may only adopt an owned publication.
            identity(&preimage, &before.node, &last.fence)?;
        }
        if event.closed() && event != last {
            return Err(Error::Internal);
        }
        before = receipt;
        previous = &event.operation;
    }
    if rows.last() != Some(last) {
        return Err(Error::Internal);
    }
    Ok(before)
}
fn identity(now: &Node, before: &Node, fence: &RemovalFence) -> Result<()> {
    if now.id != before.id
        || now.parent != before.parent
        || now.name != before.name
        || now.placement != before.placement
        || (now.kind == Kind::Directory) != (before.kind == Kind::Directory)
        || (now.payload != before.payload
            && now
                .payload
                .as_ref()
                .and_then(|p| p.reservation.guard.as_ref())
                .and_then(|g| g.removal.as_ref())
                != Some(fence))
    {
        return Err(Error::Internal);
    }
    Ok(())
}
pub(super) fn verify_revision(now: &Node, before: &Node, fence: &RemovalFence) -> Result<()> {
    identity(now, before, fence)?;
    if now.revision < before.revision
        || (now.revision == before.revision && now != before)
        || (now.revision > before.revision && now.payload == before.payload)
    {
        return Err(Error::Internal);
    }
    Ok(())
}
