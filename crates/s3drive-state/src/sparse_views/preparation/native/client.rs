//! Bounded native sparse adapter capability.
use super::*;

#[derive(Clone)]
pub struct Client {
    pub(super) owner: crate::worker::Owner,
    pub(super) key: crate::chunk_mount::Key,
    pub(super) views: crate::sparse_views::Options,
    pub(super) plans: Options,
}
impl Client {
    pub(crate) fn new(
        owner: crate::worker::Owner,
        key: crate::chunk_mount::Key,
        views: crate::sparse_views::Options,
        plans: Options,
    ) -> Result<Self> {
        Ok(Self {
            owner,
            key,
            views: views.validate()?,
            plans: plans.validate()?,
        })
    }
    pub async fn bind(&self, open: String) -> Result<Token> {
        let key = self.key.clone();
        self.owner
            .call(move |worker| {
                let state = &mut worker.state;
                state.verify_staging_storage()?;
                let tx = state
                    .connection
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let record = opens::sparse_open(&tx, &open)?;
                crate::chunk_mount::require(&tx, &key)?.open(&record.input)?;
                opens::sparse_current(
                    &tx,
                    state.incarnation.as_deref().ok_or(Failure::Conflict)?,
                    &record,
                )?;
                if !record.input.controlled {
                    return Err(Failure::Conflict.into());
                }
                let closing = state.native_sparse.token(&tx, &open)?;
                if closing.load(Ordering::Acquire) {
                    return Err(Failure::Conflict.into());
                }
                let saved = ledger::inspect(&tx)?;
                for file in saved.views.files.values() {
                    let base = file.binding.base()?;
                    let head = saved.views.record(file.head)?;
                    if base.volume() == record.input.volume
                        && base.node() == record.input.node
                        && !head.clean()
                        && epoch_owner(&tx, &head.request.binding)?.is_none()
                    {
                        return Err(Failure::Conflict.into());
                    }
                }
                let roots = publication::ledger::inspect(&tx)?;
                for plan in saved.plans.keys() {
                    let info = saved.info(*plan)?;
                    let base = info.view().binding().base()?;
                    if info.native_ownership().is_none()
                        && base.volume() == record.input.volume
                        && base.node() == record.input.node
                        && info.settlement().is_none()
                        && roots
                            .records
                            .get(plan)
                            .is_none_or(|r| r.acknowledgment().is_none())
                    {
                        return Err(Failure::Conflict.into());
                    }
                }
                let appended = opens::append_in_transaction(
                    &tx,
                    Some(&key),
                    &open,
                    opens::Event {
                        id: hex(&hash(b"s3drive-native-sparse-bind-v1\0", &open)?[..16]),
                        action: Action::BindSparse,
                    },
                    false,
                )?;
                if appended.changed {
                    tx.commit()?;
                    checkpoint("native-bind", "Committed");
                }
                Ok(Token {
                    input: record.input,
                    key,
                    closing,
                })
            })
            .await
    }
    pub async fn write(
        &self,
        token: Token,
        context: Context,
        nonce: Id,
        offset: u64,
        bytes: Vec<u8>,
    ) -> Result<View> {
        if token.key != self.key {
            return Err(Failure::Conflict.into());
        }
        let options = self.views;
        self.owner
            .call(move |worker| {
                let scoped = scoped_operation(&token, &context, nonce)?;
                let edit = crate::sparse_views::mutations::write_edit(
                    scoped.id()?,
                    Some(scoped),
                    context.view,
                    offset,
                    &bytes,
                )?;
                crate::sparse_views::Journal::new(&mut worker.state, options)?.change_native(
                    edit,
                    &Mutation {
                        token: &token,
                        context: &context,
                    },
                )
            })
            .await
    }
    pub async fn resize(
        &self,
        token: Token,
        context: Context,
        nonce: Id,
        size: u64,
    ) -> Result<View> {
        if token.key != self.key {
            return Err(Failure::Conflict.into());
        }
        let options = self.views;
        self.owner
            .call(move |worker| {
                let scoped = scoped_operation(&token, &context, nonce)?;
                let edit = crate::sparse_views::mutations::resize_edit(
                    scoped.id()?,
                    Some(scoped),
                    context.view,
                    size,
                )?;
                crate::sparse_views::Journal::new(&mut worker.state, options)?.change_native(
                    edit,
                    &Mutation {
                        token: &token,
                        context: &context,
                    },
                )
            })
            .await
    }
}
fn scoped_operation(token: &Token, context: &Context, nonce: Id) -> Result<ScopedOperation> {
    if nonce == [0; 32] {
        return Err(Failure::Invalid.into());
    }
    ScopedOperation::new(
        &context.binding,
        hash(
            b"s3drive-native-sparse-operation-v1\0",
            &(&token.input.id, nonce),
        )?,
    )
}

impl Client {
    /// Attempt local linked-native retirement after actual Close completion.
    /// Live opens and readers defer collection; unknown/corrupt evidence errors.
    pub async fn retire_linked(&self, node: String) -> Result<bool> {
        let key = self.key.clone();
        let views = self.views;
        let plans = self.plans;
        self.owner
            .call(move |worker| {
                let state = &mut worker.state;
                if !crate::chunk_mount::retry_enabled(&state.connection)? {
                    return Ok(false);
                }
                let mount = crate::chunk_mount::require(&state.connection, &key)?;
                if opens::sparse_records(&state.connection)?
                    .iter()
                    .any(|r| r.input.volume == mount.volume && r.input.node == node)
                {
                    return Ok(false);
                }
                let snapshot = ledger::inspect(&state.connection)?;
                // A fully checked source pin already forbids any cut. Defer
                // without repeatedly loading all retired open histories while
                // a long-lived reader applies backpressure. This issues no
                // checkpoint and mutates nothing; release still takes the full
                // terminal/candidate/physical proof path below.
                for pin in snapshot.views.read_pins.pins.values() {
                    let view = snapshot.views.record(pin.view_id())?;
                    let base = view.request.binding.base()?;
                    if base.volume() == mount.volume && base.node() == node {
                        return Ok(false);
                    }
                }
                let roots = publication::ledger::inspect(&state.connection)?;
                let mut selected = None;
                let mut any = false;
                for (id, saved) in &snapshot.plans {
                    let info = saved.info(&snapshot.views)?;
                    let base = info.view().binding().base()?;
                    if base.volume() != mount.volume || base.node() != node {
                        continue;
                    }
                    any = true;
                    let file = snapshot
                        .views
                        .files
                        .get(&info.view().binding().target()?)
                        .ok_or(Failure::Corrupt)?;
                    if roots
                        .records
                        .get(id)
                        .and_then(|r| r.acknowledgment())
                        .and_then(|a| a.clean_view())
                        == Some(file.head)
                        && selected.replace(*id).is_some()
                    {
                        return Err(Failure::Corrupt.into());
                    }
                }
                if let Some(plan) = selected {
                    let proof = Journal::new(state, plans)?.checkpoint(plan)?;
                    if !proof.unblocked() {
                        return Ok(false);
                    }
                    publication::Journal::new(state, publication::Limits::default())?
                        .retire_checkpoint(&proof, views)?;
                    return Ok(true);
                }
                if any {
                    return Ok(false);
                }
                // No surviving source/plan uses these opens. No-effect closed opens
                // need no historical output after expiry: their admission ordinals
                // remain permanently below the owner frontier.
                let tx = state
                    .connection
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let records = opens::native_candidates(&tx, &mount.volume, &node)?;
                if records.is_empty() {
                    return Ok(false);
                }
                metadata::validate_retirement(&tx, &records)?;
                if records.iter().any(|r| {
                    r.events.iter().any(|e| {
                        matches!(
                            e.action,
                            Action::OwnSparseEpoch { .. } | Action::FreezeSparse { .. }
                        )
                    })
                }) {
                    return Err(Failure::Conflict.into());
                }
                opens::erase_native(
                    &tx,
                    &records,
                    state.incarnation.as_deref().ok_or(Failure::Conflict)?,
                )?;
                metadata::validate(&tx)?;
                tx.commit()?;
                state.native_sparse.forget_retired(&records);
                Ok(true)
            })
            .await
    }
}
