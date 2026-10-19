use super::*;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub(super) plan: Id,
    pub(super) operation: Id,
    pub(super) view: Id,
    pub(super) binding: Binding,
    pub(super) generation: [u8; 16],
    pub(super) original_root: Vec<u8>,
    pub(super) root: Vec<u8>,
    pub(super) base: ns::Node,
    pub(super) store: ns::Store,
    pub(super) reserve: ns::Apply,
    pub(super) publish_operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) predecessor: Option<Id>,
}
impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MixedRootInput([private])")
    }
}
impl Input {
    pub fn predecessor(&self) -> Option<Id> {
        self.predecessor
    }
    pub fn plan(&self) -> Id {
        self.plan
    }
    pub fn view(&self) -> Id {
        self.view
    }
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    pub fn root(&self) -> Result<RootDescriptor> {
        Ok(RootDescriptor::decode(&self.root)?)
    }
    pub fn original_root(&self) -> Result<RootDescriptor> {
        Ok(RootDescriptor::decode(&self.original_root)?)
    }
    pub fn base(&self) -> &ns::Node {
        &self.base
    }
    pub fn store(&self) -> &ns::Store {
        &self.store
    }
    pub fn reserve(&self) -> &ns::Apply {
        &self.reserve
    }
    pub fn publish_operation(&self) -> &str {
        &self.publish_operation
    }
    pub fn reservation(&self) -> Result<ns::Reservation> {
        self.binding.validate()?;
        let base = self.binding.base()?;
        let root = self.root()?;
        let original = self.original_root()?;
        self.reserve.validate().map_err(|_| Failure::Corrupt)?;
        let ns::Mutation::ReserveManifest {
            node,
            expected_revision,
            store,
            size,
            sha256,
            manifest,
            guard,
        } = &self.reserve.mutation
        else {
            return Err(Failure::Corrupt.into());
        };
        let expected = ns::Reservation {
            id: self.reserve.operation.clone(),
            volume: self.reserve.volume.clone(),
            node: node.clone(),
            expected_revision: *expected_revision,
            store: self.store.clone(),
            key: s3drive_core::manifest::objects::Namespace::new(
                self.binding.root().into(),
                *root.volume(),
                *root.node(),
                *self.binding.store(),
            )?
            .root_key(&root)?
            .as_str()
            .into(),
            size: *size,
            sha256: sha256.clone(),
            manifest: Some((**manifest).clone()),
            guard: Some(guard.clone()),
        };
        let old = self.base.payload.as_ref().ok_or(Failure::Corrupt)?;
        if self.plan == [0; 32]
            || self.operation == [0; 32]
            || self.view == [0; 32]
            || self.generation == [0; 16]
            || !expected.valid()
            || !self.store.valid()
            || self.store.root != self.binding.root()
            || self.store.id != hex(self.binding.store())
            || root.volume() != original.volume()
            || root.node() != original.node()
            || root.page().store() != self.binding.store()
            || root.page().generation() != &self.generation
            || hex(original.volume()) != base.volume()
            || hex(original.node()) != base.node()
            || original.length() != self.binding.base_size()
            || original.page().store() != self.binding.store()
            || self.reserve.operation != hex(&self.generation)
            || self.reserve.volume != base.volume()
            || node != base.node()
            || store != &self.store.id
            || manifest.root().map_err(|_| Failure::Corrupt)? != root
            || *expected_revision != self.base.revision
            || self.base.revision > self.reserve.expected_sequence
            || guard.detached
            || guard.removal.is_some()
            || guard.expected != ns::payload_reference(&self.base.payload)
            || self.base.id != base.node()
            || self.base.kind != ns::Kind::File
            || self.base.deleted
            || self.base.parent.as_deref().is_none_or(|p| !id(p))
            || !name(&self.base.name)
            || self
                .base
                .placement
                .as_ref()
                .is_some_and(|s| s != &self.store)
            || !old.valid()
            || old.reservation.volume != base.volume()
            || old.reservation.node != base.node()
            || old.reservation.store != self.store
            || match self.predecessor {
                None => {
                    old.reservation.id != base.reservation()
                        || old
                            .reservation
                            .manifest
                            .as_ref()
                            .ok_or(Failure::Corrupt)?
                            .root()
                            .map_err(|_| Failure::Corrupt)?
                            != original
                        || ns::payload_reference(&self.base.payload).as_deref()
                            != Some(base.payload_reference())
                }
                Some(p) => p == [0; 32] || p == self.plan,
            }
            || self.publish_operation == ns::ROOT
            || !id(&self.publish_operation)
            || self.publish_operation == self.reserve.operation
        {
            return Err(Failure::Corrupt.into());
        }
        Ok(expected)
    }
    pub(super) fn matches(&self, info: &Info) -> Result<bool> {
        Ok(self.plan == info.id()?
            && self.operation == info.operation()
            && self.view == info.view().id()
            && self.binding == *info.view().binding()
            && self.generation == *info.generation()
            && self.original_root == info.original_root()?.encode()
            && Some(self.root()?) == info.root()?)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Physical {
    pub(super) generation: GenerationId,
    pub(super) operation: Id,
    pub(super) checksum: Id,
    pub(super) size: u64,
    pub(super) sha256: Id,
    pub(super) etag: String,
    pub(super) version: Option<String>,
}
impl Physical {
    pub fn generation(&self) -> GenerationId {
        self.generation
    }
    pub(super) fn from_tree(tree: &super::super::TreeReceipt) -> Result<Self> {
        let r = tree.root_receipt();
        let i = r.info();
        let revision = i.revision.as_ref().ok_or(Failure::Corrupt)?;
        Ok(Self {
            generation: i.generation,
            operation: *i.operation.as_bytes(),
            checksum: *r.checksum(),
            size: i.size,
            sha256: i.payload_sha256,
            etag: revision.etag().into(),
            version: revision.version().map(Into::into),
        })
    }
    fn mutation(&self, r: &ns::Reservation) -> ns::Mutation {
        ns::Mutation::Publish {
            reservation: r.id.clone(),
            upload_operation: hex(&self.operation),
            etag: self.etag.clone(),
            version: self.version.clone(),
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rejection {
    pub(super) request: ns::Apply,
    pub(super) error: ns::Error,
}
impl Rejection {
    pub fn request(&self) -> &ns::Apply {
        &self.request
    }
    pub fn error(&self) -> ns::Error {
        self.error
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub(super) format: u8,
    pub(super) input: Input,
    pub(super) physical: Physical,
    pub(super) semantics: Id,
    pub(super) reservation: Option<ns::Receipt>,
    pub(super) publication: Option<ns::Apply>,
    pub(super) receipt: Option<ns::Receipt>,
    pub(super) rejection: Option<Rejection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) acknowledgment: Option<Acknowledgment>,
}
impl fmt::Debug for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MixedRootRecord([private; not file flush])")
    }
}
impl Record {
    pub fn acknowledgment(&self) -> Option<&Acknowledgment> {
        self.acknowledgment.as_ref()
    }
    pub fn input(&self) -> &Input {
        &self.input
    }
    pub fn physical(&self) -> &Physical {
        &self.physical
    }
    pub fn reservation(&self) -> Option<&ns::Receipt> {
        self.reservation.as_ref()
    }
    pub fn publication(&self) -> Option<&ns::Apply> {
        self.publication.as_ref()
    }
    pub fn receipt(&self) -> Option<&ns::Receipt> {
        self.receipt.as_ref()
    }
    pub fn rejection(&self) -> Option<&Rejection> {
        self.rejection.as_ref()
    }
    pub(super) fn prepare(&mut self, sequence: u64) -> Result<()> {
        if let Some(p) = &self.publication {
            if p.expected_sequence == sequence {
                return Ok(());
            }
            return Err(Failure::Conflict.into());
        }
        if self.reservation.is_none() || self.rejection.is_some() {
            return Err(Failure::Conflict.into());
        }
        self.publication = Some(ns::Apply {
            volume: self.input.reserve.volume.clone(),
            operation: self.input.publish_operation.clone(),
            expected_sequence: sequence,
            mutation: self.physical.mutation(&self.input.reservation()?),
        });
        self.validate()
    }
    pub(super) fn validate(&self) -> Result<()> {
        let expected = self.input.reservation()?;
        if self.format
            != if self.acknowledgment.is_some() {
                3
            } else if self.input.predecessor.is_some() {
                2
            } else {
                1
            }
            || self.physical.operation == [0; 32]
            || self.physical.size != expected.size
            || hex(&self.physical.sha256) != expected.sha256
            || crate::remote::Revision::new(&self.physical.etag, self.physical.version.as_deref())
                .is_err()
        {
            return Err(Failure::Corrupt.into());
        }
        if let Some(r) = &self.reservation
            && (r.sequence <= self.input.reserve.expected_sequence
                || !receipt_node(&r.node, &expected, r.sequence)
                || r.artifact != Some(ns::Artifact::Reservation(expected.clone()))
                || ns::payload_reference(&r.node.payload)
                    != expected.guard.as_ref().ok_or(Failure::Corrupt)?.expected)
        {
            return Err(Failure::Corrupt.into());
        }
        if let Some(p) = &self.publication {
            p.validate().map_err(|_| Failure::Corrupt)?;
            if self
                .reservation
                .as_ref()
                .is_none_or(|r| p.expected_sequence < r.sequence)
                || p.volume != expected.volume
                || p.operation != self.input.publish_operation
                || p.mutation != self.physical.mutation(&expected)
            {
                return Err(Failure::Corrupt.into());
            }
        }
        if let Some(r) = &self.receipt {
            let p = self.publication.as_ref().ok_or(Failure::Corrupt)?;
            let payload = ns::Payload {
                reservation: expected.clone(),
                upload_operation: hex(&self.physical.operation),
                etag: self.physical.etag.clone(),
                version: self.physical.version.clone(),
            };
            if !payload.valid()
                || r.sequence <= p.expected_sequence
                || r.node.revision != r.sequence
                || !receipt_node(&r.node, &expected, r.sequence)
                || r.node.kind != ns::Kind::File
                || r.node.payload.as_ref() != Some(&payload)
                || r.artifact != Some(ns::Artifact::Published(payload))
            {
                return Err(Failure::Corrupt.into());
            }
        }
        if let Some(r) = &self.rejection
            && (r.error != ns::Error::Conflict
                || self.receipt.is_some()
                || match &self.publication {
                    Some(p) => r.request != *p,
                    None => self.reservation.is_some() || r.request != self.input.reserve,
                })
        {
            return Err(Failure::Corrupt.into());
        }
        settlement::validate_record(self)?;
        Ok(())
    }
}
fn receipt_node(n: &ns::Node, r: &ns::Reservation, seq: u64) -> bool {
    n.id == r.node
        && !n.deleted
        && n.kind != ns::Kind::Directory
        && n.revision <= seq
        && seq < i64::MAX as u64
        && n.parent.as_deref().is_some_and(id)
        && name(&n.name)
        && n.placement.as_ref().is_none_or(|s| s == &r.store)
        && (n.kind == ns::Kind::File) == n.payload.is_some()
        && n.payload.as_ref().is_none_or(|p| {
            p.valid()
                && p.reservation.volume == r.volume
                && p.reservation.node == r.node
                && p.reservation.store == r.store
        })
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Check {
    pub plan: Id,
    pub slot: u64,
    pub candidate: Id,
    pub root: Id,
    pub witness: Option<Id>,
    pub original: Option<Id>,
}
