//! Local SQLite bootstrap. Never place this database on the S3-backed mount.

mod directory;
#[cfg(windows)]
#[allow(unsafe_code)]
#[path = "directory/windows.rs"]
mod directory_windows;

use rusqlite::{Connection, TransactionBehavior, params};
use std::{error::Error, path::Path, time::Duration};

const APPLICATION_ID: i32 = 0x5333_4452;
pub const SCHEMA_VERSION: i32 = 2;
pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub struct State {
    // Drop the connection before releasing exclusive directory ownership.
    connection: Connection,
    _directory: Option<directory::OwnedDirectory>,
}

#[derive(Debug)]
pub struct StateInfo {
    pub schema_version: i32,
    pub journal_mode: String,
    pub sqlite_version: String,
}

impl State {
    /// Open a private local state directory, creating its last component when
    /// needed. The parent must exist. Ownership is held until this State drops.
    pub fn open_managed(directory: &Path) -> Result<Self> {
        let owner = directory::OwnedDirectory::acquire(directory)?;
        let mut state = Self::open(&owner.database_path())?;
        // Only tighten legacy file modes after confirming this is our DB.
        owner.finish_initialization()?;
        state._directory = Some(owner);
        Ok(state)
    }

    /// Initialize, migrate schema 1, or reopen schema 2; refuse unknown DBs.
    /// Internal schema helper; public callers must acquire directory ownership.
    fn open(path: &Path) -> Result<Self> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        {
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let app: i32 = tx.pragma_query_value(None, "application_id", |r| r.get(0))?;
            let mut version: i32 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
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
                version = 1;
            } else if app != APPLICATION_ID || !(1..=SCHEMA_VERSION).contains(&version) {
                return Err(
                    "unknown database application or schema version; migration required".into(),
                );
            }
            if version == 1 {
                tx.prepare("SELECT source_alias, object_key, etag, version_id, size_bytes FROM cached_objects LIMIT 0")?;
                tx.execute_batch(include_str!("../migrations/002.sql"))?;
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            }
            // Catch a damaged/current-version database before returning success.
            tx.prepare("SELECT source_alias, object_key, etag, version_id, size_bytes FROM legacy_cached_objects_v1 LIMIT 0")?;
            tx.prepare("SELECT source_id, config_fingerprint FROM source_bindings LIMIT 0")?;
            tx.commit()?;
        }
        let journal: String =
            connection.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        if !journal.eq_ignore_ascii_case("wal") {
            return Err("SQLite did not enable WAL; use a local disk database".into());
        }
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        Ok(Self {
            connection,
            _directory: None,
        })
    }

    /// Register an authorized source generation in private local state.
    /// The caller must supply the identity from a successful S3 ReadHandle and
    /// that reader's configuration fingerprint. This registry does not perform
    /// authorization; a stored row must never replace online HEAD on a new open.
    /// No network or cache-file operations belong inside this transaction.
    pub fn register_source(
        &mut self,
        source_id: &[u8; 32],
        config_fingerprint: &str,
    ) -> Result<()> {
        if config_fingerprint.len() != 64
            || !config_fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid configuration fingerprint".into());
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO source_bindings (source_id, config_fingerprint) VALUES (?1, ?2)
             ON CONFLICT(source_id) DO NOTHING",
            params![source_id.as_slice(), config_fingerprint],
        )?;
        let existing: String = tx.query_row(
            "SELECT config_fingerprint FROM source_bindings WHERE source_id = ?1",
            [source_id.as_slice()],
            |r| r.get(0),
        )?;
        if existing != config_fingerprint {
            return Err("source binding conflicts with stored configuration".into());
        }
        tx.commit()?;
        Ok(())
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

    fn schema_one(path: &Path) -> Connection {
        let db = Connection::open(path).unwrap();
        db.execute_batch(include_str!("../migrations/001.sql"))
            .unwrap();
        db.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        db.pragma_update(None, "user_version", 1).unwrap();
        db.execute(
            "INSERT INTO cached_objects VALUES ('docs', 'café.txt', 'opaque', NULL, 42)",
            [],
        )
        .unwrap();
        db
    }

    #[test]
    fn migration_preserves_unbound_rows_without_adopting_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metadata.sqlite3");
        drop(schema_one(&path));
        let state = State::open(&path).unwrap();
        let row: (String, String, String, Option<String>, i64) = state
            .connection
            .query_row("SELECT * FROM legacy_cached_objects_v1", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap();
        assert_eq!(
            row,
            ("docs".into(), "café.txt".into(), "opaque".into(), None, 42)
        );
        assert_eq!(state.info().unwrap().schema_version, 2);
        let bindings: i64 = state
            .connection
            .query_row("SELECT count(*) FROM source_bindings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(bindings, 0);
        assert!(
            state
                .connection
                .prepare("SELECT * FROM cached_objects")
                .is_err()
        );
        drop(state);
        assert_eq!(
            State::open(&path).unwrap().info().unwrap().schema_version,
            2
        );
    }

    #[test]
    fn migration_failure_rolls_back_rename_and_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metadata.sqlite3");
        let db = schema_one(&path);
        // The rename succeeds, then CREATE fails; both must roll back.
        db.execute_batch(
            "CREATE TABLE source_bindings (keep TEXT);
                          INSERT INTO source_bindings VALUES ('untouched');",
        )
        .unwrap();
        assert!(State::open(&path).is_err());
        let version: i32 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, 1);
        let size: i64 = db
            .query_row("SELECT size_bytes FROM cached_objects", [], |r| r.get(0))
            .unwrap();
        assert_eq!(size, 42);
        assert!(
            db.prepare("SELECT * FROM legacy_cached_objects_v1")
                .is_err()
        );
        let value: String = db
            .query_row("SELECT keep FROM source_bindings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(value, "untouched");
    }

    #[test]
    fn registration_is_idempotent_isolated_and_rejects_rebinding() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metadata.sqlite3");
        let mut state = State::open(&path).unwrap();
        let first = "a".repeat(64);
        let other = "b".repeat(64);
        state.register_source(&[1; 32], &first).unwrap();
        state.register_source(&[1; 32], &first).unwrap();
        state.register_source(&[2; 32], &first).unwrap();
        assert!(state.register_source(&[1; 32], &other).is_err());
        for invalid in ["a".repeat(63), "A".repeat(64), "g".repeat(64)] {
            assert!(state.register_source(&[3; 32], &invalid).is_err());
        }
        drop(state);
        let mut state = State::open(&path).unwrap();
        state.register_source(&[1; 32], &first).unwrap();
        let count: i64 = state
            .connection
            .query_row("SELECT count(*) FROM source_bindings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let distinct: i64 = state
            .connection
            .query_row(
                "SELECT count(DISTINCT config_fingerprint) FROM source_bindings",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(distinct, 1);
    }

    #[test]
    fn database_enforces_binding_shapes() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::open(&dir.path().join("metadata.sqlite3")).unwrap();
        for (id, fingerprint) in [
            (vec![0_u8; 31], "a".repeat(64)),
            (vec![0_u8; 32], "A".repeat(64)),
            (vec![0_u8; 32], "g".repeat(64)),
            (vec![0_u8; 32], "a".repeat(63)),
        ] {
            assert!(
                state
                    .connection
                    .execute(
                        "INSERT INTO source_bindings VALUES (?1, ?2)",
                        params![id, fingerprint],
                    )
                    .is_err()
            );
        }
    }

    #[test]
    fn damaged_known_schemas_are_refused_without_version_change() {
        for version in [1, 2] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("metadata.sqlite3");
            let db = if version == 1 {
                let db = schema_one(&path);
                db.execute_batch("ALTER TABLE cached_objects DROP COLUMN etag;")
                    .unwrap();
                db
            } else {
                drop(State::open(&path).unwrap());
                let db = Connection::open(&path).unwrap();
                db.execute_batch("DROP TABLE source_bindings;").unwrap();
                db
            };
            assert!(State::open(&path).is_err());
            let actual: i32 = db
                .pragma_query_value(None, "user_version", |r| r.get(0))
                .unwrap();
            assert_eq!(actual, version);
        }
    }

    #[test]
    fn reopening_preserves_data_and_durability_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metadata.sqlite3");
        {
            let mut state = State::open(&path).unwrap();
            state.register_source(&[1; 32], &"a".repeat(64)).unwrap();
        }
        let state = State::open(&path).unwrap();
        let fingerprint: String = state
            .connection
            .query_row("SELECT config_fingerprint FROM source_bindings", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(fingerprint, "a".repeat(64));
        assert_eq!(state.info().unwrap().schema_version, 2);
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
                    "INSERT INTO legacy_cached_objects_v1 VALUES ('docs', 'bad', NULL, NULL, -1)",
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
