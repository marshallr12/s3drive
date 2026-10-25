use super::*;
use crate::sparse_views::preparation as prep;

impl Journal<'_> {
    /// Explicit local retirement, not a remote deletion or a fresh clean receipt.
    /// The returned ID names the retained historical clean anchor. Default and
    /// native owners are never activated by this operation.
    pub fn retire_checkpoint(
        &mut self,
        proof: &retry::Checkpoint,
        options: views::Options,
    ) -> Result<Id> {
        let options = options.validate()?;
        let readers = self.state.staging_readers.active();
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ledger::no_triggers(&tx)?;
        let scope = crate::chunk_mount::read_scope_current(&tx)?;
        let mounted = crate::chunk_mount::load(&tx)?.is_some();
        let native_mode = crate::chunk_mount::retry_enabled(&tx)?;
        let enabled = match &scope {
            Some(scope) => retry::enabled_scoped(scope)?,
            None => retry::enabled(&tx)?,
        };
        if !enabled || (mounted && !native_mode) {
            return Err(Failure::Conflict.into());
        }
        let before = match &scope {
            Some(scope) => prep::ledger::inspect_scoped(scope)?,
            None => prep::ledger::inspect(&tx)?,
        };
        let mut anchors = before.views.anchors.clone();
        // Lost post-commit reply: return only the exact retained anchor result.
        if let Some(a) = anchors
            .values()
            .find(|a| a.publication.input.plan == proof.plan() && a.view.id == proof.anchor())
        {
            return Ok(a.view.id);
        }
        let meta = ledger::inspect(&tx)?;
        if retry::checkpoint_snapshot(&tx, proof.plan(), &readers, &before, &meta)? != *proof
            || !proof.unblocked()
        {
            return Err(Failure::Conflict.into());
        }
        let info = before.info(proof.plan())?;
        let target = info.view().binding().target()?;
        let native_records = if native_mode {
            let base = info.view().binding().base()?;
            crate::namespace_client::opens::native_candidates(&tx, base.volume(), base.node())?
        } else {
            Vec::new()
        };
        match &scope {
            Some(scope) => prep::native::validate_retirement_scoped(scope, &native_records)?,
            None => prep::native::validate_retirement(&tx, &native_records)?,
        }
        let mut plans = BTreeSet::new();
        let mut generations = BTreeSet::new();
        for (plan, saved) in &before.plans {
            let i = saved.info(&before.views)?;
            if i.view().binding().target()? != target {
                continue;
            }
            let r = meta.records.get(plan).ok_or(Failure::Incomplete)?;
            if i.ownership().is_some()
                || (i.native_ownership().is_some() && !native_mode)
                || r.receipt().is_none()
                || r.acknowledgment().is_none()
            {
                return Err(Failure::Incomplete.into());
            }
            plans.insert(*plan);
            for (_, level, slot) in before.objects.keys().filter(|(p, _, _)| p == plan) {
                if let Some(t) = prep::transfers::transfer(
                    &tx,
                    &before,
                    &i,
                    prep::Position {
                        level: *level,
                        slot: *slot,
                    },
                )? {
                    generations.insert(t.generation().ok_or(Failure::Incomplete)?.value());
                }
            }
        }
        if let Some(old) = anchors.get(&target) {
            generations.insert(old.generation().value());
        }
        if readers.iter().any(|g| generations.contains(&g.value())) {
            return Err(Failure::Conflict.into());
        }
        let owner = if let Some(a) = anchors.values().next() {
            a.owner
        } else {
            let mut id = [0; 32];
            getrandom::fill(&mut id).map_err(|_| "retention owner generation failed")?;
            if id == [0; 32] {
                return Err(Failure::Corrupt.into());
            }
            id
        };
        let (anchor, _) = Anchor {
            canonical_len: 0,
            format: if native_mode { 2 } else { 1 },
            native: if native_mode {
                Some(native::Terminal::capture(
                    &tx,
                    &info,
                    proof.anchor(),
                    &before,
                    &native_records,
                )?)
            } else {
                None
            },
            owner,
            view: before.views.record(proof.anchor())?.clone(),
            published: Identity::capture(&info.view().0),
            input: info.input().clone(),
            progress: info.progress.clone(),
            original_page: before
                .blobs
                .get(&info.input().base_page)
                .ok_or(Failure::Corrupt)?
                .clone(),
            publication: meta
                .records
                .get(&proof.plan())
                .ok_or(Failure::Missing)?
                .clone(),
        }
        .canonical()?;
        // No read proof may cross the first witness or source mutation.
        drop(scope);
        if let Some(terminal) = &anchor.native {
            crate::chunk_mount::admission::seal_terminal(&tx, &target, terminal.digest()?)?;
            prep::journal::checkpoint("retire", "NativeWitnessStored");
        }
        anchor.validate(&tx, owner)?;
        let keep_root = anchor.generation();
        generations.remove(&keep_root.value());
        anchors.insert(target, anchor);
        let mut expected = before.clone();
        expected.views.anchors = anchors.clone();
        expected
            .views
            .records
            .retain(|id, row| row.target != target || *id == proof.anchor());
        let used_blocks: BTreeSet<_> = expected
            .views
            .records
            .values()
            .flat_map(|r| match &r.record.request.edit {
                Some(Change::Write { blocks, .. }) => {
                    blocks.iter().map(|b| b.block).collect::<Vec<_>>()
                }
                _ => Vec::new(),
            })
            .collect();
        expected
            .views
            .blocks
            .retain(|id, _| used_blocks.contains(id));
        expected.plans.retain(|id, _| !plans.contains(id));
        expected.objects.retain(|(id, _, _), _| !plans.contains(id));
        let mut used_blobs: BTreeSet<_> =
            expected.plans.values().map(|p| p.input.base_page).collect();
        for o in expected.objects.values() {
            used_blobs.extend(o.object.blobs());
        }
        expected.blobs.retain(|id, _| used_blobs.contains(id));
        expected.views.admit(options)?;
        let mut remaining = meta.clone();
        remaining.records.retain(|id, _| !plans.contains(id));
        remaining.checks.retain(|(id, _), _| !plans.contains(id));
        remaining.validate()?;
        prep::journal::checkpoint("retire", "Eligible");
        save(&tx, &anchors, owner, target)?;
        prep::journal::checkpoint("retire", "AnchorStored");
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
            tx.execute("UPDATE sparse_publication_role SET admitted=?1,checked=?2,state_hash=?3 WHERE singleton=1",params![remaining.records.len() as i64,remaining.checks.len() as i64,remaining.role()?.as_slice()])?;
        }
        for (id, row) in &before.views.records {
            if row.target == target && *id != proof.anchor() {
                tx.execute(
                    "DELETE FROM sparse_view_operations WHERE view_id=?1",
                    [id.as_slice()],
                )?;
                tx.execute("DELETE FROM sparse_views WHERE view_id=?1", [id.as_slice()])?;
            }
        }
        for id in before
            .views
            .blocks
            .keys()
            .filter(|id| !expected.views.blocks.contains_key(*id))
        {
            tx.execute(
                "DELETE FROM sparse_view_blocks WHERE block=?1",
                [id.as_slice()],
            )?;
        }
        for id in before
            .blobs
            .keys()
            .filter(|id| !expected.blobs.contains_key(*id))
        {
            tx.execute(
                "DELETE FROM sparse_preparation_blobs WHERE blob=?1",
                [id.as_slice()],
            )?;
        }
        prep::journal::checkpoint("retire", "SourceStored");
        for generation in &generations {
            // Exact verified, parentless plans only. Foreign-key dependencies
            // outside this forest fail the transaction rather than being erased.
            if tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM staged_generations WHERE parent_id=?1)",
                [generation],
                |r| r.get::<_, bool>(0),
            )? {
                return Err(Failure::Conflict.into());
            }
            let mut q=tx.prepare("SELECT chunk_hash FROM staged_extents WHERE generation_id=?1 AND chunk_hash IS NOT NULL")?;
            let chunks = q
                .query_map([generation], |r| r.get::<_, Vec<u8>>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(q);
            tx.execute(
                "DELETE FROM write_settlements WHERE generation_id=?1",
                [generation],
            )?;
            tx.execute(
                "DELETE FROM remote_flushes WHERE generation_id=?1",
                [generation],
            )?;
            tx.execute("DELETE FROM retained_remote_intents WHERE operation_id IN (SELECT operation_id FROM remote_intents WHERE generation_id=?1)",[generation])?;
            tx.execute(
                "DELETE FROM remote_intents WHERE generation_id=?1",
                [generation],
            )?;
            for table in [
                "write_journal",
                "staged_heads",
                "staged_operations",
                "staged_extents",
                "staged_generations",
            ] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE generation_id=?1"),
                    [generation],
                )?;
            }
            for chunk in chunks {
                if !tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM staged_extents WHERE chunk_hash=?1)",
                    [&chunk],
                    |r| r.get::<_, bool>(0),
                )? {
                    tx.execute(
                        "DELETE FROM retained_staged_chunks WHERE chunk_hash=?1",
                        [&chunk],
                    )?;
                    tx.execute("DELETE FROM staged_chunks WHERE chunk_hash=?1", [&chunk])?;
                }
            }
        }
        prep::journal::checkpoint("retire", "PhysicalStored");
        if native_mode {
            crate::namespace_client::opens::erase_native(
                &tx,
                &native_records,
                self.state.incarnation.as_deref().ok_or(Failure::Conflict)?,
            )?;
            prep::journal::checkpoint("retire", "NativeStored");
        }
        // Fresh proof only after every deletion; never reuse the pre-cut scope.
        {
            let scope = crate::chunk_mount::read_scope_current(&tx)?;
            let actual = match &scope {
                Some(scope) => prep::ledger::inspect_scoped(scope)?,
                None => prep::ledger::inspect(&tx)?,
            };
            if actual != expected || ledger::inspect(&tx)? != remaining {
                return Err(Failure::Corrupt.into());
            }
            ledger::validate_source_snapshot(&tx, &remaining, &expected)?;
            if scope.is_none() {
                prep::native::validate_metadata(&tx)?;
            }
        }
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
            [],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(Failure::Corrupt.into());
        }
        prep::journal::checkpoint("retire", "Validated");
        tx.commit()?;
        self.state.native_sparse.forget_retired(&native_records);
        prep::journal::checkpoint("retire", "Committed");
        Ok(proof.anchor())
    }
}
impl Client {
    pub async fn retire_checkpoint(
        &self,
        proof: retry::Checkpoint,
        options: views::Options,
    ) -> Result<Id> {
        self.call(move |j| j.retire_checkpoint(&proof, options))
            .await
    }
}
