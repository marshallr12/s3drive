//! Canonical initial publication and original deferred ownership share one effect.
//! Physical S3 fields are caller claims, as for ordinary empty creation.
use super::*;
fn is_deferred(r: &Apply) -> bool {
    matches!(r.mutation, Mutation::CreateDeferredEmptyManifest { .. })
}
/// Project only the canonical physical proof; never store or send this projection.
fn ordinary(r: &Apply) -> Result<Apply> {
    let Mutation::CreateDeferredEmptyManifest {
        parent,
        parent_revision,
        name,
        store,
        upload_operation,
        etag,
        version,
        ..
    } = &r.mutation
    else {
        return Err(Error::Invalid);
    };
    Ok(Apply {
        volume: r.volume.clone(),
        operation: r.operation.clone(),
        expected_sequence: r.expected_sequence,
        mutation: Mutation::CreateEmptyManifest {
            parent: parent.clone(),
            parent_revision: *parent_revision,
            name: name.clone(),
            store: store.clone(),
            upload_operation: upload_operation.clone(),
            etag: etag.clone(),
            version: version.clone(),
        },
    })
}
pub(in crate::namespace) fn preflight(db: &Connection, r: &Apply) -> Result<()> {
    if !is_deferred(r) {
        return Ok(());
    }
    let version: i32 = sql(db.pragma_query_value(None, "user_version", |row| row.get(0)))?;
    if version < 42 {
        return Err(Error::Internal);
    }
    empty::preflight(db, &ordinary(r)?)
}
/// Reject new wire records under old markers without rewriting legacy bytes.
pub(crate) fn require_pre42(db: &Connection) -> crate::Result<()> {
    let mut q=db.prepare("SELECT CASE WHEN length(request) BETWEEN 1 AND 16384 THEN request END,CASE WHEN length(outcome) BETWEEN 1 AND 65536 THEN outcome END FROM namespace_operations")?;
    let mut rows = q.query([])?;
    while let Some(row) = rows.next()? {
        let request: Option<Vec<u8>> = row.get(0)?;
        let outcome: Option<Vec<u8>> = row.get(1)?;
        let new_request = request
            .as_deref()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok())
            .is_some_and(|v| {
                v.get("mutation")
                    .and_then(|v| v.get("kind"))
                    .and_then(|v| v.as_str())
                    == Some("create-deferred-empty-manifest")
            });
        let new_outcome = outcome
            .as_deref()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok())
            .is_some_and(|v| {
                v.get("Ok")
                    .and_then(|v| v.get("artifact"))
                    .and_then(|v| v.get("type"))
                    .and_then(|v| v.as_str())
                    == Some("published-removal")
            });
        if new_request || new_outcome {
            return Err("deferred canonical creation records require schema42".into());
        }
    }
    Ok(())
}
pub(super) fn mutate(db: &Connection, r: &Apply, sequence: u64) -> Result<(String, Artifact)> {
    preflight(db, r)?;
    let Mutation::CreateDeferredEmptyManifest { owner, .. } = &r.mutation else {
        return Err(Error::Invalid);
    };
    let (node, artifact) = empty::mutate(db, &ordinary(r)?, sequence)?;
    let Artifact::Published(payload) = artifact else {
        return Err(Error::Internal);
    };
    removals::created(db, r, sequence as i64)?;
    empty::checkpoint(&ordinary(r)?, "OwnershipStored")?;
    Ok((
        node,
        Artifact::PublishedRemoval {
            payload,
            fence: RemovalFence {
                intent: r.operation.clone(),
                owner: owner.clone(),
            },
        },
    ))
}
pub(in crate::namespace) fn replay(db: &Connection, r: &Apply, receipt: &Receipt) -> Result<()> {
    preflight(db, r)?;
    let Mutation::CreateDeferredEmptyManifest { owner, .. } = &r.mutation else {
        return Err(Error::Internal);
    };
    let Some(Artifact::PublishedRemoval { payload, fence }) = &receipt.artifact else {
        return Err(Error::Internal);
    };
    if r.validate().is_err()
        || !fence.valid()
        || fence.intent != r.operation
        || fence.owner != *owner
    {
        return Err(Error::Internal);
    }
    // Initial ownership evidence is immutable even after later close/cancel/move.
    let event:(String,String,String,String,i64)=sql(db.query_row(
        "SELECT node_id,intent_id,owner_id,mode,sequence FROM namespace_removal_events WHERE node_id=?1 AND operation_id=?1",
        [&r.operation],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))))?;
    let events: i64 = sql(db.query_row(
        "SELECT count(*) FROM namespace_removal_events WHERE operation_id=?1",
        [&r.operation],
        |row| row.get(0),
    ))?;
    if events != 1
        || event
            != (
                r.operation.clone(),
                fence.intent.clone(),
                fence.owner.clone(),
                "deferred".into(),
                i64::try_from(receipt.sequence).map_err(|_| Error::Internal)?,
            )
    {
        return Err(Error::Internal);
    }
    let mut canonical = receipt.clone();
    canonical.artifact = Some(Artifact::Published(payload.clone()));
    empty::replay(db, &ordinary(r)?, &canonical).map_err(|_| Error::Internal)
}
#[cfg(test)]
mod tests;
