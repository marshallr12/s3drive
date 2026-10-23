use super::{Error, Inspection, Limits, Manifest, Result};
use crate::{
    APPLICATION_ID, SCHEMA_VERSION,
    directory::{OwnedDirectory, SnapshotDirectory},
    namespace,
};
use rusqlite::{
    Connection, OpenFlags, TransactionBehavior,
    backup::{Backup, StepResult},
    limits::Limit,
};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const FORMAT: &str = "s3drive-authority-snapshot";
const VALIDATION: &str = "sqlite-integrity-authority-identity-v1";
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;

struct Control {
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
}
impl Control {
    fn new(limits: &Limits) -> Result<Self> {
        if !(4096..=MAX_BYTES).contains(&limits.max_bytes)
            || limits.timeout.is_zero()
            || limits.timeout > Duration::from_secs(300)
        {
            return Err(Error::InvalidLimits);
        }
        let value = Self {
            deadline: Instant::now() + limits.timeout,
            cancelled: limits.cancelled.clone(),
        };
        value.check()?;
        Ok(value)
    }
    fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(Error::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(Error::Deadline)
        } else {
            Ok(())
        }
    }
    fn configure(&self, db: &Connection) -> Result<()> {
        db.busy_timeout(Duration::from_millis(100))
            .map_err(|_| Error::Validation)?;
        db.set_limit(Limit::SQLITE_LIMIT_LENGTH, 4 * 1024 * 1024)
            .map_err(|_| Error::Validation)?;
        let deadline = self.deadline;
        let cancelled = self.cancelled.clone();
        db.progress_handler(
            1000,
            Some(move || cancelled.load(Ordering::Relaxed) || Instant::now() >= deadline),
        )
        .map_err(|_| Error::Validation)?;
        db.pragma_update(None, "trusted_schema", false)
            .map_err(|_| Error::Validation)?;
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Created,
    Copying,
    Copied,
    Validated,
    DataSynced,
    ManifestWritten,
    ManifestSynced,
    Published,
}
fn checkpoint(
    control: &Control,
    hook: &mut impl FnMut(Stage, Option<&Connection>) -> Result<()>,
    stage: Stage,
    db: Option<&Connection>,
) -> Result<()> {
    control.check()?;
    hook(stage, db)?;
    control.check()
}
fn open_read(path: &Path, control: &Control) -> Result<Connection> {
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| Error::Validation)?;
    control.configure(&db)?;
    db.pragma_update(None, "query_only", true)
        .map_err(|_| Error::Validation)?;
    Ok(db)
}
fn identity(
    db: &Connection,
    volume: &str,
    policy: &str,
    max_bytes: u64,
) -> Result<(i32, namespace::Volume)> {
    let app: i32 = db
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .map_err(|_| Error::Validation)?;
    let version: i32 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|_| Error::Validation)?;
    if app != APPLICATION_ID || !(8..=SCHEMA_VERSION).contains(&version) {
        return Err(Error::Validation);
    }
    let pages: i64 = db
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .map_err(|_| Error::Validation)?;
    let size: i64 = db
        .pragma_query_value(None, "page_size", |r| r.get(0))
        .map_err(|_| Error::Validation)?;
    let pages = u64::try_from(pages).map_err(|_| Error::Validation)?;
    let size = u64::try_from(size).map_err(|_| Error::Validation)?;
    if !(512..=65_536).contains(&size) || !size.is_power_of_two() {
        return Err(Error::Validation);
    }
    if pages.checked_mul(size).is_none_or(|n| n > max_bytes) {
        return Err(Error::Limit);
    }
    let tables = {
        let mut stmt = db.prepare("SELECT name,type FROM sqlite_schema WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' LIMIT 66").map_err(|_| Error::Validation)?;
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|_| Error::Validation)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|_| Error::Validation)?
    };
    let authority = [
        "namespace_volume",
        "namespace_nodes",
        "namespace_operations",
        "namespace_stores",
        "namespace_uploads",
        "namespace_removals",
        "namespace_removal_events",
    ];
    let local = [
        "chunk_mount_role",
        "manifest_creation_role",
        "manifest_creations",
        "sparse_scoped_mode",
        "sparse_retry_mode",
        "source_read_pin_session",
        "source_read_pins",
        "sparse_publication_role",
        "sparse_publications",
        "sparse_publication_checks",
        "sparse_preparations",
        "sparse_preparation_objects",
        "sparse_preparation_blobs",
        "sparse_view_role",
        "sparse_view_files",
        "sparse_views",
        "sparse_view_operations",
        "sparse_view_blocks",
        "legacy_cached_objects_v1",
        "source_bindings",
        "clean_blobs",
        "staged_generations",
        "staged_chunks",
        "retained_staged_chunks",
        "staged_extents",
        "staged_heads",
        "write_journal",
        "staged_operations",
        "remote_intents",
        "retained_remote_intents",
        "remote_flushes",
        "write_settlements",
        "managed_publications",
        "logical_files",
        "logical_edits",
        "logical_publication_links",
        "namespace_intents",
        "client_identity",
        "retained_client_identity",
        "managed_rejections",
        "logical_conflicts",
        "windows_open_journal",
        "retained_windows_opens",
        "windows_open_events",
        "retired_windows_opens",
        "windows_mount_sessions",
        "windows_session_recoveries",
        "recovered_windows_opens",
        "multipart_plans",
        "multipart_parts",
        "chunk_plans",
        "chunk_objects",
        "chunk_client_role",
        "chunk_publications",
        "chunk_source_role",
        "chunk_source_settlements",
    ];
    if tables.len() > 65 {
        return Err(Error::Validation);
    }
    for (name, kind) in &tables {
        if kind != "table"
            || (!authority.contains(&name.as_str()) && !local.contains(&name.as_str()))
        {
            return Err(Error::Validation);
        }
        if local.contains(&name.as_str()) {
            // Only allowlisted, constant identifiers can reach this statement.
            let used: bool = db
                .query_row(
                    &format!("SELECT EXISTS(SELECT 1 FROM \"{name}\")"),
                    [],
                    |r| r.get(0),
                )
                .map_err(|_| Error::Validation)?;
            if used {
                return Err(Error::Validation);
            }
        }
    }
    for name in &authority {
        let required = match *name {
            "namespace_stores" | "namespace_uploads" => version >= 9,
            "namespace_removals" => version >= 13,
            "namespace_removal_events" => version >= 16,
            _ => true,
        };
        if required && !tables.iter().any(|(n, _)| n == name) {
            return Err(Error::Validation);
        }
    }
    let info = namespace::existing_identity(db, volume, policy).map_err(|_| Error::Validation)?;
    Ok((version, info))
}
fn integrity(db: &Connection) -> Result<()> {
    let mut statement = db
        .prepare("PRAGMA integrity_check")
        .map_err(|_| Error::Validation)?;
    let mut rows = statement.query([]).map_err(|_| Error::Validation)?;
    let first: String = rows
        .next()
        .map_err(|_| Error::Validation)?
        .ok_or(Error::Validation)?
        .get(0)
        .map_err(|_| Error::Validation)?;
    if first != "ok" || rows.next().map_err(|_| Error::Validation)?.is_some() {
        return Err(Error::Validation);
    }
    let mut statement = db
        .prepare("PRAGMA foreign_key_check")
        .map_err(|_| Error::Validation)?;
    if statement
        .query([])
        .map_err(|_| Error::Validation)?
        .next()
        .map_err(|_| Error::Validation)?
        .is_some()
    {
        return Err(Error::Validation);
    }
    Ok(())
}
fn digest(file: &File, max_bytes: u64, control: &Control) -> Result<(u64, String)> {
    let length = file.metadata().map_err(|_| Error::InvalidArtifact)?.len();
    if length > max_bytes {
        return Err(Error::Limit);
    }
    let mut file = file.try_clone().map_err(|_| Error::InvalidArtifact)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|_| Error::InvalidArtifact)?;
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 65_536];
    loop {
        control.check()?;
        let count = file.read(&mut buffer).map_err(|_| Error::InvalidArtifact)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > max_bytes {
            return Err(Error::Limit);
        }
        hash.update(&buffer[..count]);
    }
    if total != length || file.metadata().map_err(|_| Error::InvalidArtifact)?.len() != length {
        return Err(Error::InvalidArtifact);
    }
    Ok((
        total,
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect(),
    ))
}
/// Create only a new private artifact. Requires an offline, exclusively owned
/// existing authority. No migrations, credentials, provider I/O or restore.
/// Limits are cooperative between bounded copy/read steps, not a hard deadline
/// against an OS call stalled inside its filesystem driver.
pub fn create(
    source: &Path,
    volume: &str,
    policy: &str,
    output: &Path,
    limits: &Limits,
) -> Result<Manifest> {
    let started = Instant::now();
    normalize(
        create_inner(source, volume, policy, output, limits, &mut |_, _| Ok(())),
        limits,
        started,
    )
}
fn create_inner(
    source: &Path,
    volume: &str,
    policy: &str,
    output: &Path,
    limits: &Limits,
    hook: &mut impl FnMut(Stage, Option<&Connection>) -> Result<()>,
) -> Result<Manifest> {
    let control = Control::new(limits)?;
    let source_owner = OwnedDirectory::acquire_existing(source).map_err(|_| Error::Source)?;
    source_owner
        .verify_staging_storage()
        .map_err(|_| Error::Source)?;
    let mut source =
        open_read(&source_owner.database_path(), &control).map_err(|_| Error::Source)?;
    let tx = source
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|_| Error::Source)?;
    let expected = identity(&tx, volume, policy, limits.max_bytes)?;
    integrity(&tx)?;
    control.check()?;
    let owner = SnapshotDirectory::create(output, &source_owner).map_err(|_| Error::Destination)?;
    checkpoint(&control, hook, Stage::Created, None)?;
    owner.recheck_database().map_err(|_| Error::Destination)?;
    let mut destination = Connection::open_with_flags(
        owner.database_path(),
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| Error::Destination)?;
    control.configure(&destination)?;
    destination
        .pragma_update(None, "journal_mode", "DELETE")
        .map_err(|_| Error::Destination)?;
    destination
        .pragma_update(None, "synchronous", "FULL")
        .map_err(|_| Error::Destination)?;
    checkpoint(&control, hook, Stage::Copying, Some(&destination))?;
    {
        let backup = Backup::new(&tx, &mut destination).map_err(|_| Error::Copy)?;
        loop {
            control.check()?;
            match backup.step(32).map_err(|_| Error::Copy)? {
                StepResult::Done => break,
                StepResult::More => checkpoint(&control, hook, Stage::Copying, None)?,
                StepResult::Busy | StepResult::Locked => {
                    checkpoint(&control, hook, Stage::Copying, None)?;
                    std::thread::sleep(Duration::from_millis(2));
                }
                _ => return Err(Error::Copy),
            }
        }
        // Resolved SQLite0.38.2: step reports DONE only after destination commit;
        // finish maps that retained DONE state to OK. No unchecked early finish
        // or unbounded run_to_completion loop is used here.
    }
    destination
        .pragma_update(None, "journal_mode", "DELETE")
        .map_err(|_| Error::Copy)?;
    destination.close().map_err(|_| Error::Copy)?;
    tx.commit().map_err(|_| Error::Source)?;
    source.close().map_err(|_| Error::Source)?;
    checkpoint(&control, hook, Stage::Copied, None)?;
    owner.recheck_database().map_err(|_| Error::Destination)?;
    let verified = open_read(&owner.database_path(), &control)?;
    let observed = identity(&verified, volume, policy, limits.max_bytes)?;
    integrity(&verified)?;
    verified.close().map_err(|_| Error::Validation)?;
    if observed != expected {
        return Err(Error::Validation);
    }
    checkpoint(&control, hook, Stage::Validated, None)?;
    owner.recheck_database().map_err(|_| Error::Destination)?;
    owner.database.sync_all().map_err(|_| Error::Destination)?;
    checkpoint(&control, hook, Stage::DataSynced, None)?;
    let (byte_length, sha256) = digest(&owner.database, limits.max_bytes, &control)?;
    let manifest = Manifest {
        format: FORMAT.into(),
        version: 1,
        application_id: APPLICATION_ID,
        schema_version: observed.0,
        volume: observed.1,
        database: SnapshotDirectory::DATABASE.into(),
        byte_length,
        sha256,
        validation: VALIDATION.into(),
        created_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Validation)?
            .as_millis()
            .try_into()
            .map_err(|_| Error::Validation)?,
        writer_version: env!("CARGO_PKG_VERSION").into(),
    };
    let bytes = serde_json::to_vec(&manifest).map_err(|_| Error::Validation)?;
    // Once marker publication starts, any error leaves completion uncertain;
    // never delete output or imply that retrying under a new name is required.
    owner
        .write_manifest(&bytes, || {
            checkpoint(&control, hook, Stage::ManifestWritten, None)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
        .map_err(|_| Error::CompletionUnknown)?;
    checkpoint(&control, hook, Stage::ManifestSynced, None)
        .map_err(|_| Error::CompletionUnknown)?;
    owner
        .recheck_database()
        .map_err(|_| Error::CompletionUnknown)?;
    checkpoint(&control, hook, Stage::Published, None).map_err(|_| Error::CompletionUnknown)?;
    Ok(manifest)
}
/// Inspect completed standalone bytes without migrating or granting restoration.
pub fn inspect(output: &Path, volume: &str, policy: &str, limits: &Limits) -> Result<Inspection> {
    let started = Instant::now();
    normalize(
        inspect_inner(output, volume, policy, limits),
        limits,
        started,
    )
}
fn normalize<T>(result: Result<T>, limits: &Limits, started: Instant) -> Result<T> {
    match result {
        Err(error) if !matches!(error, Error::InvalidLimits | Error::CompletionUnknown) => {
            if limits.cancelled.load(Ordering::Relaxed) {
                Err(Error::Cancelled)
            } else if started.elapsed() >= limits.timeout {
                Err(Error::Deadline)
            } else {
                Err(error)
            }
        }
        result => result,
    }
}
fn inspect_inner(output: &Path, volume: &str, policy: &str, limits: &Limits) -> Result<Inspection> {
    let control = Control::new(limits)?;
    let owner = SnapshotDirectory::inspect(output).map_err(|_| Error::InvalidArtifact)?;
    let manifest: Manifest =
        serde_json::from_slice(&owner.read_manifest().map_err(|_| Error::InvalidArtifact)?)
            .map_err(|_| Error::InvalidArtifact)?;
    if manifest.format != FORMAT
        || manifest.version != 1
        || manifest.application_id != APPLICATION_ID
        || !(8..=SCHEMA_VERSION).contains(&manifest.schema_version)
        || manifest.database != SnapshotDirectory::DATABASE
        || manifest.validation != VALIDATION
        || manifest.volume.id != volume
        || manifest.volume.name_policy != policy
        || manifest.sha256.len() != 64
        || !manifest
            .sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || manifest.writer_version.is_empty()
        || manifest.writer_version.len() > 64
    {
        return Err(Error::InvalidArtifact);
    }
    let (length, hash) = digest(&owner.database, limits.max_bytes, &control)?;
    if length != manifest.byte_length || hash != manifest.sha256 {
        return Err(Error::InvalidArtifact);
    }
    owner
        .recheck_database()
        .map_err(|_| Error::InvalidArtifact)?;
    let db = open_read(&owner.database_path(), &control)?;
    let (version, info) = identity(&db, volume, policy, limits.max_bytes)?;
    integrity(&db)?;
    db.close().map_err(|_| Error::Validation)?;
    owner
        .recheck_database()
        .map_err(|_| Error::InvalidArtifact)?;
    if version != manifest.schema_version
        || info != manifest.volume
        || digest(&owner.database, limits.max_bytes, &control)? != (length, hash)
    {
        return Err(Error::InvalidArtifact);
    }
    control.check()?;
    Ok(Inspection {
        manifest,
        history_continuity_proven: false,
        restore_authorized: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        State,
        namespace::{Apply, Kind, Mutation, ROOT},
    };
    use std::{
        fs,
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
    };
    const POLICY: &str = "portable-collision-v1";
    fn seed(path: &Path) -> namespace::Volume {
        let mut state = State::open_managed(path).unwrap();
        let v = state.namespace_initialize_with_policy(POLICY).unwrap();
        state
            .namespace_apply(&Apply {
                volume: v.id.clone(),
                operation: "1".repeat(32),
                expected_sequence: 0,
                mutation: Mutation::RegisterStore {
                    route: "2".repeat(64),
                    root: "managed/.s3drive/".into(),
                },
            })
            .unwrap();
        state
            .namespace_apply(&Apply {
                volume: v.id.clone(),
                operation: "3".repeat(32),
                expected_sequence: 1,
                mutation: Mutation::CreateEntry {
                    parent: ROOT.into(),
                    parent_revision: 0,
                    store: None,
                    name: "資料 café 🦀".into(),
                    node_kind: Kind::Directory,
                },
            })
            .unwrap();
        state.namespace_volume().unwrap()
    }
    type OperationRecords = (i32, Vec<(Vec<u8>, Vec<u8>)>);
    fn records(path: &Path) -> OperationRecords {
        let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let rows = db
            .prepare("SELECT request,outcome FROM namespace_operations ORDER BY operation_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        (
            db.pragma_query_value(None, "user_version", |r| r.get(0))
                .unwrap(),
            rows,
        )
    }
    #[test]
    fn complete_private_snapshot_preserves_exact_rows_and_cannot_boot_as_live_state() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("authority");
        let target = tmp.path().join("snapshot 資料 café 🦀");
        let v = seed(&source);
        let before = records(&source.join("metadata.sqlite3"));
        let manifest = create(&source, &v.id, POLICY, &target, &Limits::default()).unwrap();
        let check = inspect(&target, &v.id, POLICY, &Limits::default()).unwrap();
        assert_eq!(check.manifest, manifest);
        assert_eq!(manifest.volume, v);
        assert!(!check.restore_authorized && !check.history_continuity_proven);
        assert_eq!(records(&target.join("snapshot.sqlite3")), before);
        assert_eq!(records(&source.join("metadata.sqlite3")), before);
        let saved = fs::read(target.join("snapshot.json")).unwrap();
        assert_eq!(
            create(&source, &v.id, POLICY, &target, &Limits::default()),
            Err(Error::Destination)
        );
        assert_eq!(fs::read(target.join("snapshot.json")).unwrap(), saved);
        assert!(State::open_managed(&target).is_err());
        assert!(State::open_existing_authority(&target, &v.id, POLICY).is_err());
        assert!(!target.join("metadata.sqlite3").exists() && !target.join("volume.lock").exists());
        assert_eq!(
            create(
                &source,
                &v.id,
                POLICY,
                &source.join("nested"),
                &Limits::default()
            ),
            Err(Error::Destination)
        );
        assert!(!source.join("nested").exists());
    }
    #[test]
    fn invalid_pins_roles_schemas_and_limits_refuse_before_creating_output() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("authority");
        let target = tmp.path().join("output");
        let v = seed(&source);
        for (volume, policy) in [(&"4".repeat(32), POLICY), (&v.id, "exact-utf8-v1")] {
            assert!(create(&source, volume, policy, &target, &Limits::default()).is_err());
            assert!(!target.exists());
        }
        let live = State::open_existing_authority(&source, &v.id, POLICY).unwrap();
        assert_eq!(
            create(&source, &v.id, POLICY, &target, &Limits::default()),
            Err(Error::Source)
        );
        drop(live);
        let limits = Limits {
            max_bytes: 4096,
            ..Limits::default()
        };
        assert_eq!(
            create(&source, &v.id, POLICY, &target, &limits),
            Err(Error::Limit)
        );
        assert!(!target.exists());
        let limits = Limits {
            max_bytes: 0,
            ..Limits::default()
        };
        assert_eq!(
            create(&source, &v.id, POLICY, &target, &limits),
            Err(Error::InvalidLimits)
        );
        let cancelled = Limits::default();
        cancelled.cancelled.store(true, Ordering::Relaxed);
        assert_eq!(
            create(&source, &v.id, POLICY, &target, &cancelled),
            Err(Error::Cancelled)
        );
        for sql in [
            "PRAGMA application_id=99".to_owned(),
            format!("PRAGMA user_version={}", SCHEMA_VERSION + 1),
            "INSERT INTO client_identity VALUES(1,'44444444444444444444444444444444',zeroblob(32))"
                .to_owned(),
            "INSERT INTO sparse_view_role VALUES(1,1,zeroblob(32))".to_owned(),
            "PRAGMA foreign_keys=OFF; INSERT INTO sparse_view_files VALUES(zeroblob(32),x'7b',zeroblob(32),zeroblob(32))".to_owned(),
            "PRAGMA foreign_keys=OFF; INSERT INTO sparse_views VALUES(zeroblob(32),zeroblob(32),x'7b',zeroblob(32))".to_owned(),
            "PRAGMA foreign_keys=OFF; INSERT INTO sparse_view_operations VALUES(zeroblob(32),zeroblob(32),x'7b',zeroblob(32))".to_owned(),
            "INSERT INTO sparse_view_blocks VALUES(zeroblob(32),x'01')".to_owned(),
            "PRAGMA foreign_keys=OFF; INSERT INTO sparse_preparations VALUES(zeroblob(32),zeroblob(32),zeroblob(32),x'7b',x'7b',zeroblob(32))".to_owned(),
            "PRAGMA foreign_keys=OFF; INSERT INTO sparse_preparation_objects VALUES(zeroblob(32),-1,0,x'7b',zeroblob(32))".to_owned(),
            "INSERT INTO sparse_preparation_blobs VALUES(zeroblob(32),x'01')".to_owned(),
            "INSERT INTO sparse_scoped_mode VALUES(1,1,zeroblob(32))".to_owned(),
            "INSERT INTO source_read_pin_session VALUES(1,1,'01010101010101010101010101010101',0,0,zeroblob(32),zeroblob(32))".to_owned(),
            "INSERT INTO sparse_publication_role VALUES(1,0,0,zeroblob(32))".to_owned(),
            "PRAGMA foreign_keys=OFF; INSERT INTO sparse_publications VALUES(zeroblob(32),'11111111111111111111111111111111','22222222222222222222222222222222',x'7b',zeroblob(32))".to_owned(),
            "PRAGMA foreign_keys=OFF; INSERT INTO sparse_publication_checks VALUES(zeroblob(32),0,x'7b',zeroblob(32))".to_owned(),
        ] {
            let case = tempfile::tempdir().unwrap();
            let source = case.path().join("bad");
            let v = seed(&source);
            let db = Connection::open(source.join("metadata.sqlite3")).unwrap();
            db.execute_batch(&sql).unwrap();
            drop(db);
            let output = case.path().join("out");
            assert!(create(&source, &v.id, POLICY, &output, &Limits::default()).is_err());
            assert!(!output.exists());
        }
    }
    #[test]
    fn strict_manifest_hash_and_recovery_entry_checks_never_echo_private_input() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let v = seed(&source);
        for case in [
            "digest", "unknown", "filename", "future", "schema", "policy", "sidecar", "bytes",
            "missing",
        ] {
            let target = tmp.path().join(case);
            create(&source, &v.id, POLICY, &target, &Limits::default()).unwrap();
            let path = target.join("snapshot.json");
            let mut m: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            match case {
                "digest" => m["sha256"] = "0".repeat(64).into(),
                "unknown" => m["private marker"] = "never echo this configuration".into(),
                "filename" => m["database"] = "../private marker".into(),
                "future" => m["version"] = 2.into(),
                "schema" => m["schema_version"] = (SCHEMA_VERSION + 1).into(),
                "policy" => m["volume"]["name_policy"] = "exact-utf8-v1".into(),
                "sidecar" => {
                    crate::create_private_document(
                        &target,
                        "snapshot.sqlite3-wal",
                        b"private marker",
                    )
                    .unwrap();
                }
                "bytes" => {
                    let db = target.join("snapshot.sqlite3");
                    let mut bytes = fs::read(&db).unwrap();
                    bytes[100] ^= 1;
                    fs::write(db, bytes).unwrap();
                }
                "missing" => {
                    fs::remove_file(target.join("snapshot.sqlite3")).unwrap();
                }
                _ => unreachable!(),
            }
            fs::write(path, serde_json::to_vec(&m).unwrap()).unwrap();
            let error = inspect(&target, &v.id, POLICY, &Limits::default()).unwrap_err();
            assert!(!format!("{error:?} {error}").contains("private marker"));
            assert!(std::error::Error::source(&error).is_none());
        }
    }
    #[test]
    fn failures_and_cancellation_preserve_source_and_distinguish_uncertain_completion() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let v = seed(&source);
        let before = records(&source.join("metadata.sqlite3"));
        for stage in [
            Stage::Created,
            Stage::Copying,
            Stage::Copied,
            Stage::Validated,
            Stage::DataSynced,
            Stage::ManifestWritten,
            Stage::ManifestSynced,
            Stage::Published,
        ] {
            let target = tmp.path().join(format!("{stage:?}"));
            let result = create_inner(
                &source,
                &v.id,
                POLICY,
                &target,
                &Limits::default(),
                &mut |at, _| {
                    if at == stage {
                        Err(Error::Cancelled)
                    } else {
                        Ok(())
                    }
                },
            );
            assert!(result.is_err());
            let late = matches!(
                stage,
                Stage::ManifestWritten | Stage::ManifestSynced | Stage::Published
            );
            assert_eq!(
                result.unwrap_err(),
                if late {
                    Error::CompletionUnknown
                } else {
                    Error::Cancelled
                }
            );
            assert_eq!(
                inspect(&target, &v.id, POLICY, &Limits::default()).is_ok(),
                late
            );
            assert_eq!(records(&source.join("metadata.sqlite3")), before);
        }
        let target = tmp.path().join("full");
        let result = create_inner(
            &source,
            &v.id,
            POLICY,
            &target,
            &Limits::default(),
            &mut |stage, db| {
                if stage == Stage::Copying
                    && let Some(db) = db
                {
                    db.pragma_update(None, "max_page_count", 1).unwrap();
                }
                Ok(())
            },
        );
        assert_eq!(result, Err(Error::Copy));
        assert!(inspect(&target, &v.id, POLICY, &Limits::default()).is_err());
        assert_eq!(records(&source.join("metadata.sqlite3")), before);
        let options = Limits {
            timeout: Duration::from_millis(50),
            ..Limits::default()
        };
        let target = tmp.path().join("deadline");
        let started = Instant::now();
        let result = create_inner(
            &source,
            &v.id,
            POLICY,
            &target,
            &options,
            &mut |stage, _| {
                if stage == Stage::Created {
                    std::thread::sleep(Duration::from_millis(70));
                }
                Ok(())
            },
        );
        // Exercise the same normalization as the public entry point: on a
        // slow host the deadline may expire during validation before the hook.
        assert_eq!(normalize(result, &options, started), Err(Error::Deadline));
    }
    #[test]
    fn actual_schema_eight_is_copied_without_migration() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("legacy");
        let owner = OwnedDirectory::acquire(&source).unwrap();
        let db = Connection::open(owner.database_path()).unwrap();
        for sql in [
            include_str!("../../migrations/001.sql"),
            include_str!("../../migrations/002.sql"),
            include_str!("../../migrations/003.sql"),
            include_str!("../../migrations/004.sql"),
            include_str!("../../migrations/005.sql"),
            include_str!("../../migrations/006.sql"),
            include_str!("../../migrations/007.sql"),
            include_str!("../../migrations/008.sql"),
        ] {
            db.execute_batch(sql).unwrap();
        }
        db.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        crate::test_schema::demote(&db, 8).unwrap();
        let volume = "4".repeat(32);
        let request = Apply {
            volume: volume.clone(),
            operation: "1".repeat(32),
            expected_sequence: 0,
            mutation: Mutation::Create {
                parent: ROOT.into(),
                name: "old 資料 café 🦀".into(),
                node_kind: Kind::Directory,
            },
        };
        let receipt = namespace::Receipt {
            sequence: 1,
            node: namespace::Node {
                id: request.operation.clone(),
                parent: Some(ROOT.into()),
                name: "old 資料 café 🦀".into(),
                kind: Kind::Directory,
                revision: 1,
                deleted: false,
                payload: None,
                placement: None,
            },
            artifact: None,
        };
        db.execute(
            "INSERT INTO namespace_volume VALUES(1,?1,1,'exact-utf8-v1')",
            [&volume],
        )
        .unwrap();
        db.execute(
            "INSERT INTO namespace_nodes VALUES(?1,NULL,'','directory',0,0)",
            [ROOT],
        )
        .unwrap();
        db.execute(
            "INSERT INTO namespace_nodes VALUES(?1,?2,?3,'directory',1,0)",
            rusqlite::params![request.operation, ROOT, receipt.node.name],
        )
        .unwrap();
        let outcome: std::result::Result<namespace::Receipt, namespace::Error> = Ok(receipt);
        // Distinct formatting proves backup does not decode/re-encode receipts.
        db.execute(
            "INSERT INTO namespace_operations VALUES(?1,?2,?3)",
            rusqlite::params![
                request.operation,
                serde_json::to_vec_pretty(&request).unwrap(),
                serde_json::to_vec_pretty(&outcome).unwrap(),
            ],
        )
        .unwrap();
        drop(db);
        owner.finish_initialization().unwrap();
        drop(owner);
        let before = records(&source.join("metadata.sqlite3"));
        assert_eq!(before.1.len(), 1);
        let target = tmp.path().join("snapshot");
        let m = create(
            &source,
            &volume,
            "exact-utf8-v1",
            &target,
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(m.schema_version, 8);
        assert_eq!(m.volume.sequence, 1);
        assert_eq!(records(&source.join("metadata.sqlite3")), before);
        assert_eq!(records(&target.join("snapshot.sqlite3")), before);
        assert!(inspect(&target, &volume, "exact-utf8-v1", &Limits::default()).is_ok());
    }
    struct ChildOwner(Child);
    impl Drop for ChildOwner {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn snapshot_crash_child() {
        let Some(source) = std::env::var_os("S3DRIVE_SNAPSHOT_CRASH_SOURCE") else {
            return;
        };
        let output = std::env::var_os("S3DRIVE_SNAPSHOT_CRASH_OUTPUT").unwrap();
        let volume = std::env::var("S3DRIVE_SNAPSHOT_CRASH_VOLUME").unwrap();
        let stop = std::env::var("S3DRIVE_SNAPSHOT_CRASH_STAGE").unwrap();
        let result = create_inner(
            Path::new(&source),
            &volume,
            POLICY,
            Path::new(&output),
            &Limits::default(),
            &mut |stage, db| {
                // Copying/None occurs after a real nonfinal backup.step, rather
                // than before the first page copy (Copying/Some).
                if format!("{stage:?}") == stop && (stage != Stage::Copying || db.is_none()) {
                    println!("SNAPSHOT_STAGE {stage:?}");
                    std::io::Write::flush(&mut std::io::stdout()).unwrap();
                    let mut line = String::new();
                    std::io::stdin().lock().read_line(&mut line).unwrap();
                    panic!("crash fixture was released without being killed");
                }
                Ok(())
            },
        );
        panic!("crash fixture returned without reaching boundary: {result:?}");
    }
    #[test]
    fn killed_snapshot_processes_leave_inspectable_or_explicit_partial_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source 資料 café 🦀");
        let v = seed(&source);
        let before = records(&source.join("metadata.sqlite3"));
        {
            let db = Connection::open_with_flags(
                source.join("metadata.sqlite3"),
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            let pages: i64 = db
                .pragma_query_value(None, "page_count", |r| r.get(0))
                .unwrap();
            assert!(pages > 32, "fixture must exercise a nonfinal backup step");
        }
        for stage in [
            Stage::Created,
            Stage::Copying,
            Stage::Copied,
            Stage::Validated,
            Stage::DataSynced,
            Stage::ManifestWritten,
            Stage::ManifestSynced,
            Stage::Published,
        ] {
            let output = tmp.path().join(format!("snapshot-{stage:?}"));
            let mut child = ChildOwner(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "authority_snapshot::native::tests::snapshot_crash_child",
                        "--nocapture",
                    ])
                    .env("S3DRIVE_SNAPSHOT_CRASH_SOURCE", &source)
                    .env("S3DRIVE_SNAPSHOT_CRASH_OUTPUT", &output)
                    .env("S3DRIVE_SNAPSHOT_CRASH_VOLUME", &v.id)
                    .env("S3DRIVE_SNAPSHOT_CRASH_STAGE", format!("{stage:?}"))
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap(),
            );
            let stdout = child.0.stdout.take().unwrap();
            let (send, receive) = std::sync::mpsc::channel();
            let reader = std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    if let Some(at) = line.unwrap().strip_prefix("SNAPSHOT_STAGE ") {
                        let _ = send.send(at.to_owned());
                        break;
                    }
                }
            });
            let reached = receive.recv_timeout(Duration::from_secs(20));
            // Always kill/reap the owned process and release the reader before
            // asserting, including a missing-boundary or failed worker case.
            let _ = child.0.kill();
            let status = child.0.wait().unwrap();
            reader.join().unwrap();
            let mut diagnostic = String::new();
            child
                .0
                .stderr
                .take()
                .unwrap()
                .read_to_string(&mut diagnostic)
                .unwrap();
            assert_eq!(
                reached.as_deref(),
                Ok(format!("{stage:?}").as_str()),
                "worker boundary: {diagnostic}"
            );
            assert!(!status.success(), "killed worker falsely returned success");
            assert_eq!(records(&source.join("metadata.sqlite3")), before);
            let complete_bytes = matches!(
                stage,
                Stage::ManifestWritten | Stage::ManifestSynced | Stage::Published
            );
            let result = inspect(&output, &v.id, POLICY, &Limits::default());
            assert_eq!(result.is_ok(), complete_bytes, "{stage:?}: {result:?}");
            if let Ok(inspection) = result {
                assert_eq!(inspection.manifest.volume, v);
                assert!(!inspection.history_continuity_proven && !inspection.restore_authorized);
                assert_eq!(records(&output.join("snapshot.sqlite3")), before);
            }
            assert!(State::open_existing_authority(&output, &v.id, POLICY).is_err());
            assert!(!output.join("metadata.sqlite3").exists());
        }
        // Process termination tests do not establish power-loss durability.
    }
    #[test]
    fn killed_owner_wal_only_namespace_and_receipt_are_included() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let mut child = ChildOwner(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "authority_restart_tests::existing_authority_wal_child",
                    "--nocapture",
                ])
                .env("S3DRIVE_AUTHORITY_WAL_CHILD", &source)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let stdout = child.0.stdout.take().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.unwrap();
                if let Some(raw) = line.strip_prefix("AUTHORITY_WAL ") {
                    send.send(raw.to_owned()).unwrap();
                    break;
                }
            }
        });
        let raw = receive.recv_timeout(Duration::from_secs(20)).unwrap();
        reader.join().unwrap();
        let (v, request, receipt): (namespace::Volume, Apply, namespace::Receipt) =
            serde_json::from_str(&raw).unwrap();
        let target = tmp.path().join("snapshot");
        assert_eq!(
            create(&source, &v.id, POLICY, &target, &Limits::default()),
            Err(Error::Source)
        );
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        assert!(
            fs::metadata(source.join("metadata.sqlite3-wal"))
                .unwrap()
                .len()
                > 32
        );
        let base = Connection::open_with_flags(
            format!(
                "file:{}?immutable=1",
                source.join("metadata.sqlite3").display()
            ),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .unwrap();
        let count: i64 = base
            .query_row("SELECT count(*) FROM namespace_volume", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        drop(base);
        let m = create(&source, &v.id, POLICY, &target, &Limits::default()).unwrap();
        assert_eq!(m.volume.sequence, 1);
        let db = open_read(
            &target.join("snapshot.sqlite3"),
            &Control::new(&Limits::default()).unwrap(),
        )
        .unwrap();
        let outcome: Vec<u8> = db
            .query_row(
                "SELECT outcome FROM namespace_operations WHERE operation_id=?1",
                [request.operation],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<std::result::Result<namespace::Receipt, namespace::Error>>(
                &outcome
            )
            .unwrap(),
            Ok(receipt)
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn private_mode_links_and_source_path_aliases_refuse() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let v = seed(&source);
        let target = tmp.path().join("snapshot");
        create(&source, &v.id, POLICY, &target, &Limits::default()).unwrap();
        fs::set_permissions(
            target.join("snapshot.sqlite3"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(inspect(&target, &v.id, POLICY, &Limits::default()).is_err());
        fs::set_permissions(
            target.join("snapshot.sqlite3"),
            fs::Permissions::from_mode(0o400),
        )
        .unwrap();
        assert!(inspect(&target, &v.id, POLICY, &Limits::default()).is_ok());
        fs::hard_link(target.join("snapshot.sqlite3"), tmp.path().join("linked")).unwrap();
        assert!(inspect(&target, &v.id, POLICY, &Limits::default()).is_err());
        symlink(&source, tmp.path().join("alias")).unwrap();
        assert!(
            create(
                &tmp.path().join("alias"),
                &v.id,
                POLICY,
                &tmp.path().join("other"),
                &Limits::default()
            )
            .is_err()
        );
        symlink(&source, tmp.path().join("parent-alias")).unwrap();
        assert!(
            create(
                &source,
                &v.id,
                POLICY,
                &tmp.path().join("parent-alias").join("nested"),
                &Limits::default()
            )
            .is_err()
        );
        assert!(!source.join("nested").exists());
    }
    #[test]
    fn empty_creation_tables_are_allowed_but_each_nonempty_client_table_refuses_snapshot() {
        for table in [
            "manifest_creation_role",
            "manifest_creations",
            "chunk_mount_role",
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let source = tmp.path().join("authority");
            let target = tmp.path().join("snapshot");
            let v = seed(&source);
            let empty = tmp.path().join("empty-local-tables");
            let m = create(&source, &v.id, POLICY, &empty, &Limits::default()).unwrap();
            assert_eq!(m.schema_version, SCHEMA_VERSION);
            let db = Connection::open(source.join("metadata.sqlite3")).unwrap();
            match table {
                "chunk_mount_role" => {
                    db.execute(
                        "INSERT INTO chunk_mount_role VALUES(1,x'7b7d',zeroblob(32))",
                        [],
                    )
                    .unwrap();
                }
                "manifest_creation_role" => {
                    db.execute(
                        "INSERT INTO manifest_creation_role VALUES(1,1,zeroblob(32))",
                        [],
                    )
                    .unwrap();
                }
                _ => {
                    db.execute(
                        "INSERT INTO manifest_creations VALUES(?1,x'7b7d',zeroblob(32))",
                        ["a".repeat(32)],
                    )
                    .unwrap();
                }
            }
            drop(db);
            let before = records(&source.join("metadata.sqlite3"));
            assert_eq!(
                create(&source, &v.id, POLICY, &target, &Limits::default()),
                Err(Error::Validation)
            );
            assert!(!target.exists());
            assert_eq!(records(&source.join("metadata.sqlite3")), before);
            let db = Connection::open(source.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
    }
}
