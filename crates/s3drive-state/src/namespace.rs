//! Authoritative managed namespace prototype, separate from local source binding.
//! No S3 publication or platform name-comparison semantics are implemented here.
mod entries;
mod removals;
pub use removals::{PendingRemoval, RemovalFence};
pub mod publication;
use crate::State;
pub use publication::{Artifact, ContentGuard, Payload, Reservation, Store, payload_reference};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use s3drive_core::names;
use serde::{Deserialize, Serialize};

pub const ROOT: &str = "00000000000000000000000000000000";
pub const MAX_NODES: i64 = 4096;
pub const MAX_OPERATIONS: i64 = 8192;
pub const MAX_CHILDREN: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Error {
    Invalid,
    WrongVolume,
    Conflict,
    NotFound,
    AlreadyExists,
    NotDirectory,
    IsDirectory,
    NotEmpty,
    DeletePending,
    Cycle,
    Root,
    OperationMismatch,
    Limit,
    Unavailable,
    Internal,
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Directory,
    EmptyFile,
    File,
}
impl Kind {
    fn sql(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::EmptyFile | Self::File => "empty-file",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Volume {
    pub id: String,
    pub sequence: u64,
    pub name_policy: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub parent: Option<String>,
    pub name: String,
    pub kind: Kind,
    pub revision: u64,
    pub deleted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Payload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Store>,
}
/// Exact authority identity and revision, independent of path spelling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeGuard {
    pub id: String,
    pub revision: u64,
}
impl NodeGuard {
    fn valid(&self, sequence: u64) -> bool {
        valid_id(&self.id) && self.revision <= sequence
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Mutation {
    RegisterStore {
        route: String,
        root: String,
    },
    Reserve {
        node: String,
        expected_revision: u64,
        store: String,
        size: u64,
        sha256: String,
    },
    ReserveContent {
        node: String,
        expected_revision: u64,
        store: String,
        size: u64,
        sha256: String,
        expected_payload: Option<String>,
    },
    ReserveDetachedContent {
        node: String,
        expected_revision: u64,
        store: String,
        size: u64,
        sha256: String,
        expected_payload: Option<String>,
    },
    Publish {
        reservation: String,
        upload_operation: String,
        etag: String,
        version: Option<String>,
    },
    ArmRemoval {
        node: NodeGuard,
        parent: NodeGuard,
        owner: String,
    },
    CancelRemoval {
        node: String,
        fence: RemovalFence,
    },
    CommitRemoval {
        node: NodeGuard,
        parent: NodeGuard,
        fence: RemovalFence,
    },
    ReservePendingContent {
        node: String,
        expected_revision: u64,
        store: String,
        size: u64,
        sha256: String,
        expected_payload: Option<String>,
        fence: RemovalFence,
    },
    CreateEntry {
        parent: String,
        parent_revision: u64,
        store: Option<String>,
        name: String,
        node_kind: Kind,
    },
    Create {
        parent: String,
        name: String,
        node_kind: Kind,
    },
    MoveEntry {
        node: NodeGuard,
        source_parent: NodeGuard,
        parent: NodeGuard,
        name: String,
        destination: Option<NodeGuard>,
    },
    RemoveEntry {
        node: NodeGuard,
        parent: NodeGuard,
    },
    Move {
        node: String,
        parent: String,
        name: String,
    },
    Remove {
        node: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Apply {
    pub volume: String,
    pub operation: String,
    pub expected_sequence: u64,
    pub mutation: Mutation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub sequence: u64,
    pub node: Node,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listing {
    pub sequence: u64,
    pub children: Vec<Node>,
}

fn sql<T>(r: rusqlite::Result<T>) -> Result<T> {
    r.map_err(|_| Error::Internal)
}
fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control())
}
impl Apply {
    pub fn validate(&self) -> Result<()> {
        if !valid_id(&self.volume)
            || !valid_id(&self.operation)
            || self.operation == ROOT
            || self.expected_sequence >= i64::MAX as u64
        {
            return Err(Error::Invalid);
        }
        if let Mutation::Reserve {
            expected_revision, ..
        }
        | Mutation::ReserveContent {
            expected_revision, ..
        }
        | Mutation::ReserveDetachedContent {
            expected_revision, ..
        }
        | Mutation::ReservePendingContent {
            expected_revision, ..
        } = &self.mutation
            && *expected_revision > self.expected_sequence
        {
            return Err(Error::Invalid);
        }
        let valid = match &self.mutation {
            Mutation::RegisterStore { .. }
            | Mutation::Reserve { .. }
            | Mutation::ReserveContent { .. }
            | Mutation::ReserveDetachedContent { .. }
            | Mutation::ReservePendingContent { .. }
            | Mutation::Publish { .. } => publication::valid(&self.mutation),
            Mutation::ArmRemoval {
                node,
                parent,
                owner,
            } => {
                node.valid(self.expected_sequence)
                    && parent.valid(self.expected_sequence)
                    && valid_id(owner)
                    && owner != ROOT
            }
            Mutation::CancelRemoval { node, fence } => {
                valid_id(node) && node != ROOT && fence.valid()
            }
            Mutation::CommitRemoval {
                node,
                parent,
                fence,
            } => {
                node.valid(self.expected_sequence)
                    && parent.valid(self.expected_sequence)
                    && fence.valid()
            }
            Mutation::CreateEntry {
                parent,
                parent_revision,
                name,
                node_kind,
                store,
            } => {
                (*node_kind == Kind::Directory && store.is_none()
                    || *node_kind == Kind::EmptyFile
                        && store.as_ref().is_some_and(|s| valid_id(s) && s != ROOT))
                    && *parent_revision <= self.expected_sequence
                    && *node_kind != Kind::File
                    && valid_id(parent)
                    && names::collision_key(name).is_some()
            }
            Mutation::Create {
                parent,
                name,
                node_kind,
            } => *node_kind != Kind::File && valid_id(parent) && valid_name(name),
            Mutation::MoveEntry {
                node,
                source_parent,
                parent,
                name,
                destination,
            } => {
                node.valid(self.expected_sequence)
                    && source_parent.valid(self.expected_sequence)
                    && parent.valid(self.expected_sequence)
                    && names::collision_key(name).is_some()
                    && destination
                        .as_ref()
                        .is_none_or(|d| d.valid(self.expected_sequence))
            }
            Mutation::RemoveEntry { node, parent } => {
                node.valid(self.expected_sequence) && parent.valid(self.expected_sequence)
            }
            Mutation::Move { node, parent, name } => {
                valid_id(node) && valid_id(parent) && valid_name(name)
            }
            Mutation::Remove { node } => valid_id(node),
        };
        if valid { Ok(()) } else { Err(Error::Invalid) }
    }
}

pub(crate) fn check_schema(db: &Connection) -> crate::Result<()> {
    db.prepare("SELECT volume_id, sequence, name_policy FROM namespace_volume LIMIT 0")?;
    db.prepare("SELECT name_key FROM namespace_nodes LIMIT 0")?;
    db.prepare(
        "SELECT node_id, parent_id, name, kind, revision, deleted, payload, placement FROM namespace_nodes LIMIT 0",
    )?;
    db.prepare("SELECT operation_id, request, outcome FROM namespace_operations LIMIT 0")?;
    publication::check_schema(db)?;
    removals::check_schema(db)?;
    Ok(())
}
fn volume(db: &Connection) -> Result<Volume> {
    sql(db.query_row(
        "SELECT volume_id, sequence, name_policy FROM namespace_volume WHERE singleton=1",
        [],
        |r| {
            Ok(Volume {
                id: r.get(0)?,
                sequence: nonnegative(r, 1)?,
                name_policy: r.get(2)?,
            })
        },
    ))
}
fn check_volume(db: &Connection, id: &str) -> Result<Volume> {
    if !valid_id(id) {
        return Err(Error::Invalid);
    }
    let v = volume(db)?;
    if id != v.id {
        return Err(Error::WrongVolume);
    }
    Ok(v)
}
fn nonnegative(r: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    u64::try_from(r.get::<_, i64>(index)?).map_err(|_| rusqlite::Error::InvalidQuery)
}
fn row_node(r: &rusqlite::Row<'_>) -> rusqlite::Result<Node> {
    let kind: String = r.get(3)?;
    let payload: Option<Vec<u8>> = r.get(6)?;
    let payload: Option<Payload> = payload
        .map(|bytes| serde_json::from_slice(&bytes).map_err(|_| rusqlite::Error::InvalidQuery))
        .transpose()?;
    if payload.as_ref().is_some_and(|p| {
        !p.valid() || r.get::<_, String>(0).ok().as_deref() != Some(&p.reservation.node)
    }) {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let placement: Option<Vec<u8>> = r.get(7)?;
    let placement: Option<Store> = placement
        .map(|b| serde_json::from_slice(&b).map_err(|_| rusqlite::Error::InvalidQuery))
        .transpose()?;
    if placement.as_ref().is_some_and(|p| {
        !p.valid()
            || payload
                .as_ref()
                .is_some_and(|body| body.reservation.store != *p)
    }) || (kind == "directory" && (placement.is_some() || payload.is_some()))
    {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let kind = match kind.as_str() {
        "directory" => Kind::Directory,
        "empty-file" if payload.is_some() => Kind::File,
        "empty-file" => Kind::EmptyFile,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(Node {
        id: r.get(0)?,
        parent: r.get(1)?,
        name: r.get(2)?,
        kind,
        revision: nonnegative(r, 4)?,
        deleted: r.get(5)?,
        payload,
        placement,
    })
}
fn node(db: &Connection, id: &str) -> Result<Node> {
    if !valid_id(id) {
        return Err(Error::Invalid);
    }
    let value=sql(db.query_row("SELECT node_id, parent_id, name, kind, revision, deleted, payload, placement FROM namespace_nodes WHERE node_id=?", [id], row_node).optional())?.ok_or(Error::NotFound)?;
    check_name_key(db, &value)?;
    Ok(value)
}
fn check_name_key(db: &Connection, node: &Node) -> Result<()> {
    if let Some(placement) = &node.placement
        && publication::store(db, &placement.id)? != *placement
    {
        return Err(Error::Internal);
    }
    let expected = if node.parent.is_some() && volume(db)?.name_policy == names::PORTABLE {
        Some(names::collision_key(&node.name).ok_or(Error::Internal)?)
    } else {
        None
    };
    let actual: Option<String> = sql(db.query_row(
        "SELECT name_key FROM namespace_nodes WHERE node_id=?",
        [&node.id],
        |r| r.get(0),
    ))?;
    if actual != expected {
        return Err(Error::Internal);
    }
    Ok(())
}
fn live_node(db: &Connection, id: &str) -> Result<Node> {
    let n = node(db, id)?;
    if n.deleted {
        Err(Error::NotFound)
    } else {
        removals::require_clear(db, id)?;
        Ok(n)
    }
}
fn directory(db: &Connection, id: &str) -> Result<Node> {
    let n = live_node(db, id)?;
    if n.kind != Kind::Directory {
        return Err(Error::NotDirectory);
    }
    Ok(n)
}
fn named(db: &Connection, parent: &str, name: &str) -> Result<Option<String>> {
    let policy = volume(db)?.name_policy;
    let key = if policy == names::PORTABLE {
        Some(names::collision_key(name).ok_or(Error::Invalid)?)
    } else {
        None
    };
    let existing: Option<String> = if let Some(key) = key {
        // Validate retained sibling keys before admitting a new entry. A
        // damaged index value must not let an alias bypass the policy.
        let mut statement=sql(db.prepare("SELECT node_id,name,name_key FROM namespace_nodes WHERE parent_id=? AND deleted=0 LIMIT 1001"))?;
        let rows = sql(sql(statement.query_map([parent], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        }))?
        .collect::<rusqlite::Result<Vec<_>>>())?;
        if rows.len() > MAX_CHILDREN {
            return Err(Error::Limit);
        }
        let mut found = None;
        for (id, name, actual) in rows {
            let expected = names::collision_key(&name).ok_or(Error::Internal)?;
            if actual.as_ref() != Some(&expected) {
                return Err(Error::Internal);
            }
            if expected == key {
                found = Some(id);
            }
        }
        found
    } else {
        sql(db
            .query_row(
                "SELECT node_id FROM namespace_nodes WHERE parent_id=?1 AND name=?2 AND deleted=0",
                params![parent, name],
                |r| r.get(0),
            )
            .optional())?
    };
    Ok(existing)
}
fn available(db: &Connection, parent: &str, name: &str, own_id: Option<&str>) -> Result<()> {
    let existing = named(db, parent, name)?;
    if existing.is_some() && existing.as_deref() != own_id {
        Err(Error::AlreadyExists)
    } else {
        Ok(())
    }
}
fn stored(db: &Connection, operation: &str) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
    sql(db
        .query_row(
            "SELECT request, outcome FROM namespace_operations WHERE operation_id=?",
            [operation],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional())
}

impl State {
    /// Initialize one authority volume. IDs are independent of AWS profiles.
    /// Keep this DB separate from all clients' local cache/staging databases.
    pub fn namespace_initialize(&mut self) -> Result<Volume> {
        self.namespace_initialize_with_policy(names::EXACT)
    }
    pub fn namespace_initialize_with_policy(&mut self, policy: &str) -> Result<Volume> {
        if !names::supported(policy) {
            return Err(Error::Invalid);
        }
        self._directory
            .as_ref()
            .ok_or(Error::Internal)?
            .verify_staging_storage()
            .map_err(|_| Error::Internal)?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        let count: i64 =
            sql(tx.query_row("SELECT count(*) FROM namespace_volume", [], |r| r.get(0)))?;
        if count == 0 {
            let local_rows: i64 = sql(tx.query_row(
                "SELECT (SELECT count(*) FROM source_bindings) +
                        (SELECT count(*) FROM legacy_cached_objects_v1) +
                        (SELECT count(*) FROM staged_generations) +
                        (SELECT count(*) FROM clean_blobs) +
                        (SELECT count(*) FROM managed_publications) +
                        (SELECT count(*) FROM logical_files) +
                        (SELECT count(*) FROM logical_edits) +
                        (SELECT count(*) FROM logical_publication_links) +
                        (SELECT count(*) FROM namespace_intents)",
                [],
                |r| r.get(0),
            ))?;
            if local_rows != 0 {
                return Err(Error::Invalid);
            }
            let mut bytes = [0u8; 16];
            getrandom::fill(&mut bytes).map_err(|_| Error::Internal)?;
            let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            if id == ROOT {
                return Err(Error::Internal);
            }
            sql(tx.execute(
                "INSERT INTO namespace_volume VALUES(1, ?1, 0, ?2)",
                params![id, policy],
            ))?;
            sql(tx.execute(
                "INSERT INTO namespace_nodes(node_id,parent_id,name,kind,revision,deleted) VALUES(?1, NULL, '', 'directory', 0, 0)",
                [ROOT],
            ))?;
        }
        let v = volume(&tx)?;
        let root = directory(&tx, ROOT)?;
        if root.parent.is_some() || !root.name.is_empty() || v.name_policy != policy {
            return Err(Error::Internal);
        }
        sql(tx.commit())?;
        Ok(v)
    }
    pub fn namespace_volume(&self) -> Result<Volume> {
        volume(&self.connection)
    }
    /// Return tombstones too; callers must not treat them as live paths.
    pub fn namespace_node(&self, volume_id: &str, id: &str) -> Result<Node> {
        check_volume(&self.connection, volume_id)?;
        let n = node(&self.connection, id)?;
        removals::require_clear(&self.connection, id)?;
        Ok(n)
    }
    /// Bounded, consistent listing; never silently truncate a directory.
    pub fn namespace_list(&mut self, volume_id: &str, parent: &str) -> Result<Listing> {
        let tx = sql(self.connection.transaction())?;
        let v = check_volume(&tx, volume_id)?;
        directory(&tx, parent)?;
        let children = {
            let mut stmt = sql(tx.prepare("SELECT node_id,parent_id,name,kind,revision,deleted,payload,placement FROM namespace_nodes WHERE parent_id=? AND deleted=0 ORDER BY name LIMIT 1001"))?;
            sql(sql(stmt.query_map([parent], row_node))?.collect::<rusqlite::Result<Vec<_>>>())?
        };
        for child in &children {
            check_name_key(&tx, child)?;
        }
        if children.len() > MAX_CHILDREN {
            return Err(Error::Limit);
        }
        sql(tx.commit())?;
        Ok(Listing {
            sequence: v.sequence,
            children,
        })
    }
    /// A committed result is stable even after later mutations. Unknown ID does
    /// not prove rollback while a request is still in flight: resubmit SAME bytes.
    pub fn namespace_operation(
        &self,
        volume_id: &str,
        operation: &str,
    ) -> Result<Option<Result<Receipt>>> {
        check_volume(&self.connection, volume_id)?;
        if !valid_id(operation) || operation == ROOT {
            return Err(Error::Invalid);
        }
        stored(&self.connection, operation)?
            .map(|(_, outcome)| serde_json::from_slice(&outcome).map_err(|_| Error::Internal))
            .transpose()
    }
    /// Sequence guard, namespace effect and dedup result commit together.
    /// Errors such as disk full/commit failure have UNKNOWN outcome to callers.
    pub fn namespace_apply(&mut self, request: &Apply) -> Result<Receipt> {
        request.validate()?;
        let encoded = serde_json::to_vec(request).map_err(|_| Error::Invalid)?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        let v = check_volume(&tx, &request.volume)?;
        if let Some((original, outcome)) = stored(&tx, &request.operation)? {
            if original != encoded {
                return Err(Error::OperationMismatch);
            }
            return serde_json::from_slice(&outcome).map_err(|_| Error::Internal)?;
        }
        let count: i64 = sql(
            tx.query_row("SELECT count(*) FROM namespace_operations", [], |r| {
                r.get(0)
            }),
        )?;
        if count >= MAX_OPERATIONS {
            return Err(Error::Limit);
        }
        // A savepoint rolls back any partial mutation before retaining rejection.
        sql(tx.execute_batch("SAVEPOINT namespace_effect"))?;
        let content_guard = publication::uses_content_guard(&tx, &request.mutation)?
            || matches!(
                request.mutation,
                Mutation::CreateEntry { .. }
                    | Mutation::MoveEntry { .. }
                    | Mutation::RemoveEntry { .. }
                    | Mutation::ArmRemoval { .. }
                    | Mutation::CancelRemoval { .. }
                    | Mutation::CommitRemoval { .. }
            );
        let outcome = if request.expected_sequence > v.sequence
            || (!content_guard && request.expected_sequence != v.sequence)
        {
            Err(Error::Conflict)
        } else {
            mutate(&tx, request, v.sequence + 1)
        };
        if outcome.is_err() {
            sql(tx.execute_batch("ROLLBACK TO namespace_effect"))?;
        }
        sql(tx.execute_batch("RELEASE namespace_effect"))?;
        if matches!(outcome, Err(Error::Internal | Error::DeletePending)) {
            return outcome;
        }
        // Leave one exact-result slot for releasing each outstanding node
        // reservation. Ordinary work (including failed requests) must not use
        // the last capacity needed by a matching cancel/commit. The outer
        // transaction rolls back a successful effect if admission is refused.
        let pending: i64 =
            sql(tx.query_row("SELECT count(*) FROM namespace_removals", [], |r| r.get(0)))?;
        if count + 1 + pending > MAX_OPERATIONS {
            return Err(Error::Limit);
        }
        let output = serde_json::to_vec(&outcome).map_err(|_| Error::Internal)?;
        let inserted = sql(tx.execute(
            "INSERT INTO namespace_operations VALUES (?1, ?2, ?3)",
            params![request.operation, encoded, output],
        ))?;
        if inserted != 1 || stored(&tx, &request.operation)? != Some((encoded, output)) {
            return Err(Error::Internal);
        }
        sql(tx.commit())?;
        outcome
    }
}
fn mutate(db: &Connection, request: &Apply, sequence: u64) -> Result<Receipt> {
    let sql_sequence = i64::try_from(sequence).map_err(|_| Error::Invalid)?;
    if matches!(
        &request.mutation,
        Mutation::RegisterStore { .. }
            | Mutation::Reserve { .. }
            | Mutation::ReserveContent { .. }
            | Mutation::ReserveDetachedContent { .. }
            | Mutation::ReservePendingContent { .. }
            | Mutation::Publish { .. }
    ) {
        let (changed, artifact) = publication::mutate(db, request, sequence)?;
        return finish(db, request, sequence, &changed, Some(artifact));
    }
    if matches!(
        &request.mutation,
        Mutation::ArmRemoval { .. }
            | Mutation::CancelRemoval { .. }
            | Mutation::CommitRemoval { .. }
    ) {
        let (changed, fence) = removals::mutate(db, request, sql_sequence)?;
        return finish(
            db,
            request,
            sequence,
            &changed,
            Some(Artifact::Removal(fence)),
        );
    }
    if matches!(
        &request.mutation,
        Mutation::MoveEntry { .. } | Mutation::RemoveEntry { .. }
    ) {
        let (changed, displaced) = entries::mutate(db, request, sql_sequence)?;
        return finish(
            db,
            request,
            sequence,
            &changed,
            Some(Artifact::Namespace { displaced }),
        );
    }
    let changed = match &request.mutation {
        Mutation::RegisterStore { .. }
        | Mutation::Reserve { .. }
        | Mutation::ReserveContent { .. }
        | Mutation::ReserveDetachedContent { .. }
        | Mutation::Publish { .. }
        | Mutation::MoveEntry { .. }
        | Mutation::RemoveEntry { .. }
        | Mutation::ReservePendingContent { .. }
        | Mutation::ArmRemoval { .. }
        | Mutation::CancelRemoval { .. }
        | Mutation::CommitRemoval { .. } => {
            return Err(Error::Invalid);
        }
        Mutation::CreateEntry {
            parent,
            parent_revision,
            name,
            node_kind,
            store,
        } => {
            if volume(db)?.name_policy != names::PORTABLE {
                return Err(Error::Invalid);
            }
            if directory(db, parent)?.revision != *parent_revision {
                return Err(Error::Conflict);
            }
            let placement = store
                .as_ref()
                .map(|id| publication::store(db, id))
                .transpose()?;
            create(
                db,
                request,
                parent,
                name,
                *node_kind,
                placement,
                sql_sequence,
            )?;
            request.operation.as_str()
        }
        Mutation::Create {
            parent,
            name,
            node_kind,
        } => {
            if *node_kind == Kind::EmptyFile && volume(db)?.name_policy == names::PORTABLE {
                return Err(Error::Invalid);
            }
            create(db, request, parent, name, *node_kind, None, sql_sequence)?;
            request.operation.as_str()
        }
        Mutation::Move {
            node: id,
            parent,
            name,
        } => {
            if id == ROOT {
                return Err(Error::Root);
            }
            let n = live_node(db, id)?;
            directory(db, parent)?;
            available(db, parent, name, Some(id))?;
            if n.kind == Kind::Directory {
                let mut cursor = Some(parent.clone());
                let mut depth = 0;
                while let Some(id2) = cursor {
                    if &id2 == id {
                        return Err(Error::Cycle);
                    }
                    depth += 1;
                    if depth > MAX_NODES {
                        return Err(Error::Internal);
                    }
                    cursor = directory(db, &id2)?.parent;
                }
            }
            let key = if volume(db)?.name_policy == names::PORTABLE {
                Some(names::collision_key(name).ok_or(Error::Invalid)?)
            } else {
                None
            };
            sql(db.execute("UPDATE namespace_nodes SET parent_id=?1,name=?2,revision=?3,name_key=?5 WHERE node_id=?4 AND deleted=0", params![parent,name,sql_sequence,id,key]))?;
            id.as_str()
        }
        Mutation::Remove { node: id } => {
            if id == ROOT {
                return Err(Error::Root);
            }
            live_node(db, id)?;
            let count: i64 = sql(db.query_row(
                "SELECT count(*) FROM namespace_nodes WHERE parent_id=? AND deleted=0",
                [id],
                |r| r.get(0),
            ))?;
            if count != 0 {
                return Err(Error::NotEmpty);
            }
            sql(db.execute(
                "UPDATE namespace_nodes SET deleted=1,revision=?1 WHERE node_id=?2",
                params![sql_sequence, id],
            ))?;
            id.as_str()
        }
    };
    finish(db, request, sequence, changed, None)
}
fn create(
    db: &Connection,
    request: &Apply,
    parent: &str,
    name: &str,
    kind: Kind,
    placement: Option<Store>,
    sequence: i64,
) -> Result<()> {
    directory(db, parent)?;
    available(db, parent, name, None)?;
    let count: i64 = sql(db.query_row("SELECT count(*) FROM namespace_nodes", [], |r| r.get(0)))?;
    let children: i64 = sql(db.query_row(
        "SELECT count(*) FROM namespace_nodes WHERE parent_id=? AND deleted=0",
        [parent],
        |r| r.get(0),
    ))?;
    if count >= MAX_NODES || children >= MAX_CHILDREN as i64 {
        return Err(Error::Limit);
    }
    let key = if volume(db)?.name_policy == names::PORTABLE {
        Some(names::collision_key(name).ok_or(Error::Invalid)?)
    } else {
        None
    };
    sql(db.execute("INSERT INTO namespace_nodes(node_id,parent_id,name,kind,revision,deleted,name_key,placement) VALUES(?1,?2,?3,?4,?5,0,?6,?7)",params![request.operation,parent,name,kind.sql(),sequence,key,placement.as_ref().map(serde_json::to_vec).transpose().map_err(|_|Error::Internal)?]))?;
    Ok(())
}

fn finish(
    db: &Connection,
    _request: &Apply,
    sequence: u64,
    changed: &str,
    artifact: Option<Artifact>,
) -> Result<Receipt> {
    let sql_sequence = i64::try_from(sequence).map_err(|_| Error::Invalid)?;
    let expected = sql_sequence.checked_sub(1).ok_or(Error::Invalid)?;
    let rows = sql(db.execute(
        "UPDATE namespace_volume SET sequence=?1 WHERE singleton=1 AND sequence=?2",
        params![sql_sequence, expected],
    ))?;
    if rows != 1 {
        return Err(Error::Internal);
    }
    Ok(Receipt {
        sequence,
        node: node(db, changed)?,
        artifact,
    })
}

#[cfg(test)]
mod tests;
