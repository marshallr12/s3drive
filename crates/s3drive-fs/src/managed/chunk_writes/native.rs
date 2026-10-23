//! Candidate-only original-handle adapter. No default capability activation.
use super::*;
use s3drive_managed::{
    chunks::NativeSparseFile,
    disposition::{Controller, NativeContent, Task},
};
use s3drive_state::namespace_client::opens::{Callback, OpenInput};

pub(super) struct Mount {
    session: String,
    runtime: tokio::runtime::Handle,
    controllers: HashMap<String, Controller>,
    // Strong original capabilities survive kernel-handle removal and failed Close.
    // Never rebuilt from persisted IDs. Bound before admitting another Open.
    opens: Mutex<HashMap<String, Entry>>,
}
#[derive(Clone)]
struct Entry {
    native: NativeSparseFile,
    file: Arc<File>,
}
#[derive(Clone)]
pub(crate) struct Object {
    writer: Arc<Writes>,
    entry: Entry,
    controller: Controller,
}
struct Content(Weak<Writes>);
fn callback(open: &str, role: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b"s3drive-native-sparse-callback-v1\0");
    h.update(open);
    h.update(role);
    hex(&h.finalize()[..16])
}
impl Writes {
    pub(crate) async fn start_native(self: &Arc<Self>) -> Result<()> {
        // Opting in does not adopt an epoch created by the legacy profile.
        // Saved known roots were reconciled by the constructor; unknown native
        // heads retain their original records and use recover_previous below.
        let retained = self._capability.opens().records().await.map_err(error)?;
        for head in self.local.heads().await.map_err(error)? {
            let base = head.binding().base().map_err(error)?;
            let node = base.node();
            if !head.is_clean()
                && !retained
                    .iter()
                    .any(|r| r.input.volume == self.volume && r.input.node == node)
            {
                return Err(Error::Conflict);
            }
        }
        let session = hex(&fresh()?[..16]);
        let mut controllers = HashMap::new();
        for (id, (store, executor, _)) in &self.stores {
            controllers.insert(
                id.clone(),
                Controller::attach_native(
                    self._capability.clone(),
                    executor.clone(),
                    self.authority.clone(),
                    store.clone(),
                    session.clone(),
                    Arc::new(Content(Arc::downgrade(self))),
                    self.native_serial.clone(),
                )
                .await
                .map_err(error)?,
            );
        }
        self.native
            .set(Mount {
                session,
                runtime: tokio::runtime::Handle::current(),
                controllers,
                opens: Mutex::new(HashMap::new()),
            })
            .map_err(|_| Error::Internal)?;
        // Only explicit previous-session policy may abandon clean old opens.
        // Dirty or unknown original work remains a conflict, never adopted.
        let mount = self.native.get().ok_or(Error::Internal)?;
        let mut nodes = HashSet::new();
        for record in self._capability.opens().records().await.map_err(error)? {
            if record.input.volume == self.volume && nodes.insert(record.input.node.clone()) {
                mount
                    .controllers
                    .get(&record.input.store.id)
                    .ok_or(Error::Conflict)?
                    .recover_previous(&record.input.node)
                    .await
                    .map_err(error)?;
            }
        }
        Ok(())
    }
    pub(crate) async fn native_object(self: &Arc<Self>, file: Arc<File>) -> Result<Object> {
        let _open = self.open_serial.lock().await;
        let mount = self.native.get().ok_or(Error::Internal)?;
        if mount.opens.lock().map_err(|_| Error::Internal)?.len() >= 256 {
            return Err(Error::Busy);
        }
        let (store, executor, _) = self
            .stores
            .get(&hex(file.binding.store()))
            .ok_or(Error::ReadOnly)?;
        let controller = mount
            .controllers
            .get(&store.id)
            .ok_or(Error::ReadOnly)?
            .clone();
        let target = executor
            .recovery_target(&format!("{}binding", store.root))
            .map_err(error)?;
        let open = hex(&fresh()?[..16]);
        controller
            .open(OpenInput {
                id: open.clone(),
                session: mount.session.clone(),
                volume: self.volume.clone(),
                node: file.binding.base().map_err(error)?.node().into(),
                store: store.clone(),
                source: *target.source(),
                fingerprint: target.fingerprint().into(),
                delete_on_close: false,
                controlled: true,
                session_bound: true,
                posix: true,
                admission: None,
                creation: None,
            })
            .await
            .map_err(error)?;
        let native = NativeSparseFile::attach(
            &self._capability,
            self.local.clone(),
            open.clone(),
            views::Options::default(),
            plans::Options::default(),
        )
        .await
        .map_err(error)?;
        let entry = Entry { native, file };
        mount
            .opens
            .lock()
            .map_err(|_| Error::Internal)?
            .insert(open, entry.clone());
        Ok(Object {
            writer: self.clone(),
            entry,
            controller,
        })
    }
    pub(super) async fn flush_native_node(&self, node: &str) -> Result<()> {
        // Caller owns native_serial. Never reacquire it from the content hook.
        let _publishing = self.publish_serial.lock().await;
        let mount = self.native.get().ok_or(Error::Internal)?;
        let entries: Vec<_> = mount
            .opens
            .lock()
            .map_err(|_| Error::Internal)?
            .values()
            .filter(|e| e.file.binding.base().is_ok_and(|b| b.node() == node))
            .cloned()
            .collect();
        let Some(first) = entries.first() else {
            // Cold metadata replay cannot create a replacement runtime token.
            return if self
                .local
                .heads()
                .await
                .map_err(error)?
                .iter()
                .any(|v| !v.is_clean() && v.binding().base().is_ok_and(|b| b.node() == node))
            {
                Err(Error::Conflict)
            } else {
                Ok(())
            };
        };
        let head = first.file.head().await?;
        if !head.is_clean() {
            let known = first.entry_plans().await?;
            if !known.iter().any(|p| p.view().id() == head.id()) {
                let edited = first.edited(&head).await?;
                first.native.prepare(&edited).await.map_err(error)?;
            }
        }
        for info in first.entry_plans().await? {
            let plan = info.id().map_err(error)?;
            if self
                .roots
                .records()
                .await
                .map_err(error)?
                .iter()
                .any(|r| r.input().plan() == plan && r.acknowledgment().is_some())
            {
                continue;
            }
            let owner = info.native_ownership().ok_or(Error::Conflict)?.open();
            let original = mount
                .opens
                .lock()
                .map_err(|_| Error::Internal)?
                .get(owner)
                .cloned()
                .ok_or(Error::Conflict)?;
            let prep = original
                .native
                .reopen(&self.reader, plan)
                .await
                .map_err(error)?;
            prep.prepare().await.map_err(error)?;
            let (_, executor, _) = self
                .stores
                .get(&hex(info.view().binding().store()))
                .ok_or(Error::ReadOnly)?;
            prep.uploader(executor.clone())
                .map_err(error)?
                .upload_tree()
                .await
                .map_err(error)?;
            let publisher = prep
                .publisher(executor.clone(), roots::Limits::default())
                .map_err(error)?;
            publisher.check_semantics().await.map_err(error)?;
            publisher
                .begin(hex(
                    &derived(b"s3drive-native-sparse-publish-v1\0", &plan)[..16]
                ))
                .await
                .map_err(error)?;
            publisher.resume().await.map_err(error)?;
            publisher
                .acknowledge_scoped(
                    token(
                        info.view().binding(),
                        derived(b"s3drive-native-chunk-ack-v1\0", &plan),
                    )?,
                    views::Options::default(),
                )
                .await
                .map_err(error)?;
        }
        Ok(())
    }
}
impl Entry {
    async fn entry_plans(&self) -> Result<Vec<plans::Info>> {
        self.native.plans().await.map_err(error)
    }
    async fn edited(&self, head: &View) -> Result<EditedFile> {
        let saved = self
            .file
            .edited
            .lock()
            .map_err(|_| Error::Internal)?
            .clone();
        let next = match saved {
            Some(e) if e.view().id() == head.id() => e,
            Some(e) if e.view().binding() == head.binding() => {
                self.native.refresh(&e, head).await.map_err(error)?
            }
            _ => self
                .file
                .reader
                .reopen_edited(self.file.local.clone(), head.id())
                .await
                .map_err(error)?,
        };
        *self.file.edited.lock().map_err(|_| Error::Internal)? = Some(next.clone());
        Ok(next)
    }
}
impl NativeContent for Content {
    fn flush<'a>(&'a self, node: &'a str) -> Task<'a, ()> {
        Box::pin(async move {
            self.0
                .upgrade()
                .ok_or(Error::Internal)?
                .flush_native_node(node)
                .await?;
            Ok(())
        })
    }
    fn complete<'a>(&'a self, open: &'a str, cb: &'a str, completion: String) -> Task<'a, ()> {
        Box::pin(async move {
            let writer = self.0.upgrade().ok_or(Error::Internal)?;
            let entry = writer
                .native
                .get()
                .ok_or(Error::Internal)?
                .opens
                .lock()
                .map_err(|_| Error::Internal)?
                .get(open)
                .cloned()
                .ok_or(Error::Conflict)?;
            entry.native.complete(cb.into(), completion).await?;
            Ok(())
        })
    }
}
impl Object {
    pub(crate) fn is_unlinked(&self) -> bool {
        self.entry.file.unlinked.load(Ordering::Acquire)
    }
    pub(crate) async fn rename(
        &self,
        parent: &str,
        name: &str,
        replace: bool,
        same_name: fn(&str, &str) -> bool,
    ) -> Result<RemoteNode> {
        self.writer
            .native_rename(
                self.entry.file.binding.base().map_err(error)?.node(),
                parent,
                name,
                replace,
                same_name,
            )
            .await
    }
    fn job<T: Send + 'static, F: std::future::Future<Output = Result<T>> + Send + 'static>(
        &self,
        future: F,
    ) -> impl std::future::Future<Output = Result<T>> + Send + use<T, F> {
        let permit = self.writer.admit();
        dispatch::run(async move {
            let _permit = permit?;
            tokio::time::timeout(DEADLINE, future)
                .await
                .map_err(|_| Error::Timeout)?
        })
    }
    pub(crate) fn read(
        &self,
        offset: u64,
        length: usize,
    ) -> impl std::future::Future<Output = Result<Vec<u8>>> + Send + use<> {
        let object = self.clone();
        self.job(async move { Box::pin(object.read_inner(offset, length)).await })
    }
    pub(crate) fn write(
        &self,
        offset: u64,
        bytes: Arc<[u8]>,
        append: bool,
        constrained: bool,
    ) -> impl std::future::Future<Output = Result<usize>> + Send + use<> {
        let object = self.clone();
        self.job(
            async move { Box::pin(object.write_inner(offset, bytes, append, constrained)).await },
        )
    }
    pub(crate) fn truncate(
        &self,
        size: u64,
    ) -> impl std::future::Future<Output = Result<()>> + Send + use<> {
        let object = self.clone();
        self.job(async move { Box::pin(object.truncate_inner(size)).await })
    }
    pub(crate) fn flush(&self) -> impl std::future::Future<Output = Result<()>> + Send + use<> {
        let object = self.clone();
        self.job(async move { Box::pin(object.flush_inner()).await })
    }
    fn allow_write(&self) -> Result<()> {
        let (store, e, _) = self
            .writer
            .stores
            .get(&hex(self.entry.file.binding.store()))
            .ok_or(Error::ReadOnly)?;
        e.check_create(&format!("{}binding", store.root), 0)
            .map_err(error)
    }
    async fn read_inner(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        let _serial = self.entry.file.serial.lock().await;
        let head = self.entry.file.head().await?;
        self.entry
            .edited(&head)
            .await?
            .read_range(offset, length)
            .await
            .map_err(error)
    }
    async fn write_inner(
        &self,
        offset: u64,
        bytes: Arc<[u8]>,
        append: bool,
        constrained: bool,
    ) -> Result<usize> {
        let _serial = self.writer.native_serial.lock().await;
        let _file = self.entry.file.serial.lock().await;
        self.allow_write()?;
        let head = self.entry.file.head().await?;
        let offset = if append { head.size() } else { offset };
        let count = if constrained {
            bytes.len().min(head.size().saturating_sub(offset) as usize)
        } else {
            bytes.len()
        };
        let edited = self.entry.edited(&head).await?;
        let next = self
            .entry
            .native
            .write(&edited, fresh()?, offset, bytes[..count].to_vec())
            .await
            .map_err(error)?;
        self.entry
            .file
            .metadata
            .lock()
            .map_err(|_| Error::Internal)?
            .size = next.size();
        Ok(count)
    }
    async fn truncate_inner(&self, size: u64) -> Result<()> {
        let _serial = self.writer.native_serial.lock().await;
        let _file = self.entry.file.serial.lock().await;
        self.allow_write()?;
        let head = self.entry.file.head().await?;
        if size == head.size() {
            return Ok(());
        }
        let edited = self.entry.edited(&head).await?;
        let next = self
            .entry
            .native
            .resize(&edited, fresh()?, size)
            .await
            .map_err(error)?;
        self.entry
            .file
            .metadata
            .lock()
            .map_err(|_| Error::Internal)?
            .size = next.size();
        Ok(())
    }
    async fn flush_inner(&self) -> Result<()> {
        let _serial = self.writer.native_serial.lock().await;
        self.writer
            .flush_native_node(self.entry.file.binding.base().map_err(error)?.node())
            .await
    }
    pub(crate) fn close(&self) -> impl std::future::Future<Output = Result<()>> + Send + 'static {
        let object = self.clone();
        let cb = callback(self.entry.native.open(), b"close");
        let observed = self.entry.native.observe_close(&cb).map_err(error);
        let permit = object.writer.admit();
        // Spawn through the mount's runtime at callback entry, including from a
        // driver thread outside Tokio. Dropping an unpolled join cannot cancel
        // the bounded accepted Close job or lose its original capability.
        let runtime = object.writer.native.get().map(|m| m.runtime.clone());
        let task = runtime.map(|runtime| {
            runtime.spawn(async move {
                observed?;
                let _permit = permit?;
                tokio::time::timeout(DEADLINE, async {
                    object.entry.native.close(cb.clone()).await.map_err(error)?;
                    object
                        .controller
                        .close(object.entry.native.open(), &cb)
                        .await
                        .map_err(error)?;
                    object
                        .writer
                        .native
                        .get()
                        .ok_or(Error::Internal)?
                        .opens
                        .lock()
                        .map_err(|_| Error::Internal)?
                        .remove(object.entry.native.open());
                    Ok(())
                })
                .await
                .map_err(|_| Error::Timeout)?
            })
        });
        async move {
            task.ok_or(Error::Internal)?
                .await
                .map_err(|_| Error::Internal)?
        }
    }
    pub(crate) async fn cleanup(&self, flags: u32) -> Result<()> {
        let object = self.clone();
        let slot = self.writer.admit()?;
        tokio::spawn(async move {
            let _slot = slot;
            tokio::time::timeout(DEADLINE, async {
                let open = object.entry.native.open();
                let cb = callback(open, b"cleanup");
                object
                    .writer
                    ._capability
                    .opens()
                    .callback(open.into(), cb.clone(), Callback::Cleanup { flags })
                    .await
                    .map_err(error)?;
                let result = object
                    .controller
                    .cleanup(open, &cb, flags)
                    .await
                    .map_err(error);
                object
                    .writer
                    .native_sync_node(object.entry.file.binding.base().map_err(error)?.node())
                    .await?;
                result
            })
            .await
            .map_err(|_| Error::Timeout)?
        })
        .await
        .map_err(|_| Error::Internal)?
    }
    pub(crate) async fn set_delete(&self, delete: bool) -> Result<()> {
        let object = self.clone();
        let slot = self.writer.admit()?;
        tokio::spawn(async move {
            let _slot = slot;
            tokio::time::timeout(DEADLINE, async {
                let result = object
                    .controller
                    .set_delete(object.entry.native.open(), &hex(&fresh()?[..16]), delete)
                    .await
                    .map(|_| ())
                    .map_err(error);
                object
                    .writer
                    .native_sync_node(object.entry.file.binding.base().map_err(error)?.node())
                    .await?;
                result
            })
            .await
            .map_err(|_| Error::Timeout)?
        })
        .await
        .map_err(|_| Error::Internal)?
    }
}
