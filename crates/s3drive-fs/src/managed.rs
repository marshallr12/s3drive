use super::*;
pub(super) mod writes;
use s3drive_authority::{Token, WireClient};
pub(super) struct Backend {
    pub reader: s3drive_managed::Reader,
    pub chunks: Option<s3drive_managed::chunks::Reader>,
    pub writes: Option<Arc<writes::Writes>>,
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
    let token = Token::from_private_file(&settings.token_file).map_err(|_| Error::Configuration)?;
    let authority = WireClient::from_config(settings, token).map_err(|_| Error::Configuration)?;
    if config.writes.is_some() {
        let reply = authority
            .request(&s3drive_authority::Request::Features {})
            .await
            .map_err(error)?;
        let s3drive_authority::Reply::Features(features) = reply else {
            return Err(Error::Configuration);
        };
        let mut required = vec!["content-cas-v1"];
        if config.version >= 5 {
            required.push("namespace-create-v1");
        }
        if config.version >= 6 {
            required.extend(["namespace-entry-cas-v1", "detached-content-cas-v1"]);
        }
        if config.version >= 7 {
            required.extend([
                "pending-removal-v1",
                "deferred-removal-v1",
                "deferred-create-v1",
                "deferred-owner-move-v1",
                "owned-target-replace-v1",
            ]);
        }
        if required
            .iter()
            .any(|needed| !features.iter().any(|f| f == needed))
        {
            return Err(Error::Configuration);
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
    let mut bindings = Vec::new();
    let mut publishers = HashMap::new();
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
        bindings.push((mapping.id.clone(), reader));
    }
    // Chunk snapshots are read-only until their separate mutation/recovery
    // lifecycle is accepted. Valid capability absence retains legacy behavior;
    // transport/protocol failures never silently disable validation.
    let chunks = if config.writes.is_none() {
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
    let writes = storage.map(|s| {
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
        })),
    })
}
impl Nodes {
    pub(super) fn insert_managed(
        &mut self,
        n: &s3drive_authority::Node,
        parent: u64,
    ) -> Result<Arc<Node>> {
        validate_name(&n.name)?;
        let kind = if n.kind == s3drive_authority::Kind::Directory {
            Kind::Directory
        } else {
            Kind::File
        };
        let identity = (usize::MAX, n.id.clone(), kind);
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
        });
        self.by_id.insert(id, node.clone());
        Ok(node)
    }
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
