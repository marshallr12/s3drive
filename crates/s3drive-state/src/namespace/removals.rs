//! Reversible namespace reservation. No OS callback or S3 deletion occurs here.
use super::*;
mod events;
pub(super) mod moves;
pub(super) mod replacement;

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
/// Exact owned destination epoch consumed by an atomic replacement. The source
/// keeps its identity; the displaced node and every payload remain retained.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemovalClaim {
    pub fence: RemovalFence,
    pub previous: String,
    pub mode: RemovalMode,
}
/// Publication can retain a committed old identity after unlink/replacement.
/// Cancellation is deliberately not a publication mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemovalPublicationMode {
    Deferred,
    Pending,
    Committed,
}
impl RemovalPublicationMode {
    pub fn detached(self) -> bool {
        self == Self::Committed
    }
    fn text(self) -> &'static str {
        match self {
            Self::Deferred => "deferred",
            Self::Pending => "pending",
            Self::Committed => "committed",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemovalPublicationClaim {
    pub fence: RemovalFence,
    pub previous: String,
    pub mode: RemovalPublicationMode,
}
impl From<RemovalClaim> for RemovalPublicationClaim {
    fn from(value: RemovalClaim) -> Self {
        Self {
            fence: value.fence,
            previous: value.previous,
            mode: match value.mode {
                RemovalMode::Deferred => RemovalPublicationMode::Deferred,
                RemovalMode::Pending => RemovalPublicationMode::Pending,
            },
        }
    }
}
/// A coherent metadata observation, not a live filesystem handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemovalEpoch {
    pub node: Node,
    pub claim: RemovalPublicationClaim,
}
impl Mutation {
    pub fn owned_destination(&self) -> Option<(&NodeGuard, &RemovalClaim)> {
        match self {
            Self::MoveEntry {
                destination: Some(node),
                destination_removal: Some(claim),
                ..
            }
            | Self::MoveDeferredEntry {
                destination: Some(node),
                destination_removal: Some(claim),
                ..
            } => Some((node, claim)),
            _ => None,
        }
    }
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
    if let Mutation::CreateDeferredEmptyManifest { owner, .. } = &input.mutation {
        if input.volume != volume(db)?.id
            || input.operation != fence.intent
            || input.operation != id
            || owner != &fence.owner
        {
            return Err(Error::Internal);
        }
        publication::deferred::replay(db, &input, &receipt)?;
        return Ok((receipt, RemovalMode::Deferred));
    }
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
    moves::verify_revision(&now, &receipt.node, fence)?;
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
            moves::chain(db, &event)?;
            Ok(None)
        }
        (Some(tag), Some(event))
            if !event.closed()
                && event.operation == tag.event
                && event.fence == tag.fence
                && event.mode == tag.mode.text() =>
        {
            let receipt = moves::chain(db, &event)?;
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
    if let Some(claim) = guard.and_then(|g| g.removal_claim.as_ref())
        && claim.mode == RemovalPublicationMode::Committed
    {
        let active = pending(db, id)?;
        let e = events::latest(db, id)?.ok_or(Error::Conflict)?;
        if active.is_some()
            || !node(db, id)?.deleted
            || e.operation != claim.previous
            || e.fence != claim.fence
            || e.mode != "committed"
        {
            return Err(Error::Conflict);
        }
        events::proof(db, &e)?;
        return Ok(());
    }
    match (pending(db, id)?, guard.and_then(|g| g.removal.as_ref())) {
        (None, None) => Ok(()),
        (Some(actual), Some(expected)) if actual.fence == *expected => {
            if let Some(claim) = guard.and_then(|g| g.removal_claim.as_ref())
                && (claim.fence != actual.fence
                    || claim.previous != actual.event
                    || claim.mode.text() != actual.mode.text())
            {
                return Err(Error::Conflict);
            }
            Ok(())
        }
        (Some(_), None) => Err(Error::DeletePending),
        (Some(_), Some(_)) => Err(Error::Conflict),
        (None, Some(_)) => Err(Error::Conflict),
    }
}
impl State {
    /// Explicit inspection for the disposition owner/recovery. Normal Node
    /// lookup refuses pending names; this API does not create a read handle.
    /// Read the node and independently checked current ownership event in one
    /// read transaction. This is observational metadata, not native activation.
    pub fn namespace_removal_epoch(
        &mut self,
        volume_id: &str,
        id: &str,
    ) -> Result<Option<RemovalEpoch>> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred))?;
        let version: i32 = sql(tx.pragma_query_value(None, "user_version", |r| r.get(0)))?;
        if version < 46 {
            return Err(Error::Internal);
        }
        check_volume(&tx, volume_id)?;
        let n = node(&tx, id)?;
        let claim = match pending(&tx, id)? {
            Some(t) => Some(RemovalPublicationClaim::from(RemovalClaim {
                fence: t.fence,
                previous: t.event,
                mode: t.mode,
            })),
            None => match events::latest(&tx, id)? {
                Some(e) if e.mode == "committed" && n.deleted => {
                    events::proof(&tx, &e)?;
                    Some(RemovalPublicationClaim {
                        fence: e.fence,
                        previous: e.operation,
                        mode: RemovalPublicationMode::Committed,
                    })
                }
                _ => None,
            },
        };
        let result = claim.map(|claim| RemovalEpoch { node: n, claim });
        sql(tx.commit())?;
        Ok(result)
    }
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
    let (Mutation::CreateDeferredEntry { owner, .. }
    | Mutation::CreateDeferredEmptyManifest { owner, .. }) = &r.mutation
    else {
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
    publication::empty::checkpoint(r, "RemovalStored")?;
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

pub(in crate::namespace) fn publication_claim_history(
    db: &Connection,
    id: &str,
    claim: &RemovalPublicationClaim,
    before: u64,
) -> Result<()> {
    let e = events::get(db, id, &claim.previous)?.ok_or(Error::Internal)?;
    if e.fence != claim.fence || e.mode != claim.mode.text() || e.sequence > before {
        return Err(Error::Internal);
    }
    events::proof(db, &e)?;
    Ok(())
}
