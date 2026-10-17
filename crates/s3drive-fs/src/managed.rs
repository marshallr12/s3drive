use super::*;
use s3drive_authority::{Token, WireClient};
pub(super) struct Backend {
    pub reader: s3drive_managed::Reader,
}
pub(super) async fn filesystem(config: &Config) -> Result<ReadOnlyFs> {
    let settings = config.managed.as_ref().ok_or(Error::Configuration)?;
    let token = Token::from_private_file(&settings.token_file).map_err(|_| Error::Configuration)?;
    let authority =
        WireClient::loopback(&settings.authority, token).map_err(|_| Error::Configuration)?;
    let mut bindings = Vec::new();
    for mapping in &settings.stores {
        let source = config
            .sources
            .iter()
            .find(|s| s.alias == mapping.source)
            .ok_or(Error::Configuration)?;
        bindings.push((
            mapping.id.clone(),
            Box::pin(S3Reader::from_source(source)).await?,
        ));
    }
    let reader = s3drive_managed::Reader::open(authority, settings.volume.clone(), bindings)
        .await
        .map_err(error)?;
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
        managed: Some(Arc::new(Backend { reader })),
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
pub(super) fn error(e: Box<dyn std::error::Error + Send + Sync>) -> Error {
    if let Some(f) = e.downcast_ref::<s3drive_managed::Failure>() {
        use s3drive_managed::Failure;
        return match f {
            Failure::Busy => Error::Busy,
            Failure::NotFound => Error::NotFound,
            Failure::IsDirectory => Error::IsDirectory,
            Failure::Authority(s3drive_authority::Error::NotFound) => Error::NotFound,
            Failure::Authority(s3drive_authority::Error::NotDirectory) => Error::NotDirectory,
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
