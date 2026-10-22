//! Original-open source mutation and cold-session fences.
use super::*;

pub(in crate::sparse_views) struct Mutation<'a> {
    pub(super) token: &'a Token,
    pub(super) context: &'a Context,
}
pub(in crate::sparse_views) fn checkpoint(kind: &str, point: &str) {
    journal::checkpoint(kind, point);
}
type EpochEvent = (crate::chunk_mount::Key, String, opens::Event);
pub(in crate::sparse_views) fn before_mutation(
    db: &Connection,
    incarnation: Option<&str>,
    runtime: &mut Runtime,
    mutation: Option<&Mutation<'_>>,
    parent: &crate::sparse_views::Record,
    operation: Id,
) -> Result<Option<EpochEvent>> {
    let binding = &parent.request.binding;
    let publisher = epoch_owner(db, binding)?;
    let records = opens::sparse_records(db)?;
    let base = binding.base()?;
    let bound = records
        .iter()
        .filter(|record| {
            record.input.volume == base.volume()
                && record.input.node == base.node()
                && is_bound(record)
        })
        .collect::<Vec<_>>();
    let Some(mutation) = mutation else {
        if publisher.is_some() || !bound.is_empty() {
            return Err(Failure::Conflict.into());
        }
        return Ok(None);
    };
    for record in bound {
        let observed = record
            .close()
            .map(|e| !record.completed(&e.id))
            .unwrap_or(false);
        let memory = runtime
            .tokens
            .get(&record.input.id)
            .is_some_and(|flag| flag.load(Ordering::Acquire))
            && record.close().is_none_or(|e| !record.completed(&e.id));
        if observed || memory {
            // A new clean epoch may follow exact abandonment of an obsolete
            // session. This never completes its callback or adopts dirty work.
            if !opens::sparse_recovered(db, &record.input.id)? {
                return Err(Failure::Conflict.into());
            }
            if parent.clean() {
                publication::namespace_pending(db, base.volume(), base.node())?;
            } else {
                let (proof, active) = publisher.as_ref().ok_or(Failure::Conflict)?;
                opens::sparse_current(db, incarnation.ok_or(Failure::Conflict)?, active)?;
                if runtime.closing(&proof.open)? {
                    return Err(Failure::Conflict.into());
                }
            }
        }
    }
    let token = mutation.token;
    let record = token.validate(db, runtime)?;
    if !is_bound(&record) {
        return Err(Failure::Conflict.into());
    }
    let incarnation = incarnation.ok_or(Failure::Conflict)?;
    opens::sparse_current(db, incarnation, &record)?;
    if runtime.closing(&record.input.id)?
        || mutation.context.binding != *binding
        || mutation.context.view != parent.id
        || crate::namespace_client::pending_for_node(db, &record.input.volume, &record.input.node)?
    {
        return Err(Failure::Conflict.into());
    }
    let base = binding.base()?;
    if base.volume() != record.input.volume
        || base.node() != record.input.node
        || binding.source() != &record.input.source
        || binding.fingerprint() != record.input.fingerprint
        || hex(binding.store()) != record.input.store.id
        || binding.root() != record.input.store.root
    {
        return Err(Failure::Conflict.into());
    }
    if crate::namespace_client::removals::sparse_claim(db, base.volume(), base.node())?
        .is_some_and(|claim| claim.fence.owner != record.owner)
    {
        return Err(Failure::Conflict.into());
    }
    let event = if let Some((original, record)) = publisher {
        opens::sparse_current(db, incarnation, &record)?;
        if runtime.closing(&original.open)? {
            return Err(Failure::Conflict.into());
        }
        None
    } else {
        if !parent.clean() {
            return Err(Failure::Conflict.into());
        }
        let plans = ledger::inspect(db)?;
        let roots = publication::ledger::inspect(db)?;
        for (id, saved) in &plans.plans {
            let info = saved.info(&plans.views)?;
            if info.view().binding().epoch()? == binding.epoch()?
                && info.settlement().is_none()
                && !roots
                    .records
                    .get(id)
                    .is_some_and(|r| r.acknowledgment().is_some())
            {
                return Err(Failure::Conflict.into());
            }
        }
        Some((
            token.key.clone(),
            record.input.id.clone(),
            opens::Event {
                id: hex(&hash(
                    b"s3drive-native-sparse-epoch-v1\0",
                    &(&record.input.id, binding.epoch()?, operation),
                )?[..16]),
                action: Action::OwnSparseEpoch {
                    binding: Box::new(binding.clone()),
                    operation,
                },
            },
        ))
    };
    runtime.remember(mutation.context.clone())?;
    Ok(event)
}
