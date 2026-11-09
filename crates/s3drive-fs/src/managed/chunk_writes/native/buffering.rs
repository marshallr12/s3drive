//! Commit points for the per-node write buffer (`coalesce`) on the native path.
//!
//! Lock order is always `native_serial`, then the file's `serial`. Journaling a
//! buffered run needs both, exactly like the unbuffered write it replaces.
use super::*;
use coalesce::Offer;

impl Writes {
    /// Journals the node's buffered run as one write through the handle that
    /// accepted it. The caller holds `native_serial` and the file's `serial`.
    /// On failure the bytes stay buffered, so the next operation retries it.
    pub(super) async fn drain(&self, file: &File) -> Result<()> {
        let Some((open, offset, bytes)) = file.run()? else {
            return Ok(());
        };
        let owner = self.native_entry(&open)?;
        let head = file.head().await?;
        let edited = owner.edited(&head).await?;
        let next = owner
            .native
            .write(&edited, fresh()?, offset, bytes)
            .await
            .map_err(error)?;
        file.journaled(next.size())
    }
    /// Journals buffered writes of `node` if one of its handles holds any. The
    /// caller holds `native_serial`.
    pub(in super::super) async fn drain_node(&self, node: &str) -> Result<()> {
        let held = {
            let entries = self.entries.lock().map_err(|_| Error::Internal)?;
            entries.get(node).and_then(Weak::upgrade)
        };
        let Some(file) = held else {
            return Ok(());
        };
        let _file = file.serial.lock().await;
        self.drain(&file).await
    }
    /// The original handle that accepted a buffered run.
    fn native_entry(&self, open: &str) -> Result<Entry> {
        let mount = self.native.get().ok_or(Error::Internal)?;
        let opens = mount.opens.lock().map_err(|_| Error::Internal)?;
        opens.get(open).cloned().ok_or(Error::Conflict)
    }
    /// Starts the idle timer of a new run. The timer lives as long as the run:
    /// each time no write has extended the run for `coalesce::IDLE`, it tries
    /// once to journal that state without waiting for a commit point.
    fn watch(self: &Arc<Self>, file: Arc<File>) {
        let writer = Arc::downgrade(self);
        let Ok(Some((run, _))) = file.buffered_mark() else {
            return;
        };
        tokio::spawn(async move {
            // The last state offered to the journal; a refused state is not
            // offered again until a write changes it.
            let mut tried = None;
            while let Some(seen) = Self::idle_mark(&writer, &file, run).await {
                if tried != Some(seen) {
                    Self::drain_idle(&writer, &file, seen).await;
                    tried = Some(seen);
                }
            }
        });
    }
    /// Waits until run `run` has gone one idle period without a write and
    /// returns its mark. Returns `None` once the run is no longer buffered or
    /// the mount is stopping, which ends the timer.
    async fn idle_mark(writer: &Weak<Self>, file: &File, run: u64) -> Option<(u64, u64)> {
        loop {
            let seen = file.buffered_mark().ok().flatten().filter(|m| m.0 == run)?;
            tokio::time::sleep(coalesce::IDLE).await;
            let stopping = |w: Arc<Self>| w.closing.load(Ordering::Acquire);
            if writer.upgrade().is_none_or(stopping) {
                return None;
            }
            if file.buffered_mark().ok().flatten() == Some(seen) {
                return Some(seen);
            }
        }
    }
    /// Idle-timer commit, serialized with every other journal user by the two
    /// order locks. A failure is kept: the bytes stay buffered and the handle's
    /// next operation retries the journal write and reports it.
    async fn drain_idle(writer: &Weak<Self>, file: &File, seen: (u64, u64)) {
        let Some(writer) = writer.upgrade() else {
            return;
        };
        // No work slot is taken: callbacks must never see `Busy` because of this
        // timer. Unmount journals every buffer itself, so the timer stands down.
        if writer.closing.load(Ordering::Acquire) {
            return;
        }
        let _serial = writer.native_serial.lock().await;
        let _file = file.serial.lock().await;
        // While this timer waited for the locks the run changed. If a write
        // extended it, `watch` waits for it to go idle again; if it was
        // journaled, any newer run has its own timer.
        if file.buffered_mark().ok().flatten() != Some(seen) {
            return;
        }
        if let Err(failure) = writer.drain(file).await {
            eprintln!("Buffered write not journaled yet ({failure:?}); it stays buffered");
        }
    }
}

impl Object {
    /// Journals this node's buffered writes before an operation that reads or
    /// reshapes the journal. Takes the mount-wide order lock only when needed.
    pub(super) async fn settle(&self) -> Result<()> {
        if self.entry.file.buffered_size()?.is_none() {
            return Ok(());
        }
        let _serial = self.writer.native_serial.lock().await;
        let _file = self.entry.file.serial.lock().await;
        self.writer.drain(&self.entry.file).await
    }
    /// Close-time commit of a run this handle still owns. The capability can
    /// never write after Close, so a run the journal still refuses is discarded
    /// and reported on standard error. Keeping it would leave the handle, and
    /// with it the file, unable to close for the rest of the mount.
    pub(super) async fn settle_or_discard(&self) {
        let _serial = self.writer.native_serial.lock().await;
        let _file = self.entry.file.serial.lock().await;
        let Err(failure) = self.writer.drain(&self.entry.file).await else {
            return;
        };
        if let Ok(Some(lost)) = self.entry.file.discard(self.entry.native.open()) {
            eprintln!(
                "Discarded {lost} buffered bytes at Close: journal refused them ({failure:?})"
            );
        }
    }
    /// Tries to hold a write in memory. Returns false when the caller must
    /// journal it directly: it is empty, too large, or this handle no longer
    /// buffers. The caller holds `native_serial` and the file's `serial`.
    pub(super) async fn buffer(&self, size: u64, offset: u64, input: &[u8]) -> Result<bool> {
        let (file, open) = (&self.entry.file, self.entry.native.open());
        if !bufferable(offset, input.len()) {
            return Ok(false);
        }
        if file.obstructs(open, offset, input.len())? {
            self.writer.drain(file).await?;
        }
        match file.offer(open, &self.entry.direct, size, offset, input)? {
            Offer::Direct => return Ok(false),
            Offer::Started => self.writer.watch(file.clone()),
            Offer::Extended => {}
        }
        Ok(true)
    }
    /// Ends buffering for this handle (cleanup) and journals what it holds, so
    /// later writes on the handle reach the journal before they return.
    pub(super) async fn stop_buffering(&self) -> Result<()> {
        self.entry
            .file
            .retire(self.entry.native.open(), &self.entry.direct);
        self.settle().await
    }
}

/// Whether a write is small enough to buffer and ends inside the size limit.
/// Anything else takes the direct path and its existing error handling.
fn bufferable(offset: u64, count: usize) -> bool {
    count != 0
        && count < coalesce::CAPACITY
        && offset
            .checked_add(count as u64)
            .is_some_and(|end| end <= views::MAX_FILE_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_small_nonempty_writes_inside_the_size_limit_are_buffered() {
        assert!(bufferable(0, 1));
        assert!(bufferable(views::MAX_FILE_BYTES - 1, 1));
        assert!(!bufferable(0, 0), "empty write");
        assert!(!bufferable(0, coalesce::CAPACITY), "write at capacity");
        assert!(!bufferable(views::MAX_FILE_BYTES, 1), "past the size limit");
        assert!(!bufferable(u64::MAX, 2), "offset overflow");
    }
}
