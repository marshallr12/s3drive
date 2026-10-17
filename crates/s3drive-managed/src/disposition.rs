//! Executable disposition policy. Native callback delivery/retention is gated
//! separately; a namespace receipt is never a fabricated kernel response.
use super::*;
use s3drive_authority::{Error, Mutation, NodeGuard, PendingRemoval, RemovalMode};
use s3drive_state::namespace_client::{
    Input as NamespaceInput,
    opens::{Action, Callback, OpenInput, OpenRecord},
};
type Task<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send + 'a>>;
/// Confirmed namespace disposition; completion bookkeeping may remain queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallbackReceipt {
    pub maintenance_pending: bool,
}
#[derive(Clone)]
pub struct Controller {
    publisher: Publisher,
}
fn identity(open: &str, callback: &str, role: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"s3drive-disposition-controller-v1\0");
    h.update(open);
    h.update(callback);
    h.update(role);
    hex(&h.finalize()[..16])
}
fn guard(n: &Node) -> NodeGuard {
    NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    }
}
fn reason(e: &(dyn std::error::Error + Send + Sync + 'static)) -> Error {
    match e.downcast_ref::<Failure>() {
        Some(Failure::Authority(e)) => *e,
        _ => Error::Unavailable,
    }
}
impl Controller {
    pub fn new(publisher: Publisher) -> Self {
        Self { publisher }
    }
    fn p(&self) -> &Publisher {
        &self.publisher
    }
    fn scope(&self, input: &OpenInput) -> Result<()> {
        let p = self.p();
        let target =
            p.0.executor
                .recovery_target(&format!("{}binding", p.0.store.root))?;
        if !input.controlled
            || input.volume != p.0.volume
            || input.store != p.0.store
            || input.source != *target.source()
            || input.fingerprint != target.fingerprint()
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    async fn record(&self, open: &str) -> Result<OpenRecord> {
        let r = self.p().storage().opens().get(open.into()).await?;
        self.scope(&r.input)?;
        Ok(r)
    }
    async fn records(&self, node: &str) -> Result<Vec<OpenRecord>> {
        let all = self.p().storage().opens().records().await?;
        let records = all
            .into_iter()
            .filter(|r| r.input.volume == self.p().0.volume && r.input.node == node)
            .collect::<Vec<_>>();
        for r in &records {
            self.scope(&r.input)?;
        }
        Ok(records)
    }
    async fn inspect(&self, node: &str) -> Result<(Node, Option<PendingRemoval>)> {
        let p = self.p();
        let Reply::PendingRemoval(pending) = reply(
            p.0.authority
                .request(&Request::PendingRemoval {
                    volume: p.0.volume.clone(),
                    node: node.into(),
                })
                .await?,
        )?
        else {
            return Err(Failure::Protocol.into());
        };
        let owned = p.owned_removal(node).await?;
        if pending.as_ref().map(|r| (&r.fence, r.mode))
            != owned.as_ref().map(|r| (&r.fence, r.mode))
        {
            return Err(Failure::RetainedConflict.into());
        }
        if let Some(pending) = pending {
            Ok((pending.node.clone(), Some(pending)))
        } else {
            let Reply::Node(node) = reply(
                p.0.authority
                    .request(&Request::Node {
                        volume: p.0.volume.clone(),
                        node: node.into(),
                    })
                    .await?,
            )?
            else {
                return Err(Failure::Protocol.into());
            };
            Ok((node, None))
        }
    }
    async fn request(
        &self,
        record: &OpenRecord,
        callback: &str,
        role: &str,
        mutation: Mutation,
        before: &Node,
    ) -> Result<NamespaceInput> {
        let Reply::Info(info) = reply(self.p().0.authority.request(&Request::Info {}).await?)?
        else {
            return Err(Failure::Protocol.into());
        };
        if info.id != record.input.volume {
            return Err(Failure::Binding.into());
        }
        self.p().namespace_change_input(
            Apply {
                volume: info.id,
                operation: identity(&record.input.id, callback, role),
                expected_sequence: info.sequence.max(before.revision),
                mutation,
            },
            std::slice::from_ref(before),
        )
    }
    async fn parent(&self, node: &Node) -> Result<Node> {
        let Reply::Node(p) = reply(
            self.p()
                .0
                .authority
                .request(&Request::Node {
                    volume: self.p().0.volume.clone(),
                    node: node.parent.clone().ok_or(Failure::Binding)?,
                })
                .await?,
        )?
        else {
            return Err(Failure::Protocol.into());
        };
        Ok(p)
    }
    async fn effect(
        &self,
        open: &str,
        cb: &str,
        role: &str,
        new: Option<NamespaceInput>,
    ) -> Result<s3drive_state::namespace_client::Record> {
        let ledger = self.p().storage().opens();
        let record = self.record(open).await?;
        let operation = identity(open, cb, role);
        let input = record
            .links(cb)
            .into_iter()
            .find(|i| i.request.operation == operation)
            .cloned()
            .or(new)
            .ok_or(Failure::Incomplete)?;
        if input.request.operation != operation {
            return Err(Failure::Binding.into());
        }
        ledger
            .link(
                open.into(),
                identity(open, cb, &format!("link-{role}")),
                cb.into(),
                input.clone(),
            )
            .await?;
        // A definitive authority rejection is still an exact terminal outcome;
        // acknowledge it in the callback ledger, but never report it as success.
        let result = self.p().change_namespace(input.clone()).await;
        let saved = self
            .p()
            .storage()
            .namespace()
            .get(operation.clone())
            .await?;
        if saved.input != input {
            return Err(Failure::Binding.into());
        }
        if saved.outcome.is_some() {
            ledger
                .acknowledge(
                    open.into(),
                    identity(open, cb, &format!("ack-{role}")),
                    operation,
                )
                .await?;
        }
        result?;
        Ok(saved)
    }
    fn linked(record: &OpenRecord, cb: &str, role: &str) -> bool {
        record
            .links(cb)
            .iter()
            .any(|i| i.request.operation == identity(&record.input.id, cb, role))
    }
    async fn complete(&self, open: &str, cb: &str) -> Result<()> {
        self.p()
            .storage()
            .opens()
            .complete(open.into(), identity(open, cb, "complete"), cb.into())
            .await?;
        Ok(())
    }
    async fn rejection(&self, open: &str, cb: &str, error: Error) -> Result<()> {
        self.p()
            .storage()
            .opens()
            .reply(
                open.into(),
                identity(open, cb, "reply"),
                cb.into(),
                Some(error),
            )
            .await?;
        Ok(())
    }
    async fn restore(&self, open: &str, cb: &str) -> Result<()> {
        let record = self.record(open).await?;
        if let Some(first) = record.links(cb).first() {
            let saved = self
                .p()
                .storage()
                .namespace()
                .get(first.request.operation.clone())
                .await?;
            if matches!(saved.outcome, Some(Ok(_))) {
                let wanted = match &first.request.mutation {
                    Mutation::ArmRemoval { .. } => RemovalMode::Deferred,
                    Mutation::TransitionRemoval { expected, .. } => *expected,
                    _ => return Err(Failure::Protocol.into()),
                };
                if Self::linked(&record, cb, "restore") {
                    Box::pin(self.effect(open, cb, "restore", None)).await?;
                } else {
                    let (node, pending) = self.inspect(&record.input.node).await?;
                    let p = pending.ok_or(Failure::RetainedConflict)?;
                    let original = match &first.request.mutation {
                        Mutation::ArmRemoval { owner, .. } => {
                            s3drive_state::namespace::RemovalFence {
                                intent: first.request.operation.clone(),
                                owner: owner.clone(),
                            }
                        }
                        Mutation::TransitionRemoval { fence, .. } => fence.clone(),
                        _ => return Err(Failure::Protocol.into()),
                    };
                    if p.fence != original {
                        return Err(Failure::RetainedConflict.into());
                    }
                    if p.mode != wanted {
                        let parent = self.parent(&node).await?;
                        let input = self
                            .request(
                                &record,
                                cb,
                                "restore",
                                Mutation::TransitionRemoval {
                                    node: guard(&node),
                                    parent: guard(&parent),
                                    fence: p.fence,
                                    expected: p.mode,
                                    mode: wanted,
                                },
                                &node,
                            )
                            .await?;
                        Box::pin(self.effect(open, cb, "restore", Some(input))).await?;
                    }
                }
            }
        }
        self.complete(open, cb).await
    }
    async fn recover_set(&self, open: &str, cb: &str) -> Result<()> {
        let record = self.record(open).await?;
        if record.completed(cb) {
            return Ok(());
        }
        if record.reply(cb).is_none() {
            self.rejection(open, cb, Error::Unavailable).await?;
        }
        let r = self.record(open).await?;
        for input in r.pending().into_iter().cloned().collect::<Vec<_>>() {
            if !r.links(cb).contains(&&input) {
                continue;
            }
            let role = if input.request.operation == identity(open, cb, "restore") {
                "restore"
            } else {
                "set"
            };
            let _ = Box::pin(self.effect(open, cb, role, None)).await;
            let saved = self
                .p()
                .storage()
                .namespace()
                .get(input.request.operation)
                .await?;
            if saved.outcome.is_none() {
                return Err(Failure::Incomplete.into());
            }
        }
        if self.record(open).await?.reply(cb) == Some(None) {
            self.complete(open, cb).await
        } else {
            Box::pin(self.restore(open, cb)).await
        }
    }
    async fn recover_node(&self, node: &str, except: Option<(&str, &str)>) -> Result<()> {
        // One node-wide effect may be unresolved across independent file objects.
        // Process it before deriving another operation; never rebase its input.
        for record in self.records(node).await? {
            for event in &record.events {
                if except.is_some_and(|(o, c)| o == record.input.id && c == event.id)
                    || record.completed(&event.id)
                {
                    continue;
                }
                if let Action::Callback(Callback::SetDelete { .. }) = &event.action {
                    Box::pin(self.recover_set(&record.input.id, &event.id)).await?;
                }
            }
        }
        Ok(())
    }
    async fn recover_void(&self, node: &str, except: Option<(&str, &str)>) -> Result<()> {
        for record in self.records(node).await? {
            for event in &record.events {
                if except.is_some_and(|(o, c)| o == record.input.id && c == event.id)
                    || record.completed(&event.id)
                {
                    continue;
                }
                if let Action::Callback(Callback::Cleanup { flags }) = event.action {
                    Box::pin(self.cleanup_inner(&record.input.id, &event.id, flags)).await?;
                }
            }
        }
        for record in self.records(node).await? {
            for event in &record.events {
                if except.is_some_and(|(o, c)| o == record.input.id && c == event.id)
                    || record.completed(&event.id)
                {
                    continue;
                }
                if matches!(event.action, Action::Callback(Callback::Close))
                    && record.cleanup().is_some_and(|c| record.completed(&c.id))
                {
                    Box::pin(self.close_inner(&record.input.id, &event.id)).await?;
                }
            }
        }
        Ok(())
    }
    pub fn open(&self, input: OpenInput) -> Task<'_, OpenRecord> {
        Box::pin(self.open_impl(input))
    }
    async fn open_impl(&self, input: OpenInput) -> Result<OpenRecord> {
        let _serial = self.p().0.disposition.lock().await;
        self.scope(&input)?;
        if let Some(existing) = self
            .p()
            .storage()
            .opens()
            .records()
            .await?
            .into_iter()
            .find(|r| r.input.id == input.id)
        {
            if existing.input != input {
                return Err(Failure::Binding.into());
            }
            if existing.close().is_some() {
                return Err(Failure::NotFound.into());
            }
            if existing.active() {
                return Ok(existing);
            }
        }
        self.recover_node(&input.node, None).await?;
        Box::pin(self.recover_void(&input.node, None)).await?;
        if input.admission.is_none() {
            let (node, pending) = self.inspect(&input.node).await?;
            if node.deleted {
                return Err(Failure::NotFound.into());
            }
            if pending.is_some_and(|p| p.mode == RemovalMode::Pending) {
                return Err(Failure::Authority(Error::DeletePending).into());
            }
        }
        let ledger = self.p().storage().opens();
        let prepared = ledger.prepare(input.clone()).await?;
        if prepared.active() {
            return Ok(prepared);
        }
        if let Some(admission) = input.admission {
            self.p().change_namespace(*admission).await?;
        }
        ledger
            .activate(input.id.clone(), identity(&input.id, &input.id, "activate"))
            .await
    }
    pub fn set_delete<'a>(
        &'a self,
        open: &'a str,
        cb: &'a str,
        delete: bool,
    ) -> Task<'a, CallbackReceipt> {
        Box::pin(self.set_delete_impl(open, cb, delete))
    }
    async fn set_delete_impl(&self, open: &str, cb: &str, delete: bool) -> Result<CallbackReceipt> {
        let _serial = self.p().0.disposition.lock().await;
        let old = self.record(open).await?;
        self.recover_node(&old.input.node, Some((open, cb))).await?;
        Box::pin(self.recover_void(&old.input.node, None)).await?;
        let exists = old.events.iter().any(|e| e.id == cb);
        let ledger = self.p().storage().opens();
        ledger
            .callback(open.into(), cb.into(), Callback::SetDelete { delete })
            .await?;
        if exists {
            if self.record(open).await?.reply(cb) == Some(None) {
                let pending = Box::pin(self.recover_set(open, cb)).await.is_err();
                return Ok(CallbackReceipt {
                    maintenance_pending: pending,
                });
            }
            Box::pin(self.recover_set(open, cb)).await?;
            return match self.record(open).await?.reply(cb) {
                Some(None) => Ok(CallbackReceipt {
                    maintenance_pending: false,
                }),
                Some(Some(e)) => Err(Failure::Authority(e).into()),
                None => Err(Failure::Incomplete.into()),
            };
        }
        let result = Box::pin(self.set_new(open, cb, delete)).await;
        if let Err(error) = result {
            let code = reason(error.as_ref());
            self.rejection(open, cb, code).await?;
            let _ = Box::pin(self.restore(open, cb)).await;
            return Err(error);
        }
        if let Err(error) = ledger
            .reply(open.into(), identity(open, cb, "reply"), cb.into(), None)
            .await
            && self.record(open).await?.reply(cb) != Some(None)
        {
            return Err(error);
        }
        // Acceptance already has exact durable proof. A failed completion is
        // visible as pending maintenance; returning rejection here would lie
        // about the disposition we have just accepted.
        let maintenance_pending = self.complete(open, cb).await.is_err();
        Ok(CallbackReceipt {
            maintenance_pending,
        })
    }
    async fn set_new(&self, open: &str, cb: &str, delete: bool) -> Result<()> {
        let record = self.record(open).await?;
        let (node, pending) = self.inspect(&record.input.node).await?;
        if node.deleted {
            return Err(Failure::NotFound.into());
        }
        let wanted = if delete {
            RemovalMode::Pending
        } else {
            RemovalMode::Deferred
        };
        let mutation = match pending {
            Some(p) if p.mode == wanted => return Ok(()),
            Some(p) => {
                let parent = self.parent(&node).await?;
                Mutation::TransitionRemoval {
                    node: guard(&node),
                    parent: guard(&parent),
                    fence: p.fence,
                    expected: p.mode,
                    mode: wanted,
                }
            }
            None if !delete => return Ok(()),
            None => {
                let parent = self.parent(&node).await?;
                Mutation::ArmRemoval {
                    node: guard(&node),
                    parent: guard(&parent),
                    owner: record.owner.clone(),
                }
            }
        };
        let input = self.request(&record, cb, "set", mutation, &node).await?;
        Box::pin(self.effect(open, cb, "set", Some(input))).await?;
        Ok(())
    }
    async fn flush_retained(&self, node: &str) -> Result<()> {
        let local = self.p().storage().logical();
        let Some(file) = local.lookup(self.p().0.volume.clone(), node.into()).await? else {
            return Ok(());
        };
        self.p().check_edit_binding(&file.binding)?;
        let mut records = Vec::new();
        let mut after = None;
        loop {
            let page = local.pending(after, 1000).await?;
            if page.is_empty() {
                break;
            }
            after = page.last().map(|r| r.input.id.clone());
            records.extend(
                page.into_iter()
                    .filter(|r| r.input.node == node && r.input.volume == self.p().0.volume),
            );
        }
        records.sort_by_key(|r| r.generation);
        for record in &records {
            if local
                .retained_conflict(record.input.id.clone())
                .await?
                .is_some()
            {
                continue;
            }
            if record.publication.is_some() {
                self.p().flush_edit(&record.input.id).await?;
            }
        }
        let file = local.file(self.p().0.volume.clone(), node.into()).await?;
        if file.head != file.settled {
            let record = local
                .for_generation(file.head.ok_or(Failure::Incomplete)?)
                .await?;
            if local
                .retained_conflict(record.input.id.clone())
                .await?
                .is_some()
            {
                return Ok(());
            }
            if record.publication.is_none() {
                let Reply::Info(info) =
                    reply(self.p().0.authority.request(&Request::Info {}).await?)?
                else {
                    return Err(Failure::Protocol.into());
                };
                let owned = self.p().owned_removal(node).await?;
                let mutation = if let Some(owned) = owned {
                    Mutation::ReservePendingContent {
                        node: node.into(),
                        expected_revision: file.base.revision,
                        store: self.p().0.store.id.clone(),
                        size: record.input.size,
                        sha256: record.input.sha256.clone(),
                        expected_payload: s3drive_authority::payload_reference(&file.base.payload),
                        fence: owned.fence,
                    }
                } else if file.base.deleted {
                    Mutation::ReserveDetachedContent {
                        node: node.into(),
                        expected_revision: file.base.revision,
                        store: self.p().0.store.id.clone(),
                        size: record.input.size,
                        sha256: record.input.sha256.clone(),
                        expected_payload: s3drive_authority::payload_reference(&file.base.payload),
                    }
                } else {
                    Mutation::ReserveContent {
                        node: node.into(),
                        expected_revision: file.base.revision,
                        store: self.p().0.store.id.clone(),
                        size: record.input.size,
                        sha256: record.input.sha256.clone(),
                        expected_payload: s3drive_authority::payload_reference(&file.base.payload),
                    }
                };
                self.p()
                    .stage_edit(
                        &record.input.id,
                        Apply {
                            volume: info.id,
                            operation: identity(node, &record.input.id, "flush"),
                            expected_sequence: info.sequence,
                            mutation,
                        },
                        identity(node, &record.input.id, "publish"),
                    )
                    .await?;
            }
            self.p().flush_edit(&record.input.id).await?;
        }
        Ok(())
    }
    pub fn cleanup<'a>(&'a self, open: &'a str, cb: &'a str, flags: u32) -> Task<'a, ()> {
        Box::pin(self.cleanup_impl(open, cb, flags))
    }
    async fn cleanup_impl(&self, open: &str, cb: &str, flags: u32) -> Result<()> {
        let _serial = self.p().0.disposition.lock().await;
        let record = self.record(open).await?;
        self.recover_node(&record.input.node, None).await?;
        self.p()
            .storage()
            .opens()
            .callback(open.into(), cb.into(), Callback::Cleanup { flags })
            .await?;
        Box::pin(self.recover_void(&record.input.node, Some((open, cb)))).await?;
        Box::pin(self.cleanup_inner(open, cb, flags)).await?;
        Box::pin(self.recover_void(&record.input.node, None)).await
    }
    async fn cleanup_inner(&self, open: &str, cb: &str, flags: u32) -> Result<()> {
        let record = self.record(open).await?;
        if record.completed(cb) {
            return Ok(());
        }
        if Self::linked(&record, cb, "promote")
            && let Err(e) = Box::pin(self.effect(open, cb, "promote", None)).await
        {
            if reason(e.as_ref()) == Error::NotEmpty {
                return self.complete(open, cb).await;
            }
            return Err(e);
        }
        if Self::linked(&record, cb, "commit") {
            Box::pin(self.effect(open, cb, "commit", None)).await?;
            return self.complete(open, cb).await;
        }
        let (node, pending) = self.inspect(&record.input.node).await?;
        if node.deleted {
            return self.complete(open, cb).await;
        }
        if record.input.delete_on_close || flags & 1 != 0 {
            let p = pending.ok_or(Failure::RetainedConflict)?;
            if p.mode == RemovalMode::Deferred {
                let parent = self.parent(&node).await?;
                let input = self
                    .request(
                        &record,
                        cb,
                        "promote",
                        Mutation::TransitionRemoval {
                            node: guard(&node),
                            parent: guard(&parent),
                            fence: p.fence,
                            expected: p.mode,
                            mode: RemovalMode::Pending,
                        },
                        &node,
                    )
                    .await?;
                if let Err(e) = Box::pin(self.effect(open, cb, "promote", Some(input))).await {
                    if reason(e.as_ref()) == Error::NotEmpty {
                        return self.complete(open, cb).await;
                    }
                    return Err(e);
                }
            }
        }
        if flags & 1 != 0 {
            for other in self.records(&record.input.node).await? {
                if other.active() && other.cleanup().is_none() {
                    return Err(Failure::RetainedConflict.into());
                }
            }
            Box::pin(self.flush_retained(&record.input.node)).await?;
            let (node, p) = self.inspect(&record.input.node).await?;
            let p = p.ok_or(Failure::RetainedConflict)?;
            if p.mode != RemovalMode::Pending {
                return Err(Failure::RetainedConflict.into());
            }
            let parent = self.parent(&node).await?;
            let input = self
                .request(
                    &record,
                    cb,
                    "commit",
                    Mutation::CommitRemoval {
                        node: guard(&node),
                        parent: guard(&parent),
                        fence: p.fence,
                    },
                    &node,
                )
                .await?;
            Box::pin(self.effect(open, cb, "commit", Some(input))).await?;
        }
        self.complete(open, cb).await
    }
    pub fn close<'a>(&'a self, open: &'a str, cb: &'a str) -> Task<'a, ()> {
        Box::pin(self.close_impl(open, cb))
    }
    async fn close_impl(&self, open: &str, cb: &str) -> Result<()> {
        let _serial = self.p().0.disposition.lock().await;
        let record = self.record(open).await?;
        self.recover_node(&record.input.node, None).await?;
        self.p()
            .storage()
            .opens()
            .callback(open.into(), cb.into(), Callback::Close)
            .await?;
        Box::pin(self.recover_void(&record.input.node, Some((open, cb)))).await?;
        Box::pin(self.close_inner(open, cb)).await
    }
    async fn close_inner(&self, open: &str, cb: &str) -> Result<()> {
        let record = self.record(open).await?;
        if record.completed(cb) {
            return Ok(());
        }
        if record.cleanup().is_none_or(|c| !record.completed(&c.id)) {
            return Err(Failure::RetainedConflict.into());
        }
        for other in self.records(&record.input.node).await? {
            if other.active() && other.close().is_none() {
                return self.complete(open, cb).await;
            }
        }
        if Self::linked(&record, cb, "release") {
            Box::pin(self.effect(open, cb, "release", None)).await?;
            return self.complete(open, cb).await;
        }
        Box::pin(self.flush_retained(&record.input.node)).await?;
        let (node, pending) = self.inspect(&record.input.node).await?;
        if let Some(p) = pending {
            if p.mode == RemovalMode::Pending {
                return Err(Failure::RetainedConflict.into());
            }
            let input = self
                .request(
                    &record,
                    cb,
                    "release",
                    Mutation::CancelRemoval {
                        node: node.id.clone(),
                        fence: p.fence,
                    },
                    &node,
                )
                .await?;
            Box::pin(self.effect(open, cb, "release", Some(input))).await?;
        }
        self.complete(open, cb).await
    }
    /// Recover only recorded callbacks. Missing cleanup/session death requires
    /// explicit mount recovery policy; no elapsed-time deletion is inferred.
    pub fn recover<'a>(&'a self, node: &'a str) -> Task<'a, ()> {
        Box::pin(self.recover_impl(node))
    }
    async fn recover_impl(&self, node: &str) -> Result<()> {
        let _serial = self.p().0.disposition.lock().await;
        self.recover_node(node, None).await?;
        Box::pin(self.recover_void(node, None)).await
    }
}
