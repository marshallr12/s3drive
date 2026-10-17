//! Append-only mode evidence. Rows and exact operation results commit together.
use super::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Event {
    pub node: String,
    pub operation: String,
    pub fence: RemovalFence,
    pub mode: String,
    pub sequence: u64,
}
impl Event {
    pub fn closed(&self) -> bool {
        matches!(self.mode.as_str(), "cancelled" | "committed")
    }
}
pub(super) fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    Ok(Event {
        node: row.get(0)?,
        operation: row.get(1)?,
        fence: RemovalFence {
            intent: row.get(2)?,
            owner: row.get(3)?,
        },
        mode: row.get(4)?,
        sequence: row.get::<_, i64>(5)?.try_into().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Integer,
                Box::new(e),
            )
        })?,
    })
}
pub(super) fn latest(db: &Connection, id: &str) -> Result<Option<Event>> {
    sql(db.query_row("SELECT node_id,operation_id,intent_id,owner_id,mode,sequence FROM namespace_removal_events WHERE node_id=?1 ORDER BY sequence DESC LIMIT 1",[id],read).optional())
}
pub(super) fn get(db: &Connection, id: &str, operation: &str) -> Result<Option<Event>> {
    sql(db.query_row("SELECT node_id,operation_id,intent_id,owner_id,mode,sequence FROM namespace_removal_events WHERE node_id=?1 AND operation_id=?2",params![id,operation],read).optional())
}
pub(super) fn insert(
    db: &Connection,
    r: &Apply,
    id: &str,
    fence: &RemovalFence,
    mode: &str,
    sequence: u64,
) -> Result<()> {
    let event = Event {
        node: id.into(),
        operation: r.operation.clone(),
        fence: fence.clone(),
        mode: mode.into(),
        sequence,
    };
    let rows = sql(db.execute(
        "INSERT INTO namespace_removal_events VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            id,
            r.operation,
            fence.intent,
            fence.owner,
            mode,
            sequence as i64
        ],
    ))?;
    if rows != 1 || get(db, id, &r.operation)? != Some(event) {
        return Err(Error::Internal);
    }
    Ok(())
}
pub(super) fn proof(db: &Connection, event: &Event) -> Result<Receipt> {
    let (request, outcome) = stored(db, &event.operation)?.ok_or(Error::Internal)?;
    let request: Apply = serde_json::from_slice(&request).map_err(|_| Error::Internal)?;
    let outcome: Result<Receipt> = serde_json::from_slice(&outcome).map_err(|_| Error::Internal)?;
    let receipt = outcome.map_err(|_| Error::Internal)?;
    if request
        .mutation
        .owned_destination()
        .is_some_and(|(n, _)| n.id == event.node)
    {
        return replacement::proof(db, event, &request, &receipt);
    }
    if matches!(request.mutation, Mutation::MoveDeferredEntry { .. }) {
        moves::proof(db, event, &request, &receipt)?;
        return Ok(receipt);
    }
    let (id, parent, fence, mode, guard) = match &request.mutation {
        Mutation::CreateDeferredEntry { parent, owner, .. } => {
            origin(db, &event.node, &event.fence)?;
            (
                &request.operation,
                Some(parent),
                RemovalFence {
                    intent: request.operation.clone(),
                    owner: owner.clone(),
                },
                "deferred",
                None,
            )
        }
        Mutation::ArmRemoval {
            node,
            parent,
            owner,
        } => (
            &node.id,
            Some(&parent.id),
            RemovalFence {
                intent: request.operation.clone(),
                owner: owner.clone(),
            },
            "pending",
            Some(node),
        ),
        Mutation::PrepareRemoval {
            node,
            parent,
            owner,
        } => (
            &node.id,
            Some(&parent.id),
            RemovalFence {
                intent: request.operation.clone(),
                owner: owner.clone(),
            },
            "deferred",
            Some(node),
        ),
        Mutation::TransitionRemoval {
            node,
            parent,
            fence,
            mode,
            ..
        } => (
            &node.id,
            Some(&parent.id),
            fence.clone(),
            mode.text(),
            Some(node),
        ),
        Mutation::CancelRemoval { node, fence } => (node, None, fence.clone(), "cancelled", None),
        Mutation::CommitRemoval {
            node,
            parent,
            fence,
        } => (
            &node.id,
            Some(&parent.id),
            fence.clone(),
            "committed",
            Some(node),
        ),
        _ => return Err(Error::Internal),
    };
    if request.validate().is_err()
        || request.operation != event.operation
        || request.volume != volume(db)?.id
        || *id != event.node
        || receipt.node.id != event.node
        || !fence.valid()
        || fence != event.fence
        || mode != event.mode
        || event.sequence != receipt.sequence
        || receipt.sequence != receipt.node.revision
        || receipt.sequence <= request.expected_sequence
        || guard.is_some_and(|g| g.revision >= receipt.sequence)
        || parent.is_some_and(|p| receipt.node.parent.as_ref() != Some(p))
        || receipt.node.deleted != (mode == "committed")
        || receipt.artifact != Some(Artifact::Removal(fence))
        || receipt.node.payload.as_ref().is_some_and(|p| {
            !p.valid() || p.reservation.node != event.node || p.reservation.volume != request.volume
        })
    {
        return Err(Error::Internal);
    }
    Ok(receipt)
}
pub(super) fn migrate(db: &Connection) -> Result<()> {
    let mut statement = sql(db.prepare(
        "SELECT node_id,intent_id,owner_id FROM namespace_removals ORDER BY node_id LIMIT 4097",
    ))?;
    let rows = sql(sql(statement.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    }))?
    .collect::<rusqlite::Result<Vec<_>>>())?;
    if rows.len() > MAX_NODES as usize {
        return Err(Error::Internal);
    }
    for (id, intent, owner) in rows {
        let fence = RemovalFence { intent, owner };
        if !fence.valid() || id == ROOT || node(db, &id)?.deleted {
            return Err(Error::Internal);
        }
        let (receipt, mode) = origin(db, &id, &fence)?;
        if mode != RemovalMode::Pending {
            return Err(Error::Internal);
        }
        verify_current(db, &id, &receipt, &fence, mode)?;
        let request: Apply =
            serde_json::from_slice(&stored(db, &fence.intent)?.ok_or(Error::Internal)?.0)
                .map_err(|_| Error::Internal)?;
        insert(db, &request, &id, &fence, "pending", receipt.sequence)?;
        let rows=sql(db.execute("UPDATE namespace_removals SET event_id=intent_id WHERE node_id=?1 AND event_id IS NULL AND mode='pending'",[&id]))?;
        if rows != 1 {
            return Err(Error::Internal);
        }
        check_effect(db, &id)?;
    }
    Ok(())
}
