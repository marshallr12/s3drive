//! Local SQLite bootstrap. Never place this database on the S3-backed mount.

use rusqlite::{Connection, TransactionBehavior};
use std::{error::Error, path::Path, time::Duration};

const APPLICATION_ID: i32 = 0x5333_4452;
pub const SCHEMA_VERSION: i32 = 1;
pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub struct State {
    connection: Connection,
}

#[derive(Debug)]
pub struct StateInfo {
    pub schema_version: i32,
    pub journal_mode: String,
    pub sqlite_version: String,
}

impl State {
    /// Initialize an empty DB or reopen our current schema; refuse unknown DBs.
    /// The caller must supply a private, local directory outside the mount.
    pub fn open(path: &Path) -> Result<Self> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        {
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let app: i32 = tx.pragma_query_value(None, "application_id", |r| r.get(0))?;
            let version: i32 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
            if app == 0 && version == 0 {
                let tables: i64 = tx.query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                    [],
                    |r| r.get(0),
                )?;
                if tables != 0 {
                    return Err(
                        "refusing to initialize a nonempty database owned by another application"
                            .into(),
                    );
                }
                tx.execute_batch(include_str!("../migrations/001.sql"))?;
                tx.pragma_update(None, "application_id", APPLICATION_ID)?;
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            } else if app != APPLICATION_ID || version != SCHEMA_VERSION {
                return Err(
                    "unknown database application or schema version; migration required".into(),
                );
            }
            // Catch a damaged/current-version database before returning success.
            tx.prepare("SELECT source_alias, object_key, etag, version_id, size_bytes FROM cached_objects LIMIT 0")?;
            tx.commit()?;
        }
        let journal: String =
            connection.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        if !journal.eq_ignore_ascii_case("wal") {
            return Err("SQLite did not enable WAL; use a local disk database".into());
        }
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        Ok(Self { connection })
    }

    pub fn info(&self) -> Result<StateInfo> {
        Ok(StateInfo {
            schema_version: self
                .connection
                .pragma_query_value(None, "user_version", |r| r.get(0))?,
            journal_mode: self
                .connection
                .pragma_query_value(None, "journal_mode", |r| r.get(0))?,
            sqlite_version: self
                .connection
                .query_row("SELECT sqlite_version()", [], |r| r.get(0))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopening_preserves_data_and_durability_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metadata.sqlite3");
        {
            let state = State::open(&path).unwrap();
            state
                .connection
                .execute(
                    "INSERT INTO cached_objects VALUES ('docs', 'x', 'opaque', NULL, 42)",
                    [],
                )
                .unwrap();
        }
        let state = State::open(&path).unwrap();
        let size: i64 = state
            .connection
            .query_row("SELECT size_bytes FROM cached_objects", [], |r| r.get(0))
            .unwrap();
        assert_eq!(size, 42);
        assert_eq!(state.info().unwrap().journal_mode, "wal");
        let sync: i32 = state
            .connection
            .pragma_query_value(None, "synchronous", |r| r.get(0))
            .unwrap();
        let fk: bool = state
            .connection
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert_eq!(sync, 2);
        assert!(fk);
        assert!(
            state
                .connection
                .execute(
                    "INSERT INTO cached_objects VALUES ('docs', 'bad', NULL, NULL, -1)",
                    []
                )
                .is_err()
        );
    }

    #[test]
    fn refuses_future_schema_without_resetting_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metadata.sqlite3");
        {
            let state = State::open(&path).unwrap();
            state
                .connection
                .pragma_update(None, "user_version", 99)
                .unwrap();
        }
        assert!(State::open(&path).is_err());
        let db = Connection::open(path).unwrap();
        let version: i32 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, 99);
    }

    #[test]
    fn refuses_unowned_database_without_changing_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metadata.sqlite3");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE unrelated (value TEXT); INSERT INTO unrelated VALUES ('keep');",
        )
        .unwrap();
        assert!(State::open(&path).is_err());
        let value: String = db
            .query_row("SELECT value FROM unrelated", [], |r| r.get(0))
            .unwrap();
        assert_eq!(value, "keep");
        let app: i32 = db
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .unwrap();
        assert_eq!(app, 0);
    }
}
