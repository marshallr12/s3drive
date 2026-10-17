//! Native entry operations share the same publication/mutation lock order as
//! writes. No S3 delete or mutable-key rename is performed.
use super::*;
use s3drive_authority::{Artifact, Node, NodeGuard, Receipt};
fn guard(n: &Node) -> NodeGuard {
    NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    }
}
impl Writes {
    pub(super) fn publisher_for(&self, n: &Node) -> Result<&Publisher> {
        let store = n
            .payload
            .as_ref()
            .map(|p| p.reservation.store.id.as_str())
            .or_else(|| n.placement.as_ref().map(|p| p.id.as_str()))
            .unwrap_or(&self.default_store);
        let p = self.publishers.get(store).ok_or(Error::ReadOnly)?;
        if !p.can_edit(&n.id, 0) {
            return Err(Error::ReadOnly);
        }
        Ok(p)
    }
    pub(super) async fn align_metadata(&self, n: &Node) -> Result<()> {
        if n.kind == s3drive_authority::Kind::Directory {
            return Ok(());
        }
        let p = self.publisher_for(n)?;
        let Some(state) = self
            .storage
            .logical()
            .lookup(self.volume.clone(), n.id.clone())
            .await
            .map_err(error)?
        else {
            return Ok(());
        };
        p.check_edit_binding(&state.binding).map_err(error)?;
        if state.base.deleted
            || payload_reference(&state.base.payload) != payload_reference(&n.payload)
            || state.base.placement != n.placement
            || n.revision < state.base.revision
        {
            return Err(Error::Conflict);
        }
        if state.base != *n {
            // Same immutable bytes, credential binding and placement. This is
            // metadata observation, never adoption of an external payload.
            let mut binding = state.binding;
            binding.node = n.clone();
            self.storage
                .logical()
                .observe(binding)
                .await
                .map_err(error)?;
        }
        Ok(())
    }
    pub(super) fn held(&self, node: &str) -> Result<Option<Arc<File>>> {
        Ok(self
            .entries
            .lock()
            .map_err(|_| Error::Internal)?
            .get(node)
            .and_then(Weak::upgrade))
    }
    /// Caller holds publish_serial then edit_serial. Never reenter flush().
    pub(super) async fn prepare_namespace_file(
        &self,
        node: &str,
        flush_latest: bool,
    ) -> Result<()> {
        let Some(state) = self
            .storage
            .logical()
            .lookup(self.volume.clone(), node.into())
            .await
            .map_err(error)?
        else {
            return Ok(());
        };
        let publisher = self
            .publishers
            .get(&state.binding.store.id)
            .ok_or(Error::ReadOnly)?;
        publisher
            .check_edit_binding(&state.binding)
            .map_err(error)?;
        if let Some(file) = self.held(node)? {
            self.check_base(&file, &state)?;
        }
        if flush_latest {
            Box::pin(self.flush_node(node)).await?;
        } else {
            let mut records = self
                .pending()
                .await?
                .into_iter()
                .filter(|r| r.input.volume == self.volume && r.input.node == node)
                .collect::<Vec<_>>();
            records.sort_by_key(|r| r.generation);
            for r in records {
                let generation = r.generation.ok_or(Error::Conflict)?;
                if self
                    .storage
                    .staging()
                    .info(generation)
                    .await
                    .map_err(error)?
                    .phase
                    != s3drive_state::journal::Phase::Sealed
                {
                    return Err(Error::Conflict);
                }
                if r.publication.is_some() {
                    Box::pin(self.finish(publisher, r)).await?;
                }
            }
        }
        let state = self.state(node).await?;
        if let Some(file) = self.held(node)? {
            let mut view = file.view.lock().map_err(|_| Error::Internal)?;
            view.base = payload_reference(&state.base.payload);
            view.deleted = state.base.deleted;
        }
        Ok(())
    }
    fn observe_memory(&self, r: &Receipt) -> Result<()> {
        let mut nodes = vec![&r.node];
        if let Some(
            Artifact::Namespace { displaced: Some(n) }
            | Artifact::RemovalMove {
                displaced: Some(n), ..
            },
        ) = &r.artifact
        {
            nodes.push(n);
        }
        for n in nodes {
            if n.deleted {
                self.removed
                    .lock()
                    .map_err(|_| Error::Internal)?
                    .insert(n.id.clone());
            }
            if let Some(file) = self.held(&n.id)? {
                let mut view = file.view.lock().map_err(|_| Error::Internal)?;
                // The durable bridge already verified that this effect keeps
                // the complete payload reference. It never clears local bytes.
                view.deleted = n.deleted;
            }
        }
        Ok(())
    }
    pub(super) async fn apply_change(
        &self,
        input: s3drive_state::namespace_client::Input,
    ) -> Result<Node> {
        let publisher = self
            .publishers
            .get(&input.store.id)
            .ok_or(Error::ReadOnly)?;
        if let Some(control) = &input.destination_control {
            let target = self
                .publishers
                .get(&control.store.id)
                .ok_or(Error::ReadOnly)?;
            if target.control().map_err(error)? != **control {
                return Err(Error::Conflict);
            }
        }
        let record = publisher.change_namespace(input).await.map_err(error)?;
        let receipt = record
            .outcome
            .ok_or(Error::Conflict)?
            .map_err(|e| error(s3drive_managed::Failure::Authority(e)))?;
        self.observe_memory(&receipt)?;
        Ok(receipt.node)
    }
    pub async fn rename(
        self: &Arc<Self>,
        source: &str,
        parent: &str,
        name: &str,
        destination: (&str, &str),
        replace: bool,
        same_name: fn(&str, &str) -> bool,
    ) -> Result<Node> {
        let slot = self.admit()?;
        let writer = self.clone();
        let source = source.to_owned();
        let parent = parent.to_owned();
        let name = name.to_owned();
        let new_parent = destination.0.to_owned();
        let new_name = destination.1.to_owned();
        tokio::spawn(async move {
            let _slot = slot;
            Box::pin(writer.rename_inner(
                &source,
                &parent,
                &name,
                (&new_parent, &new_name),
                replace,
                same_name,
            ))
            .await
        })
        .await
        .map_err(|_| Error::Internal)?
    }
    async fn rename_inner(
        &self,
        source: &str,
        parent: &str,
        name: &str,
        destination: (&str, &str),
        replace: bool,
        same_name: fn(&str, &str) -> bool,
    ) -> Result<Node> {
        if !self.mutations {
            return Err(Error::ReadOnly);
        }
        let (new_parent, new_name) = destination;
        let key = s3drive_core::names::collision_key(new_name).ok_or(Error::InvalidName)?;
        let _publication = self.publish_serial.lock().await;
        let _mutation = self.edit_serial.lock().await;
        self.recover_namespace().await?;
        let initial = self.reader.node(source).await.map_err(error)?;
        if initial.parent.as_deref() != Some(parent) || !same_name(&initial.name, name) {
            return Err(Error::Conflict);
        }
        self.publisher_for(&initial)?;
        if initial.kind != s3drive_authority::Kind::Directory {
            self.prepare_namespace_file(source, true).await?;
        }
        let source = self.reader.node(source).await.map_err(error)?;
        if source.parent.as_deref() != Some(parent) || !same_name(&source.name, name) {
            return Err(Error::Conflict);
        }
        self.align_metadata(&source).await?;
        let source_parent = self.reader.node(parent).await.map_err(error)?;
        let target_parent = self.reader.node(new_parent).await.map_err(error)?;
        let listing = self.reader.list(new_parent).await.map_err(error)?;
        let target = listing
            .children
            .into_iter()
            .find(|n| s3drive_core::names::collision_key(&n.name).as_ref() == Some(&key));
        let target = if let Some(target) = target {
            if target.id != source.id {
                if !replace || !same_name(&target.name, new_name) {
                    return Err(Error::AlreadyExists);
                }
                self.publisher_for(&target)?;
                if target.kind != s3drive_authority::Kind::Directory {
                    self.prepare_namespace_file(&target.id, false).await?;
                }
                let fresh = self.reader.node(&target.id).await.map_err(error)?;
                if fresh.parent.as_deref() != Some(new_parent) || fresh.name != target.name {
                    return Err(Error::Conflict);
                }
                self.align_metadata(&fresh).await?;
                Some(fresh)
            } else {
                Some(source.clone())
            }
        } else {
            None
        };
        let info = self.reader.volume().await.map_err(error)?;
        let sequence = [
            info.sequence,
            source.revision,
            source_parent.revision,
            target_parent.revision,
            target.as_ref().map_or(0, |n| n.revision),
        ]
        .into_iter()
        .max()
        .ok_or(Error::Internal)?;
        let request = Apply {
            volume: self.volume.clone(),
            operation: random_id()?,
            expected_sequence: sequence,
            mutation: Mutation::MoveEntry {
                destination_removal: None,
                node: guard(&source),
                source_parent: guard(&source_parent),
                parent: guard(&target_parent),
                name: new_name.into(),
                destination: target.as_ref().map(guard),
            },
        };
        let mut before = vec![source.clone()];
        if let Some(target) = target
            && target.id != source.id
        {
            before.push(target);
        }
        let input = self
            .publisher_for(&source)?
            .namespace_change_input(request, &before)
            .map_err(error)?;
        self.apply_change(input).await
    }
    pub async fn remove(
        self: &Arc<Self>,
        node: &str,
        parent: &str,
        name: &str,
        directory: bool,
    ) -> Result<()> {
        let slot = self.admit()?;
        let writer = self.clone();
        let node = node.to_owned();
        let parent = parent.to_owned();
        let name = name.to_owned();
        tokio::spawn(async move {
            let _slot = slot;
            Box::pin(writer.remove_inner(&node, &parent, &name, directory)).await
        })
        .await
        .map_err(|_| Error::Internal)?
    }
    async fn remove_inner(
        &self,
        node: &str,
        parent: &str,
        name: &str,
        directory: bool,
    ) -> Result<()> {
        if !self.mutations {
            return Err(Error::ReadOnly);
        }
        let _publication = self.publish_serial.lock().await;
        let _mutation = self.edit_serial.lock().await;
        self.recover_namespace().await?;
        let initial = self.reader.node(node).await.map_err(error)?;
        if initial.parent.as_deref() != Some(parent) || initial.name != name {
            return Err(Error::Conflict);
        }
        if (initial.kind == s3drive_authority::Kind::Directory) != directory {
            return Err(if directory {
                Error::NotDirectory
            } else {
                Error::IsDirectory
            });
        }
        self.publisher_for(&initial)?;
        if !directory {
            self.prepare_namespace_file(node, false).await?;
        }
        let node = self.reader.node(node).await.map_err(error)?;
        if node.parent.as_deref() != Some(parent) || node.name != name {
            return Err(Error::Conflict);
        }
        self.align_metadata(&node).await?;
        let parent = self.reader.node(parent).await.map_err(error)?;
        let info = self.reader.volume().await.map_err(error)?;
        let request = Apply {
            volume: self.volume.clone(),
            operation: random_id()?,
            expected_sequence: info.sequence.max(node.revision).max(parent.revision),
            mutation: Mutation::RemoveEntry {
                node: guard(&node),
                parent: guard(&parent),
            },
        };
        let input = self
            .publisher_for(&node)?
            .namespace_change_input(request, std::slice::from_ref(&node))
            .map_err(error)?;
        self.apply_change(input).await?;
        Ok(())
    }
}
