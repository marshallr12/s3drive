//! Reversible namespace reservation. No OS callback or S3 deletion occurs here.
use super::*;
mod events;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemovalMode {
    Deferred,
    #[default]
    Pending,
}
impl RemovalMode {
    fn text(self) -> &'static str {
        match self {
            Self::Deferred => "deferred",
            Self::Pending => "pending",
        }
    }
    fn parse(s: &str) -> Result<Self> {
        match s {
            "deferred" => Ok(Self::Deferred),
            "pending" => Ok(Self::Pending),
            _ => Err(Error::Internal),
        }
    }
    fn is_pending(&self) -> bool {
        *self == Self::Pending
    }
    pub fn closing_slots(self) -> usize {
        if self == Self::Deferred { 2 } else { 1 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemovalFence {
    pub intent: String,
    pub owner: String,
}
impl RemovalFence {
    pub fn valid(&self) -> bool {
        valid_id(&self.intent) && self.intent != ROOT && valid_id(&self.owner) && self.owner != ROOT
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingRemoval {
    pub node: Node,
    pub fence: RemovalFence,
    #[serde(default, skip_serializing_if = "RemovalMode::is_pending")]
    pub mode: RemovalMode,
}
pub(super) fn check_schema(db: &Connection) -> crate::Result<()> {
    db.prepare("SELECT node_id,intent_id,owner_id,mode,event_id FROM namespace_removals LIMIT 0")?;
    db.prepare("SELECT node_id,operation_id,intent_id,owner_id,mode,sequence FROM namespace_removal_events LIMIT 0")?;
    Ok(())
}
pub(super) fn migrate(db: &Connection) -> Result<()> {
    events::migrate(db)
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tag {
    fence: RemovalFence,
    mode: RemovalMode,
    event: String,
}
fn raw_pending(db: &Connection, id: &str) -> Result<Option<Tag>> {
    let row = sql(db
        .query_row(
            "SELECT intent_id,owner_id,mode,event_id FROM namespace_removals WHERE node_id=?",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional())?;
    let Some((intent, owner, mode, event)) = row else {
        return Ok(None);
    };
    let fence = RemovalFence { intent, owner };
    let event = event.ok_or(Error::Internal)?;
    if !fence.valid() || !valid_id(&event) || event == ROOT || id == ROOT || node(db, id)?.deleted {
        return Err(Error::Internal);
    }
    Ok(Some(Tag {
        fence,
        mode: RemovalMode::parse(&mode)?,
        event,
    }))
}
// Validate the live reservation against its immutable, atomically committed arm
// operation. Valid-looking changes to owner/intent or the reserved identity must
// fail closed, not become a cancellation/publication capability.
fn origin(db: &Connection, id: &str, fence: &RemovalFence) -> Result<(Receipt, RemovalMode)> {
    let (input, outcome) = stored(db, &fence.intent)?.ok_or(Error::Internal)?;
    let input: Apply = serde_json::from_slice(&input).map_err(|_| Error::Internal)?;
    let outcome: Result<Receipt> = serde_json::from_slice(&outcome).map_err(|_| Error::Internal)?;
    let receipt = outcome.map_err(|_| Error::Internal)?;
    if let Mutation::CreateDeferredEntry { store, owner, .. } = &input.mutation {
        let placement = store
            .as_ref()
            .map(|id| publication::store(db, id))
            .transpose()?;
        if input.volume != volume(db)?.id
            || input.operation != fence.intent
            || input.operation != id
            || owner != &fence.owner
            || receipt.node != creation::expected(&input, placement, receipt.sequence)?
            || receipt.artifact != creation::artifact(&input)
        {
            return Err(Error::Internal);
        }
        return Ok((receipt, RemovalMode::Deferred));
    }
    let (before, parent, owner, mode) = match &input.mutation {
        Mutation::ArmRemoval {
            node,
            parent,
            owner,
        } => (node, parent, owner, RemovalMode::Pending),
        Mutation::PrepareRemoval {
            node,
            parent,
            owner,
        } => (node, parent, owner, RemovalMode::Deferred),
        _ => return Err(Error::Internal),
    };
    if input.validate().is_err()
        || input.operation != fence.intent
        || input.volume != volume(db)?.id
        || before.id != id
        || owner != &fence.owner
        || input.expected_sequence >= receipt.sequence
        || receipt.sequence != receipt.node.revision
        || receipt.node.id != id
        || receipt.node.deleted
        || receipt.node.parent.as_ref() != Some(&parent.id)
        || receipt.artifact != Some(Artifact::Removal(fence.clone()))
    {
        return Err(Error::Internal);
    }
    Ok((receipt, mode))
}
fn verify_current(
    db: &Connection,
    id: &str,
    receipt: &Receipt,
    fence: &RemovalFence,
    mode: RemovalMode,
) -> Result<()> {
    let now = node(db, id)?;
    if now.revision < receipt.node.revision
        || now.parent != receipt.node.parent
        || now.name != receipt.node.name
        || now.placement != receipt.node.placement
        || (now.kind == Kind::Directory) != (receipt.node.kind == Kind::Directory)
        || (now.revision == receipt.node.revision && now != receipt.node)
        || (now.revision > receipt.node.revision
            && (now.payload == receipt.node.payload
                || now
                    .payload
                    .as_ref()
                    .and_then(|p| p.reservation.guard.as_ref())
                    .and_then(|g| g.removal.as_ref())
                    != Some(fence)))
    {
        return Err(Error::Internal);
    }
    if mode == RemovalMode::Pending && empty(db, id).is_err() {
        return Err(Error::Internal);
    }
    Ok(())
}
fn pending(db: &Connection, id: &str) -> Result<Option<Tag>> {
    let raw = raw_pending(db, id)?;
    let latest = events::latest(db, id)?;
    match (raw, latest) {
        (None, None) => Ok(None),
        (None, Some(event)) if event.closed() => {
            events::proof(db, &event)?;
            Ok(None)
        }
        (Some(tag), Some(event))
            if !event.closed()
                && event.operation == tag.event
                && event.fence == tag.fence
                && event.mode == tag.mode.text() =>
        {
            let (initial, initial_mode) = origin(db, id, &tag.fence)?;
            let first = events::get(db, id, &tag.fence.intent)?.ok_or(Error::Internal)?;
            if first.mode != initial_mode.text()
                || first.sequence != initial.sequence
                || first.fence != tag.fence
            {
                return Err(Error::Internal);
            }
            events::proof(db, &first)?;
            let receipt = events::proof(db, &event)?;
            if receipt.sequence < initial.sequence
                || receipt.node.parent != initial.node.parent
                || receipt.node.name != initial.node.name
                || receipt.node.placement != initial.node.placement
                || (receipt.node.kind == Kind::Directory) != (initial.node.kind == Kind::Directory)
                || (receipt.node.payload != initial.node.payload
                    && receipt
                        .node
                        .payload
                        .as_ref()
                        .and_then(|p| p.reservation.guard.as_ref())
                        .and_then(|g| g.removal.as_ref())
                        != Some(&tag.fence))
            {
                return Err(Error::Internal);
            }
            verify_current(db, id, &receipt, &tag.fence, tag.mode)?;
            Ok(Some(tag))
        }
        _ => Err(Error::Internal),
    }
}
pub(super) fn check_effect(db: &Connection, id: &str) -> Result<()> {
    pending(db, id).map(|_| ())
}
pub(super) fn require_read(db: &Connection, id: &str) -> Result<()> {
    if pending(db, id)?.is_some_and(|t| t.mode == RemovalMode::Pending) {
        Err(Error::DeletePending)
    } else {
        Ok(())
    }
}
pub(super) fn require_clear(db: &Connection, id: &str) -> Result<()> {
    if pending(db, id)?.is_some() {
        Err(Error::DeletePending)
    } else {
        Ok(())
    }
}
pub(super) fn publication_gate(
    db: &Connection,
    id: &str,
    guard: Option<&ContentGuard>,
) -> Result<()> {
    match (pending(db, id)?, guard.and_then(|g| g.removal.as_ref())) {
        (None, None) => Ok(()),
        (Some(actual), Some(expected)) if actual.fence == *expected => Ok(()),
        (Some(_), None) => Err(Error::DeletePending),
        (Some(_), Some(_)) => Err(Error::Conflict),
        (None, Some(_)) => Err(Error::Conflict),
    }
}
impl State {
    /// Explicit inspection for the disposition owner/recovery. Normal Node
    /// lookup refuses pending names; this API does not create a read handle.
    pub fn namespace_pending_removal(
        &self,
        volume: &str,
        id: &str,
    ) -> Result<Option<PendingRemoval>> {
        check_volume(&self.connection, volume)?;
        let n = node(&self.connection, id)?;
        Ok(pending(&self.connection, id)?.map(|t| PendingRemoval {
            node: n,
            fence: t.fence,
            mode: t.mode,
        }))
    }
}
fn guarded(db: &Connection, g: &NodeGuard) -> Result<Node> {
    let n = node(db, &g.id)?;
    if n.deleted || n.revision != g.revision {
        return Err(Error::Conflict);
    }
    Ok(n)
}
fn parent(db: &Connection, g: &NodeGuard) -> Result<Node> {
    let n = guarded(db, g)?;
    require_read(db, &g.id)?;
    if n.kind != Kind::Directory {
        return Err(Error::NotDirectory);
    }
    Ok(n)
}
fn empty(db: &Connection, id: &str) -> Result<()> {
    let children: i64 = sql(db.query_row(
        "SELECT count(*) FROM namespace_nodes WHERE parent_id=? AND deleted=0",
        [id],
        |r| r.get(0),
    ))?;
    if children != 0 {
        return Err(Error::NotEmpty);
    }
    Ok(())
}
fn revision(db: &Connection, n: &Node, sequence: i64, deleted: bool) -> Result<()> {
    let changed = sql(db.execute("UPDATE namespace_nodes SET revision=?1,deleted=?2 WHERE node_id=?3 AND revision=?4 AND deleted=0", params![sequence,deleted,n.id,n.revision as i64]))?;
    let mut expected = n.clone();
    expected.revision = sequence as u64;
    expected.deleted = deleted;
    if changed != 1 || node(db, &n.id)? != expected {
        return Err(Error::Internal);
    }
    Ok(())
}
pub(super) fn created(db: &Connection, r: &Apply, sequence: i64) -> Result<()> {
    let Mutation::CreateDeferredEntry { owner, .. } = &r.mutation else {
        return Err(Error::Internal);
    };
    let fence = RemovalFence {
        intent: r.operation.clone(),
        owner: owner.clone(),
    };
    let rows = sql(db.execute(
        "INSERT INTO namespace_removals VALUES(?1,?1,?2,'deferred',?1)",
        params![r.operation, owner],
    ))?;
    if rows != 1
        || raw_pending(db, &r.operation)?
            != Some(Tag {
                fence: fence.clone(),
                mode: RemovalMode::Deferred,
                event: r.operation.clone(),
            })
    {
        return Err(Error::Internal);
    }
    events::insert(db, r, &r.operation, &fence, "deferred", sequence as u64)
}
pub(super) fn mutate(db: &Connection, r: &Apply, sequence: i64) -> Result<(String, RemovalFence)> {
    if volume(db)?.name_policy != names::PORTABLE {
        return Err(Error::Invalid);
    }
    match &r.mutation {
        Mutation::ArmRemoval {
            node: g,
            parent: p,
            owner,
        }
        | Mutation::PrepareRemoval {
            node: g,
            parent: p,
            owner,
        } => {
            if g.id == ROOT {
                return Err(Error::Root);
            }
            let n = guarded(db, g)?;
            let p = parent(db, p)?;
            require_clear(db, &n.id)?;
            if n.parent.as_ref() != Some(&p.id) {
                return Err(Error::Conflict);
            }
            let mode = if matches!(r.mutation, Mutation::PrepareRemoval { .. }) {
                RemovalMode::Deferred
            } else {
                RemovalMode::Pending
            };
            if mode == RemovalMode::Pending {
                empty(db, &n.id)?;
            }
            let fence = RemovalFence {
                intent: r.operation.clone(),
                owner: owner.clone(),
            };
            let inserted = sql(db.execute(
                "INSERT INTO namespace_removals VALUES(?1,?2,?3,?4,?5)",
                params![n.id, fence.intent, fence.owner, mode.text(), r.operation],
            ))?;
            if inserted != 1
                || raw_pending(db, &n.id)?
                    != Some(Tag {
                        fence: fence.clone(),
                        mode,
                        event: r.operation.clone(),
                    })
            {
                return Err(Error::Internal);
            }
            revision(db, &n, sequence, false)?;
            events::insert(db, r, &n.id, &fence, mode.text(), sequence as u64)?;
            Ok((n.id, fence))
        }
        Mutation::TransitionRemoval {
            node: g,
            parent: p,
            fence,
            expected,
            mode,
        } => {
            if g.id == ROOT {
                return Err(Error::Root);
            }
            let n = guarded(db, g)?;
            let p = parent(db, p)?;
            let current = pending(db, &n.id)?.ok_or(Error::Conflict)?;
            if current.fence != *fence
                || current.mode != *expected
                || n.parent.as_ref() != Some(&p.id)
                || mode == expected
            {
                return Err(Error::Conflict);
            }
            if *mode == RemovalMode::Pending {
                empty(db, &n.id)?;
            }
            let rows=sql(db.execute("UPDATE namespace_removals SET mode=?1,event_id=?2 WHERE node_id=?3 AND mode=?4 AND event_id=?5",params![mode.text(),r.operation,n.id,expected.text(),current.event]))?;
            let wanted = Tag {
                fence: fence.clone(),
                mode: *mode,
                event: r.operation.clone(),
            };
            if rows != 1 || raw_pending(db, &n.id)? != Some(wanted) {
                return Err(Error::Internal);
            }
            revision(db, &n, sequence, false)?;
            events::insert(db, r, &n.id, fence, mode.text(), sequence as u64)?;
            Ok((n.id, fence.clone()))
        }
        Mutation::CancelRemoval { node: id, fence } => {
            let n = node(db, id)?;
            if n.deleted || pending(db, id)?.as_ref().map(|t| &t.fence) != Some(fence) {
                return Err(Error::Conflict);
            }
            release(db, id, fence)?;
            revision(db, &n, sequence, false)?;
            events::insert(db, r, id, fence, "cancelled", sequence as u64)?;
            Ok((id.clone(), fence.clone()))
        }
        Mutation::CommitRemoval {
            node: g,
            parent: p,
            fence,
        } => {
            if g.id == ROOT {
                return Err(Error::Root);
            }
            let n = guarded(db, g)?;
            let p = parent(db, p)?;
            if n.parent.as_ref() != Some(&p.id)
                || pending(db, &n.id)?.as_ref().map(|t| (&t.fence, t.mode))
                    != Some((fence, RemovalMode::Pending))
            {
                return Err(Error::Conflict);
            }
            empty(db, &n.id)?;
            release(db, &n.id, fence)?;
            revision(db, &n, sequence, true)?;
            events::insert(db, r, &n.id, fence, "committed", sequence as u64)?;
            Ok((n.id, fence.clone()))
        }
        _ => Err(Error::Invalid),
    }
}
fn release(db: &Connection, id: &str, fence: &RemovalFence) -> Result<()> {
    let changed = sql(db.execute(
        "DELETE FROM namespace_removals WHERE node_id=?1 AND intent_id=?2 AND owner_id=?3",
        params![id, fence.intent, fence.owner],
    ))?;
    if changed != 1 || raw_pending(db, id)?.is_some() {
        return Err(Error::Internal);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
