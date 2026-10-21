//! Native file-object facts share publication/edit serialization with data I/O.
use super::*;
use s3drive_authority::{Kind as ManagedKind, Node as ManagedNode, NodeGuard};
use s3drive_managed::disposition::Controller;
use s3drive_state::namespace_client::{
    Input,
    opens::{Callback, OpenInput},
};
use sha2::{Digest, Sha256};

pub(super) struct Mount {
    session: String,
    controllers: HashMap<String, Controller>,
}
#[derive(Clone)]
pub struct NativeFileObject(Arc<Object>);
struct Object {
    writer: Arc<Writes>,
    controller: Controller,
    id: String,
    node: String,
}
enum Operation {
    Set(String, bool),
    Cleanup(u32),
    Close,
}
fn callback(id: &str, tag: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"s3drive-native-object-v1\0");
    h.update(id);
    h.update(tag);
    h.finalize()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
impl NativeFileObject {
    pub async fn rename(
        &self,
        parent: &str,
        name: &str,
        replace: bool,
        same_name: fn(&str, &str) -> bool,
    ) -> Result<ManagedNode> {
        let slot = self.0.writer.admit()?;
        let object = self.0.clone();
        let parent = parent.to_owned();
        let name = name.to_owned();
        tokio::spawn(async move {
            let _slot = slot;
            let writer = &object.writer;
            let _publication = writer.publish_serial.lock().await;
            let _mutation = writer.edit_serial.lock().await;
            writer.recover_namespace().await?;
            object
                .controller
                .recover(&object.node)
                .await
                .map_err(error)?;
            let initial = writer.observed_node(&object.node).await?;
            if initial.deleted {
                return Err(Error::NotFound);
            }
            if initial.kind != ManagedKind::Directory {
                writer.prepare_namespace_file(&initial.id, true).await?;
            }
            let source = writer.observed_node(&object.node).await?;
            writer.align_metadata(&source).await?;
            let source_parent = writer
                .observed_node(source.parent.as_ref().ok_or(Error::InvalidOperation)?)
                .await?;
            let parent = writer.observed_node(&parent).await?;
            let key = s3drive_core::names::collision_key(&name).ok_or(Error::InvalidName)?;
            let listed = writer.reader.list(&parent.id).await.map_err(error)?;
            let mut target = None;
            if let Some(found) = listed
                .children
                .into_iter()
                .find(|n| s3drive_core::names::collision_key(&n.name).as_ref() == Some(&key))
            {
                if found.id != source.id {
                    let target_publisher = writer.publisher_for(&found)?;
                    let controller = writer
                        .lifecycle
                        .get()
                        .ok_or(Error::Internal)?
                        .controllers
                        .get(&target_publisher.store().id)
                        .ok_or(Error::ReadOnly)?;
                    controller.recover(&found.id).await.map_err(error)?;
                    if !replace || !same_name(&found.name, &name) {
                        return Err(Error::AlreadyExists);
                    }
                    if found.kind != ManagedKind::Directory {
                        writer.prepare_namespace_file(&found.id, false).await?;
                    }
                    let fresh = writer.observed_node(&found.id).await?;
                    if fresh.parent.as_ref() != Some(&parent.id)
                        || fresh.name != found.name
                        || fresh.deleted
                    {
                        return Err(Error::Conflict);
                    }
                    writer.align_metadata(&fresh).await?;
                    target = Some(fresh);
                } else {
                    target = Some(source.clone());
                }
            }
            let publisher = writer.publisher_for(&source)?;
            let owned = publisher.owned_removal(&source.id).await.map_err(error)?;
            let target_publisher = target
                .as_ref()
                .filter(|t| t.id != source.id)
                .map(|t| writer.publisher_for(t))
                .transpose()?;
            let target_owned = if let (Some(t), Some(p)) = (&target, target_publisher) {
                p.owned_removal(&t.id).await.map_err(error)?
            } else {
                None
            };
            let destination_removal = target_owned.map(|o| {
                Box::new(s3drive_authority::RemovalClaim {
                    fence: o.fence,
                    previous: o.event,
                    mode: o.mode,
                })
            });
            let guard = |n: &ManagedNode| NodeGuard {
                id: n.id.clone(),
                revision: n.revision,
            };
            let mutation = if let Some(owned) = owned {
                Mutation::MoveDeferredEntry {
                    node: guard(&source),
                    source_parent: guard(&source_parent),
                    parent: guard(&parent),
                    name,
                    destination: target.as_ref().map(guard),
                    fence: Box::new(owned.fence),
                    previous: owned.event.into(),
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
            let sequence = writer.reader.volume().await.map_err(error)?.sequence;
            let request = Apply {
                volume: writer.volume.clone(),
                operation: random_id()?,
                expected_sequence: sequence
                    .max(source.revision)
                    .max(parent.revision)
                    .max(source_parent.revision)
                    .max(target.as_ref().map_or(0, |n| n.revision)),
                mutation,
            };
            let mut before = vec![source];
            if let Some(target) = target
                && target.id != object.node
            {
                before.push(target);
            }
            let input = if request.mutation.owned_destination().is_some() {
                publisher
                    .namespace_change_input_with_destination(
                        request,
                        &before,
                        target_publisher.ok_or(Error::Conflict)?,
                    )
                    .map_err(error)?
            } else {
                publisher
                    .namespace_change_input(request, &before)
                    .map_err(error)?
            };
            writer.apply_change(input).await
        })
        .await
        .map_err(|_| Error::Internal)?
    }
    pub async fn set_delete(&self, delete: bool) -> Result<()> {
        self.process(Operation::Set(random_id()?, delete)).await
    }
    pub async fn cleanup(&self, flags: u32) -> Result<()> {
        // No network or filesystem mutation lock precedes the observed fact.
        self.0
            .writer
            .storage
            .opens()
            .callback(
                self.0.id.clone(),
                callback(&self.0.id, "cleanup"),
                Callback::Cleanup { flags },
            )
            .await
            .map_err(error)?;
        self.process(Operation::Cleanup(flags)).await
    }
    pub async fn close(&self) -> Result<()> {
        self.0
            .writer
            .storage
            .opens()
            .callback(
                self.0.id.clone(),
                callback(&self.0.id, "close"),
                Callback::Close,
            )
            .await
            .map_err(error)?;
        self.process(Operation::Close).await
    }
    async fn process(&self, operation: Operation) -> Result<()> {
        let slot = self.0.writer.admit()?;
        let object = self.0.clone();
        // A native timeout may drop the JoinHandle, never the accepted task.
        tokio::spawn(async move {
            let _slot = slot;
            let _publication = object.writer.publish_serial.lock().await;
            let _mutation = object.writer.edit_serial.lock().await;
            let result = match operation {
                Operation::Set(cb, delete) => object
                    .controller
                    .set_delete(&object.id, &cb, delete)
                    .await
                    .map(|_| ()),
                Operation::Cleanup(flags) => {
                    object
                        .controller
                        .cleanup(&object.id, &callback(&object.id, "cleanup"), flags)
                        .await
                }
                Operation::Close => {
                    object
                        .controller
                        .close(&object.id, &callback(&object.id, "close"))
                        .await
                }
            };
            // Even a failed acknowledgment can follow an authoritative effect.
            // Synchronize only our already-verified durable logical base.
            object.writer.sync_object_view(&object.node).await?;
            result.map_err(error)
        })
        .await
        .map_err(|_| Error::Internal)?
    }
}
impl Writes {
    pub fn native_lifecycle_enabled(&self) -> bool {
        self.lifecycle_enabled
    }
    pub async fn start_native_lifecycle(&self) -> Result<()> {
        if !self.lifecycle_enabled {
            return Ok(());
        }
        let session = random_id()?;
        let mut controllers = HashMap::new();
        for (store, publisher) in &self.publishers {
            controllers.insert(
                store.clone(),
                Controller::attach(publisher.clone(), session.clone())
                    .await
                    .map_err(error)?,
            );
        }
        self.lifecycle
            .set(Mount {
                session,
                controllers,
            })
            .map_err(|_| Error::Internal)
    }
    pub fn recover_native_lifecycle(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(self.recover_lifecycle(true))
    }
    pub(super) fn recover_native_callbacks(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(self.recover_lifecycle(false))
    }
    async fn recover_lifecycle(&self, all: bool) -> Result<()> {
        let Some(mount) = self.lifecycle.get() else {
            return Ok(());
        };
        let mut nodes = HashSet::new();
        for record in self.storage.opens().records().await.map_err(error)? {
            if record.input.volume == self.volume
                && (all
                    || record.events.iter().any(|event| {
                        matches!(
                            event.action,
                            s3drive_state::namespace_client::opens::Action::Callback(_)
                        ) && !record.completed(&event.id)
                    }))
                && nodes.insert(record.input.node.clone())
            {
                let controller = mount
                    .controllers
                    .get(&record.input.store.id)
                    .ok_or(Error::ReadOnly)?;
                controller
                    .recover(&record.input.node)
                    .await
                    .map_err(error)?;
                self.sync_object_view(&record.input.node).await?;
            }
        }
        Ok(())
    }
    async fn sync_object_view(&self, node: &str) -> Result<()> {
        let Some(state) = self
            .storage
            .logical()
            .lookup(self.volume.clone(), node.into())
            .await
            .map_err(error)?
        else {
            return Ok(());
        };
        if state.base.deleted {
            self.removed
                .lock()
                .map_err(|_| Error::Internal)?
                .insert(node.into());
        }
        if let Some(file) = self.held(node)? {
            let mut view = file.view.lock().map_err(|_| Error::Internal)?;
            view.base = payload_reference(&state.base.payload);
            view.deleted = state.base.deleted;
        }
        Ok(())
    }
    pub(super) async fn observed_node(&self, node: &str) -> Result<ManagedNode> {
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
                    .storage
                    .namespace()
                    .owned_removal(self.volume.clone(), node.into())
                    .await
                    .map_err(error)?
                    .ok_or(Error::DeletePending)?;
                let publisher = self
                    .publishers
                    .get(&local.input.store.id)
                    .ok_or(Error::ReadOnly)?;
                publisher
                    .check_namespace_input(&local.input)
                    .map_err(error)?;
                if local.fence != p.fence || local.mode != p.mode {
                    return Err(Error::Conflict);
                }
                Ok(p.node)
            }
            Reply::PendingRemoval(None) => match self
                .authority
                .request(&Request::Node {
                    volume: self.volume.clone(),
                    node: node.into(),
                })
                .await
                .map_err(error)?
            {
                Reply::Node(n) if n.id == node => Ok(n),
                Reply::Error(e) => Err(error(s3drive_managed::Failure::Authority(e))),
                _ => Err(Error::Conflict),
            },
            Reply::Error(e) => Err(error(s3drive_managed::Failure::Authority(e))),
            _ => Err(Error::Conflict),
        }
    }
    async fn object(
        self: &Arc<Self>,
        publisher: &Publisher,
        node: &str,
        doc: bool,
        admission: Option<Input>,
    ) -> Result<NativeFileObject> {
        let mount = self.lifecycle.get().ok_or(Error::ReadOnly)?;
        let controller = mount
            .controllers
            .get(&publisher.store().id)
            .ok_or(Error::ReadOnly)?
            .clone();
        let control = publisher.control().map_err(error)?;
        let id = random_id()?;
        controller
            .open(OpenInput {
                id: id.clone(),
                session: mount.session.clone(),
                volume: self.volume.clone(),
                node: node.into(),
                store: control.store,
                source: control.source,
                fingerprint: control.fingerprint,
                delete_on_close: doc,
                controlled: true,
                session_bound: true,
                posix: true,
                admission: admission.map(Box::new),
                creation: None,
            })
            .await
            .map_err(error)?;
        Ok(NativeFileObject(Arc::new(Object {
            writer: self.clone(),
            controller,
            id,
            node: node.into(),
        })))
    }
    pub async fn native_open(
        self: &Arc<Self>,
        node: &str,
        doc: bool,
    ) -> Result<Option<NativeFileObject>> {
        if !self.lifecycle_enabled || node == s3drive_authority::ROOT {
            return if doc { Err(Error::ReadOnly) } else { Ok(None) };
        }
        let slot = self.admit()?;
        let writer = self.clone();
        let node = node.to_owned();
        tokio::spawn(async move {
            let _slot = slot;
            let _publication = writer.publish_serial.lock().await;
            let _mutation = writer.edit_serial.lock().await;
            writer.recover_namespace().await?;
            let before = writer.observed_node(&node).await?;
            let publisher = match writer.publisher_for(&before) {
                Ok(p) => p,
                Err(Error::ReadOnly) if !doc => return Ok(None),
                Err(e) => return Err(e),
            };
            let controller = writer
                .lifecycle
                .get()
                .ok_or(Error::Internal)?
                .controllers
                .get(&publisher.store().id)
                .ok_or(Error::ReadOnly)?;
            controller.recover_previous(&node).await.map_err(error)?;
            let before = writer.observed_node(&node).await?;
            if before.deleted {
                return Err(Error::NotFound);
            }
            let admission = if doc
                && publisher
                    .owned_removal(&node)
                    .await
                    .map_err(error)?
                    .is_none()
            {
                let parent = writer
                    .reader
                    .node(before.parent.as_ref().ok_or(Error::InvalidOperation)?)
                    .await
                    .map_err(error)?;
                let info = writer.reader.volume().await.map_err(error)?;
                Some(
                    publisher
                        .namespace_change_input(
                            Apply {
                                volume: writer.volume.clone(),
                                operation: random_id()?,
                                expected_sequence: info
                                    .sequence
                                    .max(before.revision)
                                    .max(parent.revision),
                                mutation: Mutation::PrepareRemoval {
                                    node: NodeGuard {
                                        id: node.clone(),
                                        revision: before.revision,
                                    },
                                    parent: NodeGuard {
                                        id: parent.id,
                                        revision: parent.revision,
                                    },
                                    owner: publisher.client_identity().await.map_err(error)?,
                                },
                            },
                            std::slice::from_ref(&before),
                        )
                        .map_err(error)?,
                )
            } else {
                None
            };
            writer
                .object(publisher, &node, doc, admission)
                .await
                .map(Some)
        })
        .await
        .map_err(|_| Error::Internal)?
    }
    pub async fn native_create(
        self: &Arc<Self>,
        parent: &str,
        name: &str,
        kind: ManagedKind,
    ) -> Result<(ManagedNode, NativeFileObject)> {
        if !self.lifecycle_enabled {
            return Err(Error::ReadOnly);
        }
        let slot = self.admit()?;
        let writer = self.clone();
        let parent = parent.to_owned();
        let name = name.to_owned();
        tokio::spawn(async move {
            let _slot = slot;
            let _publication = writer.publish_serial.lock().await;
            let _mutation = writer.edit_serial.lock().await;
            writer.recover_namespace().await?;
            let parent = writer.reader.node(&parent).await.map_err(error)?;
            let publisher = writer
                .publishers
                .get(&writer.default_store)
                .ok_or(Error::ReadOnly)?;
            let node = random_id()?;
            if !publisher.can_edit(&node, 0) {
                return Err(Error::ReadOnly);
            }
            let info = writer.reader.volume().await.map_err(error)?;
            let admission = publisher
                .namespace_input(Apply {
                    volume: writer.volume.clone(),
                    operation: node.clone(),
                    expected_sequence: info.sequence.max(parent.revision),
                    mutation: Mutation::CreateDeferredEntry {
                        parent: parent.id,
                        parent_revision: parent.revision,
                        store: (kind == ManagedKind::EmptyFile)
                            .then(|| writer.default_store.clone()),
                        name,
                        node_kind: kind,
                        owner: publisher.client_identity().await.map_err(error)?,
                    },
                })
                .map_err(error)?;
            let object = writer
                .object(publisher, &node, true, Some(admission))
                .await?;
            Ok((writer.observed_node(&node).await?, object))
        })
        .await
        .map_err(|_| Error::Internal)?
    }
}
