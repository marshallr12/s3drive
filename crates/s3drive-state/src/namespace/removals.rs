//! Reversible namespace reservation. No OS callback or S3 deletion occurs here.
use super::*;

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
}
pub(super) fn check_schema(db: &Connection) -> crate::Result<()> {
    db.prepare("SELECT node_id,intent_id,owner_id FROM namespace_removals LIMIT 0")?;
    Ok(())
}
fn raw_pending(db: &Connection, id: &str) -> Result<Option<RemovalFence>> {
    let fence = sql(db
        .query_row(
            "SELECT intent_id,owner_id FROM namespace_removals WHERE node_id=?",
            [id],
            |r| {
                Ok(RemovalFence {
                    intent: r.get(0)?,
                    owner: r.get(1)?,
                })
            },
        )
        .optional())?;
    if let Some(fence) = &fence
        && (!fence.valid() || id == ROOT || node(db, id)?.deleted)
    {
        return Err(Error::Internal);
    }
    Ok(fence)
}
// Validate the live reservation against its immutable, atomically committed arm
// operation. Valid-looking changes to owner/intent or the reserved identity must
// fail closed, not become a cancellation/publication capability.
fn pending(db: &Connection, id: &str) -> Result<Option<RemovalFence>> {
    let Some(fence) = raw_pending(db, id)? else {
        return Ok(None);
    };
    let (input, outcome) = stored(db, &fence.intent)?.ok_or(Error::Internal)?;
    let input: Apply = serde_json::from_slice(&input).map_err(|_| Error::Internal)?;
    let outcome: Result<Receipt> = serde_json::from_slice(&outcome).map_err(|_| Error::Internal)?;
    let receipt = outcome.map_err(|_| Error::Internal)?;
    let Mutation::ArmRemoval {
        node: before,
        parent,
        owner,
    } = &input.mutation
    else {
        return Err(Error::Internal);
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
                    != Some(&fence)))
    {
        return Err(Error::Internal);
    }
    Ok(Some(fence))
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
        (Some(actual), Some(expected)) if actual == *expected => Ok(()),
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
        Ok(pending(&self.connection, id)?.map(|fence| PendingRemoval { node: n, fence }))
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
    require_clear(db, &g.id)?;
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
pub(super) fn mutate(db: &Connection, r: &Apply, sequence: i64) -> Result<(String, RemovalFence)> {
    if volume(db)?.name_policy != names::PORTABLE {
        return Err(Error::Invalid);
    }
    match &r.mutation {
        Mutation::ArmRemoval {
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
            empty(db, &n.id)?;
            let fence = RemovalFence {
                intent: r.operation.clone(),
                owner: owner.clone(),
            };
            let inserted = sql(db.execute(
                "INSERT INTO namespace_removals VALUES(?1,?2,?3)",
                params![n.id, fence.intent, fence.owner],
            ))?;
            if inserted != 1 || raw_pending(db, &n.id)? != Some(fence.clone()) {
                return Err(Error::Internal);
            }
            revision(db, &n, sequence, false)?;
            Ok((n.id, fence))
        }
        Mutation::CancelRemoval { node: id, fence } => {
            let n = node(db, id)?;
            if n.deleted || pending(db, id)?.as_ref() != Some(fence) {
                return Err(Error::Conflict);
            }
            release(db, id, fence)?;
            revision(db, &n, sequence, false)?;
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
            if n.parent.as_ref() != Some(&p.id) || pending(db, &n.id)?.as_ref() != Some(fence) {
                return Err(Error::Conflict);
            }
            empty(db, &n.id)?;
            release(db, &n.id, fence)?;
            revision(db, &n, sequence, true)?;
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
    if changed != 1 || pending(db, id)?.is_some() {
        return Err(Error::Internal);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
