//! Bounded clean snapshots. Callers must authorize every new open online before
//! constructing a key. This module performs no network I/O or authorization.
use crate::{Result, State};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fmt,
    path::Path,
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

pub const MAX_FILE_BYTES: usize = 1024 * 1024;
const MAX_ENTRIES: usize = 65_536;
const QUEUE_SIZE: usize = 8;
const DEADLINE: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Logical clean payload quota; SQLite pages, WAL and metadata are extra.
    pub max_bytes: u64,
    pub max_file_bytes: usize,
    pub max_entries: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_file_bytes: MAX_FILE_BYTES,
            max_entries: 4096,
        }
    }
}
impl Options {
    fn validate(self) -> Result<Self> {
        if !(1..=1024 * 1024 * 1024).contains(&self.max_bytes)
            || !(1..=MAX_FILE_BYTES).contains(&self.max_file_bytes)
            || self.max_file_bytes as u64 > self.max_bytes
            || !(1..=MAX_ENTRIES).contains(&self.max_entries)
        {
            return Err("invalid clean cache limits".into());
        }
        Ok(self)
    }
}

/// A private cache address, never an authorization token. All strings are hashed
/// with lengths and option tags; remote object names are not local paths.
#[derive(Clone)]
pub struct Key {
    digest: [u8; 32],
    source: [u8; 32],
    fingerprint: String,
    size: u64,
}
impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CacheKey([redacted])")
    }
}
impl Key {
    pub fn new(
        source: [u8; 32],
        fingerprint: &str,
        object_key: &str,
        etag: Option<&str>,
        version: Option<&str>,
        size: u64,
    ) -> Result<Self> {
        if object_key.is_empty()
            || object_key.len() > 1024
            || (etag.is_none() && version.is_none())
            || [etag, version]
                .into_iter()
                .flatten()
                .any(|s| s.is_empty() || s.len() > 8192)
            || fingerprint.len() != 64
            || !fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid authorized cache key metadata".into());
        }
        let mut hash = Sha256::new();
        hash.update(b"s3drive-clean-blob-v1\0");
        hash.update(source);
        for value in [Some(fingerprint), Some(object_key), etag, version] {
            hash.update([u8::from(value.is_some())]);
            if let Some(value) = value {
                hash.update((value.len() as u64).to_be_bytes());
                hash.update(value.as_bytes());
            }
        }
        hash.update(size.to_be_bytes());
        Ok(Self {
            digest: hash.finalize().into(),
            source,
            fingerprint: fingerprint.into(),
            size,
        })
    }

    /// Stable stripe selection for callers coalescing downloads. Not identity
    /// information and not a substitute for online authorization.
    pub fn stripe(&self) -> usize {
        usize::from(self.digest[0] & 15)
    }
}

/// An immutable, checksum-verified snapshot. Keeping any clone alive pins its
/// database entry against eviction. The same entry shares one payload buffer.
#[derive(Clone)]
pub struct Pinned(Arc<[u8]>);
impl Pinned {
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

enum Request {
    Get(Key, oneshot::Sender<Result<Option<Pinned>>>),
    Put(Key, Arc<[u8]>, oneshot::Sender<Result<Option<Pinned>>>),
    Close(oneshot::Sender<()>),
}

/// A bounded command queue to one dedicated OS thread. SQLite and checksumming
/// never run on the async executor. No SQL transaction includes a remote read.
#[derive(Clone)]
pub struct Cache {
    sender: mpsc::Sender<Request>,
    options: Options,
}
impl Cache {
    pub async fn open(directory: &Path, options: Options) -> Result<Self> {
        let options = options.validate()?;
        let directory = directory.to_path_buf();
        let (sender, mut receiver) = mpsc::channel(QUEUE_SIZE);
        let (ready, started) = oneshot::channel();
        std::thread::Builder::new()
            .name("s3drive-cache".into())
            .spawn(move || {
                let mut store = match Store::open(&directory, options) {
                    Ok(store) => store,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                if ready.send(Ok(())).is_err() {
                    return;
                }
                let mut closing = None;
                while let Some(request) = receiver.blocking_recv() {
                    match request {
                        Request::Get(key, reply) => {
                            if !reply.is_closed() {
                                let _ = reply.send(store.get(&key));
                            }
                        }
                        Request::Put(key, bytes, reply) => {
                            if !reply.is_closed() {
                                let _ = reply.send(store.put(&key, bytes));
                            }
                        }
                        Request::Close(reply) => {
                            closing = Some(reply);
                            break;
                        }
                    }
                }
                // Signal completion only after closing SQLite and releasing its lock.
                drop(store);
                if let Some(reply) = closing {
                    let _ = reply.send(());
                }
            })?;
        tokio::time::timeout(DEADLINE, started)
            .await
            .map_err(|_| "cache initialization timed out")?
            .map_err(|_| "cache worker stopped during initialization")??;
        Ok(Self { sender, options })
    }

    pub fn accepts(&self, size: u64) -> bool {
        size <= self.options.max_file_bytes as u64
    }

    pub async fn get(&self, key: Key) -> Result<Option<Pinned>> {
        self.request(|reply| Request::Get(key, reply)).await
    }

    /// None means ineligible size or insufficient unpinned quota. SQL errors,
    /// including disk full, are explicit; no partially written entry is exposed.
    pub async fn put(&self, key: Key, bytes: Arc<[u8]>) -> Result<Option<Pinned>> {
        if key.size != bytes.len() as u64 {
            return Err("cache payload size mismatch".into());
        }
        if !self.accepts(key.size) {
            return Ok(None);
        }
        self.request(|reply| Request::Put(key, bytes, reply)).await
    }

    async fn request(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<Option<Pinned>>>) -> Request,
    ) -> Result<Option<Pinned>> {
        let (reply, receive) = oneshot::channel();
        // Backpressure is explicit: never accumulate an unbounded queue of
        // already downloaded file buffers while the local disk is slow.
        self.sender
            .try_send(build(reply))
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => "cache command queue is full",
                mpsc::error::TrySendError::Closed(_) => "cache worker is closed",
            })?;
        tokio::time::timeout(DEADLINE, receive)
            .await
            .map_err(|_| "cache operation timed out")?
            .map_err(|_| "cache worker stopped")?
    }

    /// Close all clones of this worker after filesystem callbacks have stopped.
    /// Dropping the last Cache also lets the worker drain and release ownership.
    pub async fn close(&self) -> Result<()> {
        let (reply, receive) = oneshot::channel();
        tokio::time::timeout(DEADLINE, async {
            self.sender
                .send(Request::Close(reply))
                .await
                .map_err(|_| "cache worker is closed")?;
            receive
                .await
                .map_err(|_| "cache worker stopped before close")
        })
        .await
        .map_err(|_| "cache close timed out")??;
        Ok(())
    }
}

struct Store {
    state: State,
    options: Options,
    pins: HashMap<[u8; 32], Weak<[u8]>>,
    clock: i64,
}
impl Store {
    fn open(path: &Path, options: Options) -> Result<Self> {
        let options = options.validate()?;
        let state = State::open_managed(path)?;
        let clock = state.connection.query_row(
            "SELECT coalesce(max(last_used),0) FROM clean_blobs",
            [],
            |r| r.get(0),
        )?;
        let mut store = Self {
            state,
            options,
            pins: HashMap::new(),
            clock,
        };
        let tx = store
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        // A lowered limit is applied before any snapshot can be pinned.
        tx.execute(
            "DELETE FROM clean_blobs WHERE size_bytes > ?1",
            [options.max_file_bytes as i64],
        )?;
        if !make_room(&tx, &store.pins, options, 0, 0)? {
            return Err("cannot enforce clean cache limits".into());
        }
        tx.commit()?;
        Ok(store)
    }

    fn next_tick(&mut self) -> Result<i64> {
        self.clock = self
            .clock
            .checked_add(1)
            .ok_or("cache LRU counter exhausted")?;
        Ok(self.clock)
    }

    fn get(&mut self, key: &Key) -> Result<Option<Pinned>> {
        if key.size > self.options.max_file_bytes as u64 {
            return Ok(None);
        }
        self.state.register_source(&key.source, &key.fingerprint)?;
        let bytes = if let Some(bytes) = self.pins.get(&key.digest).and_then(Weak::upgrade) {
            bytes
        } else {
            // Bound allocation before loading any BLOB, even with damaged rows.
            let shape: Option<(i64, i64, i64, bool)> = self.state.connection.query_row(
                "SELECT size_bytes, length(data), length(checksum), source_id=?2 FROM clean_blobs WHERE cache_key=?1",
                params![key.digest.as_slice(), key.source.as_slice()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            ).optional()?;
            let Some((size, length, checksum_length, source)) = shape else {
                return Ok(None);
            };
            if size != key.size as i64 || length != size || checksum_length != 32 || !source {
                self.discard(key)?;
                return Ok(None);
            }
            let (data, checksum): (Vec<u8>, Vec<u8>) = self.state.connection.query_row(
                "SELECT data, checksum FROM clean_blobs WHERE cache_key=?1",
                [key.digest.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if Sha256::digest(&data).as_slice() != checksum {
                self.discard(key)?;
                return Ok(None);
            }
            Arc::from(data)
        };
        let tick = self.next_tick()?;
        self.state.connection.execute(
            "UPDATE clean_blobs SET last_used=?1 WHERE cache_key=?2",
            params![tick, key.digest.as_slice()],
        )?;
        self.pins.retain(|_, weak| weak.strong_count() != 0);
        self.pins.insert(key.digest, Arc::downgrade(&bytes));
        Ok(Some(Pinned(bytes)))
    }

    fn discard(&mut self, key: &Key) -> Result<()> {
        self.state.connection.execute(
            "DELETE FROM clean_blobs WHERE cache_key=?1",
            [key.digest.as_slice()],
        )?;
        Ok(())
    }

    fn put(&mut self, key: &Key, bytes: Arc<[u8]>) -> Result<Option<Pinned>> {
        if bytes.len() as u64 != key.size {
            return Err("cache payload size mismatch".into());
        }
        if key.size > self.options.max_file_bytes as u64 {
            return Ok(None);
        }
        if let Some(existing) = self.get(key)? {
            return Ok(Some(existing));
        }
        let checksum = Sha256::digest(bytes.as_ref());
        let tick = self.next_tick()?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !make_room(&tx, &self.pins, self.options, key.size, 1)? {
            return Ok(None);
        }
        tx.execute(
            "INSERT INTO clean_blobs VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                key.digest.as_slice(),
                key.source.as_slice(),
                key.size as i64,
                checksum.as_slice(),
                bytes.as_ref(),
                tick,
            ],
        )?;
        tx.commit()?;
        self.pins.retain(|_, weak| weak.strong_count() != 0);
        self.pins.insert(key.digest, Arc::downgrade(&bytes));
        Ok(Some(Pinned(bytes)))
    }
}

fn make_room(
    tx: &Transaction<'_>,
    pins: &HashMap<[u8; 32], Weak<[u8]>>,
    options: Options,
    incoming: u64,
    entries: usize,
) -> Result<bool> {
    let (bytes, count): (i64, i64) = tx.query_row(
        "SELECT coalesce(sum(size_bytes),0), count(*) FROM clean_blobs",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut bytes = u64::try_from(bytes).map_err(|_| "invalid cache size accounting")?;
    let mut count = usize::try_from(count).map_err(|_| "invalid cache entry accounting")?;
    let fits = |bytes: u64, count: usize| {
        bytes <= options.max_bytes - incoming && count <= options.max_entries - entries
    };
    if fits(bytes, count) {
        return Ok(true);
    }
    if count > MAX_ENTRIES {
        return Err("clean cache entry count exceeds supported bound".into());
    }
    let mut victims = Vec::new();
    let mut statement =
        tx.prepare("SELECT cache_key, size_bytes FROM clean_blobs ORDER BY last_used, cache_key")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let digest: Vec<u8> = row.get(0)?;
        let digest: [u8; 32] = digest
            .try_into()
            .map_err(|_| "invalid stored cache address")?;
        if pins
            .get(&digest)
            .is_some_and(|weak| weak.strong_count() != 0)
        {
            continue;
        }
        bytes = bytes
            .checked_sub(
                u64::try_from(row.get::<_, i64>(1)?).map_err(|_| "invalid cache entry size")?,
            )
            .ok_or("invalid cache size accounting")?;
        count -= 1;
        victims.push(digest);
        if fits(bytes, count) {
            break;
        }
    }
    if !fits(bytes, count) {
        return Ok(false);
    }
    drop(rows);
    drop(statement);
    for digest in victims {
        tx.execute(
            "DELETE FROM clean_blobs WHERE cache_key=?1",
            [digest.as_slice()],
        )?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
