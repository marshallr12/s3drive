//! Ownership is derived from exact acknowledged operations, never from a
//! replaceable in-memory owner or a second mutable lifecycle index.
use super::*;
use namespace::{RemovalFence, RemovalMode};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedRemoval {
    pub input: Input,
    pub receipt: Receipt,
    pub fence: RemovalFence,
    pub mode: RemovalMode,
    pub event: String,
}
pub(super) fn fence(input: &Input) -> Option<RemovalFence> {
    match &input.request.mutation {
        Mutation::ArmRemoval { owner, .. }
        | Mutation::PrepareRemoval { owner, .. }
        | Mutation::CreateDeferredEntry { owner, .. } => Some(RemovalFence {
            intent: input.request.operation.clone(),
            owner: owner.clone(),
        }),
        Mutation::CancelRemoval { fence, .. }
        | Mutation::CommitRemoval { fence, .. }
        | Mutation::TransitionRemoval { fence, .. } => Some(fence.clone()),
        Mutation::MoveDeferredEntry { fence, .. } => Some(fence.as_ref().clone()),
        _ => None,
    }
}
pub(super) fn check_identity(db: &Connection, input: &Input) -> Result<()> {
    if let Some(fence) = fence(input)
        && identity::read(db)?.as_ref() != Some(&fence.owner)
    {
        return Err("namespace removal requires this client's retained identity".into());
    }
    if let Some((_, claim)) = input.request.mutation.owned_destination()
        && identity::read(db)?.as_ref() != Some(&claim.fence.owner)
    {
        return Err("replacement target requires this client's retained identity".into());
    }
    Ok(())
}
pub(super) fn records(db: &Connection) -> Result<Vec<Record>> {
    let ids = db
        .prepare("SELECT operation_id FROM namespace_intents ORDER BY operation_id LIMIT 4097")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if ids.len() > LIMIT as usize {
        return Err("namespace intent quota or integrity failure".into());
    }
    ids.iter()
        .map(|id| load(db, id)?.ok_or_else(|| "namespace row vanished".into()))
        .collect()
}
fn same_control(a: &Input, b: &Input) -> bool {
    a.request.volume == b.request.volume
        && a.store == b.store
        && a.source == b.source
        && a.fingerprint == b.fingerprint
}
type Key = (String, String);
struct History<'a> {
    active: HashMap<Key, OwnedRemoval>,
    cancellations: HashMap<Key, &'a Record>,
}
fn history(records: &[Record]) -> Result<History<'_>> {
    let mut ordered = BTreeMap::new();
    for r in records {
        if (fence(&r.input).is_some() || r.input.request.mutation.owned_destination().is_some())
            && let Some(Ok(receipt)) = &r.outcome
            && ordered
                .insert((r.input.request.volume.clone(), receipt.sequence), r)
                .is_some()
        {
            return Err("duplicate removal receipt sequence".into());
        }
    }
    let mut active: HashMap<Key, OwnedRemoval> = HashMap::new();
    let mut cancellations = HashMap::new();
    for r in ordered.values() {
        let Some(Ok(receipt)) = &r.outcome else {
            unreachable!()
        };
        if let Some(f) = fence(&r.input) {
            let key = (r.input.request.volume.clone(), receipt.node.id.clone());
            if matches!(
                r.input.request.mutation,
                Mutation::ArmRemoval { .. }
                    | Mutation::PrepareRemoval { .. }
                    | Mutation::CreateDeferredEntry { .. }
            ) {
                if active
                    .insert(
                        key,
                        OwnedRemoval {
                            input: r.input.clone(),
                            receipt: receipt.clone(),
                            event: r.input.request.operation.clone(),
                            fence: f,
                            mode: if matches!(
                                r.input.request.mutation,
                                Mutation::PrepareRemoval { .. }
                                    | Mutation::CreateDeferredEntry { .. }
                            ) {
                                RemovalMode::Deferred
                            } else {
                                RemovalMode::Pending
                            },
                        },
                    )
                    .is_some()
                {
                    return Err("overlapping owned removal history".into());
                }
            } else {
                let mut old = active
                    .remove(&key)
                    .ok_or("closing removal lacks acknowledged arm")?;
                if old.fence != f || !same_control(&old.input, &r.input) {
                    return Err("closing removal changed retained ownership".into());
                }
                if matches!(r.input.request.mutation, Mutation::CancelRemoval { .. })
                    && cancellations
                        .insert((r.input.request.volume.clone(), f.intent.clone()), *r)
                        .is_some()
                {
                    return Err("duplicate cancellation epoch".into());
                }
                match &r.input.request.mutation {
                    Mutation::TransitionRemoval { expected, mode, .. } => {
                        if old.mode != *expected {
                            return Err("removal transition changed expected mode".into());
                        }
                        old.mode = *mode;
                        old.receipt = receipt.clone();
                        old.event = r.input.request.operation.clone();
                        active.insert(key, old);
                    }
                    Mutation::MoveDeferredEntry { previous, .. } => {
                        if old.mode != RemovalMode::Deferred || previous.as_ref() != old.event {
                            return Err("owner move changed previous event or deferred mode".into());
                        }
                        check_retained(&old, &r.input)?;
                        old.receipt = receipt.clone();
                        old.event = r.input.request.operation.clone();
                        active.insert(key, old);
                    }
                    Mutation::CommitRemoval { .. } if old.mode != RemovalMode::Pending => {
                        return Err("deferred removal cannot commit".into());
                    }
                    _ => (),
                }
            }
        }
        if let Some((target, claim)) = r.input.request.mutation.owned_destination() {
            let old = active
                .remove(&(r.input.request.volume.clone(), target.id.clone()))
                .ok_or("replacement lacks acknowledged target ownership")?;
            check_target(&old, &r.input, claim)?;
        }
    }
    Ok(History {
        active,
        cancellations,
    })
}
pub(crate) fn owned(db: &Connection, volume: &str, node: &str) -> Result<Option<OwnedRemoval>> {
    Ok(history(&records(db)?)?
        .active
        .remove(&(volume.into(), node.into())))
}
/// Evidence of epoch closure; the transition's original outcome remains intact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionCancellation {
    pub transition: Record,
    pub cancellation: Record,
}
impl<'a> History<'a> {
    fn cancellation(&self, input: &Input) -> Result<Option<&'a Record>> {
        let Mutation::TransitionRemoval { node, fence, .. } = &input.request.mutation else {
            return Ok(None);
        };
        let Some(close) = self
            .cancellations
            .get(&(input.request.volume.clone(), fence.intent.clone()))
            .copied()
        else {
            return Ok(None);
        };
        let Some(Ok(receipt)) = &close.outcome else {
            return Err("cancellation proof lacks exact receipt".into());
        };
        let mut expected = input.entries[0].clone();
        expected.revision = receipt.sequence;
        if !cancels_transition(&close.input, input)
            || receipt.sequence <= input.request.expected_sequence
            || receipt.node.id != node.id
            || !expected.matches(&receipt.node)
            || receipt.node.deleted
        {
            return Err("closed transition changed frozen binding or identity".into());
        }
        Ok(Some(close))
    }
}
fn cancels_transition(cancel: &Input, transition: &Input) -> bool {
    match (&cancel.request.mutation, &transition.request.mutation) {
        (
            Mutation::CancelRemoval { node, fence },
            Mutation::TransitionRemoval {
                node: before,
                fence: old,
                ..
            },
        ) => node == &before.id && fence == old && same_control(cancel, transition),
        _ => false,
    }
}
pub(super) fn transition_cancellation(
    db: &Connection,
    operation: &str,
) -> Result<Option<TransitionCancellation>> {
    let rows = records(db)?;
    let h = history(&rows)?;
    let r = rows
        .iter()
        .find(|r| r.input.request.operation == operation)
        .ok_or("namespace intent not found")?;
    if r.outcome.is_some() {
        return Ok(None);
    }
    Ok(h.cancellation(&r.input)?
        .map(|close| TransitionCancellation {
            transition: r.clone(),
            cancellation: close.clone(),
        }))
}
pub(super) fn blocks_for_node(
    db: &Connection,
    volume: &str,
    node: &str,
    cancellation: Option<&Input>,
) -> Result<bool> {
    let rows = records(db)?;
    let h = history(&rows)?;
    for r in &rows {
        if r.outcome.is_none()
            && r.input.request.volume == volume
            && protected(&r.input).contains(&node)
            && h.cancellation(&r.input)?.is_none()
            && !cancellation.is_some_and(|c| cancels_transition(c, &r.input))
        {
            return Ok(true);
        }
    }
    Ok(false)
}
// Deferred ownership reserves promotion plus final closure; pending reserves closure.
fn required_slots(records: &[Record]) -> Result<usize> {
    let unknown = records
        .iter()
        .filter(|r| r.outcome.is_none())
        .map(|r| match r.input.request.mutation {
            Mutation::ArmRemoval { .. } => 1,
            Mutation::PrepareRemoval { .. } | Mutation::CreateDeferredEntry { .. } => 2,
            _ => 0,
        })
        .sum::<usize>();
    let reserved = history(records)?
        .active
        .values()
        .map(|o| o.mode.closing_slots())
        .sum::<usize>();
    Ok(records.len() + reserved + unknown)
}
pub(super) fn check_capacity(records: &[Record]) -> Result<()> {
    if required_slots(records)? > LIMIT as usize {
        return Err("namespace intent limit preserves removal closing headroom".into());
    }
    Ok(())
}
pub(super) fn check_begin(db: &Connection, input: &Input) -> Result<()> {
    check_identity(db, input)?;
    let records = records(db)?;
    let h = history(&records)?;
    let owned = &h.active;
    if fence(input).is_some() {
        let created = matches!(input.request.mutation, Mutation::CreateDeferredEntry { .. });
        let id = if created {
            &input.request.operation
        } else {
            &input.entries[0].id
        };
        let old = owned.get(&(input.request.volume.clone(), id.clone()));
        match &input.request.mutation {
            Mutation::ArmRemoval { .. }
            | Mutation::PrepareRemoval { .. }
            | Mutation::CreateDeferredEntry { .. }
                if old.is_none() => {}
            Mutation::CancelRemoval { .. }
            | Mutation::CommitRemoval { .. }
            | Mutation::TransitionRemoval { .. }
            | Mutation::MoveDeferredEntry { .. } => {
                let old = old.ok_or("removal closure requires acknowledged local arm")?;
                if matches!(&input.request.mutation, Mutation::CommitRemoval { .. })
                    && old.mode != RemovalMode::Pending
                {
                    return Err("deferred removal must promote before commit".into());
                }
                if let Mutation::TransitionRemoval { expected, .. } = &input.request.mutation
                    && old.mode != *expected
                {
                    return Err("removal transition mode mismatch".into());
                }
                if let Mutation::MoveDeferredEntry { previous, .. } = &input.request.mutation
                    && (old.mode != RemovalMode::Deferred || previous.as_ref() != old.event)
                {
                    return Err("owner move requires the latest deferred event".into());
                }
                check_retained(old, input)?;
            }
            _ => return Err("removal already owned; recover exact disposition first".into()),
        }
    }
    if let Some((target, claim)) = input.request.mutation.owned_destination() {
        let old = owned
            .get(&(input.request.volume.clone(), target.id.clone()))
            .ok_or("replacement requires acknowledged local target ownership")?;
        check_target(old, input, claim)?;
    }
    // Block source, target and parent use *before* persisting a doomed ordinary
    // intent. Otherwise its temporary server refusal could block cancellation.
    let touched = touched(input);
    for ((volume, node), reservation) in owned {
        if volume == &input.request.volume && touched.contains(&node.as_str()) {
            let own_close = matches!(
                input.request.mutation,
                Mutation::CancelRemoval { .. }
                    | Mutation::CommitRemoval { .. }
                    | Mutation::TransitionRemoval { .. }
                    | Mutation::MoveDeferredEntry { .. }
            ) && input.entries[0].id == *node
                || input
                    .request
                    .mutation
                    .owned_destination()
                    .is_some_and(|(target, _)| target.id == *node);
            let child_only = reservation.mode == RemovalMode::Deferred
                && !input.entries.iter().any(|e| e.id == *node);
            if !own_close && !child_only {
                return Err("namespace node is locally delete-pending".into());
            }
        }
    }
    for r in &records {
        if r.outcome.is_none()
            && h.cancellation(&r.input)?.is_none()
            && !cancels_transition(input, &r.input)
            && r.input.request.volume == input.request.volume
            && (fence(&r.input).is_some() || r.input.request.mutation.owned_destination().is_some())
            && protected(&r.input).iter().any(|id| touched.contains(id))
        {
            return Err("recover exact removal outcome before another namespace effect".into());
        }
    }
    // Project the successful mode change, allowing only its reserved credit.
    // A rejected acknowledgment must still pass the ordinary capacity check.
    let credit = if let Some(old) = input
        .entries
        .first()
        .and_then(|e| owned.get(&(input.request.volume.clone(), e.id.clone())))
    {
        match input.request.mutation {
            Mutation::CancelRemoval { .. } | Mutation::CommitRemoval { .. } => {
                old.mode.closing_slots() as isize
            }
            Mutation::TransitionRemoval { mode, .. } => {
                old.mode.closing_slots() as isize - mode.closing_slots() as isize
            }
            _ => 0,
        }
    } else {
        0
    };
    let mut projected = records;
    projected.push(Record {
        input: input.clone(),
        outcome: None,
    });
    if required_slots(&projected)? as isize - credit > LIMIT as isize {
        return Err("namespace removal closing quota".into());
    }
    Ok(())
}
fn protected(input: &Input) -> Vec<&str> {
    let mut ids: Vec<_> = input.entries.iter().map(|e| e.id.as_str()).collect();
    if matches!(input.request.mutation, Mutation::CreateDeferredEntry { .. }) {
        ids.push(&input.request.operation);
    }
    ids
}
fn touched(input: &Input) -> Vec<&str> {
    let mut ids = protected(input);
    match &input.request.mutation {
        Mutation::CreateEntry { parent, .. } | Mutation::CreateDeferredEntry { parent, .. } => {
            ids.push(parent)
        }
        Mutation::RemoveEntry { parent, .. }
        | Mutation::ArmRemoval { parent, .. }
        | Mutation::PrepareRemoval { parent, .. }
        | Mutation::TransitionRemoval { parent, .. }
        | Mutation::CommitRemoval { parent, .. } => ids.push(&parent.id),
        Mutation::MoveEntry {
            source_parent,
            parent,
            ..
        }
        | Mutation::MoveDeferredEntry {
            source_parent,
            parent,
            ..
        } => {
            ids.push(&source_parent.id);
            ids.push(&parent.id);
        }
        _ => (),
    }
    ids
}

fn check_retained(old: &OwnedRemoval, input: &Input) -> Result<()> {
    let f = fence(input).ok_or("missing retained source fence")?;
    check_entry(old, input, &input.entries[0], &f, None)
}
fn check_target(old: &OwnedRemoval, input: &Input, claim: &namespace::RemovalClaim) -> Result<()> {
    if old.mode != claim.mode || old.event != claim.previous {
        return Err("replacement target mode or latest event changed".into());
    }
    check_entry(
        old,
        input,
        input.entries.get(1).ok_or("missing replacement target")?,
        &claim.fence,
        input.destination_control.as_deref(),
    )
}
fn check_entry(
    old: &OwnedRemoval,
    input: &Input,
    e: &super::Entry,
    f: &RemovalFence,
    destination: Option<&super::Control>,
) -> Result<()> {
    let n = &old.receipt.node;
    if f != &old.fence
        || old.input.request.volume != input.request.volume
        || old.input.control() != destination.cloned().unwrap_or_else(|| input.control())
        || e.revision < n.revision
        || Some(&e.parent) != n.parent.as_ref()
        || e.name != n.name
        || e.placement != n.placement
        || (e.kind == namespace::Kind::Directory) != (n.kind == namespace::Kind::Directory)
    {
        return Err("removal changed retained ownership or identity".into());
    }
    Ok(())
}
