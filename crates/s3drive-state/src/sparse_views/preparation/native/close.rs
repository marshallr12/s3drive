//! Final preparation and actual Close share a single immediate transaction.
use super::*;

#[derive(Clone, Debug)]
pub struct Close {
    pub open: OpenRecord,
    pub plans: Vec<Info>,
}
impl Client {
    pub async fn prepare(&self, token: Token, context: Context) -> Result<Info> {
        if token.key != self.key {
            return Err(Failure::Conflict.into());
        }
        let options = self.plans;
        self.owner
            .call(move |worker| {
                let state = &mut worker.state;
                state.verify_staging_storage()?;
                let tx = state
                    .connection
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let caller = token.validate(&tx, &state.native_sparse)?;
                opens::sparse_current(
                    &tx,
                    state.incarnation.as_deref().ok_or(Failure::Conflict)?,
                    &caller,
                )?;
                if state.native_sparse.closing(&caller.input.id)? {
                    return Err(Failure::Conflict.into());
                }
                let (info, changed) = prepare(
                    &tx,
                    state.incarnation.as_deref(),
                    &state.native_sparse,
                    options,
                    &token.key,
                    &context,
                    false,
                )?;
                if changed {
                    tx.commit()?;
                    checkpoint("native-prepare", "Committed");
                }
                Ok(info)
            })
            .await
    }
    /// Caller must mark the token at native callback entry, before other waits.
    /// This method also marks it defensively before the bounded worker enqueue.
    pub async fn close(&self, token: Token, callback: String) -> Result<Close> {
        token.observe_close();
        if token.key != self.key {
            return Err(Failure::Conflict.into());
        }
        token.observe_close_as(&callback)?;
        let options = self.plans;
        self.owner
            .call(move |worker| {
                let state = &mut worker.state;
                state.verify_staging_storage()?;
                let result = close(state, options, &token, &callback);
                if result.is_err() {
                    // Never grant finalization after a failed atomic admission. Preserve
                    // the observed fact separately when the journal remains writable.
                    let _ = close_fact(state, &token, &callback);
                }
                result
            })
            .await
    }
    /// Bounded original plans in source ancestry order, including known outcomes.
    pub async fn plans(&self, node: String) -> Result<Vec<Info>> {
        let key = self.key.clone();
        self.owner
            .call(move |worker| {
                let db = &worker.state.connection;
                let binding = crate::chunk_mount::require(db, &key)?;
                let saved = ledger::inspect(db)?;
                let mut plans = Vec::new();
                for plan in saved.plans.values() {
                    let info = plan.info(&saved.views)?;
                    let base = info.view().binding().base()?;
                    if info.native_ownership().is_some() && base.node() == node {
                        binding.volume(base.volume())?;
                        plans.push(info);
                    }
                }
                sort(&saved, &mut plans)?;
                Ok(plans)
            })
            .await
    }
}
fn prepare(
    tx: &rusqlite::Transaction<'_>,
    incarnation: Option<&str>,
    runtime: &Runtime,
    options: Options,
    key: &crate::chunk_mount::Key,
    context: &Context,
    finalizing: bool,
) -> Result<(Info, bool)> {
    let (native, original) = epoch_owner(tx, &context.binding)?.ok_or(Failure::Conflict)?;
    crate::chunk_mount::verify_key(tx, key)?.open(&original.input)?;
    let operation = hash(b"s3drive-native-sparse-plan-v1\0", &(&native, context.view))?;
    let saved = ledger::inspect(tx)?;
    if let Some(plan) = saved
        .plans
        .values()
        .find(|plan| plan.input.operation == operation)
    {
        if plan.input.native.as_ref() != Some(&native)
            || plan.input.view != context.view
            || plan.input.base_root != context.root
            || plan.input.base_page != digest(&context.page)
            || saved.blobs.get(&plan.input.base_page) != Some(&context.page)
        {
            return Err(Failure::Conflict.into());
        }
        return Ok((plan.info(&saved.views)?, false));
    }
    let view = saved.views.record(context.view)?;
    if view.request.binding != context.binding
        || view.clean()
        || saved
            .views
            .files
            .get(&context.binding.target()?)
            .ok_or(Failure::NotFound)?
            .head
            != context.view
    {
        return Err(Failure::Conflict.into());
    }
    let base = context.binding.base()?;
    let ownership =
        crate::namespace_client::removals::sparse_claim(tx, base.volume(), base.node())?
            .map(|claim| (key.clone(), original.input.id.clone(), claim));
    let generation = hash(b"s3drive-native-sparse-generation-v1\0", &operation)?[..16]
        .try_into()
        .unwrap();
    journal::begin_in_transaction(
        tx,
        options,
        incarnation,
        runtime,
        Request {
            operation,
            view: context.view,
            generation,
            base_root: context.root.clone(),
            base_page: context.page.clone(),
        },
        journal::Admission {
            ownership,
            native: Some(native),
            finalizing,
        },
    )
}
fn close(state: &mut State, options: Options, token: &Token, callback: &str) -> Result<Close> {
    let tx = state
        .connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let original = token.validate(&tx, &state.native_sparse)?;
    if let Some(old) = original.close() {
        if old.id != callback {
            return Err(Failure::Conflict.into());
        }
        return result(&tx, original);
    }
    opens::sparse_current(
        &tx,
        state.incarnation.as_deref().ok_or(Failure::Conflict)?,
        &original,
    )?;
    if !state.native_sparse.closing(&original.input.id)? {
        return Err(Failure::Conflict.into());
    }
    let before = ledger::inspect(&tx)?;
    for file in before.views.files.values() {
        let base = file.binding.base()?;
        if base.volume() != original.input.volume || base.node() != original.input.node {
            continue;
        }
        let head = before.views.record(file.head)?;
        if !head.clean()
            && epoch_owner(&tx, &head.request.binding)?
                .is_some_and(|(owner, _)| owner.open == original.input.id)
        {
            let context = state
                .native_sparse
                .contexts
                .get(&head.request.binding.target()?)
                .ok_or(Failure::Conflict)?;
            if context.binding != head.request.binding {
                return Err(Failure::Conflict.into());
            }
            let mut context = context.clone();
            context.view = head.id;
            prepare(
                &tx,
                state.incarnation.as_deref(),
                &state.native_sparse,
                options,
                &token.key,
                &context,
                true,
            )?;
        }
    }
    checkpoint("native-close", "PlanStored");
    let event = opens::Event {
        id: callback.into(),
        action: Action::Callback(opens::Callback::Close),
    };
    let mut record =
        opens::append_in_transaction(&tx, Some(&token.key), &original.input.id, event, false)?
            .record;
    checkpoint("native-close", "CloseStored");
    let saved = ledger::inspect(&tx)?;
    for (plan, input) in &saved.plans {
        if input
            .input
            .native
            .as_ref()
            .is_some_and(|p| p.open == original.input.id)
        {
            record = opens::append_in_transaction(
                &tx,
                Some(&token.key),
                &original.input.id,
                opens::Event {
                    id: hex(&hash(b"s3drive-native-sparse-freeze-v1\0", &(callback, plan))?[..16]),
                    action: Action::FreezeSparse {
                        callback: callback.into(),
                        plan: *plan,
                    },
                },
                false,
            )?
            .record;
            checkpoint("native-close", "FreezeStored");
        }
    }
    let result = result(&tx, record)?;
    metadata::validate(&tx)?;
    checkpoint("native-close", "Validated");
    tx.commit()?;
    checkpoint("native-close", "Committed");
    Ok(result)
}
fn close_fact(state: &mut State, token: &Token, callback: &str) -> Result<()> {
    let tx = state
        .connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    token.validate(&tx, &state.native_sparse)?;
    let appended = opens::append_in_transaction(
        &tx,
        Some(&token.key),
        &token.input.id,
        opens::Event {
            id: callback.into(),
            action: Action::Callback(opens::Callback::Close),
        },
        false,
    )?;
    if appended.changed {
        tx.commit()?;
        checkpoint("native-close", "FactCommitted");
    }
    Ok(())
}
fn result(db: &Connection, open: OpenRecord) -> Result<Close> {
    let saved = ledger::inspect(db)?;
    let mut plans = Vec::new();
    for event in &open.events {
        if let Action::FreezeSparse { plan, .. } = event.action {
            plans.push(saved.info(plan)?);
        }
    }
    sort(&saved, &mut plans)?;
    Ok(Close { open, plans })
}
fn sort(saved: &ledger::Snapshot, plans: &mut [Info]) -> Result<()> {
    let mut ranks = BTreeMap::new();
    for plan in plans.iter() {
        let mut at = saved.views.record(plan.view().id())?;
        let mut depth = 0usize;
        while let Some(edit) = &at.request.edit {
            depth += 1;
            if depth > saved.views.records.len() {
                return Err(Failure::Corrupt.into());
            }
            at = saved.views.record(edit.parent())?;
        }
        ranks.insert(plan.id()?, depth);
    }
    plans.sort_by_key(|plan| (ranks[&plan.id().expect("validated plan")], plan.view().id()));
    Ok(())
}

impl Client {
    /// Completion is separate from observed Close and requires every node barrier.
    pub async fn complete(
        &self,
        token: Token,
        callback: String,
        completion: String,
    ) -> Result<OpenRecord> {
        if token.key != self.key {
            return Err(Failure::Conflict.into());
        }
        self.owner
            .call(move |worker| {
                let state = &mut worker.state;
                state.verify_staging_storage()?;
                let tx = state
                    .connection
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let original = token.validate(&tx, &state.native_sparse)?;
                if original.close().map(|e| &e.id) != Some(&callback) {
                    return Err(Failure::Conflict.into());
                }
                if original.completed(&callback) {
                    if original.events.iter().any(|e| e.id == completion
                    && matches!(&e.action, Action::Completed { callback: c } if c == &callback)) {
                    return Ok(original);
                }
                    return Err(Failure::Conflict.into());
                }
                opens::sparse_original_session(
                    &tx,
                    state.incarnation.as_deref().ok_or(Failure::Conflict)?,
                    &original,
                )?;
                publication::namespace_pending(&tx, &original.input.volume, &original.input.node)?;
                let saved = ledger::inspect(&tx)?;
                let mut view = None;
                for file in saved.views.files.values() {
                    let base = file.binding.base()?;
                    if base.volume() == original.input.volume && base.node() == original.input.node
                    {
                        let head = saved.views.record(file.head)?;
                        if !head.clean() || view.replace(head.id).is_some() {
                            return Err(Failure::Conflict.into());
                        }
                    }
                }
                let frozen = frozen_hash(&original)?;
                opens::append_in_transaction(
                    &tx,
                    Some(&token.key),
                    &original.input.id,
                    opens::Event {
                        id: hex(&hash(
                            b"s3drive-native-sparse-complete-v1\0",
                            &(&callback, &completion),
                        )?[..16]),
                        action: Action::CompleteSparse {
                            callback: callback.clone(),
                            view,
                            frozen,
                        },
                    },
                    false,
                )?;
                checkpoint("native-complete", "WitnessStored");
                let record = opens::append_in_transaction(
                    &tx,
                    Some(&token.key),
                    &original.input.id,
                    opens::Event {
                        id: completion,
                        action: Action::Completed { callback },
                    },
                    false,
                )?
                .record;
                metadata::validate(&tx)?;
                checkpoint("native-complete", "Validated");
                tx.commit()?;
                checkpoint("native-complete", "Committed");
                Ok(record)
            })
            .await
    }
}
