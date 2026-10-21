//! Prospective reservations derive from retained creator proof, never counters.
use super::*;
use crate::namespace_client::{self as names, opens::creation};

pub(crate) fn original(db: &Connection, operation: &str) -> Result<Record> {
    let r = inspect_with_witnesses(db, false)?
        .remove(operation)
        .ok_or(Failure::Missing)?;
    check_witness(db, &r)?;
    Ok(r)
}

pub(crate) fn reserved(db: &Connection, records: &[names::Record]) -> Result<usize> {
    let mut slots = 0usize;
    for r in inspect_with_witnesses(db, false)?
        .values()
        .filter(|r| r.input.open.is_some())
    {
        check_witness(db, r)?;
        let imported = records
            .iter()
            .find(|n| n.input.request.operation == r.input.operation);
        if let Some(imported) = imported {
            if r.request.as_ref() != Some(&imported.input.request)
                || imported.input.store != r.input.store
                || imported.input.source != r.input.source
                || imported.input.fingerprint != r.input.fingerprint
                || !imported.input.entries.is_empty()
                || imported.input.destination_control.is_some()
                || !matches!(&imported.outcome, Some(Ok(receipt)) if r.receipt.as_ref()==Some(receipt))
            {
                return Err(Failure::Conflict.into());
            }
        } else if r.rejection.is_none() {
            slots = slots.checked_add(3).ok_or(Failure::Quota)?;
        }
    }
    Ok(slots)
}

/// Inspect original-marker raw shapes before any transaction-local marker jump.
pub(crate) fn require_pre45(db: &Connection) -> Result<()> {
    for (sql, creator) in [
        (
            "SELECT CASE WHEN typeof(record)='blob' AND length(record) BETWEEN 1 AND 65536 THEN record END FROM manifest_creations",
            true,
        ),
        (
            "SELECT CASE WHEN typeof(input)='blob' AND length(input) BETWEEN 1 AND 16384 THEN input END FROM windows_open_journal",
            false,
        ),
    ] {
        let mut stmt = db.prepare(sql)?;
        for row in stmt.query_map([], |r| r.get::<_, Option<Vec<u8>>>(0))? {
            let raw = row?.ok_or(Failure::Corrupt)?;
            let value: serde_json::Value = serde_json::from_slice(&raw)?;
            let new_shape = if creator {
                value.get("format").and_then(|v| v.as_u64()) == Some(3)
                    || value.get("input").and_then(|v| v.get("open")).is_some()
            } else {
                value.get("creation").is_some()
            };
            if new_shape {
                return Err(Failure::Corrupt.into());
            }
        }
    }
    Ok(())
}

pub(super) fn begin_open(db: &Connection, input: &Input, runtime: &str) -> Result<()> {
    creation::insert(db, input, runtime)
}
