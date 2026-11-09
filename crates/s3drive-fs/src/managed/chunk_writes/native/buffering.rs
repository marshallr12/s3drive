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
        #[cfg(test)]
        if coalesce::REFUSE_RUNS.load(Ordering::Acquire) {
            return Err(Error::NoSpace);
        }
        let carrier = self.carrier(file, &open)?;
        let head = file.head().await?;
        let edited = carrier.edited(&head).await?;
        let write = carrier.native.write(&edited, fresh()?, offset, bytes);
        file.journaled(write.await.map_err(error)?.size())?;
        self.release(file)
    }
    /// Journals the node's buffered run; when the journal is full, publishes
    /// what it already holds to make room and tries once more. The caller
    /// holds `native_serial`.
    pub(in super::super) async fn drain_making_room(&self, node: &str) -> Result<()> {
        match self.drain_node(node).await {
            Err(Error::NoSpace) => {}
            outcome => return outcome,
        }
        self.publish_node(node).await?;
        self.drain_node(node).await
    }
    /// Journals buffered writes of `node` if the node holds any. The caller
    /// holds `native_serial`. Type-erased for the same reason as `publish_node`.
    pub(in super::super) fn drain_node<'a>(
        &'a self,
        node: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let held = {
                let entries = self.entries.lock().map_err(|_| Error::Internal)?;
                entries.get(node).and_then(Weak::upgrade)
            };
            let Some(file) = held else {
                return Ok(());
            };
            let _file = file.serial.lock().await;
            self.drain(&file).await
        })
    }
    /// Retries every run kept after its handle closed and returns the first
    /// failure, so flush-all and unmount report bytes that are still unsaved.
    /// The caller holds `native_serial`.
    pub(in super::super) async fn drain_kept(&self) -> Result<()> {
        let mount = self.native.get().ok_or(Error::Internal)?;
        let nodes: Vec<String> = {
            let held = mount.kept.lock().map_err(|_| Error::Internal)?;
            held.keys().cloned().collect()
        };
        let mut unsaved = Ok(());
        for node in nodes {
            unsaved = unsaved.and(self.drain_node(&node).await);
        }
        unsaved
    }
    /// The open handle whose capability journals a run: the handle that
    /// accepted it or, for a run kept after that handle closed, any handle of
    /// the file that Close has not reached. With none, the bytes stay in
    /// memory and the caller gets a conflict.
    fn carrier(&self, file: &File, open: &str) -> Result<Entry> {
        let mount = self.native.get().ok_or(Error::Internal)?;
        let opens = mount.opens.lock().map_err(|_| Error::Internal)?;
        if !open.is_empty() {
            return opens.get(open).cloned().ok_or(Error::Conflict);
        }
        let open_handle = |e: &&Entry| e.close.state.load(Ordering::Acquire) == 0;
        let same_file = |e: &&Entry| std::ptr::eq(Arc::as_ptr(&e.file), file);
        let mut handles = opens.values().filter(same_file);
        handles.find(open_handle).cloned().ok_or(Error::Conflict)
    }
    /// Detaches a refused run from its closing handle and holds the file, so
    /// the bytes and the size they imply outlive the last handle. Returns the
    /// number of bytes kept.
    fn keep(&self, entry: &Entry) -> Result<Option<usize>> {
        let Some(kept) = entry.file.orphan(entry.native.open())? else {
            return Ok(None);
        };
        let node = entry.file.binding.base().map_err(error)?.node().to_owned();
        let mount = self.native.get().ok_or(Error::Internal)?;
        let mut held = mount.kept.lock().map_err(|_| Error::Internal)?;
        held.insert(node, entry.file.clone());
        Ok(Some(kept))
    }
    /// Lets go of a file once its buffered run has reached the journal.
    fn release(&self, file: &File) -> Result<()> {
        let base = file.binding.base().map_err(error)?;
        let mount = self.native.get().ok_or(Error::Internal)?;
        let mut held = mount.kept.lock().map_err(|_| Error::Internal)?;
        held.remove(base.node());
        Ok(())
    }
    /// Starts the idle timer of a new run. The timer lives as long as the run:
    /// each time no write has extended the run for `coalesce::IDLE`, it tries
    /// once to journal that state without waiting for a commit point. It holds
    /// the file weakly: a timer must never keep a file object alive after its
    /// last handle closed, or a reopen would find the old object.
    fn watch(self: &Arc<Self>, file: &Arc<File>) {
        let writer = Arc::downgrade(self);
        let Ok(Some((run, _))) = file.buffered_mark() else {
            return;
        };
        let file = Arc::downgrade(file);
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
    /// returns its mark. Returns `None` once the run is no longer buffered, the
    /// file is gone or the mount is stopping, which ends the timer.
    async fn idle_mark(writer: &Weak<Self>, file: &Weak<File>, run: u64) -> Option<(u64, u64)> {
        let mark = || file.upgrade()?.buffered_mark().ok().flatten();
        loop {
            let seen = mark().filter(|m| m.0 == run)?;
            tokio::time::sleep(coalesce::IDLE).await;
            let stopping = |w: Arc<Self>| w.closing.load(Ordering::Acquire);
            if writer.upgrade().is_none_or(stopping) {
                return None;
            }
            if mark() == Some(seen) {
                return Some(seen);
            }
        }
    }
    /// Idle-timer commit, serialized with every other journal user by the two
    /// order locks. A failure is kept: the bytes stay buffered and the handle's
    /// next operation retries the journal write and reports it.
    async fn drain_idle(writer: &Weak<Self>, file: &Weak<File>, seen: (u64, u64)) {
        let Some(writer) = writer.upgrade() else {
            return;
        };
        // No work slot is taken: callbacks must never see `Busy` because of this
        // timer. Unmount journals every buffer itself, so the timer stands down.
        if writer.closing.load(Ordering::Acquire) {
            return;
        }
        let _serial = writer.native_serial.lock().await;
        // Upgraded only after the wait, so a long wait cannot hold the file.
        let Some(file) = file.upgrade() else {
            return;
        };
        let _file = file.serial.lock().await;
        // While this timer waited for the locks the run changed. If a write
        // extended it, `watch` waits for it to go idle again; if it was
        // journaled, any newer run has its own timer.
        if file.buffered_mark().ok().flatten() != Some(seen) {
            return;
        }
        if let Err(failure) = writer.drain(&file).await {
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
    /// Journals this node's buffered run, publishing first if the journal is
    /// full. The caller holds `native_serial`.
    async fn drain_own_node(&self) -> Result<()> {
        let base = self.entry.file.binding.base().map_err(error)?;
        self.writer.drain_making_room(base.node()).await
    }
    /// Close-time commit of a run this handle owns. The capability can never
    /// write after Close, so a run the journal still refuses is detached from
    /// the handle and kept in memory with its file. A later handle of the
    /// file journals it; until then operations that need the bytes report
    /// the failure. Only a process exit loses them.
    pub(super) async fn settle_or_keep(&self) {
        let _serial = self.writer.native_serial.lock().await;
        let Err(failure) = self.drain_own_node().await else {
            return;
        };
        match self.writer.keep(&self.entry) {
            Ok(Some(kept)) => eprintln!(
                "Kept {kept} buffered bytes in memory at Close: journal refused them ({failure:?})"
            ),
            Ok(None) => {}
            Err(unkept) => eprintln!("Buffered bytes not kept at Close ({unkept:?}; {failure:?})"),
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
            Offer::Started => self.writer.watch(file),
            Offer::Extended => {}
        }
        Ok(true)
    }
    /// Ends buffering for this handle (cleanup) and journals what the node
    /// holds, publishing first if the journal is full, so later writes on the
    /// handle reach the journal before they return.
    pub(super) async fn stop_buffering(&self) -> Result<()> {
        self.entry
            .file
            .retire(self.entry.native.open(), &self.entry.direct);
        if self.entry.file.buffered_size()?.is_none() {
            return Ok(());
        }
        let _serial = self.writer.native_serial.lock().await;
        self.drain_own_node().await
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
