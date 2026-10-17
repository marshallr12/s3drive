//! Ownership is derived from exact acknowledged operations, never from a
//! replaceable in-memory owner or a second mutable lifecycle index.
use super::*;
use namespace::RemovalFence;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedRemoval {
    pub input: Input,
    pub receipt: Receipt,
    pub fence: RemovalFence,
}
pub(super) fn fence(input: &Input) -> Option<RemovalFence> {
    match &input.request.mutation {
        Mutation::ArmRemoval { owner, .. } => Some(RemovalFence {
            intent: input.request.operation.clone(),
            owner: owner.clone(),
        }),
        Mutation::CancelRemoval { fence, .. } | Mutation::CommitRemoval { fence, .. } => {
            Some(fence.clone())
        }
        _ => None,
    }
}
pub(super) fn check_identity(db: &Connection, input: &Input) -> Result<()> {
    if let Some(fence) = fence(input)
        && identity::read(db)?.as_ref() != Some(&fence.owner)
    {
        return Err("namespace removal requires this client's retained identity".into());
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
fn active(records: &[Record]) -> Result<HashMap<Key, OwnedRemoval>> {
    let mut ordered = BTreeMap::new();
    for r in records {
        if fence(&r.input).is_some()
            && let Some(Ok(receipt)) = &r.outcome
            && ordered
                .insert((r.input.request.volume.clone(), receipt.sequence), r)
                .is_some()
        {
            return Err("duplicate removal receipt sequence".into());
        }
    }
    let mut active: HashMap<Key, OwnedRemoval> = HashMap::new();
    for r in ordered.values() {
        let Some(Ok(receipt)) = &r.outcome else {
            unreachable!()
        };
        let key = (r.input.request.volume.clone(), receipt.node.id.clone());
        let f = fence(&r.input).ok_or("missing removal fence")?;
        if matches!(r.input.request.mutation, Mutation::ArmRemoval { .. }) {
            if active
                .insert(
                    key,
                    OwnedRemoval {
                        input: r.input.clone(),
                        receipt: receipt.clone(),
                        fence: f,
                    },
                )
                .is_some()
            {
                return Err("overlapping owned removal history".into());
            }
        } else {
            let old = active
                .remove(&key)
                .ok_or("closing removal lacks acknowledged arm")?;
            if old.fence != f || !same_control(&old.input, &r.input) {
                return Err("closing removal changed retained ownership".into());
            }
        }
    }
    Ok(active)
}
pub(crate) fn owned(db: &Connection, volume: &str, node: &str) -> Result<Option<OwnedRemoval>> {
    Ok(active(&records(db)?)?.remove(&(volume.into(), node.into())))
}
// Pending arms reserve their future closing slot too. A successful closing
// acknowledgment releases that headroom atomically with its node observation.
fn required_slots(records: &[Record]) -> Result<usize> {
    let pending_arms = records
        .iter()
        .filter(|r| {
            r.outcome.is_none() && matches!(r.input.request.mutation, Mutation::ArmRemoval { .. })
        })
        .count();
    let reserved = active(records)?.len() + pending_arms;
    Ok(records.len() + reserved)
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
    let owned = active(&records)?;
    if let Some(f) = fence(input) {
        let e = &input.entries[0];
        let old = owned.get(&(input.request.volume.clone(), e.id.clone()));
        match &input.request.mutation {
            Mutation::ArmRemoval { .. } if old.is_none() => (),
            Mutation::CancelRemoval { .. } | Mutation::CommitRemoval { .. } => {
                let old = old.ok_or("removal closure requires acknowledged local arm")?;
                let n = &old.receipt.node;
                if old.fence != f
                    || !same_control(&old.input, input)
                    || e.revision < n.revision
                    || Some(&e.parent) != n.parent.as_ref()
                    || e.name != n.name
                    || e.placement != n.placement
                    || (e.kind == namespace::Kind::Directory)
                        != (n.kind == namespace::Kind::Directory)
                {
                    return Err("removal closure changed retained ownership or identity".into());
                }
            }
            _ => return Err("removal already owned; recover exact disposition first".into()),
        }
    }
    // Block source, target and parent use *before* persisting a doomed ordinary
    // intent. Otherwise its temporary server refusal could block cancellation.
    let touched = touched(input);
    for (volume, node) in owned.keys() {
        if volume == &input.request.volume && touched.contains(&node.as_str()) {
            let own_close = matches!(
                input.request.mutation,
                Mutation::CancelRemoval { .. } | Mutation::CommitRemoval { .. }
            ) && input.entries[0].id == *node;
            if !own_close {
                return Err("namespace node is locally delete-pending".into());
            }
        }
    }
    for r in &records {
        if r.outcome.is_none()
            && r.input.request.volume == input.request.volume
            && fence(&r.input).is_some()
            && r.input
                .entries
                .iter()
                .any(|e| touched.contains(&e.id.as_str()))
        {
            return Err("recover exact removal outcome before another namespace effect".into());
        }
    }
    // A closing request may use its reserved slot; its successful receipt must
    // release it. Failed closing receipts cannot consume the final headroom.
    let closing = matches!(
        input.request.mutation,
        Mutation::CancelRemoval { .. } | Mutation::CommitRemoval { .. }
    );
    let mut projected = records;
    projected.push(Record {
        input: input.clone(),
        outcome: None,
    });
    if closing {
        if required_slots(&projected)? > LIMIT as usize + 1 {
            return Err("namespace removal closing quota".into());
        }
    } else {
        check_capacity(&projected)?;
    }
    Ok(())
}
fn touched(input: &Input) -> Vec<&str> {
    let mut ids: Vec<_> = input.entries.iter().map(|e| e.id.as_str()).collect();
    match &input.request.mutation {
        Mutation::CreateEntry { parent, .. } => ids.push(parent),
        Mutation::RemoveEntry { parent, .. }
        | Mutation::ArmRemoval { parent, .. }
        | Mutation::CommitRemoval { parent, .. } => ids.push(&parent.id),
        Mutation::MoveEntry {
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
