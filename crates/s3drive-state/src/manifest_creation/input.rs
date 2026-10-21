use super::*;
use crate::{
    journal::Target,
    namespace::{ContentGuard, ManifestRoot, NodeGuard, Payload, Reservation, Store},
};
use s3drive_core::manifest::objects::{Namespace, RootDescriptor};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub volume: String,
    pub operation: String,
    pub expected_sequence: u64,
    pub parent: NodeGuard,
    pub name: String,
    pub store: Store,
    pub source: [u8; 32],
    pub fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<CreationOpen>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreationOpen {
    pub id: String,
    pub session: String,
    pub delete_on_close: bool,
    pub posix: bool,
}
impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EmptyCreationInput([private])")
    }
}
fn id(s: &str) -> Result<[u8; 16]> {
    if s.len() != 32
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Failure::Invalid.into());
    }
    let mut out = [0; 16];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|_| Failure::Invalid)?;
    }
    Ok(out)
}
impl Input {
    pub(super) fn format(&self) -> u8 {
        if self.open.is_some() {
            3
        } else if self.owner.is_some() {
            2
        } else {
            1
        }
    }
    pub(super) fn validate(&self) -> Result<()> {
        id(&self.parent.id)?;
        if let Some(open) = &self.open {
            id(&open.id)?;
            id(&open.session)?;
            if self.owner.is_none() || open.id == ns::ROOT || open.session == ns::ROOT {
                return Err(Failure::Invalid.into());
            }
        }
        if let Some(owner) = &self.owner {
            id(owner)?;
            if owner == ns::ROOT {
                return Err(Failure::Invalid.into());
            }
        }

        if self.operation == ns::ROOT
            || self.volume == ns::ROOT
            || self.parent.id == self.operation
            || self.expected_sequence >= i64::MAX as u64
            || self.parent.revision > self.expected_sequence
            || s3drive_core::names::collision_key(&self.name).is_none()
            || !self.store.valid()
            || !ns::publication::digest(&self.fingerprint)
        {
            return Err(Failure::Invalid.into());
        }
        self.target()?;
        if serde_json::to_vec(self)?.len() > 8192 {
            return Err(Failure::Invalid.into());
        }
        Ok(())
    }
    fn root(&self) -> Result<(RootDescriptor, Vec<u8>)> {
        Ok(RootDescriptor::empty(
            id(&self.volume)?,
            id(&self.operation)?,
            id(&self.store.id)?,
            id(&self.operation)?,
        )?)
    }
    /// Canonical offline bytes only; this does not authorize a provider request.
    pub fn body(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(self.root()?.1)
    }
    pub fn object_key(&self) -> Result<String> {
        let root = self.root()?.0;
        Ok(Namespace::new(
            self.store.root.clone(),
            *root.volume(),
            *root.node(),
            *root.page().store(),
        )?
        .root_key(&root)?
        .as_str()
        .into())
    }
    pub fn target(&self) -> Result<Target> {
        Target::new(self.source, &self.fingerprint, &self.object_key()?)
    }
    pub fn stage_key(&self) -> Result<[u8; 32]> {
        self.validate()?;
        if self.open.is_some() {
            hash(b"s3drive-open-creation-stage-v3\0", self)
        } else if self.owner.is_some() {
            hash(b"s3drive-deferred-creation-stage-v2\0", self)
        } else {
            hash(b"s3drive-empty-creation-stage-v1\0", self)
        }
    }
    pub(super) fn request(
        &self,
        operation: String,
        etag: String,
        version: Option<String>,
    ) -> Result<ns::Apply> {
        let mut r = ns::Apply {
            volume: self.volume.clone(),
            operation: self.operation.clone(),
            expected_sequence: self.expected_sequence,
            mutation: ns::Mutation::CreateEmptyManifest {
                parent: self.parent.id.clone(),
                parent_revision: self.parent.revision,
                name: self.name.clone(),
                store: self.store.id.clone(),
                upload_operation: operation,
                etag,
                version,
            },
        };
        if let Some(owner) = &self.owner {
            let ns::Mutation::CreateEmptyManifest {
                parent,
                parent_revision,
                name,
                store,
                upload_operation,
                etag,
                version,
            } = r.mutation
            else {
                return Err(Failure::Invalid.into());
            };
            r.mutation = ns::Mutation::CreateDeferredEmptyManifest {
                parent,
                parent_revision,
                name,
                store,
                upload_operation,
                etag,
                version,
                owner: owner.clone(),
            };
        }
        r.validate().map_err(|_| Failure::Invalid)?;
        Ok(r)
    }
    pub(super) fn payload(&self, request: &ns::Apply, sequence: u64) -> Result<Payload> {
        let (ns::Mutation::CreateEmptyManifest {
            upload_operation,
            etag,
            version,
            ..
        }
        | ns::Mutation::CreateDeferredEmptyManifest {
            upload_operation,
            etag,
            version,
            ..
        }) = &request.mutation
        else {
            return Err(Failure::Corrupt.into());
        };
        if *request != self.request(upload_operation.clone(), etag.clone(), version.clone())? {
            return Err(Failure::Corrupt.into());
        }
        let (root, body) = self.root()?;
        let p = Payload {
            reservation: Reservation {
                id: self.operation.clone(),
                volume: self.volume.clone(),
                node: self.operation.clone(),
                expected_revision: sequence,
                store: self.store.clone(),
                key: self.object_key()?,
                size: body.len() as u64,
                sha256: hex(&Sha256::digest(&body)),
                guard: Some(ContentGuard {
                    expected: None,
                    detached: false,
                    removal: None,
                }),
                manifest: Some(ManifestRoot::new(root).map_err(|_| Failure::Invalid)?),
            },
            upload_operation: upload_operation.clone(),
            etag: etag.clone(),
            version: version.clone(),
        };
        if !p.valid() {
            return Err(Failure::Corrupt.into());
        }
        Ok(p)
    }
    pub(crate) fn check_receipt(&self, request: &ns::Apply, r: &ns::Receipt) -> Result<()> {
        let payload = self.payload(request, r.sequence)?;
        let expected = ns::Node {
            id: self.operation.clone(),
            parent: Some(self.parent.id.clone()),
            name: self.name.clone(),
            kind: ns::Kind::File,
            revision: r.sequence,
            deleted: false,
            payload: Some(payload.clone()),
            placement: Some(self.store.clone()),
        };
        let artifact = if let Some(owner) = &self.owner {
            ns::Artifact::PublishedRemoval {
                payload,
                fence: ns::RemovalFence {
                    intent: self.operation.clone(),
                    owner: owner.clone(),
                },
            }
        } else {
            ns::Artifact::Published(payload)
        };
        if r.sequence <= self.expected_sequence
            || r.node != expected
            || r.artifact != Some(artifact)
        {
            return Err(Failure::Corrupt.into());
        }
        Ok(())
    }
}
