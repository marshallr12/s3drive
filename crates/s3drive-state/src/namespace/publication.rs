//! Immutable payload reservations and publication CAS. The authenticated client
//! must verify S3 bytes; the metadata service does not possess S3 credentials.
use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Store {
    pub id: String,
    pub route: String,
    pub root: String,
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
    Store(Store),
    Reservation(Reservation),
    Published(Payload),
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
            && self.key
                == format!(
                    "{}v1/{}/{}/{}",
                    self.store.root, self.volume, self.node, self.id
                )
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
pub(super) fn valid(m: &Mutation) -> bool {
    match m {
        Mutation::RegisterStore { route, root: r } => digest(route) && root(r),
        Mutation::Reserve {
            node,
            expected_revision,
            store,
            size,
            sha256,
        } => {
            valid_id(node)
                && node != ROOT
                && valid_id(store)
                && store != ROOT
                && *expected_revision < i64::MAX as u64
                && *size <= 8 * 1024 * 1024
                && digest(sha256)
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
fn store(db: &Connection, id: &str) -> Result<Store> {
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
        Mutation::Reserve {
            node: id,
            expected_revision,
            store: store_id,
            size,
            sha256,
        } => {
            let n = live_node(db, id)?;
            if n.kind == Kind::Directory || n.revision != *expected_revision {
                return Err(Error::Conflict);
            }
            let count: i64 =
                sql(db.query_row("SELECT count(*) FROM namespace_uploads", [], |r| r.get(0)))?;
            if count >= 4096 {
                return Err(Error::Limit);
            }
            let store = store(db, store_id)?;
            let reservation = Reservation {
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
            };
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
            let n = live_node(db, &r.node)?;
            if n.kind == Kind::Directory || n.revision != r.expected_revision {
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
            let changed=sql(db.execute("UPDATE namespace_nodes SET payload=?1,revision=?2 WHERE node_id=?3 AND revision=?4 AND deleted=0",params![bytes,sequence as i64,p.reservation.node,p.reservation.expected_revision as i64]))?;
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
