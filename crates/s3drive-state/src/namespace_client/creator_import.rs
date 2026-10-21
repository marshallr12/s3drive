//! Historical import from the same verified creator/SQLite owner, never an RPC.
use super::*;

pub(super) fn canonical(m: &Mutation) -> bool {
    matches!(m, Mutation::CreateDeferredEmptyManifest { .. })
}
/// Reuse the creator's exact canonical node/payload/fence receipt checker.
pub(super) fn check_receipt(input: &Input, receipt: &Receipt) -> Result<()> {
    let Mutation::CreateDeferredEmptyManifest {
        parent,
        parent_revision,
        name,
        owner,
        ..
    } = &input.request.mutation
    else {
        return Err("not a deferred canonical creation".into());
    };
    if !input.entries.is_empty() || input.destination_control.is_some() {
        return Err("canonical creation cannot substitute namespace preconditions".into());
    }
    let original = crate::manifest_creation::Input {
        volume: input.request.volume.clone(),
        operation: input.request.operation.clone(),
        expected_sequence: input.request.expected_sequence,
        parent: namespace::NodeGuard {
            id: parent.clone(),
            revision: *parent_revision,
        },
        name: name.clone(),
        store: input.store.clone(),
        source: input.source,
        fingerprint: input.fingerprint.clone(),
        owner: Some(owner.clone()),
    };
    original.check_receipt(&input.request, receipt)
}
fn expected(db: &Connection, operation: &str) -> Result<Record> {
    if operation == namespace::ROOT
        || operation.len() != 32
        || !operation
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid creator import operation".into());
    }
    let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 44 {
        return Err("creator import requires schema44".into());
    }
    let binding = crate::chunk_mount::load(db)?.ok_or("creator import requires a chunk owner")?;
    let original = crate::manifest_creation::verified_creation(db, operation)?;
    let creator = original.input();
    if creator.owner.as_deref() != Some(binding.owner.as_str()) {
        return Err("creator import changed original owner".into());
    }
    let input = Input {
        request: original
            .request()
            .ok_or("creator request is unknown")?
            .clone(),
        store: creator.store.clone(),
        source: creator.source,
        fingerprint: creator.fingerprint.clone(),
        entries: Vec::new(),
        destination_control: None,
    };
    input.validate()?;
    binding.namespace(&input)?;
    removals::check_identity(db, &input)?;
    let receipt = original
        .receipt()
        .ok_or("creator receipt is unknown")?
        .clone();
    check_receipt(&input, &receipt)?;
    Ok(Record {
        input,
        outcome: Some(Ok(receipt)),
    })
}
pub(super) fn check_record(db: &Connection, record: &Record) -> Result<()> {
    if canonical(&record.input.request.mutation) {
        let original = expected(db, &record.input.request.operation)?;
        if original.input != record.input || original.outcome != record.outcome {
            return Err("namespace creator provenance changed".into());
        }
    }
    Ok(())
}
pub(super) fn check_link(db: &Connection, input: &Input) -> Result<()> {
    if canonical(&input.request.mutation) {
        let saved = super::load(db, &input.request.operation)?
            .ok_or("canonical creator has not been imported")?;
        if saved.input != *input || !matches!(saved.outcome, Some(Ok(_))) {
            return Err("canonical creator admission changed".into());
        }
    }
    Ok(())
}
pub(super) fn check_admission(db: &Connection, input: &opens::OpenInput) -> Result<()> {
    if let Some(admission) = &input.admission {
        check_link(db, admission)?;
    }
    Ok(())
}
/// Called before a transaction-local marker change, without using role loaders
/// that deliberately require the exact current schema marker.
pub(crate) fn require_pre44(db: &Connection) -> Result<()> {
    let mut query = db.prepare("SELECT CASE WHEN typeof(input)='blob' AND length(input) BETWEEN 1 AND 8192 THEN input END FROM namespace_intents LIMIT 4097")?;
    let rows = query.query_map([], |r| r.get::<_, Option<Vec<u8>>>(0))?;
    let mut count = 0;
    for row in rows {
        count += 1;
        if count > LIMIT {
            return Err("namespace intent limit during migration".into());
        }
        let raw = row?.ok_or("namespace intent shape during migration")?;
        let input: Input = serde_json::from_slice(&raw)
            .map_err(|_| "namespace intent encoding during migration")?;
        if canonical(&input.request.mutation) {
            return Err("imported canonical creation found under an older schema marker".into());
        }
    }
    Ok(())
}
impl Journal<'_> {
    /// Retain an already acknowledged creation from this same chunk owner.
    /// The returned record is historical; activation has separate local facts.
    pub fn import_creation(&mut self, operation: &str) -> Result<Record> {
        let key = self
            .chunk
            .clone()
            .ok_or("creator import requires a chunk capability")?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::chunk_mount::require(&tx, &key)?;
        let original = expected(&tx, operation)?;
        if let Some(old) = super::load(&tx, operation)? {
            if old.input != original.input || old.outcome != original.outcome {
                return Err("original creator import identity changed".into());
            }
            tx.commit()?;
            return Ok(old);
        }
        // Import is an acknowledged historical fact, not a fresh mutation
        // admission. Project complete history/closing capacity before inserting.
        let mut projected = removals::records(&tx)?;
        projected.push(original.clone());
        removals::check_capacity(&projected)?;
        checkpoint("ImportBeforeStored")?;
        let written = tx.execute(
            "INSERT INTO namespace_intents VALUES(?1,?2,?3,?4)",
            params![
                operation,
                serde_json::to_vec(&original.input)?,
                serde_json::to_vec(
                    original
                        .outcome
                        .as_ref()
                        .ok_or("missing imported outcome")?
                )?,
                super::hash(&original)?
            ],
        )?;
        if written != 1
            || super::load(&tx, operation)?.is_none_or(|saved| {
                saved.input != original.input || saved.outcome != original.outcome
            })
        {
            return Err("creator import was not durably retained".into());
        }
        removals::check_capacity(&removals::records(&tx)?)?;
        checkpoint("ImportStored")?;
        tx.commit()?;
        checkpoint("ImportCommitted")?;
        Ok(original)
    }
}
pub(super) fn checkpoint(label: &str) -> Result<()> {
    #[cfg(test)]
    {
        use std::io::{Read, Write};
        if std::env::var("S3DRIVE_CREATOR_IMPORT_STOP").ok().as_deref() == Some(label) {
            println!("S3DRIVE_CREATOR_IMPORT_BOUNDARY {label}");
            std::io::stdout().flush()?;
            std::io::stdin().read_exact(&mut [0u8; 1])?;
        }
    }
    let _ = label;
    Ok(())
}
