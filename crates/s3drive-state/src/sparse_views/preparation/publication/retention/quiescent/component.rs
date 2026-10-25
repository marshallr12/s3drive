//! Closed connected components, with exact untouched-forest equality afterward.
use super::*;

fn closure(
    seed: &str,
    names: &[names::Record],
    opens: &[opens::OpenRecord],
) -> Result<BTreeSet<String>> {
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut add = |nodes: BTreeSet<String>| -> Result<()> {
        if let Some(first) = nodes.first() {
            for node in &nodes {
                edges.entry(first.clone()).or_default().insert(node.clone());
                edges.entry(node.clone()).or_default().insert(first.clone());
            }
        }
        if edges.len() > 16384 {
            return Err(Failure::Corrupt.into());
        }
        Ok(())
    };
    for record in names {
        add(names::retirement_nodes(&record.input))?;
    }
    for record in opens {
        if let Some(input) = &record.input.admission {
            let mut nodes = names::retirement_nodes(input);
            nodes.insert(record.input.node.clone());
            add(nodes)?;
        }
        for event in &record.events {
            if let opens::Action::Link { input, .. } = &event.action {
                let mut nodes = names::retirement_nodes(input);
                nodes.insert(record.input.node.clone());
                add(nodes)?;
            }
        }
    }
    let mut selected = BTreeSet::new();
    let mut pending = vec![seed.to_owned()];
    while let Some(node) = pending.pop() {
        if selected.insert(node.clone()) {
            if selected.len() > 16384 {
                return Err(Failure::Corrupt.into());
            }
            if let Some(next) = edges.get(&node) {
                pending.extend(next.iter().filter(|n| !selected.contains(*n)).cloned());
            }
        }
    }
    Ok(selected)
}

fn generations(
    snapshot: &prep::ledger::Snapshot,
    roots: &ledger::Meta,
    db: &Connection,
    plans: &BTreeSet<Id>,
) -> Result<BTreeSet<i64>> {
    let mut ids = BTreeSet::new();
    for plan in plans {
        let info = snapshot.info(*plan)?;
        if let Some(root) = roots.records.get(plan) {
            ids.insert(root.physical.generation.value());
        }
        for (_, level, slot) in snapshot.objects.keys().filter(|(p, _, _)| p == plan) {
            if let Some(transfer) = prep::transfers::transfer(
                db,
                snapshot,
                &info,
                prep::Position {
                    level: *level,
                    slot: *slot,
                },
            )? && let Some(generation) = transfer.generation()
            {
                ids.insert(generation.value());
            }
        }
    }
    Ok(ids)
}

/// Discovery is not collection permission; each seed receives a new proof.
pub(in crate::sparse_views) fn candidates(
    state: &mut State,
    key: &crate::chunk_mount::Key,
) -> Result<Vec<String>> {
    state.verify_staging_storage()?;
    let tx = state
        .connection
        .transaction_with_behavior(TransactionBehavior::Deferred)?;
    if !crate::chunk_mount::quiescent_enabled(&tx)? {
        return Err(Failure::Conflict.into());
    }
    let scope = crate::chunk_mount::read_scope(&tx, key)?;
    let saved = prep::ledger::inspect_scoped(&scope)?;
    let mut nodes = BTreeSet::new();
    for file in saved.views.files.values() {
        nodes.insert(file.binding.base()?.node().to_owned());
    }
    for record in names::retirement_records(&tx)? {
        nodes.extend(names::retirement_nodes(&record.input));
    }
    for record in opens::retained_records(&tx)? {
        nodes.insert(record.input.node);
    }
    for record in crate::manifest_creation::retirement_records(&tx)? {
        nodes.insert(record.input().operation.clone());
    }
    if nodes.len() > 16384 {
        return Err(Failure::Corrupt.into());
    }
    Ok(nodes.into_iter().collect())
}

pub(in crate::sparse_views) fn retire(
    state: &mut State,
    key: &crate::chunk_mount::Key,
    node: &str,
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
    let permanent = permanent_rows(&tx)?;
    let before = prep::ledger::inspect_scoped(&scope)?;
    let roots = ledger::inspect(&tx)?;
    ledger::validate_source_snapshot(&tx, &roots, &before)?;
    if !before.views.anchors.is_empty() {
        return Err(Failure::Conflict.into());
    }
    let names_before = names::retirement_records(&tx)?;
    let opens_before = opens::retained_records(&tx)?;
    let creators_before = crate::manifest_creation::retirement_records(&tx)?;
    let nodes = closure(node, &names_before, &opens_before)?;
    let targets = before
        .views
        .files
        .iter()
        .filter_map(|(target, file)| match file.binding.base() {
            Ok(base) if nodes.contains(base.node()) => Some(Ok(*target)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<BTreeSet<_>>>()?;
    for target in &targets {
        let file = before.views.files.get(target).ok_or(Failure::Corrupt)?;
        if !before.views.record(file.head)?.clean() {
            return Ok(false);
        }
    }
    for pin in before.views.read_pins.pins.values() {
        if targets.contains(
            &before
                .views
                .record(pin.view_id())?
                .request
                .binding
                .target()?,
        ) {
            return Ok(false);
        }
    }
    if !names::component_closed(&tx, key.volume(), &nodes)? {
        return Ok(false);
    }
    let Some(selected_opens) = opens::quiescent_component_candidates(&tx, &nodes)? else {
        return Ok(false);
    };
    let namespace_ids = names_before
        .iter()
        .filter(|r| !names::retirement_nodes(&r.input).is_disjoint(&nodes))
        .map(|r| r.input.request.operation.clone())
        .collect::<BTreeSet<_>>();
    let creator_ids = creators_before
        .iter()
        .filter(|r| nodes.contains(&r.input().operation))
        .map(|r| r.input().operation.clone())
        .collect::<BTreeSet<_>>();
    let plans = before
        .plans
        .iter()
        .filter_map(|(id, saved)| {
            match saved
                .info(&before.views)
                .and_then(|i| i.view().binding().target())
            {
                Ok(target) if targets.contains(&target) => Some(Ok(*id)),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect::<Result<BTreeSet<_>>>()?;
    for plan in &plans {
        if roots
            .records
            .get(plan)
            .is_none_or(|r| r.receipt().is_none() || r.acknowledgment().is_none())
        {
            return Ok(false);
        }
    }
    let mut selected_generations = generations(&before, &roots, &tx, &plans)?;
    for record in creators_before
        .iter()
        .filter(|r| creator_ids.contains(&r.input().operation))
    {
        if !crate::manifest_creation::retirement_ready(&tx, record)? {
            return Ok(false);
        }
        if let Some(generation) = record.generation() {
            if record.request().is_none() {
                return Ok(false);
            }
            selected_generations.insert(generation.value());
        }
    }
    if readers
        .iter()
        .any(|g| selected_generations.contains(&g.value()))
    {
        return Ok(false);
    }
    let other_plans = before
        .plans
        .keys()
        .filter(|p| !plans.contains(*p))
        .copied()
        .collect();
    if !generations(&before, &roots, &tx, &other_plans)?.is_disjoint(&selected_generations)
        || creators_before
            .iter()
            .filter(|r| !creator_ids.contains(&r.input().operation))
            .any(|r| {
                r.generation()
                    .is_some_and(|g| selected_generations.contains(&g.value()))
            })
    {
        return Ok(false);
    }
    if targets.is_empty()
        && namespace_ids.is_empty()
        && creator_ids.is_empty()
        && selected_opens.is_empty()
    {
        return Ok(false);
    }
    let mut expected = before.clone();
    expected.views.files.retain(|id, _| !targets.contains(id));
    expected
        .views
        .records
        .retain(|_, r| !targets.contains(&r.target));
    let used_blocks = expected
        .views
        .records
        .values()
        .flat_map(|r| match &r.record.request.edit {
            Some(Change::Write { blocks, .. }) => {
                blocks.iter().map(|b| b.block).collect::<Vec<_>>()
            }
            _ => Vec::new(),
        })
        .collect::<BTreeSet<_>>();
    expected
        .views
        .blocks
        .retain(|id, _| used_blocks.contains(id));
    expected.plans.retain(|id, _| !plans.contains(id));
    expected.objects.retain(|(id, _, _), _| !plans.contains(id));
    let mut used_blobs: BTreeSet<_> = expected.plans.values().map(|p| p.input.base_page).collect();
    for object in expected.objects.values() {
        used_blobs.extend(object.object.blobs());
    }
    expected.blobs.retain(|id, _| used_blobs.contains(id));
    let mut remaining = roots.clone();
    remaining.records.retain(|id, _| !plans.contains(id));
    remaining.checks.retain(|(id, _), _| !plans.contains(id));
    remaining.validate()?;
    let expected_names = names_before
        .into_iter()
        .filter(|r| !namespace_ids.contains(&r.input.request.operation))
        .collect::<Vec<_>>();
    let expected_creators = creators_before
        .into_iter()
        .filter(|r| !creator_ids.contains(&r.input().operation))
        .collect::<Vec<_>>();
    let expected_opens = opens_before
        .into_iter()
        .filter(|r| !nodes.contains(&r.input.node))
        .collect::<Vec<_>>();
    drop(scope);
    prep::journal::checkpoint("component-retire", "Eligible");
    tx.pragma_update(None, "defer_foreign_keys", true)?;
    crate::manifest_creation::erase_component(&tx, &creator_ids)?;
    for plan in &plans {
        for table in [
            "sparse_publication_checks",
            "sparse_publications",
            "sparse_preparation_objects",
            "sparse_preparations",
        ] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE plan_id=?1"),
                [plan.as_slice()],
            )?;
        }
    }
    if remaining.empty() {
        tx.execute("DELETE FROM sparse_publication_role", [])?;
    } else {
        tx.execute("UPDATE sparse_publication_role SET admitted=?1,checked=?2,state_hash=?3 WHERE singleton=1", params![remaining.records.len() as i64, remaining.checks.len() as i64, remaining.role()?.as_slice()])?;
    }
    for (id, row) in &before.views.records {
        if targets.contains(&row.target) {
            tx.execute(
                "DELETE FROM sparse_view_operations WHERE view_id=?1",
                [id.as_slice()],
            )?;
            tx.execute("DELETE FROM sparse_views WHERE view_id=?1", [id.as_slice()])?;
        }
    }
    for target in &targets {
        tx.execute(
            "DELETE FROM sparse_view_files WHERE target=?1",
            [target.as_slice()],
        )?;
    }
    if expected.views.files.is_empty() {
        tx.execute("DELETE FROM sparse_view_role", [])?;
    }
    for block in before
        .views
        .blocks
        .keys()
        .filter(|id| !expected.views.blocks.contains_key(*id))
    {
        tx.execute(
            "DELETE FROM sparse_view_blocks WHERE block=?1",
            [block.as_slice()],
        )?;
    }
    for blob in before
        .blobs
        .keys()
        .filter(|id| !expected.blobs.contains_key(*id))
    {
        tx.execute(
            "DELETE FROM sparse_preparation_blobs WHERE blob=?1",
            [blob.as_slice()],
        )?;
    }
    prep::journal::checkpoint("component-retire", "SourceErased");
    for node in &nodes {
        tx.execute("DELETE FROM recovered_windows_opens WHERE recovery_id IN (SELECT recovery_id FROM windows_session_recoveries WHERE node_id=?1)", [node])?;
        tx.execute(
            "DELETE FROM windows_session_recoveries WHERE node_id=?1",
            [node],
        )?;
    }
    for record in &selected_opens {
        for table in [
            "windows_open_events",
            "retired_windows_opens",
            "retained_windows_opens",
            "windows_open_journal",
        ] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE open_id=?1"),
                [&record.input.id],
            )?;
        }
    }
    for id in &namespace_ids {
        tx.execute("DELETE FROM namespace_intents WHERE operation_id=?1", [id])?;
    }
    prep::journal::checkpoint("component-retire", "LifecycleErased");
    for generation in &selected_generations {
        super::super::collect::erase_generation(&tx, generation)?;
    }
    prep::journal::checkpoint("component-retire", "PhysicalErased");
    crate::chunk_mount::validate_all(&tx)?;
    let fresh = crate::chunk_mount::read_scope(&tx, key)?;
    let after = prep::ledger::inspect_scoped(&fresh)?;
    if after != expected
        || ledger::inspect(&tx)? != remaining
        || fresh.binding() != &binding
        || permanent_rows(&tx)? != permanent
        || names::retirement_records(&tx)? != expected_names
        || crate::manifest_creation::retirement_records(&tx)? != expected_creators
        || opens::retained_records(&tx)? != expected_opens
        || tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
            [],
            |r| r.get::<_, bool>(0),
        )?
    {
        return Err(Failure::Corrupt.into());
    }
    ledger::validate_source_snapshot(&tx, &remaining, &after)?;
    drop(fresh);
    prep::journal::checkpoint("component-retire", "Validated");
    tx.commit()?;
    state.native_sparse.forget_retired(&selected_opens);
    prep::journal::checkpoint("component-retire", "Committed");
    Ok(true)
}
