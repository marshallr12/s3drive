//! Read-only managed access does not require upload policy or write credentials.
use super::*;
use s3drive_authority::{Listing, Volume};
use s3drive_s3::S3Reader;
use std::collections::{HashMap, HashSet};
struct SharedReader {
    authority: WireClient,
    volume: String,
    stores: HashMap<String, (Store, S3Reader)>,
    slots: Arc<Semaphore>,
}
#[derive(Clone)]
pub struct Reader(Arc<SharedReader>);
impl Reader {
    pub(super) fn verified(
        authority: WireClient,
        volume: String,
        stores: Vec<(Store, S3Reader)>,
    ) -> Self {
        Self(Arc::new(SharedReader {
            authority,
            volume,
            stores: stores
                .into_iter()
                .map(|(s, r)| (s.id.clone(), (s, r)))
                .collect(),
            slots: Arc::new(Semaphore::new(4)),
        }))
    }
    pub async fn open(
        authority: WireClient,
        volume: String,
        bindings: Vec<(String, S3Reader)>,
    ) -> Result<Self> {
        if bindings.is_empty() || bindings.len() > 16 {
            return Err(Failure::Binding.into());
        }
        let Reply::Info(info) = reply(authority.request(&Request::Info {}).await?)? else {
            return Err(Failure::Protocol.into());
        };
        if info.id != volume || !s3drive_core::names::supported(&info.name_policy) {
            return Err(Failure::Binding.into());
        }
        let mut stores = Vec::new();
        let mut ids = HashSet::new();
        for (id, reader) in bindings {
            if !ids.insert(id.clone()) {
                return Err(Failure::Binding.into());
            }
            let Reply::Store(store) = reply(
                authority
                    .request(&Request::Store {
                        volume: volume.clone(),
                        store: id.clone(),
                    })
                    .await?,
            )?
            else {
                return Err(Failure::Protocol.into());
            };
            if store.id != id || !store.valid() || store.route != reader.storage_fingerprint() {
                return Err(Failure::Binding.into());
            }
            stores.push((store, reader));
        }
        Ok(Self::verified(authority, volume, stores))
    }
    pub async fn volume(&self) -> Result<Volume> {
        let Reply::Info(v) = reply(self.0.authority.request(&Request::Info {}).await?)? else {
            return Err(Failure::Protocol.into());
        };
        if v.id != self.0.volume || !s3drive_core::names::supported(&v.name_policy) {
            return Err(Failure::Binding.into());
        }
        Ok(v)
    }
    fn validate(&self, node: &Node) -> Result<()> {
        let id = |s: &str| {
            s.len() == 32
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if !id(&node.id) || node.parent.as_ref().is_some_and(|p| !id(p)) {
            return Err(Failure::Protocol.into());
        }
        if node.id == s3drive_authority::ROOT {
            if node.parent.is_some()
                || !node.name.is_empty()
                || node.kind != Kind::Directory
                || node.payload.is_some()
            {
                return Err(Failure::Protocol.into());
            }
        } else if node.parent.is_none()
            || node.name.is_empty()
            || node.name.len() > 255
            || [".", ".."].contains(&node.name.as_str())
            || node
                .name
                .chars()
                .any(|c| c.is_control() || c == '/' || c == '\\')
        {
            return Err(Failure::Protocol.into());
        }
        if let Some(placement) = &node.placement {
            let (mapped, _) = self.0.stores.get(&placement.id).ok_or(Failure::Binding)?;
            if placement != mapped
                || node.kind == Kind::Directory
                || node
                    .payload
                    .as_ref()
                    .is_some_and(|p| p.reservation.store != *placement)
            {
                return Err(Failure::Binding.into());
            }
        }
        if let Some(payload) = &node.payload {
            let r = &payload.reservation;
            let (store, _) = self.0.stores.get(&r.store.id).ok_or(Failure::Binding)?;
            if node.kind != Kind::File
                || !payload.valid()
                || r.volume != self.0.volume
                || r.node != node.id
                || &r.store != store
            {
                return Err(Failure::Binding.into());
            }
        } else if node.kind == Kind::File {
            return Err(Failure::Integrity.into());
        }
        Ok(())
    }
    pub async fn node(&self, id: &str) -> Result<Node> {
        let Reply::Node(node) = reply(
            self.0
                .authority
                .request(&Request::Node {
                    volume: self.0.volume.clone(),
                    node: id.into(),
                })
                .await?,
        )?
        else {
            return Err(Failure::Protocol.into());
        };
        if node.id != id {
            return Err(Failure::Binding.into());
        }
        self.validate(&node)?;
        if node.deleted {
            return Err(Failure::NotFound.into());
        }
        Ok(node)
    }
    pub async fn list(&self, parent: &str) -> Result<Listing> {
        let Reply::List(list) = reply(
            self.0
                .authority
                .request(&Request::List {
                    volume: self.0.volume.clone(),
                    parent: parent.into(),
                })
                .await?,
        )?
        else {
            return Err(Failure::Protocol.into());
        };
        if list.children.len() > 1000 {
            return Err(Failure::Protocol.into());
        }
        let mut names = HashSet::new();
        let mut ids = HashSet::new();
        for child in &list.children {
            self.validate(child)?;
            if child.deleted
                || child.parent.as_deref() != Some(parent)
                || child.id == parent
                || !names.insert(&child.name)
                || !ids.insert(&child.id)
            {
                return Err(Failure::Protocol.into());
            }
        }
        Ok(list)
    }
    /// Published payloads authorize with online S3 HEAD and a complete pinned
    /// GET/hash. Four retained snapshots bound payload buffers to 32 MiB.
    pub async fn read(&self, node_id: &str) -> Result<FileSnapshot> {
        let permit = self
            .0
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Failure::Busy)?;
        let node = self.node(node_id).await?;
        if node.kind == Kind::Directory {
            return Err(Failure::IsDirectory.into());
        }
        let mut authorization = None;
        let bytes = if let Some(payload) = &node.payload {
            let r = &payload.reservation;
            let (_, reader) = self.0.stores.get(&r.store.id).ok_or(Failure::Binding)?;
            let handle = Box::pin(reader.open(&r.key)).await?;
            authorization = Some((
                *handle.cache_identity().as_bytes(),
                handle.configuration_fingerprint().into(),
            ));
            let m = handle.metadata();
            if m.size != r.size
                || m.revision.etag.as_deref() != Some(&payload.etag)
                || payload
                    .version
                    .as_ref()
                    .is_some_and(|v| m.revision.version_id.as_ref() != Some(v))
            {
                return Err(Failure::Integrity.into());
            }
            let bytes = Box::pin(handle.read_range(0, r.size as usize)).await?;
            if bytes.len() as u64 != r.size || hex(&Sha256::digest(&bytes)) != r.sha256 {
                return Err(Failure::Integrity.into());
            }
            Arc::from(bytes)
        } else {
            Arc::from([])
        };
        Ok(FileSnapshot(Arc::new(Snapshot {
            metadata: node,
            volume: self.0.volume.clone(),
            authorization,
            bytes,
            _permit: permit,
        })))
    }
}
