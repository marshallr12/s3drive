//! Experimental whole-forest eviction, never an inferred Close or remote GC.
//! A retained reader or any unfinished dependency protects the entire forest.
use super::*;
use crate::namespace_client::{self as names, opens};
use crate::sparse_views::preparation as prep;

pub(in crate::sparse_views) fn retire(
    state: &mut State,
    key: &crate::chunk_mount::Key,
) -> Result<bool> {
    state.verify_staging_storage()?;
    let readers = state.staging_readers.active();
    let tx = state
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    ledger::no_triggers(&tx)?;
    if !crate::chunk_mount::quiescent_enabled(&tx)? {
        return Err(Failure::Conflict.into());
    }
    let scope = crate::chunk_mount::read_scope(&tx, key)?;
    let binding = scope.binding().clone();
    let before = prep::ledger::inspect_scoped(&scope)?;
    let roots = ledger::inspect(&tx)?;
    ledger::validate_source_snapshot(&tx, &roots, &before)?;
    if !readers.is_empty() || !before.views.read_pins.pins.is_empty() {
        return Ok(false);
    }
    if !before.views.anchors.is_empty() {
        return Err(Failure::Conflict.into());
    }
    for file in before.views.files.values() {
        if !before.views.record(file.head)?.clean() {
            return Ok(false);
        }
    }
    if !names::quiescent(&tx)? {
        return Ok(false);
    }
    let Some(records) = opens::quiescent_candidates(&tx)? else {
        return Ok(false);
    };
    let Some(creators) = crate::manifest_creation::quiescent_generations(&tx)? else {
        return Ok(false);
    };
    let mut generations: BTreeSet<i64> = creators.into_iter().map(|g| g.value()).collect();
    for (plan, saved) in &before.plans {
        let info = saved.info(&before.views)?;
        let Some(root) = roots.records.get(plan) else {
            return Ok(false);
        };
        if root.receipt().is_none() || root.acknowledgment().is_none() {
            return Ok(false);
        }
        generations.insert(root.physical.generation.value());
        for (_, level, slot) in before.objects.keys().filter(|(p, _, _)| p == plan) {
            if let Some(transfer) = prep::transfers::transfer(
                &tx,
                &before,
                &info,
                prep::Position {
                    level: *level,
                    slot: *slot,
                },
            )? {
                generations.insert(transfer.generation().ok_or(Failure::Incomplete)?.value());
            }
        }
    }
    let actual = tx
        .prepare("SELECT generation_id FROM staged_generations")?
        .query_map([], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<BTreeSet<_>>>()?;
    if actual != generations {
        return Ok(false);
    }
    let namespace_count: i64 =
        tx.query_row("SELECT count(*) FROM namespace_intents", [], |r| r.get(0))?;
    let creator_count: i64 =
        tx.query_row("SELECT count(*) FROM manifest_creations", [], |r| r.get(0))?;
    if before.views.records.is_empty()
        && records.is_empty()
        && namespace_count == 0
        && creator_count == 0
    {
        return Ok(false);
    }
    // No validated read scope crosses the first destructive statement. The
    // permanent owner, monotonically consumed IDs and retry mode are untouched.
    drop(scope);
    prep::journal::checkpoint("quiescent-retire", "Eligible");
    // Source files/views have cyclic foreign keys. Defer only within this
    // transaction; never disable checking, and explicitly check before commit.
    tx.pragma_update(None, "defer_foreign_keys", true)?;
    for table in [
        "sparse_publication_checks",
        "sparse_publications",
        "sparse_publication_role",
        "sparse_preparation_objects",
        "sparse_preparations",
        "sparse_preparation_blobs",
        "sparse_view_operations",
        "sparse_view_files",
        "sparse_views",
        "sparse_view_blocks",
        "sparse_view_role",
    ] {
        tx.execute(&format!("DELETE FROM {table}"), [])?;
    }
    prep::journal::checkpoint("quiescent-retire", "SourceErased");
    for table in [
        "recovered_windows_opens",
        "windows_session_recoveries",
        "windows_open_events",
        "retired_windows_opens",
        "retained_windows_opens",
        "windows_open_journal",
        "namespace_intents",
        "manifest_creations",
        "manifest_creation_role",
    ] {
        tx.execute(&format!("DELETE FROM {table}"), [])?;
    }
    prep::journal::checkpoint("quiescent-retire", "LifecycleErased");
    for generation in &generations {
        collect::erase_generation(&tx, generation)?;
    }
    prep::journal::checkpoint("quiescent-retire", "PhysicalErased");
    crate::chunk_mount::validate_all(&tx)?;
    let fresh_scope = crate::chunk_mount::read_scope(&tx, key)?;
    let after = prep::ledger::inspect_scoped(&fresh_scope)?;
    if fresh_scope.binding() != &binding
        || !after.views.files.is_empty()
        || !after.views.records.is_empty()
        || !after.views.blocks.is_empty()
        || !after.plans.is_empty()
        || !after.objects.is_empty()
        || !after.blobs.is_empty()
        || !ledger::inspect(&tx)?.empty()
        || tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
            [],
            |r| r.get::<_, bool>(0),
        )?
    {
        return Err(Failure::Corrupt.into());
    }
    drop(fresh_scope);
    prep::journal::checkpoint("quiescent-retire", "Validated");
    tx.commit()?;
    state.native_sparse.forget_retired(&records);
    prep::journal::checkpoint("quiescent-retire", "Committed");
    Ok(true)
}
