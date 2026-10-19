//! Existing-only metadata capability. No State, migrations, source-body reads,
//! staging, new intent, physical claims or metadata request preparation.
use super::*;
use crate::directory::OwnedDirectory;
use std::{path::Path, time::Duration};
use tokio::sync::{mpsc, oneshot};
const DEADLINE: Duration = Duration::from_secs(30);
pub struct RecoveryStore {
    connection: Connection,
    _directory: OwnedDirectory,
}
fn metadata(db: &Connection) -> Result<BTreeMap<String, Record>> {
    let app: i32 = db.pragma_query_value(None, "application_id", |r| r.get(0))?;
    if app != crate::APPLICATION_ID {
        return Err(Failure::Invalid.into());
    }
    preflight(db)?;
    check_schema(db)?;
    isolation(db)?;
    let rows = inspect(db)?;
    if rows.is_empty() {
        return Err(Failure::Missing.into());
    }
    Ok(rows)
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
        Ok(Self {
            connection,
            _directory: owner,
        })
    }
    pub fn get(&mut self, id: &str) -> Result<Record> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let rows = metadata(&tx)?;
        let r = rows.get(id).cloned().ok_or(Failure::Missing)?;
        tx.commit()?;
        Ok(r)
    }
    pub fn records(&mut self) -> Result<Vec<Record>> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let rows = metadata(&tx)?.into_values().collect();
        tx.commit()?;
        Ok(rows)
    }
    fn update(&mut self, id: &str, f: impl FnOnce(&mut Record) -> Result<()>) -> Result<Record> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut rows = metadata(&tx)?;
        let mut r = rows.get(id).cloned().ok_or(Failure::Missing)?;
        let before = r.clone();
        f(&mut r)?;
        if r != before {
            save(&tx, &mut rows, r.clone(), false)?;
        }
        if metadata(&tx)? != rows {
            return Err(Failure::Corrupt.into());
        }
        tx.commit()?;
        Ok(r)
    }
    pub fn acknowledge(
        &mut self,
        id: &str,
        request: &ns::Apply,
        receipt: ns::Receipt,
    ) -> Result<Record> {
        self.update(id, |r| acknowledge_record(r, request, receipt))
    }
    pub fn reject(&mut self, id: &str, request: &ns::Apply, error: ns::Error) -> Result<Record> {
        self.update(id, |r| reject_record(r, request, error))
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
            .name("s3drive-create-recovery".into())
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
    pub async fn get(&self, id: String) -> Result<Record> {
        self.call(move |s| s.get(&id)).await
    }
    pub async fn records(&self) -> Result<Vec<Record>> {
        self.call(|s| s.records()).await
    }
    pub async fn acknowledge(
        &self,
        id: String,
        request: ns::Apply,
        receipt: ns::Receipt,
    ) -> Result<Record> {
        self.call(move |s| s.acknowledge(&id, &request, receipt))
            .await
    }
    pub async fn reject(&self, id: String, request: ns::Apply, error: ns::Error) -> Result<Record> {
        self.call(move |s| s.reject(&id, &request, error)).await
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
