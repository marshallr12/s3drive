use super::*;
pub(super) mod chunk_writes;
pub(crate) mod native;
pub(super) mod writes;
use s3drive_authority::{Token, WireClient};
pub(super) struct Backend {
    pub reader: s3drive_managed::Reader,
    pub chunks: Option<s3drive_managed::chunks::Reader>,
    pub writes: Option<Arc<writes::Writes>>,
    pub chunk_writes: Option<Arc<chunk_writes::Writes>>,
}
impl Backend {
    pub async fn node(&self, id: &str) -> Result<s3drive_authority::Node> {
        match &self.chunks {
            Some(r) => r.node(id).await.map_err(error),
            None => self.reader.node(id).await.map_err(error),
        }
    }
    pub async fn list(&self, parent: &str) -> Result<s3drive_authority::Listing> {
        match &self.chunks {
            Some(r) => r.list(parent).await.map_err(error),
            None => self.reader.list(parent).await.map_err(error),
        }
    }
}
pub(super) async fn filesystem(config: &Config) -> Result<ReadOnlyFs> {
    let settings = config.managed.as_ref().ok_or(Error::Configuration)?;
    let native_lifecycle = settings.sparse_lifecycle.is_some();
    #[cfg(not(feature = "native-sparse-candidate"))]
    if native_lifecycle {
        return Err(Error::Configuration);
    }
    let token = Token::from_private_file(&settings.token_file).map_err(|_| Error::Configuration)?;
    let authority = WireClient::from_config(settings, token).map_err(|_| Error::Configuration)?;
    #[cfg(feature = "native-sparse-candidate")]
    if config.version == 8
        && let Some(w) = &config.writes
    {
        let directory = w.directory.clone();
        let inspect = tokio::task::spawn_blocking(move || -> s3drive_state::Result<bool> {
            match std::fs::symlink_metadata(&directory) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err(e.into()),
                Ok(_) => {
                    // An explicitly empty directory is also a fresh initialization.
                    // Links, foreign contents and partial stores use strict checks.
                    if std::fs::read_dir(&directory)?.next().is_none() {
                        return Ok(false);
                    }
                    s3drive_state::namespace_client::recovery::RecoveryStore::needs_startup_replay(
                        &directory,
                    )
                }
            }
        });
        let needed = tokio::time::timeout(Duration::from_secs(30), inspect)
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::Internal)?
            .map_err(error)?;
        if needed {
            let recovery = s3drive_managed::namespace_recovery::Recovery::open_existing(
                &w.directory,
                authority.clone(),
                settings.volume.clone(),
                Duration::from_secs(30),
            )
            .await
            .map_err(error)?;
            if native_lifecycle {
                for record in recovery.records().await.map_err(error)? {
                    recovery
                        .recover(record.input.request.operation)
                        .await
                        .map_err(error)?;
                }
                recovery.recover_controller_links().await.map_err(error)?;
            } else if !recovery.open_records().await.map_err(error)?.is_empty() {
                // Removing the opt-in cannot adopt retained original opens or
                // turn unknown native work into ordinary v8 retry permission.
                recovery.close().await.map_err(error)?;
                return Err(Error::Configuration);
            }
            recovery.close().await.map_err(error)?;
        }
    }
    let storage = if let Some(w) = &config.writes {
        Some(
            s3drive_state::worker::Storage::open(
                &w.directory,
                Options::default(),
                s3drive_state::journal::Options::default(),
            )
            .await
            .map_err(error)?,
        )
    } else {
        None
    };
    if config.writes.is_some() {
        let reply = authority
            .request(&s3drive_authority::Request::Features {})
            .await
            .map_err(error)?;
        let s3drive_authority::Reply::Features(features) = reply else {
            return Err(Error::Configuration);
        };
        let mut required = vec!["content-cas-v1"];
        if (5..=7).contains(&config.version) {
            required.push("namespace-create-v1");
        }
        if (6..=7).contains(&config.version) {
            required.extend(["namespace-entry-cas-v1", "detached-content-cas-v1"]);
        }
        if config.version == 7 {
            required.extend([
                "pending-removal-v1",
                "deferred-removal-v1",
                "deferred-create-v1",
                "deferred-owner-move-v1",
                "owned-target-replace-v1",
            ]);
        }
        if config.version == 8 {
            #[cfg(feature = "native-sparse-candidate")]
            if native_lifecycle {
                required.extend([
                    "pending-removal-v1",
                    "deferred-removal-v1",
                    "namespace-entry-cas-v1",
                    "owned-target-replace-v1",
                    "deferred-owner-move-v1",
                ]);
            }
            required.extend([
                "manifest-root-v1",
                "manifest-history-v1",
                "atomic-empty-manifest-v1",
            ]);
        }
        if required
            .iter()
            .any(|needed| !features.iter().any(|f| f == needed))
        {
            return Err(Error::Configuration);
        }
    }
    let mut bindings = Vec::new();
    let mut publishers = HashMap::new();
    let mut chunk_executors = HashMap::new();
    for mapping in &settings.stores {
        let source = config
            .sources
            .iter()
            .find(|s| s.alias == mapping.source)
            .ok_or(Error::Configuration)?;
        let mut reader = Box::pin(S3Reader::from_source(source)).await?;
        if let Some(w) = config
            .writes
            .as_ref()
            .and_then(|w| w.sources.iter().find(|w| w.alias == source.alias))
        {
            fn text(path: &std::path::Path) -> Result<String> {
                use std::io::Read;
                let file = std::fs::File::open(path).map_err(|_| Error::Configuration)?;
                if !file.metadata().map_err(|_| Error::Configuration)?.is_file() {
                    return Err(Error::Configuration);
                }
                let mut text = String::new();
                file.take(32769)
                    .read_to_string(&mut text)
                    .map_err(|_| Error::Configuration)?;
                Ok(text)
            }
            let policy = s3drive_s3::uploads::WritePolicy::from_toml(
                source,
                &text(&w.policy)?,
                &text(&w.plan)?,
                &text(&w.report)?,
            )
            .map_err(|_| Error::Configuration)?;
            let executor = s3drive_s3::uploads::Executor::open(
                reader,
                storage.clone().ok_or(Error::Internal)?,
                policy,
            )
            .await
            .map_err(error)?;
            reader = executor.reader();
            if config.version == 8 {
                chunk_executors.insert(mapping.id.clone(), executor);
            } else {
                let publisher = s3drive_managed::Publisher::open(
                    executor,
                    authority.clone(),
                    settings.volume.clone(),
                    mapping.id.clone(),
                )
                .await
                .map_err(error)?;
                publishers.insert(mapping.id.clone(), publisher);
            }
        }
        bindings.push((mapping.id.clone(), reader));
    }
    // Chunk snapshots are read-only until their separate mutation/recovery
    // lifecycle is accepted. Valid capability absence retains legacy behavior;
    // transport/protocol failures never silently disable validation.
    let chunks = if config.writes.is_none() || config.version == 8 {
        let s3drive_authority::Reply::Features(features) = authority
            .request(&s3drive_authority::Request::Features {})
            .await
            .map_err(error)?
        else {
            return Err(Error::Configuration);
        };
        if features.iter().any(|f| f == "manifest-root-v1") {
            Some(
                s3drive_managed::chunks::Reader::open(
                    authority.clone(),
                    settings.volume.clone(),
                    bindings.clone(),
                )
                .await
                .map_err(error)?,
            )
        } else {
            None
        }
    } else {
        None
    };
    let reader =
        s3drive_managed::Reader::open(authority.clone(), settings.volume.clone(), bindings)
            .await
            .map_err(error)?;
    if config.version >= 5
        && reader.volume().await.map_err(error)?.name_policy != s3drive_core::names::PORTABLE
    {
        return Err(Error::Configuration);
    }
    let chunk_writes = if config.version == 8 {
        Some(Arc::new(
            // Keep the sparse constructor's recovery state out of the outer
            // filesystem future, including legacy/read-only startup paths.
            Box::pin(chunk_writes::Writes::new(
                storage.clone().ok_or(Error::Configuration)?,
                authority.clone(),
                settings.volume.clone(),
                chunks.clone().ok_or(Error::Configuration)?,
                chunk_executors,
                settings.default_store.clone().ok_or(Error::Configuration)?,
                native_lifecycle,
            ))
            .await?,
        ))
    } else {
        None
    };
    #[cfg(feature = "native-sparse-candidate")]
    if native_lifecycle
        && let Some(w) = &chunk_writes
        && let Err(failure) = w.start_native().await
    {
        // Mount admission has not exposed callbacks. Release the one storage
        // owner before reporting refusal so an immediate original-profile
        // retry cannot race a dropped worker's directory lock. This only closes
        // SQLite ownership; it never fabricates a file-object Close or ACK.
        w.storage.close().await.map_err(error)?;
        return Err(failure);
    }
    let writes = storage.filter(|_| config.version != 8).map(|s| {
        Arc::new(writes::Writes::new(
            s,
            settings.volume.clone(),
            authority,
            publishers,
            settings.default_store.clone().unwrap_or_default(),
            config.version,
            reader.clone(),
        ))
    });
    if let Some(w) = &writes {
        w.start_native_lifecycle().await?;
        w.recover_namespace().await?;
        w.recover_native_lifecycle().await?;
    }
    let root = Arc::new(Node {
        id: ROOT,
        parent: ROOT,
        name: String::new(),
        kind: Kind::Directory,
        source: None,
        key: s3drive_authority::ROOT.into(),
        snapshot: None,
    });
    Ok(ReadOnlyFs {
        readers: Vec::new(),
        nodes: Mutex::new(Nodes {
            by_id: HashMap::from([(ROOT, root)]),
            by_key: HashMap::from([(
                (usize::MAX, s3drive_authority::ROOT.into(), Kind::Directory),
                ROOT,
            )]),
            next_id: ROOT + 1,
        }),
        aliases: Vec::new(),
        cache: None,
        writes: None,
        managed: Some(Arc::new(Backend {
            reader,
            chunks,
            writes,
            chunk_writes,
        })),
    })
}
impl Nodes {
    pub(super) fn insert_managed(
        &mut self,
        n: &s3drive_authority::Node,
        parent: u64,
    ) -> Result<Arc<Node>> {
        self.insert_managed_version(n, parent, false)
    }
    pub(super) fn insert_managed_snapshot(
        &mut self,
        n: &s3drive_authority::Node,
        parent: u64,
    ) -> Result<Arc<Node>> {
        self.insert_managed_version(n, parent, true)
    }
    fn insert_managed_version(
        &mut self,
        n: &s3drive_authority::Node,
        parent: u64,
        snapshot: bool,
    ) -> Result<Arc<Node>> {
        validate_name(&n.name)?;
        let kind = if n.kind == s3drive_authority::Kind::Directory {
            Kind::Directory
        } else {
            Kind::File
        };
        let view = if snapshot && kind == Kind::File {
            Some(SnapshotView {
                reference: s3drive_authority::payload_reference(&n.payload),
                metadata: logical_metadata(n)?,
            })
        } else {
            None
        };
        let key = match &view {
            Some(view) => format!(
                "snapshot/{}/{}",
                n.id,
                view.reference.as_deref().unwrap_or("empty")
            ),
            None => n.id.clone(),
        };
        let identity = (usize::MAX, key, kind);
        let id = if let Some(id) = self.by_key.get(&identity) {
            *id
        } else {
            if self.by_id.len() >= MAX_NODES {
                return Err(Error::ResourceLimit);
            }
            let id = self.next_id;
            self.next_id = id.checked_add(1).ok_or(Error::ResourceLimit)?;
            self.by_key.insert(identity, id);
            id
        };
        let node = Arc::new(Node {
            id,
            parent,
            name: n.name.clone(),
            kind,
            source: None,
            key: n.id.clone(),
            snapshot: view,
        });
        self.by_id.insert(id, node.clone());
        Ok(node)
    }
}
pub(super) fn check_snapshot(node: &Node, live: &s3drive_authority::Node) -> Result<()> {
    let expected = node.snapshot.as_ref().ok_or(Error::Internal)?;
    if live.id != node.key
        || live.kind == s3drive_authority::Kind::Directory
        || expected.reference != s3drive_authority::payload_reference(&live.payload)
    {
        return Err(Error::Conflict);
    }
    Ok(())
}
pub(super) fn metadata(node: &s3drive_authority::Node) -> ObjectMetadata {
    ObjectMetadata {
        key: node.id.clone(),
        size: node.payload.as_ref().map_or(0, |p| p.reservation.size),
        revision: s3drive_s3::ObjectRevision {
            etag: node.payload.as_ref().map(|p| p.etag.clone()),
            version_id: node.payload.as_ref().and_then(|p| p.version.clone()),
        },
        modified_unix_seconds: None,
    }
}
pub(super) fn logical_metadata(node: &s3drive_authority::Node) -> Result<ObjectMetadata> {
    let mut result = metadata(node);
    if let Some(manifest) = node
        .payload
        .as_ref()
        .and_then(|p| p.reservation.manifest.as_ref())
    {
        result.size = manifest.root().map_err(|_| Error::Configuration)?.length();
    }
    Ok(result)
}
pub(super) fn error<E: Into<Box<dyn std::error::Error + Send + Sync>>>(e: E) -> Error {
    let e = e.into();
    if let Some(f) = e.downcast_ref::<s3drive_s3::uploads::Failure>() {
        use s3drive_s3::uploads::Failure as F;
        return match f {
            F::Busy => Error::Busy,
            F::PolicyExpired | F::PolicyScope | F::PolicyOperation => Error::ReadOnly,
            F::PolicySize => Error::TooLarge,
            _ => Error::Managed(e),
        };
    }
    if let Some(f) = e.downcast_ref::<s3drive_managed::chunks::MixedRootFailure>() {
        use s3drive_managed::chunks::MixedRootFailure as F;
        return match f {
            F::Busy => Error::Busy,
            F::BaseChanged | F::RetainedConflict => Error::Conflict,
            _ => Error::Managed(e),
        };
    }
    if let Some(f) = e.downcast_ref::<s3drive_managed::creation::Failure>() {
        return match f {
            s3drive_managed::creation::Failure::Authority(a) => {
                error(s3drive_managed::Failure::Authority(*a))
            }
            s3drive_managed::creation::Failure::Busy => Error::Busy,
            s3drive_managed::creation::Failure::Deadline => Error::Timeout,
            s3drive_managed::creation::Failure::RetainedConflict => Error::Conflict,
            _ => Error::Managed(e),
        };
    }
    if let Some(f) = e.downcast_ref::<s3drive_state::sparse_views::Failure>() {
        use s3drive_state::sparse_views::Failure as F;
        return match f {
            F::Quota => Error::NoSpace,
            F::Bounds => Error::TooLarge,
            F::NotFound => Error::NotFound,
            F::Conflict | F::EpochClosed => Error::Conflict,
            _ => Error::Managed(e),
        };
    }
    if let Some(f) = e.downcast_ref::<s3drive_managed::chunks::ReadFailure>() {
        use s3drive_managed::chunks::ReadFailure;
        return match f {
            ReadFailure::Bounds => Error::TooLarge,
            ReadFailure::Busy => Error::Busy,
            ReadFailure::NotFound => Error::NotFound,
            ReadFailure::IsDirectory => Error::IsDirectory,
            ReadFailure::Timeout => Error::Timeout,
            ReadFailure::Authority(e) => error(s3drive_managed::Failure::Authority(*e)),
            ReadFailure::UnsupportedContent => Error::Storage(s3drive_s3::Error {
                operation: "manifest-read",
                kind: s3drive_s3::ErrorKind::Unsupported,
                http_status: None,
            }),
            _ => Error::Managed(e),
        };
    }
    if let Some(f) = e.downcast_ref::<s3drive_managed::Failure>() {
        use s3drive_managed::Failure;
        return match f {
            Failure::Busy => Error::Busy,
            Failure::NotFound => Error::NotFound,
            Failure::IsDirectory => Error::IsDirectory,
            Failure::RetainedConflict | Failure::CancelledTransition => Error::Conflict,
            Failure::Authority(s3drive_authority::Error::NotFound) => Error::NotFound,
            Failure::Authority(s3drive_authority::Error::AlreadyExists) => Error::AlreadyExists,
            Failure::Authority(s3drive_authority::Error::Invalid) => Error::InvalidName,
            Failure::Authority(s3drive_authority::Error::Conflict) => Error::Conflict,
            Failure::Authority(s3drive_authority::Error::NotDirectory) => Error::NotDirectory,
            Failure::Authority(s3drive_authority::Error::IsDirectory) => Error::IsDirectory,
            Failure::Authority(s3drive_authority::Error::DeletePending) => Error::DeletePending,
            Failure::Authority(s3drive_authority::Error::NotEmpty) => Error::NotEmpty,
            Failure::Authority(
                s3drive_authority::Error::Cycle | s3drive_authority::Error::Root,
            ) => Error::InvalidOperation,
            Failure::Authority(s3drive_authority::Error::Limit) => Error::ResourceLimit,
            Failure::Authority(s3drive_authority::Error::Unavailable) => Error::Busy,
            _ => Error::Managed(e),
        };
    }
    match e.downcast::<s3drive_s3::Error>() {
        Ok(e) => Error::Storage(*e),
        Err(e) => Error::Managed(e),
    }
}
