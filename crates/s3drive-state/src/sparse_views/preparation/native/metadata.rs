//! Whole native graph headers only: no source, preparation, or staged BLOB bodies.
use super::*;
use crate::sparse_views::{Change, ledger as views};

pub(crate) fn validate(db: &Connection) -> Result<()> {
    let mut records = opens::sparse_records(db)?;
    let live = records
        .iter()
        .map(|r| r.input.id.clone())
        .collect::<BTreeSet<_>>();
    let plans = headers(db)?;
    // Retired publisher evidence remains authoritative for retained plans.
    // Load only the bounded plan references, not an unbounded retired-open scan.
    for plan in plans.values() {
        if let Some(proof) = &plan.input.native
            && !records.iter().any(|r| r.input.id == proof.open)
        {
            records.push(opens::sparse_open(db, &proof.open)?);
        }
    }
    if !records.iter().any(is_bound) && !plans.values().any(|p| p.input.native.is_some()) {
        return Ok(());
    }
    let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 48 {
        return Err(Failure::Corrupt.into());
    }
    let source = views::inspect_metadata(db)?;
    let roots = publication::ledger::inspect(db)?;
    let mut epochs = BTreeMap::new();
    for record in &records {
        for event in &record.events {
            if let Action::OwnSparseEpoch {
                binding,
                operation,
                base_root,
                base_page,
            } = &event.action
            {
                let proof = Provenance::capture(record, binding, *operation)?;
                proof.historical(db, binding)?;
                let first = source
                    .records
                    .values()
                    .find(|r| r.record.operation == *operation)
                    .ok_or(Failure::Corrupt)?;
                let parent = source
                    .records
                    .get(&first.record.parent.ok_or(Failure::Corrupt)?)
                    .ok_or(Failure::Corrupt)?;
                if first.record.request.binding != **binding
                    || !parent.record.clean()
                    || !matches!(
                        first.record.request.edit,
                        Some(Change::Write { .. } | Change::Resize { .. })
                    )
                    || epochs
                        .insert(binding.epoch()?, (proof, base_root.clone(), *base_page))
                        .is_some()
                {
                    return Err(Failure::Corrupt.into());
                }
            }
        }
    }
    for (id, plan) in &plans {
        let Some(proof) = &plan.input.native else {
            continue;
        };
        let view = &source
            .records
            .get(&plan.input.view)
            .ok_or(Failure::Corrupt)?
            .record;
        let binding = &view.request.binding;
        proof.historical(db, binding)?;
        // The plain BLOB key and domain-separated manifest page digest are
        // intentionally different. Bind both exact original intent identities
        // through the source-epoch event; do not fetch private page bodies.
        if epochs.get(&binding.epoch()?)
            != Some(&(
                proof.clone(),
                plan.input.base_root.clone(),
                plan.input.base_page,
            ))
            || plan.input.id()? != *id
            || plan.input.operation != hash(b"s3drive-native-sparse-plan-v1\0", &(proof, view.id))?
        {
            return Err(Failure::Corrupt.into());
        }
        if plan.input.generation.as_slice()
            != &hash(
                b"s3drive-native-sparse-generation-v1\0",
                &plan.input.operation,
            )?[..16]
        {
            return Err(Failure::Corrupt.into());
        }
        let descriptor = RootDescriptor::decode(&plan.input.base_root)?;
        let base = binding.base()?;
        if descriptor.length() != binding.base_size()
            || descriptor.page().store() != binding.store()
            || hex(descriptor.volume()) != base.volume()
            || hex(descriptor.node()) != base.node()
            || plan.input.base_page == [0; 32]
        {
            return Err(Failure::Corrupt.into());
        }
        progress(db, *id, plan, view.request.size, &roots)?;
        if let Some(owned) = &plan.input.owned {
            owned.historical(db, binding)?;
            if owned.original_open().id != proof.open {
                return Err(Failure::Corrupt.into());
            }
        }
    }
    for file in source.files.values() {
        let head = &source
            .records
            .get(&file.head)
            .ok_or(Failure::Corrupt)?
            .record;
        let base = file.binding.base()?;
        if !head.clean()
            && records.iter().any(|r| {
                live.contains(&r.input.id)
                    && is_bound(r)
                    && r.input.volume == base.volume()
                    && r.input.node == base.node()
            })
            && !epochs.contains_key(&head.request.binding.epoch()?)
        {
            return Err(Failure::Corrupt.into());
        }
    }
    for record in records.iter().filter(|r| is_bound(r)) {
        let mut frozen = BTreeSet::new();
        for event in &record.events {
            match &event.action {
                Action::FreezeSparse { plan, .. } => {
                    let saved = plans.get(plan).ok_or(Failure::Corrupt)?;
                    if saved
                        .input
                        .native
                        .as_ref()
                        .is_none_or(|p| p.open != record.input.id)
                        || !frozen.insert(*plan)
                    {
                        return Err(Failure::Corrupt.into());
                    }
                }
                Action::CompleteSparse {
                    callback,
                    view,
                    frozen: witnessed,
                } => {
                    if !record.completed(callback) || *witnessed != frozen_hash(record)? {
                        return Err(Failure::Corrupt.into());
                    }
                    if let Some(view) = view {
                        let head = &source.records.get(view).ok_or(Failure::Corrupt)?.record;
                        let base = head.request.binding.base()?;
                        if !head.clean()
                            || base.volume() != record.input.volume
                            || base.node() != record.input.node
                        {
                            return Err(Failure::Corrupt.into());
                        }
                    } else if !frozen.is_empty()
                        || record
                            .events
                            .iter()
                            .any(|e| matches!(e.action, Action::OwnSparseEpoch { .. }))
                    {
                        return Err(Failure::Corrupt.into());
                    }
                    for id in &frozen {
                        let plan = plans.get(id).ok_or(Failure::Corrupt)?;
                        let binding = &source
                            .records
                            .get(&plan.input.view)
                            .ok_or(Failure::Corrupt)?
                            .record
                            .request
                            .binding;
                        let base = binding.base()?;
                        if plan.input.native.is_none()
                            || base.volume() != record.input.volume
                            || base.node() != record.input.node
                            || (plan.progress.settlement.is_none()
                                && !roots
                                    .records
                                    .get(id)
                                    .is_some_and(|r| r.acknowledgment().is_some()))
                        {
                            return Err(Failure::Corrupt.into());
                        }
                    }
                }
                _ => (),
            }
        }
        if !frozen.is_empty() {
            let admitted = plans
                .iter()
                .filter(|(_, p)| {
                    p.input
                        .native
                        .as_ref()
                        .is_some_and(|p| p.open == record.input.id)
                })
                .map(|(id, _)| *id)
                .collect::<BTreeSet<_>>();
            if admitted != frozen {
                return Err(Failure::Corrupt.into());
            }
        }
    }
    Ok(())
}
pub(super) fn headers(db: &Connection) -> Result<BTreeMap<Id, ledger::Saved>> {
    let mut query = db.prepare("SELECT CASE WHEN typeof(plan_id)='blob' AND length(plan_id)=32 THEN plan_id END,CASE WHEN typeof(operation)='blob' AND length(operation)=32 THEN operation END,CASE WHEN typeof(view_id)='blob' AND length(view_id)=32 THEN view_id END,CASE WHEN typeof(input)='blob' AND length(input) BETWEEN 1 AND 65536 THEN input END,CASE WHEN typeof(state)='blob' AND length(state) BETWEEN 1 AND 4096 THEN state END,CASE WHEN typeof(state_hash)='blob' AND length(state_hash)=32 THEN state_hash END FROM sparse_preparations ORDER BY plan_id LIMIT 257")?;
    let mut plans = BTreeMap::new();
    let mut operations = BTreeSet::new();
    for row in query.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, Vec<u8>>(1)?,
            r.get::<_, Vec<u8>>(2)?,
            r.get::<_, Vec<u8>>(3)?,
            r.get::<_, Vec<u8>>(4)?,
            r.get::<_, Vec<u8>>(5)?,
        ))
    })? {
        let (id, operation, view, raw, progress, hash) = row?;
        let saved = ledger::Saved::new(decode(&raw)?, decode(&progress)?)?;
        let id: Id = id.try_into().map_err(|_| Failure::Corrupt)?;
        if saved.input_raw != raw
            || saved.state_raw != progress
            || saved.input.id()? != id
            || operation != saved.input.operation
            || view != saved.input.view
            || hash != saved.state_hash()?
            || !operations.insert(saved.input.operation)
            || saved.input.operation == [0; 32]
            || saved.input.view == [0; 32]
            || saved.input.generation == [0; 16]
            || plans.len() >= MAX_PLANS
            || plans.insert(id, saved).is_some()
        {
            return Err(Failure::Corrupt.into());
        }
    }
    Ok(plans)
}

fn progress(
    db: &Connection,
    id: Id,
    plan: &ledger::Saved,
    size: u64,
    roots: &publication::ledger::Meta,
) -> Result<()> {
    let cursor = plan.progress.cursor;
    let level = model::root_level(size);
    if !(-1..=level).contains(&cursor.level)
        || cursor.offset > size
        || (cursor.level == -1
            && (cursor.offset >= size || cursor.slot != 0 || plan.progress.root.is_some()))
        || (cursor.level >= 0 && cursor.offset != size)
        || (cursor.level >= 0
            && plan.progress.root.is_none()
            && cursor.slot >= model::count(size, cursor.level))
    {
        return Err(Failure::Corrupt.into());
    }
    if let Some(raw) = &plan.progress.root {
        let root = RootDescriptor::decode(raw)?;
        let base = RootDescriptor::decode(&plan.input.base_root)?;
        if cursor.level != level
            || cursor.slot != 1
            || root.length() != size
            || root.volume() != base.volume()
            || root.node() != base.node()
            || root.page().store() != base.page().store()
            || root.page().generation() != &plan.input.generation
        {
            return Err(Failure::Corrupt.into());
        }
    }
    let Some(settlement) = &plan.progress.settlement else {
        return Ok(());
    };
    match settlement {
        owned::Settlement::Rejected { operation } => {
            let root = roots.records.get(&id).ok_or(Failure::Corrupt)?;
            if *operation == [0; 32]
                || root.rejection().is_none()
                || root.receipt().is_some()
                || root.acknowledgment().is_some()
            {
                return Err(Failure::Corrupt.into());
            }
        }
        owned::Settlement::NoEffect { operation } => {
            if *operation == [0; 32] || roots.records.contains_key(&id) {
                return Err(Failure::Corrupt.into());
            }
            let Some(root) = &plan.progress.root else {
                return Ok(());
            };
            // Staging identity is computable from object headers, not its BLOB.
            let total: u64 = db.query_row(
                "SELECT (SELECT coalesce(sum(length(record)),0) FROM sparse_preparation_objects) + (SELECT coalesce(sum(length(input)+length(state)),0) FROM sparse_preparations)",
                [],
                |r| number(r, 0),
            )?;
            if total > MAX_METADATA {
                return Err(Failure::Corrupt.into());
            }
            let mut query = db.prepare("SELECT level,slot,CASE WHEN typeof(record)='blob' AND length(record) BETWEEN 1 AND 65536 THEN record END,state_hash FROM sparse_preparation_objects WHERE plan_id=? ORDER BY level,slot LIMIT 262145")?;
            for (index, row) in query
                .query_map([id.as_slice()], |r| {
                    Ok((
                        r.get::<_, i8>(0)?,
                        number(r, 1)?,
                        r.get::<_, Vec<u8>>(2)?,
                        r.get::<_, Vec<u8>>(3)?,
                    ))
                })?
                .enumerate()
            {
                let (level, slot, raw, digest) = row?;
                let saved = ledger::SavedObject::new(decode(&raw)?)?;
                let object = &saved.object;
                if index >= MAX_OBJECTS
                    || object.level != level
                    || object.slot != slot
                    || saved.raw != raw
                    || saved.state_hash(id)?.as_slice() != digest
                {
                    return Err(Failure::Corrupt.into());
                }
                model::geometry(size, level, slot)?;
                if let Kind::Data { blob } | Kind::Page { blob } = object.kind {
                    let operation = hash(
                        b"s3drive-mixed-object-staging-v1\0",
                        &(id, root, level, slot, object, blob),
                    )?;
                    if db.query_row(
                        "SELECT EXISTS(SELECT 1 FROM staged_operations WHERE operation_key=?)",
                        [operation.as_slice()],
                        |r| r.get::<_, bool>(0),
                    )? {
                        return Err(Failure::Corrupt.into());
                    }
                }
            }
        }
    }
    Ok(())
}
