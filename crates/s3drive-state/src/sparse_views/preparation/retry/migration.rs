//! Explicit populated ordinary-owner upgrade. No identity rewrite or deletion.
use super::*;
use std::collections::BTreeSet;

/// Validate the complete legacy dependency forest before changing admission.
/// Keeping known operation rows is essential: their random IDs cannot be
/// translated into scoped tokens without changing the meaning of exact retry.
fn eligible(db: &Connection, s: &ledger::Snapshot) -> Result<()> {
    let meta = publication::ledger::inspect(db)?;
    publication::ledger::validate_source_snapshot(db, &meta, s)?;
    let mut generations = BTreeSet::new();
    let mut clean_heads = BTreeSet::new();
    for (plan, saved) in &s.plans {
        let info = saved.info(&s.views)?;
        if info.ownership().is_some() || info.native_ownership().is_some() {
            return Err(Failure::Isolated.into());
        }
        let record = meta.records.get(plan).ok_or(Failure::Conflict)?;
        if record.receipt().is_none() || record.acknowledgment().is_none() {
            return Err(Failure::Conflict.into());
        }
        if let Some(anchor) = record.acknowledgment().and_then(|a| a.clean_view()) {
            clean_heads.insert(anchor);
        }
        // validate_source_snapshot independently verifies every physical receipt.
        // Enumerate the exact mapped set too: an unrelated generation must not
        // be stranded when generic fresh physical admission becomes forbidden.
        for (_, level, slot) in s.objects.keys().filter(|(id, _, _)| id == plan) {
            if let Some(t) = transfers::transfer(
                db,
                s,
                &info,
                Position {
                    level: *level,
                    slot: *slot,
                },
            )? {
                let generation = t.generation().ok_or(Failure::Conflict)?.value();
                if !generations.insert(generation) {
                    return Err(Failure::Corrupt.into());
                }
            }
        }
    }
    // A later dirty head is retained and refused, never overwritten by migration.
    // Every populated target must have an independently validated clean rebase.
    if !s.plans.is_empty()
        && s.views
            .files
            .values()
            .any(|f| !clean_heads.contains(&f.head))
    {
        return Err(Failure::Conflict.into());
    }
    let mut q =
        db.prepare("SELECT generation_id FROM staged_generations ORDER BY generation_id")?;
    let actual = q
        .query_map([], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<BTreeSet<_>>>()?;
    if actual != generations {
        return Err(Failure::Isolated.into());
    }
    Ok(())
}

impl Journal<'_> {
    /// Permanently migrate an explicitly selected, fully terminal ordinary
    /// populated owner to epoch-scoped fresh admission. Known legacy requests
    /// retain their original identities and exact replay. This is NOT GC or
    /// native-owner migration. No source, plan, root, pin or staging row changes.
    pub fn migrate_retry_safety(&mut self) -> Result<()> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let before = ledger::inspect(&tx)?;
        if tx.query_row("SELECT EXISTS(SELECT 1 FROM chunk_mount_role)", [], |r| {
            r.get::<_, bool>(0)
        })? {
            return Err(Failure::Isolated.into());
        }
        if enabled(&tx)? {
            return Ok(());
        }
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger')",
            [],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(Failure::Corrupt.into());
        }
        eligible(&tx, &before)?;
        journal::checkpoint("retry_migrate", "Eligible");
        if !super::super::super::scoped::enabled(&tx)? {
            tx.execute(
                "INSERT INTO sparse_scoped_mode VALUES(1,1,?1)",
                [super::super::super::scoped::mode_hash()?.as_slice()],
            )?;
        }
        journal::checkpoint("retry_migrate", "SourceStored");
        tx.execute(
            "INSERT INTO sparse_retry_mode VALUES(1,1,?1)",
            [mode_hash()?.as_slice()],
        )?;
        journal::checkpoint("retry_migrate", "Stored");
        if !enabled(&tx)? || ledger::inspect(&tx)? != before {
            return Err(Failure::Corrupt.into());
        }
        eligible(&tx, &before)?;
        journal::checkpoint("retry_migrate", "Validated");
        tx.commit()?;
        journal::checkpoint("retry_migrate", "Committed");
        Ok(())
    }
}
impl Client {
    pub async fn migrate_retry_safety(&self) -> Result<()> {
        self.call(|j| j.migrate_retry_safety()).await
    }
}
