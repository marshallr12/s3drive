//! Existing-only metadata capability. Never opens State, reads source BLOBs,
//! migrates, admits work, stages objects or prepares authority mutations.
use super::*;
use crate::directory::OwnedDirectory;
use std::{path::Path, time::Duration};
use tokio::sync::{mpsc, oneshot};
const DEADLINE: Duration = Duration::from_secs(30);
pub struct RecoveryStore {
    connection: Connection,
    _directory: OwnedDirectory,
}
fn metadata(db: &Connection) -> Result<ledger::Meta> {
    let app: i32 = db.pragma_query_value(None, "application_id", |r| r.get(0))?;
    let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if app != crate::APPLICATION_ID || !matches!(version, 33..=crate::SCHEMA_VERSION) {
        return Err(Failure::Invalid.into());
    }
    check_schema(db)?;
    let tables:i64=db.query_row("SELECT count(*) FROM sqlite_schema WHERE type='table' AND name IN ('sparse_publication_role','sparse_publications','sparse_publication_checks','sparse_view_role','sparse_preparations')",[],|r|r.get(0))?;
    if tables != 5 {
        return Err(Failure::Corrupt.into());
    }
    crate::sparse_views::isolation(db)?;
    let roles: i64 = db.query_row("SELECT count(*) FROM sparse_view_role", [], |r| r.get(0))?;
    if roles != 1 {
        return Err(Failure::Corrupt.into());
    }
    let (single, format, sha): (i64, i64, Vec<u8>) = db.query_row(
        "SELECT singleton,format,state_hash FROM sparse_view_role",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if single != 1 || format != 1 || sha.as_slice() != crate::sparse_views::role_hash()?.as_slice()
    {
        return Err(Failure::Corrupt.into());
    }
    let meta = ledger::inspect(db)?;
    if version == 33 && meta.records.values().any(|r| r.input.predecessor.is_some()) {
        return Err(Failure::Corrupt.into());
    }
    if version < 35 && meta.records.values().any(|r| r.acknowledgment.is_some()) {
        return Err(Failure::Corrupt.into());
    }
    if meta.records.is_empty() {
        return Err(Failure::Missing.into());
    }
    for sql in [
        "SELECT EXISTS(SELECT 1 FROM sparse_publications p LEFT JOIN sparse_preparations s ON s.plan_id=p.plan_id WHERE s.plan_id IS NULL)",
        "SELECT EXISTS(SELECT 1 FROM sparse_publication_checks p LEFT JOIN sparse_preparations s ON s.plan_id=p.plan_id WHERE s.plan_id IS NULL)",
    ] {
        if db.query_row(sql, [], |r| r.get::<_, bool>(0))? {
            return Err(Failure::Corrupt.into());
        }
    }
    Ok(meta)
}
impl RecoveryStore {
    pub fn open(directory: &Path) -> Result<Self> {
        let owner = OwnedDirectory::acquire_existing(directory)?;
        owner.verify_staging_storage()?;
        let mut connection = Connection::open_with_flags(
            owner.database_path(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
                | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "trusted_schema", false)?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        let mode: String = connection.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(Failure::Invalid.into());
        }
        let tx = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
        metadata(&tx)?;
        tx.commit()?;
        // No initialization/migration, intent recovery, chmod or source-body checks.
        Ok(Self {
            connection,
            _directory: owner,
        })
    }
    pub fn get(&mut self, plan: Id) -> Result<Record> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let meta = metadata(&tx)?;
        let r = meta.records.get(&plan).ok_or(Failure::Missing)?.clone();
        tx.commit()?;
        Ok(r)
    }
    pub fn acknowledged(&mut self, plan: Id) -> Result<Option<SourceReceipt>> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let meta = metadata(&tx)?;
        let r = meta.records.get(&plan).ok_or(Failure::Missing)?;
        if r.acknowledgment.is_none() {
            return Ok(None);
        }
        settlement::validate_recovery(&tx, r)?;
        let receipt = SourceReceipt::from_record(r.clone())?;
        tx.commit()?;
        Ok(Some(receipt))
    }
    pub fn records(&mut self) -> Result<Vec<Record>> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let meta = metadata(&tx)?;
        let rows = meta.records.into_values().collect();
        tx.commit()?;
        Ok(rows)
    }
    fn update(&mut self, plan: Id, f: impl FnOnce(&mut Record) -> Result<()>) -> Result<Record> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let before = metadata(&tx)?;
        let mut after = before.clone();
        let r = after.records.get_mut(&plan).ok_or(Failure::Missing)?;
        f(r)?;
        r.validate()?;
        let result = r.clone();
        let kind = ledger::kind(&before, &after);
        ledger::save(&tx, &before, &after, false)?;
        if metadata(&tx)? != after {
            return Err(Failure::Corrupt.into());
        }
        tx.commit()?;
        checkpoint(kind, "Committed");
        Ok(result)
    }
    pub fn reserved(&mut self, plan: Id, receipt: ns::Receipt) -> Result<Record> {
        self.update(plan, |r| super::journal::reserved(r, receipt))
    }
    pub fn published(&mut self, plan: Id, receipt: ns::Receipt) -> Result<Record> {
        self.update(plan, |r| super::journal::published(r, receipt))
    }
    pub fn reject(&mut self, plan: Id, request: ns::Apply, error: ns::Error) -> Result<Record> {
        self.update(plan, |r| super::journal::reject(r, request, error))
    }
}
type Work = Box<dyn FnOnce(&mut RecoveryStore) + Send>;
enum Command {
    Work(Work),
    Close(oneshot::Sender<()>),
}
#[derive(Clone)]
pub struct RecoveryClient {
    sender: mpsc::Sender<Command>,
}
impl RecoveryClient {
    pub async fn open(directory: &Path) -> Result<Self> {
        let directory = directory.to_path_buf();
        let (sender, mut receiver) = mpsc::channel(8);
        let (ready, started) = oneshot::channel();
        std::thread::Builder::new()
            .name("s3drive-root-recovery".into())
            .spawn(move || {
                let mut store = match RecoveryStore::open(&directory) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready.send(Err(e));
                        return;
                    }
                };
                if ready.send(Ok(())).is_err() {
                    return;
                }
                let mut close = None;
                while let Some(command) = receiver.blocking_recv() {
                    match command {
                        Command::Work(work) => work(&mut store),
                        Command::Close(reply) => {
                            receiver.close();
                            close = Some(reply);
                            break;
                        }
                    }
                }
                drop(store);
                if let Some(reply) = close {
                    let _ = reply.send(());
                }
            })?;
        tokio::time::timeout(DEADLINE, started)
            .await
            .map_err(|_| crate::worker::Failure::OutcomeUnknown)?
            .map_err(|_| crate::worker::Failure::OutcomeUnknown)??;
        Ok(Self { sender })
    }
    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut RecoveryStore) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .try_send(Command::Work(Box::new(move |s| {
                if !reply.is_closed() {
                    let _ = reply.send(f(s));
                }
            })))
            .map_err(|e| match e {
                mpsc::error::TrySendError::Full(_) => crate::worker::Failure::QueueFull,
                mpsc::error::TrySendError::Closed(_) => crate::worker::Failure::Closed,
            })?;
        tokio::time::timeout(DEADLINE, receive)
            .await
            .map_err(|_| crate::worker::Failure::OutcomeUnknown)?
            .map_err(|_| crate::worker::Failure::OutcomeUnknown)?
    }
    pub async fn get(&self, plan: Id) -> Result<Record> {
        self.call(move |s| s.get(plan)).await
    }
    pub async fn acknowledged(&self, plan: Id) -> Result<Option<SourceReceipt>> {
        self.call(move |s| s.acknowledged(plan)).await
    }
    pub async fn records(&self) -> Result<Vec<Record>> {
        self.call(|s| s.records()).await
    }
    pub async fn reserved(&self, plan: Id, receipt: ns::Receipt) -> Result<Record> {
        self.call(move |s| s.reserved(plan, receipt)).await
    }
    pub async fn published(&self, plan: Id, receipt: ns::Receipt) -> Result<Record> {
        self.call(move |s| s.published(plan, receipt)).await
    }
    pub async fn reject(&self, plan: Id, request: ns::Apply, error: ns::Error) -> Result<Record> {
        self.call(move |s| s.reject(plan, request, error)).await
    }
    pub async fn close(&self) -> Result<()> {
        let (reply, receive) = oneshot::channel();
        tokio::time::timeout(DEADLINE, async {
            self.sender
                .send(Command::Close(reply))
                .await
                .map_err(|_| crate::worker::Failure::Closed)?;
            receive
                .await
                .map_err(|_| crate::worker::Failure::OutcomeUnknown)
        })
        .await
        .map_err(|_| crate::worker::Failure::OutcomeUnknown)??;
        Ok(())
    }
}
