//! Authoritative managed namespace prototype, separate from local source binding.
//! No S3 publication or platform name-comparison semantics are implemented here.
use crate::State;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
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
    NotEmpty,
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
}
impl Kind {
    fn sql(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::EmptyFile => "empty-file",
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
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Mutation {
    Create {
        parent: String,
        name: String,
        node_kind: Kind,
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
        let valid = match &self.mutation {
            Mutation::Create { parent, name, .. } => valid_id(parent) && valid_name(name),
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
    db.prepare(
        "SELECT node_id, parent_id, name, kind, revision, deleted FROM namespace_nodes LIMIT 0",
    )?;
    db.prepare("SELECT operation_id, request, outcome FROM namespace_operations LIMIT 0")?;
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
    let kind = match kind.as_str() {
        "directory" => Kind::Directory,
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
    })
}
fn node(db: &Connection, id: &str) -> Result<Node> {
    if !valid_id(id) {
        return Err(Error::Invalid);
    }
    sql(db.query_row("SELECT node_id, parent_id, name, kind, revision, deleted FROM namespace_nodes WHERE node_id=?", [id], row_node).optional())?
        .ok_or(Error::NotFound)
}
fn live_node(db: &Connection, id: &str) -> Result<Node> {
    let n = node(db, id)?;
    if n.deleted {
        Err(Error::NotFound)
    } else {
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
fn available(db: &Connection, parent: &str, name: &str, own_id: Option<&str>) -> Result<()> {
    let existing: Option<String> = sql(db
        .query_row(
            "SELECT node_id FROM namespace_nodes WHERE parent_id=?1 AND name=?2 AND deleted=0",
            params![parent, name],
            |r| r.get(0),
        )
        .optional())?;
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
                        (SELECT count(*) FROM clean_blobs)",
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
                "INSERT INTO namespace_volume VALUES(1, ?1, 0, 'exact-utf8-v1')",
                [&id],
            ))?;
            sql(tx.execute(
                "INSERT INTO namespace_nodes VALUES(?1, NULL, '', 'directory', 0, 0)",
                [ROOT],
            ))?;
        }
        let v = volume(&tx)?;
        let root = directory(&tx, ROOT)?;
        if root.parent.is_some() || !root.name.is_empty() || v.name_policy != "exact-utf8-v1" {
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
        node(&self.connection, id)
    }
    /// Bounded, consistent listing; never silently truncate a directory.
    pub fn namespace_list(&mut self, volume_id: &str, parent: &str) -> Result<Listing> {
        let tx = sql(self.connection.transaction())?;
        let v = check_volume(&tx, volume_id)?;
        directory(&tx, parent)?;
        let children = {
            let mut stmt = sql(tx.prepare("SELECT node_id,parent_id,name,kind,revision,deleted FROM namespace_nodes WHERE parent_id=? AND deleted=0 ORDER BY name LIMIT 1001"))?;
            sql(sql(stmt.query_map([parent], row_node))?.collect::<rusqlite::Result<Vec<_>>>())?
        };
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
        let outcome = if request.expected_sequence != v.sequence {
            Err(Error::Conflict)
        } else {
            mutate(&tx, request, v.sequence + 1)
        };
        if outcome.is_err() {
            sql(tx.execute_batch("ROLLBACK TO namespace_effect"))?;
        }
        sql(tx.execute_batch("RELEASE namespace_effect"))?;
        if outcome == Err(Error::Internal) {
            return outcome;
        }
        let output = serde_json::to_vec(&outcome).map_err(|_| Error::Internal)?;
        sql(tx.execute(
            "INSERT INTO namespace_operations VALUES (?1, ?2, ?3)",
            params![request.operation, encoded, output],
        ))?;
        sql(tx.commit())?;
        outcome
    }
}
fn mutate(db: &Connection, request: &Apply, sequence: u64) -> Result<Receipt> {
    let sql_sequence = i64::try_from(sequence).map_err(|_| Error::Invalid)?;
    let expected = i64::try_from(request.expected_sequence).map_err(|_| Error::Invalid)?;
    let changed = match &request.mutation {
        Mutation::Create {
            parent,
            name,
            node_kind,
        } => {
            directory(db, parent)?;
            available(db, parent, name, None)?;
            let count: i64 =
                sql(db.query_row("SELECT count(*) FROM namespace_nodes", [], |r| r.get(0)))?;
            if count >= MAX_NODES {
                return Err(Error::Limit);
            }
            sql(db.execute(
                "INSERT INTO namespace_nodes VALUES(?1,?2,?3,?4,?5,0)",
                params![
                    request.operation,
                    parent,
                    name,
                    node_kind.sql(),
                    sql_sequence
                ],
            ))?;
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
            sql(db.execute("UPDATE namespace_nodes SET parent_id=?1,name=?2,revision=?3 WHERE node_id=?4 AND deleted=0", params![parent,name,sql_sequence,id]))?;
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
    let rows = sql(db.execute(
        "UPDATE namespace_volume SET sequence=?1 WHERE singleton=1 AND sequence=?2",
        params![sql_sequence, expected],
    ))?;
    if rows != 1 {
        return Err(Error::Conflict);
    }
    Ok(Receipt {
        sequence,
        node: node(db, changed)?,
    })
}

#[cfg(test)]
mod tests;
