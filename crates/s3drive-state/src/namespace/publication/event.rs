//! Opt-in ownership event binding. Legacy guards retain their exact semantics.
use super::*;

pub(in crate::namespace) fn preflight(db: &Connection, request: &Apply) -> Result<()> {
    let version: i32 = sql(db.pragma_query_value(None, "user_version", |r| r.get(0)))?;
    if version >= 46 {
        return Ok(());
    }
    let new = match &request.mutation {
        Mutation::ReserveManifest { guard, .. } => guard.removal_claim.is_some(),
        Mutation::Publish {
            reservation: id, ..
        } => match reservation(db, id) {
            Ok(r) => r.guard.as_ref().is_some_and(|g| g.removal_claim.is_some()),
            Err(_) => false, // Normal replay/mutation retains its existing error behavior.
        },
        _ => false,
    };
    if new { Err(Error::Internal) } else { Ok(()) }
}

fn current_schema(db: &Connection) -> Result<()> {
    let version: i32 = sql(db.pragma_query_value(None, "user_version", |r| r.get(0)))?;
    if version < 46 {
        return Err(Error::Internal);
    }
    Ok(())
}

/// Immutable request/event evidence only; current ownership may since have closed.
pub(super) fn validate_replay(db: &Connection, request: &Apply) -> Result<()> {
    match &request.mutation {
        Mutation::ReserveManifest { node, guard, .. } => {
            if let Some(claim) = &guard.removal_claim {
                current_schema(db)?;
                removals::publication_claim_history(db, node, claim, request.expected_sequence)?;
            }
        }
        Mutation::Publish {
            reservation: id, ..
        } => {
            let r = reservation(db, id)?;
            if r.guard.as_ref().is_some_and(|g| g.removal_claim.is_some()) {
                current_schema(db)?;
                let (raw, outcome) = stored(db, id)?.ok_or(Error::Internal)?;
                let original: Apply = decode(raw)?;
                original.validate().map_err(|_| Error::Internal)?;
                if !matches!(original.mutation, Mutation::ReserveManifest { .. })
                    || original.operation != *id
                {
                    return Err(Error::Internal);
                }
                let saved: Result<Receipt> = decode(outcome)?;
                let saved = saved.map_err(|_| Error::Internal)?;
                if saved.artifact != Some(Artifact::Reservation(r)) {
                    return Err(Error::Internal);
                }
                validate_manifest_replay(db, &original, &saved)?;
            }
        }
        _ => (),
    }
    Ok(())
}

/// Only fixed metadata columns, never object bytes or clean-cache/staging data.
/// Any presence of the new field (including null) is a new shape. Reject before
/// a transaction-local version jump can make that shape appear to be legacy.
pub(crate) fn require_pre46(db: &Connection) -> crate::Result<()> {
    const COLUMNS: &[(&str, &str)] = &[
        ("namespace_nodes", "payload"),
        ("namespace_uploads", "descriptor"),
        ("namespace_uploads", "publication"),
        ("namespace_operations", "request"),
        ("namespace_operations", "outcome"),
        ("managed_publications", "input"),
        ("managed_publications", "reservation"),
        ("managed_publications", "publication_request"),
        ("managed_publications", "receipt"),
        ("managed_rejections", "rejection"),
        ("logical_edits", "input"),
        ("logical_edits", "publication"),
        ("logical_edits", "receipt"),
        ("logical_files", "input"),
        ("logical_files", "observed_base"),
        ("namespace_intents", "input"),
        ("namespace_intents", "outcome"),
        ("windows_open_journal", "input"),
        ("windows_open_events", "action"),
        ("windows_session_recoveries", "input"),
        ("windows_session_recoveries", "proof"),
        ("chunk_mount_role", "record"),
        ("manifest_creations", "record"),
        ("chunk_plans", "input"),
        ("chunk_publications", "record"),
        ("chunk_source_settlements", "record"),
        ("sparse_view_files", "binding"),
        ("sparse_views", "record"),
        ("sparse_view_operations", "request"),
        ("sparse_preparations", "input"),
        ("sparse_preparations", "state"),
        ("sparse_preparation_objects", "record"),
        ("sparse_publications", "record"),
        ("sparse_publication_checks", "record"),
        ("multipart_plans", "input"),
    ];
    fn contains_new(v: &serde_json::Value) -> bool {
        match v {
            serde_json::Value::Object(m) => {
                m.contains_key("removal_claim") || m.values().any(contains_new)
            }
            serde_json::Value::Array(a) => a.iter().any(contains_new),
            _ => false,
        }
    }
    for (table, column) in COLUMNS {
        let query = format!(
            "SELECT CASE WHEN typeof({column})='blob' AND length({column}) BETWEEN 1 AND 16777216 THEN {column} END FROM {table} WHERE {column} IS NOT NULL"
        );
        let mut q = db.prepare(&query)?;
        for row in q.query_map([], |r| r.get::<_, Option<Vec<u8>>>(0))? {
            let raw = row?.ok_or("pre46 metadata is not a bounded BLOB")?;
            // Some historical authority records are intentionally opaque. Their
            // existing validators/replay behavior remains authoritative.
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&raw)
                && contains_new(&v)
            {
                return Err("event-bound manifest records require schema46".into());
            }
        }
    }
    Ok(())
}

/// Owned subprocess fault boundary; inert unless all fixture selectors match.
pub(in crate::namespace) fn checkpoint(request: &Apply, stage: &str) -> Result<()> {
    if std::env::var("S3DRIVE_EVENT_MANIFEST_CRASH_POINT")
        .ok()
        .as_deref()
        != Some(stage)
        || std::env::var("S3DRIVE_EVENT_MANIFEST_CRASH_OPERATION")
            .ok()
            .as_deref()
            != Some(request.operation.as_str())
    {
        return Ok(());
    }
    let Some(path) = std::env::var_os("S3DRIVE_EVENT_MANIFEST_CRASH_READY") else {
        return Ok(());
    };
    std::fs::write(path, b"ready").map_err(|_| Error::Internal)?;
    use std::io::Read;
    let _ = std::io::stdin().read(&mut [0u8]);
    Ok(())
}
