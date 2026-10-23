//! Private SQLite state, clean cache and local staged-generation journal.
//! Never place this database on the S3-backed mount.

pub mod authority_snapshot;
pub mod cache;
pub mod chunk_mount;
pub mod chunk_publication;
pub mod chunked;
mod directory;
pub use directory::{create_private_document, create_private_secret, read_private_secret};
#[cfg(windows)]
#[allow(unsafe_code)]
#[path = "directory/windows.rs"]
mod directory_windows;
pub mod journal;
pub mod logical;
pub mod managed;
pub mod manifest_creation;
pub mod multipart;
pub mod namespace;
pub mod namespace_client;
pub mod remote;
pub mod sparse_views;
pub mod worker;

use rusqlite::{Connection, TransactionBehavior, params};
use std::{error::Error, path::Path, time::Duration};

const APPLICATION_ID: i32 = 0x5333_4452;
pub const SCHEMA_VERSION: i32 = 49;
pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
#[cfg(test)]
mod authority_restart_tests;
#[cfg(test)]
mod test_schema;

pub struct State {
    // Drop the connection before releasing exclusive directory ownership.
    connection: Connection,
    _directory: Option<directory::OwnedDirectory>,
    incarnation: Option<String>,
    source_pin_witness: Option<[u8; 32]>,
    native_sparse: sparse_views::preparation::native::Runtime,
}

#[derive(Debug)]
pub struct StateInfo {
    pub schema_version: i32,
    pub journal_mode: String,
    pub sqlite_version: String,
}

impl State {
    fn verify_staging_storage(&self) -> Result<()> {
        self.verify_client_state()?;
        sparse_views::pins::verify_owner(self)?;
        self._directory
            .as_ref()
            .ok_or("staging requires managed local state")?
            .verify_staging_storage()
    }
    fn verify_client_state(&self) -> Result<()> {
        let count: i64 =
            self.connection
                .query_row("SELECT count(*) FROM namespace_volume", [], |r| r.get(0))?;
        if count != 0 {
            return Err("authority state cannot be used for local cache or staging".into());
        }
        Ok(())
    }

    /// Open a private local state directory, creating its last component when
    /// needed. The parent must exist. Ownership is held until this State drops.
    pub fn open_managed(directory: &Path) -> Result<Self> {
        let owner = directory::OwnedDirectory::acquire(directory)?;
        Self::open_owned(owner, None)
    }

    /// Restart only an existing, pinned authority. Refuse missing files, local
    /// client state and different identities before migrations or intent recovery.
    pub fn open_existing_authority(directory: &Path, volume: &str, policy: &str) -> Result<Self> {
        let owner = directory::OwnedDirectory::acquire_existing(directory)?;
        Self::inspect_authority(&owner, volume, policy)?;
        let state = Self::open_owned(owner, Some((volume, policy)))?;
        namespace::existing_identity(&state.connection, volume, policy)
            .map_err(|_| "existing authority identity changed during opening")?;
        Ok(state)
    }

    /// Offline, exclusive read-only identity check. Does not migrate schemas or
    /// recover attempts. SQLite may maintain operational WAL/shared-memory files.
    pub fn inspect_existing_authority(
        directory: &Path,
        volume: &str,
        policy: &str,
    ) -> Result<namespace::Volume> {
        let owner = directory::OwnedDirectory::acquire_existing(directory)?;
        Self::inspect_authority(&owner, volume, policy)
    }

    fn inspect_authority(
        owner: &directory::OwnedDirectory,
        volume: &str,
        policy: &str,
    ) -> Result<namespace::Volume> {
        use rusqlite::OpenFlags;
        owner.verify_staging_storage()?;
        let mut db = Connection::open_with_flags(
            owner.database_path(),
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.busy_timeout(Duration::from_secs(5))?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let app: i32 = tx.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let version: i32 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if app != APPLICATION_ID || !(8..=SCHEMA_VERSION).contains(&version) {
            return Err("existing authority requires a recognized namespace database".into());
        }
        let info = namespace::existing_identity(&tx, volume, policy)
            .map_err(|_| "existing authority volume, name policy or root does not match")?;
        tx.commit()?;
        Ok(info)
    }

    fn open_owned(
        owner: directory::OwnedDirectory,
        expected: Option<(&str, &str)>,
    ) -> Result<Self> {
        let mut state = match expected {
            None => Self::open(&owner.database_path())?,
            Some(_) => Self::open_mode(&owner.database_path(), expected)?,
        };
        // Only tighten legacy file modes after confirming this is our DB.
        owner.finish_initialization()?;
        state._directory = Some(owner);
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| "state-owner identity generation failed")?;
        let incarnation: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        if incarnation == namespace::ROOT {
            return Err("state-owner identity generation failed".into());
        }
        state.incarnation = Some(incarnation);
        remote::recover_interrupted(&mut state)?;
        Ok(state)
    }

    /// Initialize, migrate known older schemas, or reopen; refuse unknown DBs.
    /// Internal schema helper; public callers must acquire directory ownership.
    fn open(path: &Path) -> Result<Self> {
        Self::open_mode(path, None)
    }
    fn open_mode(path: &Path, expected: Option<(&str, &str)>) -> Result<Self> {
        let mut connection = if expected.is_none() {
            Connection::open(path)?
        } else {
            Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
                    | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
            )?
        };
        connection.busy_timeout(Duration::from_secs(5))?;
        // Migrations must not depend on SQLite's compile-time FK default.
        connection.pragma_update(None, "foreign_keys", true)?;
        {
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let app: i32 = tx.pragma_query_value(None, "application_id", |r| r.get(0))?;
            let mut version: i32 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
            if let Some((volume, policy)) = expected {
                // Recheck under the SAME transaction that may migrate. Even an
                // external replacement after preflight must not be initialized,
                // migrated or admitted as a different authority.
                if app != APPLICATION_ID || !(8..=SCHEMA_VERSION).contains(&version) {
                    return Err("existing authority schema changed after inspection".into());
                }
                namespace::existing_identity(&tx, volume, policy)
                    .map_err(|_| "existing authority identity changed after inspection")?;
            }
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
                version = 2;
            }
            if version == 2 {
                tx.execute_batch(include_str!("../migrations/003.sql"))?;
                version = 3;
            }
            if version == 3 {
                tx.execute_batch(include_str!("../migrations/004.sql"))?;
                version = 4;
            }
            if version == 4 {
                tx.execute_batch(include_str!("../migrations/005.sql"))?;
                version = 5;
            }
            if version == 5 {
                tx.execute_batch(include_str!("../migrations/006.sql"))?;
                version = 6;
            }
            if version == 6 {
                tx.execute_batch(include_str!("../migrations/007.sql"))?;
                version = 7;
            }
            if version == 7 {
                tx.execute_batch(include_str!("../migrations/008.sql"))?;
                version = 8;
            }
            if version == 8 {
                tx.execute_batch(include_str!("../migrations/009.sql"))?;
                version = 9;
            }
            if version == 9 {
                tx.execute_batch(include_str!("../migrations/010.sql"))?;
                version = 10;
            }
            if version == 10 {
                tx.execute_batch(include_str!("../migrations/011.sql"))?;
                version = 11;
            }
            if version == 11 {
                tx.execute_batch(include_str!("../migrations/012.sql"))?;
                version = 12;
            }
            if version == 12 {
                tx.execute_batch(include_str!("../migrations/013.sql"))?;
                version = 13;
            }
            if version == 13 {
                tx.execute_batch(include_str!("../migrations/014.sql"))?;
                version = 14;
            }
            if version == 14 {
                tx.execute_batch(include_str!("../migrations/015.sql"))?;
                version = 15;
            }
            if version == 15 {
                tx.execute_batch(include_str!("../migrations/016.sql"))?;
                namespace::migrate_removals(&tx)?;
                version = 16;
            }
            if version == 16 {
                tx.execute_batch(include_str!("../migrations/017.sql"))?;
                version = 17;
            }
            // Version 18 adds controlled callback semantics in the existing
            // hash-bound JSON log. No old rows/hashes are rewritten.
            if version <= 18 {
                tx.execute_batch(include_str!("../migrations/019.sql"))?;
            }
            if version <= 19 {
                tx.execute_batch(include_str!("../migrations/020.sql"))?;
            }
            // Versions21–23 strengthen callback/profile policy and add paired owned-
            // target controls in the existing logs. Old hashes stay unchanged.
            if version <= 23 {
                tx.execute_batch(include_str!("../migrations/024.sql"))?;
            }
            if version <= 24 {
                tx.execute_batch(include_str!("../migrations/025.sql"))?;
            }
            if version <= 25 {
                tx.execute_batch(include_str!("../migrations/026.sql"))?;
            }
            if version <= 26 {
                tx.execute_batch(include_str!("../migrations/027.sql"))?;
            }
            if version <= 27 {
                tx.execute_batch(include_str!("../migrations/028.sql"))?;
            }
            if version <= 28 {
                tx.execute_batch(include_str!("../migrations/029.sql"))?;
            }
            if version <= 29 {
                sparse_views::migrate_thirty(&tx)?;
            }
            if version <= 30 {
                tx.execute_batch(include_str!("../migrations/031.sql"))?;
            }
            if version <= 31 {
                tx.execute_batch(include_str!("../migrations/032.sql"))?;
            }
            if version <= 32 {
                tx.execute_batch(include_str!("../migrations/033.sql"))?;
            }
            if version <= 35 {
                tx.execute_batch(include_str!("../migrations/036.sql"))?;
            }
            if version <= 36 {
                tx.execute_batch(include_str!("../migrations/037.sql"))?;
            }
            if version < 40 {
                tx.execute_batch(include_str!("../migrations/040.sql"))?;
            }
            if version < 41 {
                tx.execute_batch(include_str!("../migrations/041.sql"))?;
            }
            if version < 49 {
                tx.execute_batch(include_str!("../migrations/049.sql"))?;
            }
            // Imported creation records cannot be laundered through an old
            // marker. Existing43/44 roles keep every byte; a transaction-local
            // current marker enables their strict role validators. Any failure
            // rolls back this marker together with the rest of initialization.
            if version < 48 {
                sparse_views::preparation::native::require_pre48(&tx)?;
            }
            if version < 47 {
                sparse_views::preparation::owned::require_pre47(&tx)?;
            }
            if version < 46 {
                namespace::publication::event::require_pre46(&tx)?;
            }
            if version < 45 {
                manifest_creation::admission::require_pre45(&tx)?;
            }
            if version < 44 {
                namespace_client::creator_import::require_pre44(&tx)?;
            }
            if (43..=48).contains(&version) {
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            }
            chunk_mount::check_schema(&tx)?;
            chunk_mount::validate_all(&tx)?;
            // Version38 admits distinct format4 external-clean observation intent.
            // No DDL or old BLOB changes. Pre-marker validation rejects format4
            // records in earlier schemas, and older binaries refuse version38.
            // Version34 admits explicit local predecessor edges in format2 root
            // records. Existing schema33 rows/hashes remain byte-for-byte intact.
            // Version35 admits exact source acknowledgments and immutable rebase
            // transitions. Validation precedes the version marker; no old BLOB
            // or hash is rewritten and a failure rolls back the marker too.
            sparse_views::preparation::publication::check_schema(&tx)?;
            if version <= 34 {
                sparse_views::preparation::publication::require_pre35(&tx)?;
            }
            sparse_views::preparation::publication::validate_all(&tx)?;
            sparse_views::preparation::check_schema(&tx)?;
            sparse_views::preparation::validate_all(&tx)?;
            sparse_views::check_schema(&tx)?;
            sparse_views::validate_all(&tx)?;
            chunk_publication::check_schema(&tx)?;
            chunk_publication::validate_all(&tx)?;
            chunked::check_schema(&tx)?;
            chunked::validate_all(&tx)?;
            multipart::check_schema(&tx)?;
            multipart::validate_all(&tx)?;
            logical::check_schema(&tx)?;
            managed::check_schema(&tx)?;
            namespace::check_schema(&tx)?;
            if version < 39 {
                namespace::publication::empty::require_pre39(&tx)?;
            }
            // Schema42 changes wire records only. Never relabel new records as old.
            if version < 42 {
                namespace::publication::deferred::require_pre42(&tx)?;
            }
            namespace_client::check_schema(&tx)?;
            manifest_creation::check_schema(&tx)?;
            // Schema43 adds owner-bearing creation record format2, with no DDL.
            // Validation rejects new records under older markers before relabeling.
            manifest_creation::validate_all(&tx)?;
            // Catch a damaged/current-version database before returning success.
            tx.prepare("SELECT source_alias, object_key, etag, version_id, size_bytes FROM legacy_cached_objects_v1 LIMIT 0")?;
            tx.prepare("SELECT source_id, config_fingerprint FROM source_bindings LIMIT 0")?;
            tx.prepare("SELECT cache_key, source_id, size_bytes, checksum, data, last_used FROM clean_blobs LIMIT 0")?;
            tx.prepare("SELECT generation_id, source_id, object_key, parent_id, base_absent, base_etag, base_version, size_bytes, filled_bytes, extent_count, phase, manifest_hash FROM staged_generations LIMIT 0")?;
            tx.prepare("SELECT chunk_hash, data, created_by FROM staged_chunks LIMIT 0")?;
            tx.prepare("SELECT chunk_hash FROM retained_staged_chunks LIMIT 0")?;
            tx.prepare("SELECT generation_id, offset_bytes, length_bytes, chunk_hash FROM staged_extents LIMIT 0")?;
            tx.prepare("SELECT source_id, object_key, generation_id FROM staged_heads LIMIT 0")?;
            tx.prepare("SELECT generation_id, status FROM write_journal LIMIT 0")?;
            tx.prepare("SELECT operation_key, generation_id FROM staged_operations LIMIT 0")?;
            tx.prepare("SELECT generation_id, operation_id, source_id, config_fingerprint, object_key, manifest_hash, payload_sha256, size_bytes, guard_etag, anchor_operation, anchor_receipt, intent_hash, phase, reason, observation, rejection_status, remote_etag, remote_version, state_hash FROM remote_intents LIMIT 0")?;
            tx.prepare("SELECT operation_id FROM retained_remote_intents LIMIT 0")?;
            tx.prepare("SELECT operation_id, generation_id, covered_generations, receipt_hash FROM remote_flushes LIMIT 0")?;
            tx.prepare("SELECT generation_id, covering_operation, manifest_hash, settlement_hash FROM write_settlements LIMIT 0")?;
            if version != SCHEMA_VERSION {
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            }
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
            incarnation: None,
            source_pin_witness: None,
            native_sparse: Default::default(),
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
        self.verify_client_state()?;
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
        crate::test_schema::demote(&db, 1).unwrap();
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
        assert_eq!(state.info().unwrap().schema_version, SCHEMA_VERSION);
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
            SCHEMA_VERSION
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
                db.execute_batch(
                    "DROP TABLE clean_blobs; DROP TABLE source_bindings; PRAGMA user_version=2;",
                )
                .unwrap();
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
        assert_eq!(state.info().unwrap().schema_version, SCHEMA_VERSION);
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
