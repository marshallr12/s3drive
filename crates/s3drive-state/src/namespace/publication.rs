//! Immutable payload reservations and publication CAS. The authenticated client
//! must verify S3 bytes; the metadata service does not possess S3 credentials.
use super::*;
pub(crate) mod deferred;
pub(crate) mod empty;
mod history;
mod manifest;
pub use manifest::{MAX_MANIFEST_LENGTH, ManifestRoot};
use sha2::{Digest, Sha256};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Store {
    pub id: String,
    pub route: String,
    pub root: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentGuard {
    pub expected: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub detached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removal: Option<RemovalFence>,
}
fn is_false(value: &bool) -> bool {
    !*value
}
pub fn payload_reference(payload: &Option<Payload>) -> Option<String> {
    payload.as_ref().map(|p| {
        let mut h = Sha256::new();
        h.update(if p.reservation.manifest.is_some() {
            b"s3drive-manifest-content-reference-v1\0".as_slice()
        } else {
            b"s3drive-content-reference-v1\0".as_slice()
        });
        h.update(serde_json::to_vec(p).expect("payload serialization"));
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reservation {
    pub id: String,
    pub volume: String,
    pub node: String,
    pub expected_revision: u64,
    pub store: Store,
    pub key: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<ContentGuard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<ManifestRoot>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub reservation: Reservation,
    pub upload_operation: String,
    pub etag: String,
    pub version: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum Artifact {
    Namespace {
        displaced: Option<Node>,
    },
    Removal(RemovalFence),
    RemovalMove {
        fence: RemovalFence,
        displaced: Option<Node>,
    },
    Store(Store),
    Reservation(Reservation),
    Published(Payload),
    PublishedRemoval {
        payload: Payload,
        fence: RemovalFence,
    },
}
pub fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn root(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.ends_with('/')
        && value[..value.len() - 1].split('/').all(|c| {
            !c.is_empty()
                && c != "."
                && c != ".."
                && c.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
}
impl Store {
    pub fn valid(&self) -> bool {
        valid_id(&self.id) && self.id != ROOT && digest(&self.route) && root(&self.root)
    }
}
impl Reservation {
    pub fn valid(&self) -> bool {
        self.store.valid()
            && valid_id(&self.id)
            && self.id != ROOT
            && valid_id(&self.volume)
            && valid_id(&self.node)
            && self.node != ROOT
            && self.expected_revision < i64::MAX as u64
            && self.size <= 8 * 1024 * 1024
            && digest(&self.sha256)
            && self.guard.as_ref().is_none_or(|g| {
                g.expected.as_ref().is_none_or(|s| digest(s))
                    && g.removal.as_ref().is_none_or(|f| !g.detached && f.valid())
            })
            && match &self.manifest {
                None => {
                    self.key
                        == format!(
                            "{}v1/{}/{}/{}",
                            self.store.root, self.volume, self.node, self.id
                        )
                }
                Some(m) => self.guard.is_some() && m.key(self).is_ok_and(|key| self.key == key),
            }
    }
}
impl Payload {
    pub fn valid(&self) -> bool {
        self.reservation.valid()
            && valid_proof(&self.upload_operation, &self.etag, self.version.as_deref())
    }
}
fn valid_proof(operation: &str, etag: &str, version: Option<&str>) -> bool {
    digest(operation)
        && etag.len() <= 1024
        && version.is_none_or(|v| v != "null" && v.len() <= 1024)
        && crate::remote::Revision::new(etag, version).is_ok()
}
pub(super) fn uses_content_guard(db: &Connection, m: &Mutation) -> Result<bool> {
    match m {
        Mutation::ReserveManifest { .. }
        | Mutation::ReserveContent { .. }
        | Mutation::ReserveDetachedContent { .. }
        | Mutation::ReservePendingContent { .. } => Ok(true),
        Mutation::Publish {
            reservation: id, ..
        } => match reservation(db, id) {
            Ok(r) => Ok(r.guard.is_some()),
            Err(Error::NotFound) => Ok(false),
            Err(e) => Err(e),
        },
        _ => Ok(false),
    }
}
fn base_matches(n: &Node, revision: u64, guard: Option<&ContentGuard>) -> bool {
    n.kind != Kind::Directory
        && n.deleted == guard.is_some_and(|g| g.detached)
        && match guard {
            Some(g) => payload_reference(&n.payload) == g.expected,
            None => n.revision == revision,
        }
}
pub(super) fn valid(m: &Mutation) -> bool {
    if let Mutation::ReserveContent {
        expected_payload, ..
    }
    | Mutation::ReserveDetachedContent {
        expected_payload, ..
    }
    | Mutation::ReservePendingContent {
        expected_payload, ..
    } = m
        && expected_payload.as_ref().is_some_and(|s| !digest(s))
    {
        return false;
    }
    if let Mutation::ReservePendingContent { fence, .. } = m
        && !fence.valid()
    {
        return false;
    }
    match m {
        Mutation::RegisterStore { route, root: r } => digest(route) && root(r),
        Mutation::Reserve {
            node,
            expected_revision,
            store,
            size,
            sha256,
        }
        | Mutation::ReservePendingContent {
            node,
            expected_revision,
            store,
            size,
            sha256,
            ..
        }
        | Mutation::ReserveDetachedContent {
            node,
            expected_revision,
            store,
            size,
            sha256,
            ..
        }
        | Mutation::ReserveContent {
            node,
            expected_revision,
            store,
            size,
            sha256,
            ..
        } => {
            valid_id(node)
                && node != ROOT
                && valid_id(store)
                && store != ROOT
                && *expected_revision < i64::MAX as u64
                && *size <= 8 * 1024 * 1024
                && digest(sha256)
        }
        Mutation::ReserveManifest {
            node,
            expected_revision,
            store,
            size,
            sha256,
            manifest,
            guard,
        } => {
            valid_id(node)
                && node != ROOT
                && valid_id(store)
                && store != ROOT
                && *expected_revision < i64::MAX as u64
                && (64..=s3drive_core::manifest::MAX_PAGE_BYTES as u64).contains(size)
                && digest(sha256)
                && manifest.root().is_ok()
                && guard.expected.as_ref().is_none_or(|s| digest(s))
                && guard
                    .removal
                    .as_ref()
                    .is_none_or(|f| !guard.detached && f.valid())
        }
        Mutation::Publish {
            reservation,
            upload_operation,
            etag,
            version,
        } => {
            valid_id(reservation)
                && reservation != ROOT
                && valid_proof(upload_operation, etag, version.as_deref())
        }
        _ => false,
    }
}
pub(super) fn check_schema(db: &Connection) -> crate::Result<()> {
    db.prepare("SELECT store_id,descriptor FROM namespace_stores LIMIT 0")?;
    db.prepare("SELECT reservation_id,descriptor,publication FROM namespace_uploads LIMIT 0")?;
    Ok(())
}
fn decode<T: for<'de> Deserialize<'de>>(bytes: Vec<u8>) -> Result<T> {
    serde_json::from_slice(&bytes).map_err(|_| Error::Internal)
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|_| Error::Internal)
}
pub(super) fn store(db: &Connection, id: &str) -> Result<Store> {
    if !valid_id(id) {
        return Err(Error::Invalid);
    }
    let bytes: Vec<u8> = sql(db
        .query_row(
            "SELECT descriptor FROM namespace_stores WHERE store_id=?",
            [id],
            |r| r.get(0),
        )
        .optional())?
    .ok_or(Error::NotFound)?;
    let s: Store = decode(bytes)?;
    if !s.valid() || s.id != id {
        return Err(Error::Internal);
    }
    Ok(s)
}
fn reservation(db: &Connection, id: &str) -> Result<Reservation> {
    if !valid_id(id) {
        return Err(Error::Invalid);
    }
    let bytes: Vec<u8> = sql(db
        .query_row(
            "SELECT descriptor FROM namespace_uploads WHERE reservation_id=?",
            [id],
            |r| r.get(0),
        )
        .optional())?
    .ok_or(Error::NotFound)?;
    let r: Reservation = decode(bytes)?;
    if !r.valid() || r.id != id || store(db, &r.store.id)? != r.store {
        return Err(Error::Internal);
    }
    Ok(r)
}
impl State {
    pub fn namespace_store(&self, volume_id: &str, id: &str) -> Result<Store> {
        check_volume(&self.connection, volume_id)?;
        store(&self.connection, id)
    }
    pub fn namespace_reservation(&self, volume_id: &str, id: &str) -> Result<Reservation> {
        check_volume(&self.connection, volume_id)?;
        reservation(&self.connection, id)
    }
}
pub(super) fn mutate(
    db: &Connection,
    request: &Apply,
    sequence: u64,
) -> Result<(String, Artifact)> {
    match &request.mutation {
        Mutation::CreateEmptyManifest { .. } => empty::mutate(db, request, sequence),
        Mutation::CreateDeferredEmptyManifest { .. } => deferred::mutate(db, request, sequence),
        Mutation::RegisterStore { route, root } => {
            let count: i64 =
                sql(db.query_row("SELECT count(*) FROM namespace_stores", [], |r| r.get(0)))?;
            if count >= 16 {
                return Err(Error::Limit);
            }
            let store = Store {
                id: request.operation.clone(),
                route: route.clone(),
                root: root.clone(),
            };
            sql(db.execute(
                "INSERT INTO namespace_stores VALUES(?1,?2)",
                params![store.id, encode(&store)?],
            ))?;
            Ok((ROOT.into(), Artifact::Store(store)))
        }
        Mutation::ReserveManifest {
            node: id,
            expected_revision,
            store: store_id,
            size,
            sha256,
            ..
        }
        | Mutation::Reserve {
            node: id,
            expected_revision,
            store: store_id,
            size,
            sha256,
        }
        | Mutation::ReservePendingContent {
            node: id,
            expected_revision,
            store: store_id,
            size,
            sha256,
            ..
        }
        | Mutation::ReserveDetachedContent {
            node: id,
            expected_revision,
            store: store_id,
            size,
            sha256,
            ..
        }
        | Mutation::ReserveContent {
            node: id,
            expected_revision,
            store: store_id,
            size,
            sha256,
            ..
        } => {
            let guard = match &request.mutation {
                Mutation::ReserveManifest { guard, .. } => Some(guard.clone()),
                Mutation::ReserveContent {
                    expected_payload, ..
                }
                | Mutation::ReserveDetachedContent {
                    expected_payload, ..
                }
                | Mutation::ReservePendingContent {
                    expected_payload, ..
                } => Some(ContentGuard {
                    expected: expected_payload.clone(),
                    detached: matches!(request.mutation, Mutation::ReserveDetachedContent { .. }),
                    removal: match &request.mutation {
                        Mutation::ReservePendingContent { fence, .. } => Some(fence.clone()),
                        _ => None,
                    },
                }),
                _ => None,
            };
            let n = node(db, id)?;
            if n.deleted && !guard.as_ref().is_some_and(|g| g.detached) {
                return Err(Error::NotFound);
            }
            removals::publication_gate(db, id, guard.as_ref())?;
            if n.placement.as_ref().is_some_and(|p| &p.id != store_id) {
                return Err(Error::Conflict);
            }
            if !base_matches(&n, *expected_revision, guard.as_ref()) {
                return Err(Error::Conflict);
            }
            let count: i64 =
                sql(db.query_row("SELECT count(*) FROM namespace_uploads", [], |r| r.get(0)))?;
            if count >= 4096 {
                return Err(Error::Limit);
            }
            let store = store(db, store_id)?;
            let mut reservation = Reservation {
                id: request.operation.clone(),
                volume: request.volume.clone(),
                node: id.clone(),
                expected_revision: *expected_revision,
                key: format!(
                    "{}v1/{}/{}/{}",
                    store.root, request.volume, id, request.operation
                ),
                store,
                size: *size,
                sha256: sha256.clone(),
                guard,
                manifest: match &request.mutation {
                    Mutation::ReserveManifest { manifest, .. } => Some((**manifest).clone()),
                    _ => None,
                },
            };
            if let Some(m) = &reservation.manifest {
                reservation.key = m.key(&reservation)?;
            }
            if !reservation.valid() {
                return Err(Error::Invalid);
            }
            sql(db.execute(
                "INSERT INTO namespace_uploads VALUES(?1,?2,NULL)",
                params![reservation.id, encode(&reservation)?],
            ))?;
            Ok((id.clone(), Artifact::Reservation(reservation)))
        }
        Mutation::Publish {
            reservation: id,
            upload_operation,
            etag,
            version,
        } => {
            let r = reservation(db, id)?;
            let n = node(db, &r.node)?;
            if n.deleted && !r.guard.as_ref().is_some_and(|g| g.detached) {
                return Err(Error::NotFound);
            }
            removals::publication_gate(db, &r.node, r.guard.as_ref())?;
            if !base_matches(&n, r.expected_revision, r.guard.as_ref()) {
                return Err(Error::Conflict);
            }
            let prior: Option<Vec<u8>> = sql(db.query_row(
                "SELECT publication FROM namespace_uploads WHERE reservation_id=?",
                [id],
                |r| r.get(0),
            ))?;
            if prior.is_some() {
                return Err(Error::AlreadyExists);
            }
            let p = Payload {
                reservation: r,
                upload_operation: upload_operation.clone(),
                etag: etag.clone(),
                version: version.clone(),
            };
            let bytes = encode(&p)?;
            let changed=sql(db.execute("UPDATE namespace_nodes SET payload=?1,revision=?2 WHERE node_id=?3 AND revision=?4 AND deleted=?5",params![bytes,sequence as i64,p.reservation.node,n.revision as i64,n.deleted]))?;
            if changed != 1 {
                return Err(Error::Conflict);
            }
            let changed=sql(db.execute("UPDATE namespace_uploads SET publication=?1 WHERE reservation_id=?2 AND publication IS NULL",params![bytes,id]))?;
            if changed != 1 {
                return Err(Error::Conflict);
            }
            Ok((p.reservation.node.clone(), Artifact::Published(p)))
        }
        _ => Err(Error::Invalid),
    }
}

#[cfg(test)]
mod legacy_vectors;

/// Check the exact new-format effect again after the durable operation-result
/// insert, while still in the same transaction. A successful statement count
/// alone does not exclude an ignored/deleted/rewritten trigger effect.
pub(super) fn verify_manifest_effect(
    db: &Connection,
    request: &Apply,
    receipt: &Receipt,
) -> Result<()> {
    let (r, published) = match &receipt.artifact {
        Some(Artifact::Reservation(r)) if r.manifest.is_some() => (r, None),
        Some(Artifact::Published(p) | Artifact::PublishedRemoval { payload: p, .. })
            if p.reservation.manifest.is_some() =>
        {
            (&p.reservation, Some(p))
        }
        _ => return Ok(()),
    };
    validate_manifest_replay(db, request, receipt)?;
    if volume(db)?.sequence != receipt.sequence {
        return Err(Error::Internal);
    }
    let saved = reservation(db, &r.id).map_err(|_| Error::Internal)?;
    let n = node(db, &r.node).map_err(|_| Error::Internal)?;
    if &saved != r || n != receipt.node {
        return Err(Error::Internal);
    }
    let raw: Option<Vec<u8>> = sql(db.query_row(
        "SELECT publication FROM namespace_uploads WHERE reservation_id=?1",
        [&r.id],
        |row| row.get(0),
    ))?;
    match published {
        None if raw.is_none() && base_matches(&n, r.expected_revision, r.guard.as_ref()) => Ok(()),
        Some(p)
            if n.payload.as_ref() == Some(p) && raw.as_deref() == Some(encode(p)?.as_slice()) =>
        {
            Ok(())
        }
        _ => Err(Error::Internal),
    }
}

/// Historical receipts are checked structurally, without comparing them with a
/// newer live node. Legacy receipts keep their original decoding behavior.
pub(super) fn validate_manifest_receipt(receipt: &Receipt, volume_id: &str) -> Result<()> {
    let n = &receipt.node;
    if let Some(p) = &n.payload
        && p.reservation.manifest.is_some()
        && (!p.valid()
            || p.reservation.volume != volume_id
            || p.reservation.node != n.id
            || n.kind != Kind::File)
    {
        return Err(Error::Internal);
    }
    if let Some(Artifact::PublishedRemoval { payload, fence }) = &receipt.artifact {
        if !fence.valid() || fence.intent != n.id || payload.reservation.manifest.is_none() {
            return Err(Error::Internal);
        }
        let mut ordinary = receipt.clone();
        ordinary.artifact = Some(Artifact::Published(payload.clone()));
        validate_manifest_receipt(&ordinary, volume_id)?;
    }
    match &receipt.artifact {
        Some(Artifact::Reservation(r)) if r.manifest.is_some() => {
            if !r.valid()
                || r.volume != volume_id
                || r.node != n.id
                || n.revision > receipt.sequence
                || !base_matches(n, r.expected_revision, r.guard.as_ref())
            {
                return Err(Error::Internal);
            }
        }
        Some(Artifact::Published(p))
            if p.reservation.manifest.is_some()
                && (!p.valid()
                    || p.reservation.volume != volume_id
                    || p.reservation.node != n.id
                    || n.payload.as_ref() != Some(p)
                    || n.revision != receipt.sequence
                    || n.kind != Kind::File
                    || n.deleted != p.reservation.guard.as_ref().is_some_and(|g| g.detached)) =>
        {
            return Err(Error::Internal);
        }
        _ => (),
    }
    Ok(())
}

pub(super) fn validate_manifest_replay(
    db: &Connection,
    request: &Apply,
    receipt: &Receipt,
) -> Result<()> {
    validate_manifest_receipt(receipt, &request.volume)?;
    if matches!(receipt.artifact, Some(Artifact::PublishedRemoval { .. }))
        && !matches!(
            request.mutation,
            Mutation::CreateDeferredEmptyManifest { .. }
        )
    {
        return Err(Error::Internal);
    }

    if matches!(
        request.mutation,
        Mutation::CreateDeferredEmptyManifest { .. }
    ) {
        return deferred::replay(db, request, receipt);
    }
    if matches!(request.mutation, Mutation::CreateEmptyManifest { .. }) {
        return empty::replay(db, request, receipt);
    }
    if let Mutation::ReserveManifest {
        node,
        expected_revision,
        store,
        size,
        sha256,
        manifest,
        guard,
    } = &request.mutation
    {
        let Some(Artifact::Reservation(r)) = &receipt.artifact else {
            return Err(Error::Internal);
        };
        if receipt.sequence <= request.expected_sequence
            || r.id != request.operation
            || r.volume != request.volume
            || &r.node != node
            || r.expected_revision != *expected_revision
            || &r.store.id != store
            || r.size != *size
            || &r.sha256 != sha256
            || r.manifest.as_ref() != Some(manifest.as_ref())
            || r.guard.as_ref() != Some(guard)
        {
            return Err(Error::Internal);
        }
    }
    if let Mutation::Publish {
        reservation: id,
        upload_operation,
        etag,
        version,
    } = &request.mutation
    {
        // Reservation descriptors are immutable and retained after publication;
        // this compares no historical node with its newer live replacement.
        let expected = reservation(db, id).map_err(|_| Error::Internal)?;
        if expected.manifest.is_some()
            || matches!(&receipt.artifact,Some(Artifact::Published(p)) if p.reservation.manifest.is_some())
        {
            let Some(Artifact::Published(p)) = &receipt.artifact else {
                return Err(Error::Internal);
            };
            if p.reservation != expected
                || &p.upload_operation != upload_operation
                || &p.etag != etag
                || &p.version != version
                || receipt.sequence <= request.expected_sequence
                || receipt.node.deleted != expected.guard.as_ref().is_some_and(|g| g.detached)
            {
                return Err(Error::Internal);
            }
        }
    }

    Ok(())
}
