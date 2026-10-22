//! Existing-only chunk namespace/open metadata. Never admits an effect, invents
//! callbacks, changes sessions, reads source bodies or acknowledges source data.
use super::*;
use crate::{chunk_mount, directory::OwnedDirectory};
use std::{path::Path, time::Duration};
use tokio::sync::{mpsc, oneshot};
const DEADLINE: Duration = Duration::from_secs(30);

pub struct RecoveryStore {
    connection: Connection,
    key: chunk_mount::Key,
    _directory: OwnedDirectory,
}
pub(super) fn metadata(db: &Connection, key: &chunk_mount::Key) -> Result<()> {
    let app: i32 = db.pragma_query_value(None, "application_id", |r| r.get(0))?;
    if app != crate::APPLICATION_ID {
        return Err("namespace recovery application mismatch".into());
    }
    check_schema(db)?;
    // This checks the whole retained namespace/open/creator role and sparse
    // binding headers, but never source or staged payload BLOBs.
    chunk_mount::require(db, key)?;
    Ok(())
}
impl RecoveryStore {
    pub fn open(directory: &Path, volume: &str) -> Result<Self> {
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
            return Err("namespace recovery requires existing WAL storage".into());
        }
        let tx = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let binding = chunk_mount::load(&tx)?.ok_or("namespace recovery requires chunk role")?;
        binding.volume(volume)?;
        let key = binding.key();
        metadata(&tx, &key)?;
        tx.commit()?;
        Ok(Self {
            connection,
            key,
            _directory: owner,
        })
    }
    pub fn get(&mut self, operation: &str) -> Result<Record> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        metadata(&tx, &self.key)?;
        let record = load(&tx, operation)?.ok_or("namespace intent not saved")?;
        tx.commit()?;
        Ok(record)
    }
    pub fn records(&mut self) -> Result<Vec<Record>> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        metadata(&tx, &self.key)?;
        let ids = tx
            .prepare("SELECT operation_id FROM namespace_intents ORDER BY operation_id LIMIT 4097")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.len() > LIMIT as usize {
            return Err("namespace history limit".into());
        }
        let records = ids
            .iter()
            .map(|id| load(&tx, id)?.ok_or_else(|| "namespace intent missing".into()))
            .collect::<Result<Vec<_>>>()?;
        tx.commit()?;
        Ok(records)
    }
    pub fn open_record(&mut self, id: &str) -> Result<opens::OpenRecord> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        metadata(&tx, &self.key)?;
        let record = opens::sparse_open(&tx, id)?;
        tx.commit()?;
        Ok(record)
    }
    /// Only acknowledge this exact persisted request. No input-admission API.
    pub fn acknowledge(
        &mut self,
        request: &Apply,
        outcome: namespace::Result<Receipt>,
    ) -> Result<Record> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        metadata(&tx, &self.key)?;
        let mut record = load(&tx, &request.operation)?.ok_or("namespace intent not saved")?;
        if &record.input.request != request {
            return Err("namespace recovery request changed".into());
        }
        checked_outcome(&record.input, &outcome)?;
        if let Some(old) = &record.outcome {
            if old != &outcome {
                return Err("namespace recovery outcome changed".into());
            }
            return Ok(record);
        }
        // checked_outcome validates source/destination metadata against all
        // frozen entries. Never run fresh dirty-head guards or rebase source.
        record.outcome = Some(outcome);
        let count = tx.execute("UPDATE namespace_intents SET outcome=?1,state_hash=?2 WHERE operation_id=?3 AND outcome IS NULL",
            params![serde_json::to_vec(record.outcome.as_ref().unwrap())?, hash(&record)?, request.operation])?;
        if count != 1 || load(&tx, &request.operation)?.as_ref() != Some(&record) {
            return Err("namespace recovery acknowledgment readback mismatch".into());
        }
        removals::check_capacity(&tx, &removals::records(&tx)?)?;
        metadata(&tx, &self.key)?;
        checkpoint("OutcomeStored");
        tx.commit()?;
        checkpoint("OutcomeCommitted");
        Ok(record)
    }
    /// Only a retained Link can gain an exact namespace-outcome acknowledgment.
    /// No activation, callback, reply, completion, retirement or session API.
    pub fn acknowledge_open(
        &mut self,
        open: &str,
        event: &str,
        operation: &str,
    ) -> Result<opens::OpenRecord> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        metadata(&tx, &self.key)?;
        let record = opens::sparse_open(&tx, open)?;
        let input = record
            .events
            .iter()
            .find_map(|e| match &e.action {
                opens::Action::Link { input, .. } if input.request.operation == operation => {
                    Some(input.as_ref())
                }
                _ => None,
            })
            .ok_or("saved open effect link missing")?;
        let saved = load(&tx, operation)?.ok_or("namespace intent not saved")?;
        if saved.input != *input || saved.outcome.is_none() {
            return Err("open effect has no exact terminal outcome".into());
        }
        let action = opens::Event {
            id: event.into(),
            action: opens::Action::Acknowledge {
                operation: operation.into(),
                outcome_hash: hash(&saved)?,
            },
        };
        tx.commit()?;
        let result =
            opens::append_existing(&mut self.connection, Some(&self.key), open, action, true)?;
        // append_existing independently revalidates the exact outcome inside
        // its write transaction and rejects retired/frozen or changed history.
        Ok(result)
    }
}
#[cfg(test)]
pub(super) fn checkpoint(point: &str) {
    if std::env::var("S3DRIVE_NAMESPACE_RECOVERY_STOP").as_deref() == Ok(point) {
        use std::io::Read;
        let path =
            std::path::PathBuf::from(std::env::var_os("S3DRIVE_NAMESPACE_RECOVERY_READY").unwrap());
        let temp = path.with_extension("tmp");
        std::fs::write(&temp, point).unwrap();
        std::fs::rename(temp, path).unwrap();
        std::io::stdin().read_exact(&mut [0u8; 1]).unwrap();
    }
}
#[cfg(not(test))]
pub(super) fn checkpoint(_: &str) {}

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
    pub async fn open(directory: &Path, volume: String) -> Result<Self> {
        let directory = directory.to_path_buf();
        let (sender, mut receiver) = mpsc::channel(8);
        let (ready, started) = oneshot::channel();
        std::thread::Builder::new()
            .name("s3drive-namespace-recovery".into())
            .spawn(move || {
                let mut store = match RecoveryStore::open(&directory, &volume) {
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
    pub async fn get(&self, operation: String) -> Result<Record> {
        self.call(move |s| s.get(&operation)).await
    }
    pub async fn records(&self) -> Result<Vec<Record>> {
        self.call(|s| s.records()).await
    }
    pub async fn open_record(&self, id: String) -> Result<opens::OpenRecord> {
        self.call(move |s| s.open_record(&id)).await
    }
    pub async fn acknowledge(
        &self,
        request: Apply,
        outcome: namespace::Result<Receipt>,
    ) -> Result<Record> {
        self.call(move |s| s.acknowledge(&request, outcome)).await
    }
    pub async fn acknowledge_open(
        &self,
        open: String,
        event: String,
        operation: String,
    ) -> Result<opens::OpenRecord> {
        self.call(move |s| s.acknowledge_open(&open, &event, &operation))
            .await
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

#[cfg(test)]
mod tests;
