//! Candidate namespace effects serialized with original-handle source work.
use super::*;
use s3drive_authority::{Apply, Artifact, Kind, Mutation, Receipt, RemovalClaim, RemovalMode};
use s3drive_state::namespace_client::{self as names, Control, Input};
fn guard(n: &RemoteNode) -> NodeGuard {
    NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    }
}
impl Writes {
    fn native_control(&self, node: &RemoteNode) -> Result<Control> {
        let id = node
            .payload
            .as_ref()
            .map(|p| p.reservation.store.id.as_str())
            .or_else(|| node.placement.as_ref().map(|s| s.id.as_str()))
            .unwrap_or(&self.default_store);
        let (store, executor, _) = self.stores.get(id).ok_or(Error::ReadOnly)?;
        let target = executor
            .recovery_target(&format!("{}binding", store.root))
            .map_err(error)?;
        Ok(Control {
            store: store.clone(),
            source: *target.source(),
            fingerprint: target.fingerprint().into(),
        })
    }
    pub(super) async fn native_observed(&self, node: &str) -> Result<RemoteNode> {
        match self
            .authority
            .request(&Request::PendingRemoval {
                volume: self.volume.clone(),
                node: node.into(),
            })
            .await
            .map_err(error)?
        {
            Reply::PendingRemoval(Some(p)) => {
                let local = self
                    ._capability
                    .namespace()
                    .owned_removal(self.volume.clone(), node.into())
                    .await
                    .map_err(error)?
                    .ok_or(Error::DeletePending)?;
                if local.fence != p.fence || local.mode != p.mode {
                    return Err(Error::Conflict);
                }
                Ok(p.node)
            }
            Reply::PendingRemoval(None) => self.reader.node(node).await.map_err(error),
            Reply::Error(e) => Err(error(s3drive_managed::Failure::Authority(e))),
            _ => Err(Error::Conflict),
        }
    }
    async fn native_change(
        &self,
        mutation: Mutation,
        before: Vec<RemoteNode>,
        destination: Option<Control>,
    ) -> Result<Receipt> {
        let first = before.first().ok_or(Error::Internal)?;
        let control = self.native_control(first)?;
        let (_, executor, _) = self.stores.get(&control.store.id).ok_or(Error::ReadOnly)?;
        executor
            .check_create(&format!("{}binding", control.store.root), 0)
            .map_err(error)?;
        if let Some(d) = &destination {
            let (_, e, _) = self.stores.get(&d.store.id).ok_or(Error::ReadOnly)?;
            e.check_create(&format!("{}binding", d.store.root), 0)
                .map_err(error)?;
        }
        let Reply::Info(info) = self
            .authority
            .request(&Request::Info {})
            .await
            .map_err(error)?
        else {
            return Err(Error::Conflict);
        };
        if info.id != self.volume {
            return Err(Error::Conflict);
        }
        let input = Input {
            request: Apply {
                volume: self.volume.clone(),
                operation: hex(&fresh()?[..16]),
                expected_sequence: info.sequence,
                mutation,
            },
            store: control.store,
            source: control.source,
            fingerprint: control.fingerprint,
            entries: before
                .iter()
                .map(names::Entry::from_node)
                .collect::<std::result::Result<_, _>>()
                .map_err(error)?,
            destination_control: destination.map(Box::new),
        };
        self.native_saved_change(input).await
    }
    pub(super) async fn native_saved_change(&self, input: Input) -> Result<Receipt> {
        if input.request.volume != self.volume {
            return Err(Error::Conflict);
        }
        let (store, executor, _) = self.stores.get(&input.store.id).ok_or(Error::ReadOnly)?;
        let target = executor
            .recovery_target(&format!("{}binding", store.root))
            .map_err(error)?;
        if store != &input.store
            || *target.source() != input.source
            || target.fingerprint() != input.fingerprint
        {
            return Err(Error::Conflict);
        }
        if let Some(control) = &input.destination_control {
            let (s, e, _) = self.stores.get(&control.store.id).ok_or(Error::ReadOnly)?;
            let t = e
                .recovery_target(&format!("{}binding", s.root))
                .map_err(error)?;
            if *s != control.store
                || *t.source() != control.source
                || t.fingerprint() != control.fingerprint
            {
                return Err(Error::Conflict);
            }
        }
        let ledger = self._capability.namespace();
        let saved = ledger.begin(input).await.map_err(error)?;
        let record = if saved.outcome.is_some() {
            saved
        } else {
            let outcome = match self
                .authority
                .request(&Request::Apply {
                    request: saved.input.request.clone(),
                })
                .await
                .map_err(error)?
            {
                Reply::Applied(r) => Ok(r),
                Reply::Error(s3drive_authority::Error::DeletePending) => {
                    return Err(Error::DeletePending);
                }
                Reply::Error(e) => Err(e),
                _ => return Err(Error::Conflict),
            };
            ledger
                .acknowledge(saved.input.request.operation, outcome)
                .await
                .map_err(error)?
        };
        let receipt = record
            .outcome
            .ok_or(Error::Conflict)?
            .map_err(|e| error(s3drive_managed::Failure::Authority(e)))?;
        self.native_observe_receipt(&receipt)?;
        Ok(receipt)
    }
    pub(super) async fn native_sync_node(&self, node: &str) -> Result<()> {
        // A committed removal no longer has an active reservation. Derive the
        // unlink fact from exact acknowledged callback receipts, never absence.
        for open in self._capability.opens().records().await.map_err(error)? {
            if open.input.node != node || open.input.volume != self.volume {
                continue;
            }
            for event in open.events {
                let names::opens::Action::Link { input, .. } = event.action else {
                    continue;
                };
                let saved = self
                    ._capability
                    .namespace()
                    .get(input.request.operation.clone())
                    .await
                    .map_err(error)?;
                if saved.input != *input {
                    return Err(Error::Conflict);
                }
                if let Some(Ok(receipt)) = saved.outcome
                    && receipt.node.id == node
                    && receipt.node.deleted
                    && let Some(file) = self
                        .entries
                        .lock()
                        .map_err(|_| Error::Internal)?
                        .get(node)
                        .and_then(Weak::upgrade)
                {
                    file.unlinked.store(true, Ordering::Release);
                }
            }
        }
        Ok(())
    }
    fn native_observe_receipt(&self, receipt: &Receipt) -> Result<()> {
        let mut nodes = vec![&receipt.node];
        if let Some(
            Artifact::Namespace { displaced: Some(n) }
            | Artifact::RemovalMove {
                displaced: Some(n), ..
            },
        ) = &receipt.artifact
        {
            nodes.push(n);
        }
        let entries = self.entries.lock().map_err(|_| Error::Internal)?;
        for node in nodes {
            if let Some(file) = entries.get(&node.id).and_then(Weak::upgrade) {
                file.unlinked.store(node.deleted, Ordering::Release);
            }
        }
        Ok(())
    }
    async fn native_arm(&self, node: RemoteNode) -> Result<RemoteNode> {
        if self
            ._capability
            .namespace()
            .owned_removal(self.volume.clone(), node.id.clone())
            .await
            .map_err(error)?
            .is_some()
        {
            return Ok(node);
        }
        let parent = self
            .reader
            .node(node.parent.as_deref().ok_or(Error::Conflict)?)
            .await
            .map_err(error)?;
        self.native_change(
            Mutation::ArmRemoval {
                node: guard(&node),
                parent: guard(&parent),
                owner: self._capability.key().owner().into(),
            },
            vec![node],
            None,
        )
        .await
        .map(|r| r.node)
    }
    pub(crate) async fn native_remove(
        self: &Arc<Self>,
        node: &str,
        parent: &str,
        name: &str,
        directory: bool,
    ) -> Result<()> {
        let permit = self.admit()?;
        let w = self.clone();
        let (node, parent, name) = (node.to_owned(), parent.to_owned(), name.to_owned());
        // A timeout never cancels an already accepted namespace operation.
        tokio::spawn(async move {
            let _permit = permit;
            tokio::time::timeout(DEADLINE, async {
                let _serial = w.native_serial.lock().await;
                w.flush_native_node(&node).await?;
                let mut n = w.native_observed(&node).await?;
                if n.parent.as_deref() != Some(&parent) || n.name != name {
                    return Err(Error::Conflict);
                }
                if (n.kind == Kind::Directory) != directory {
                    return Err(if directory {
                        Error::NotDirectory
                    } else {
                        Error::IsDirectory
                    });
                }
                let records = w._capability.opens().records().await.map_err(error)?;
                let held = records.iter().any(|r| {
                    r.input.node == node
                        && r.input.volume == w.volume
                        && r.active()
                        && r.close().is_none()
                });
                if held {
                    n = w.native_arm(n).await?;
                }
                let p = w.reader.node(&parent).await.map_err(error)?;
                let owned = w
                    ._capability
                    .namespace()
                    .owned_removal(w.volume.clone(), node)
                    .await
                    .map_err(error)?;
                let mutation = if let Some(o) = owned {
                    if o.mode != RemovalMode::Pending {
                        return Err(Error::Conflict);
                    }
                    Mutation::CommitRemoval {
                        node: guard(&n),
                        parent: guard(&p),
                        fence: o.fence,
                    }
                } else {
                    Mutation::RemoveEntry {
                        node: guard(&n),
                        parent: guard(&p),
                    }
                };
                w.native_change(mutation, vec![n], None).await?;
                Ok(())
            })
            .await
            .map_err(|_| Error::Timeout)?
        })
        .await
        .map_err(|_| Error::Internal)?
    }
    pub(crate) async fn native_rename(
        self: &Arc<Self>,
        node: &str,
        parent: &str,
        name: &str,
        replace: bool,
        same_name: fn(&str, &str) -> bool,
    ) -> Result<RemoteNode> {
        let permit = self.admit()?;
        let w = self.clone();
        let (node, parent, name) = (node.to_owned(), parent.to_owned(), name.to_owned());
        tokio::spawn(async move {
            let _permit = permit;
            tokio::time::timeout(DEADLINE, async {
                let _serial = w.native_serial.lock().await;
                w.flush_native_node(&node).await?;
                let source = w.native_observed(&node).await?;
                if source.deleted {
                    return Err(Error::NotFound);
                }
                let source_parent = w
                    .reader
                    .node(source.parent.as_deref().ok_or(Error::Conflict)?)
                    .await
                    .map_err(error)?;
                let parent = w.reader.node(&parent).await.map_err(error)?;
                let key = s3drive_core::names::collision_key(&name).ok_or(Error::InvalidName)?;
                let mut target = w
                    .reader
                    .list(&parent.id)
                    .await
                    .map_err(error)?
                    .children
                    .into_iter()
                    .find(|n| s3drive_core::names::collision_key(&n.name).as_ref() == Some(&key));
                if let Some(t) = &target
                    && t.id != node
                {
                    if !replace || !same_name(&t.name, &name) {
                        return Err(Error::AlreadyExists);
                    }
                    w.flush_native_node(&t.id).await?;
                    let current = w.native_observed(&t.id).await?;
                    if current.parent != t.parent || current.name != t.name || current.deleted {
                        return Err(Error::Conflict);
                    }
                    let records = w._capability.opens().records().await.map_err(error)?;
                    target = Some(
                        if records.iter().any(|r| {
                            r.input.node == current.id
                                && r.input.volume == w.volume
                                && r.active()
                                && r.close().is_none()
                        }) {
                            w.native_arm(current).await?
                        } else {
                            current
                        },
                    );
                }
                // Target arming changes parent revisions, so refresh both guards.
                let source_parent = w.reader.node(&source_parent.id).await.map_err(error)?;
                let parent = w.reader.node(&parent.id).await.map_err(error)?;
                let owned = w
                    ._capability
                    .namespace()
                    .owned_removal(w.volume.clone(), node.clone())
                    .await
                    .map_err(error)?;
                let target_owned = if let Some(t) = target.as_ref().filter(|t| t.id != node) {
                    w._capability
                        .namespace()
                        .owned_removal(w.volume.clone(), t.id.clone())
                        .await
                        .map_err(error)?
                } else {
                    None
                };
                let destination_control = if target_owned.is_some() {
                    Some(w.native_control(target.as_ref().ok_or(Error::Internal)?)?)
                } else {
                    None
                };
                let destination_removal = target_owned.map(|o| {
                    Box::new(RemovalClaim {
                        fence: o.fence,
                        previous: o.event,
                        mode: o.mode,
                    })
                });
                let mutation = if let Some(o) = owned {
                    Mutation::MoveDeferredEntry {
                        node: guard(&source),
                        source_parent: guard(&source_parent),
                        parent: guard(&parent),
                        name,
                        destination: target.as_ref().map(guard),
                        fence: Box::new(o.fence),
                        previous: o.event.into(),
                        destination_removal,
                    }
                } else {
                    Mutation::MoveEntry {
                        node: guard(&source),
                        source_parent: guard(&source_parent),
                        parent: guard(&parent),
                        name,
                        destination: target.as_ref().map(guard),
                        destination_removal,
                    }
                };
                let mut before = vec![source];
                if let Some(t) = target
                    && t.id != node
                {
                    before.push(t);
                }
                w.native_change(mutation, before, destination_control)
                    .await
                    .map(|r| r.node)
            })
            .await
            .map_err(|_| Error::Timeout)?
        })
        .await
        .map_err(|_| Error::Internal)?
    }
}
