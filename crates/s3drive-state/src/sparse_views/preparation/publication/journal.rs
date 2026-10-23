use super::*;
use crate::worker::Owner;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataNeed {
    pub slot: u64,
    pub uses_base: bool,
}
pub struct Journal<'a> {
    pub(super) state: &'a mut State,
    limits: Limits,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State, limits: Limits) -> Result<Self> {
        state.verify_staging_storage()?;
        Ok(Self {
            state,
            limits: limits.validate()?,
        })
    }
    pub fn get(&mut self, plan: Id) -> Result<Record> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let meta = ledger::inspect(&tx)?;
        ledger::validate_source(&tx, &meta)?;
        let r = meta.records.get(&plan).ok_or(Failure::Missing)?.clone();
        tx.commit()?;
        Ok(r)
    }
    pub fn records(&mut self) -> Result<Vec<Record>> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let meta = ledger::inspect(&tx)?;
        ledger::validate_source(&tx, &meta)?;
        let rows = meta.records.into_values().collect();
        tx.commit()?;
        Ok(rows)
    }
    /// Observational fresh-base admission only. begin repeats it atomically.
    pub fn check_base(&mut self, plan: Id, node: &ns::Node) -> Result<()> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let meta = ledger::inspect(&tx)?;
        ledger::validate_source(&tx, &meta)?;
        let s = super::super::ledger::inspect(&tx)?;
        lineage::select(&meta, &s, plan, node)?;
        tx.commit()?;
        Ok(())
    }
    pub fn unchecked(&mut self, plan: Id, after: Option<u64>, limit: u16) -> Result<Vec<DataNeed>> {
        if limit == 0 || limit > 128 {
            return Err(Failure::Invalid.into());
        }
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let meta = ledger::inspect(&tx)?;
        let s = super::super::ledger::inspect(&tx)?;
        ledger::validate_checks(&meta, &s)?;
        let info = s.info(plan)?;
        info.root()?.ok_or(Failure::Incomplete)?;
        let mut rows = Vec::new();
        for ((_, level, slot), saved) in s.objects.range((plan, -1, 0)..=(plan, -1, u64::MAX)) {
            if *level != -1
                || after.is_some_and(|n| *slot <= n)
                || meta.checks.contains_key(&(plan, *slot))
                || !matches!(saved.object.kind, super::super::model::Kind::Data { .. })
            {
                continue;
            }
            let ranges = info
                .view()
                .range(saved.object.offset(), saved.object.length as usize)?;
            rows.push(DataNeed {
                slot: *slot,
                uses_base: ranges
                    .iter()
                    .any(|r| matches!(r, super::super::Range::Base { .. })),
            });
            if rows.len() == usize::from(limit) {
                break;
            }
        }
        tx.commit()?;
        Ok(rows)
    }
    pub fn check(&mut self, plan: Id, slot: u64, original: Option<&OriginalChunk>) -> Result<()> {
        if let Some(o) = original {
            o.bounds()?;
        }
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let before = ledger::inspect(&tx)?;
        let s = super::super::ledger::inspect(&tx)?;
        super::super::semantics::check(&s, plan, slot, original)?;
        let info = s.info(plan)?;
        let super::super::model::Kind::Data { blob } = s.object(plan, -1, slot)?.kind else {
            return Err(Failure::Invalid.into());
        };
        let row = Check {
            plan,
            slot,
            candidate: blob,
            root: digest(&info.root()?.ok_or(Failure::Incomplete)?.encode()),
            witness: original.map(|o| digest(&o.witness)),
            original: original.and_then(|o| o.bytes.as_deref()).map(digest),
        };
        if let Some(old) = before.checks.get(&(plan, slot)) {
            if old == &row {
                return Ok(());
            }
            return Err(Failure::Conflict.into());
        }
        if info.native_ownership().is_some() {
            super::super::owned::fresh(
                &tx,
                self.state.incarnation.as_deref(),
                &self.state.native_sparse,
                &info,
            )?;
        }
        let mut after = before.clone();
        after.checks.insert((plan, slot), row);
        after.admit(self.limits)?;
        ledger::save(&tx, &before, &after, true)?;
        tx.commit()?;
        checkpoint("check", "Committed");
        Ok(())
    }
    pub fn begin(
        &mut self,
        plan: Id,
        base: ns::Node,
        store: ns::Store,
        sequence: u64,
        publish_operation: String,
    ) -> Result<Record> {
        begin_bounds(&base, &store, &publish_operation)?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let before = ledger::inspect(&tx)?;
        let s = super::super::ledger::inspect(&tx)?;
        let info = s.info(plan)?;
        ledger::coverage(&before, &s, plan)?;
        let tree = super::super::transfers::inspect_tree(&tx, plan)?;
        let root = tree.root().clone();
        let physical = Physical::from_tree(&tree)?;
        let predecessor = lineage::select(&before, &s, plan, &base)?;
        let input = Input {
            plan,
            operation: info.operation(),
            view: info.view().id(),
            binding: info.view().binding().clone(),
            generation: *info.generation(),
            original_root: info.original_root()?.encode().to_vec(),
            root: root.encode().to_vec(),
            base: base.clone(),
            store: store.clone(),
            reserve: ns::Apply {
                volume: hex(root.volume()),
                operation: hex(info.generation()),
                expected_sequence: sequence,
                mutation: ns::Mutation::ReserveManifest {
                    node: base.id.clone(),
                    expected_revision: base.revision,
                    store: store.id,
                    size: physical.size,
                    sha256: hex(&physical.sha256),
                    manifest: Box::new(ns::ManifestRoot::new(root).map_err(|_| Failure::Invalid)?),
                    guard: ns::ContentGuard {
                        removal_claim: info.ownership().map(|p| Box::new(p.claim().clone())),
                        expected: ns::payload_reference(&base.payload),
                        detached: info.ownership().is_some_and(|p| p.claim().mode.detached()),
                        removal: info.ownership().map(|p| p.claim().fence.clone()),
                    },
                },
            },
            publish_operation,
            predecessor,
            owned: info.ownership().cloned(),
        };
        input.reservation()?;
        if let Some(old) = before.records.get(&plan) {
            if old.input == input && old.physical == physical {
                return Ok(old.clone());
            }
            return Err(Failure::Conflict.into());
        }
        super::super::retry::admit_root(&tx, &s, &info, &input.publish_operation)?;
        super::super::owned::fresh(
            &tx,
            self.state.incarnation.as_deref(),
            &self.state.native_sparse,
            &info,
        )?;
        let r = Record {
            format: if input.owned.is_some() {
                4
            } else if predecessor.is_some() {
                2
            } else {
                1
            },
            input,
            physical,
            semantics: before.semantic_hash(plan)?,
            reservation: None,
            publication: None,
            receipt: None,
            rejection: None,
            acknowledgment: None,
        };
        let mut after = before.clone();
        after.records.insert(plan, r.clone());
        after.admit(self.limits)?;
        ledger::save(&tx, &before, &after, true)?;
        tx.commit()?;
        checkpoint(
            if predecessor.is_some() {
                "lineage_begin"
            } else {
                "begin"
            },
            "Committed",
        );
        Ok(r)
    }
    pub fn acknowledged(&mut self, plan: Id) -> Result<Option<SourceReceipt>> {
        let r = self.get(plan)?;
        if r.acknowledgment.is_none() {
            return Ok(None);
        }
        Ok(Some(SourceReceipt::from_record(r)?))
    }
    pub fn acknowledge(
        &mut self,
        operation: Id,
        plan: Id,
        options: crate::sparse_views::Options,
    ) -> Result<SourceReceipt> {
        settlement::acknowledge(self.state, operation, None, plan, options)
    }
    pub fn acknowledge_scoped(
        &mut self,
        token: crate::sparse_views::ScopedOperation,
        plan: Id,
        options: crate::sparse_views::Options,
    ) -> Result<SourceReceipt> {
        settlement::acknowledge(self.state, token.id()?, Some(token), plan, options)
    }
    pub fn reserved(&mut self, plan: Id, receipt: ns::Receipt) -> Result<Record> {
        update(&mut self.state.connection, plan, true, |r| {
            reserved(r, receipt)
        })
    }
    pub fn prepare_publish(&mut self, plan: Id, sequence: u64) -> Result<Record> {
        let record = self.get(plan)?;
        if record.publication.is_none() {
            let source = super::super::ledger::inspect(&self.state.connection)?;
            super::super::owned::fresh(
                &self.state.connection,
                self.state.incarnation.as_deref(),
                &self.state.native_sparse,
                &source.info(plan)?,
            )?;
        }
        update(&mut self.state.connection, plan, true, |r| {
            r.prepare(sequence)
        })
    }
    pub fn published(&mut self, plan: Id, receipt: ns::Receipt) -> Result<Record> {
        update(&mut self.state.connection, plan, true, |r| {
            published(r, receipt)
        })
    }
    pub fn reject(&mut self, plan: Id, request: ns::Apply, error: ns::Error) -> Result<Record> {
        update(&mut self.state.connection, plan, true, |r| {
            reject(r, request, error)
        })
    }
}
pub(super) fn update(
    db: &mut Connection,
    plan: Id,
    source: bool,
    f: impl FnOnce(&mut Record) -> Result<()>,
) -> Result<Record> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let before = ledger::inspect(&tx)?;
    if source {
        ledger::validate_source(&tx, &before)?;
    }
    let mut after = before.clone();
    let r = after.records.get_mut(&plan).ok_or(Failure::Missing)?;
    f(r)?;
    r.validate()?;
    let result = r.clone();
    let kind = ledger::kind(&before, &after);
    ledger::save(&tx, &before, &after, source)?;
    tx.commit()?;
    checkpoint(kind, "Committed");
    Ok(result)
}
pub(super) fn reserved(r: &mut Record, receipt: ns::Receipt) -> Result<()> {
    if r.reservation.as_ref() == Some(&receipt) {
        return Ok(());
    }
    if r.reservation.is_some() || r.rejection.is_some() {
        return Err(Failure::Conflict.into());
    }
    r.reservation = Some(receipt);
    Ok(())
}
pub(super) fn published(r: &mut Record, receipt: ns::Receipt) -> Result<()> {
    if r.receipt.as_ref() == Some(&receipt) {
        return Ok(());
    }
    if r.receipt.is_some() || r.rejection.is_some() {
        return Err(Failure::Conflict.into());
    }
    r.receipt = Some(receipt);
    Ok(())
}
pub(super) fn reject(r: &mut Record, request: ns::Apply, error: ns::Error) -> Result<()> {
    let next = Rejection { request, error };
    if r.rejection.as_ref() == Some(&next) {
        return Ok(());
    }
    if r.rejection.is_some() || r.receipt.is_some() {
        return Err(Failure::Conflict.into());
    }
    r.rejection = Some(next);
    r.validate()
}
fn begin_bounds(base: &ns::Node, store: &ns::Store, op: &str) -> Result<()> {
    if op == ns::ROOT
        || !id(op)
        || !store.valid()
        || !id(&base.id)
        || !name(&base.name)
        || base.parent.as_deref().is_none_or(|s| !id(s))
        || base.payload.as_ref().is_none_or(|p| !p.valid())
        || base.placement.as_ref().is_some_and(|s| !s.valid())
    {
        return Err(Failure::Invalid.into());
    }
    Ok(())
}
#[derive(Clone)]
pub struct Client {
    owner: Owner,
    limits: Limits,
}
impl super::super::Client {
    pub fn publications(&self, limits: Limits) -> Result<Client> {
        Ok(Client {
            owner: self.owner.clone(),
            limits: limits.validate()?,
        })
    }
}
impl Client {
    pub fn same_storage(&self, storage: &crate::worker::Storage) -> bool {
        storage.same_owner(&self.owner)
    }
    pub(super) async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Journal<'_>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let limits = self.limits;
        self.owner
            .call(move |s| f(&mut Journal::new(&mut s.state, limits)?))
            .await
    }
    pub async fn get(&self, plan: Id) -> Result<Record> {
        self.call(move |j| j.get(plan)).await
    }
    pub async fn records(&self) -> Result<Vec<Record>> {
        self.call(move |j| j.records()).await
    }
    pub async fn check_base(&self, plan: Id, node: ns::Node) -> Result<()> {
        self.call(move |j| j.check_base(plan, &node)).await
    }
    pub async fn unchecked(
        &self,
        plan: Id,
        after: Option<u64>,
        limit: u16,
    ) -> Result<Vec<DataNeed>> {
        if limit == 0 || limit > 128 {
            return Err(Failure::Invalid.into());
        }
        self.call(move |j| j.unchecked(plan, after, limit)).await
    }
    pub async fn check(&self, plan: Id, slot: u64, original: Option<OriginalChunk>) -> Result<()> {
        if let Some(o) = &original {
            o.bounds()?;
        }
        self.call(move |j| j.check(plan, slot, original.as_ref()))
            .await
    }
    pub async fn begin(
        &self,
        plan: Id,
        base: ns::Node,
        store: ns::Store,
        sequence: u64,
        publish_operation: String,
    ) -> Result<Record> {
        begin_bounds(&base, &store, &publish_operation)?;
        self.call(move |j| j.begin(plan, base, store, sequence, publish_operation))
            .await
    }
    pub async fn acknowledged(&self, plan: Id) -> Result<Option<SourceReceipt>> {
        self.call(move |j| j.acknowledged(plan)).await
    }
    pub async fn acknowledge(
        &self,
        operation: Id,
        plan: Id,
        options: crate::sparse_views::Options,
    ) -> Result<SourceReceipt> {
        self.call(move |j| j.acknowledge(operation, plan, options))
            .await
    }
    pub async fn acknowledge_scoped(
        &self,
        token: crate::sparse_views::ScopedOperation,
        plan: Id,
        options: crate::sparse_views::Options,
    ) -> Result<SourceReceipt> {
        token.id()?;
        self.call(move |j| j.acknowledge_scoped(token, plan, options))
            .await
    }
    pub async fn reserved(&self, plan: Id, receipt: ns::Receipt) -> Result<Record> {
        self.call(move |j| j.reserved(plan, receipt)).await
    }
    pub async fn prepare_publish(&self, plan: Id, sequence: u64) -> Result<Record> {
        self.call(move |j| j.prepare_publish(plan, sequence)).await
    }
    pub async fn published(&self, plan: Id, receipt: ns::Receipt) -> Result<Record> {
        self.call(move |j| j.published(plan, receipt)).await
    }
    pub async fn reject(&self, plan: Id, request: ns::Apply, error: ns::Error) -> Result<Record> {
        self.call(move |j| j.reject(plan, request, error)).await
    }
}
