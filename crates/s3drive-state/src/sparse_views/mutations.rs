//! Atomic local mutations. Successful results never acknowledge a remote root.
use super::*;
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, PartialEq, Eq)]
pub enum Piece {
    /// Unauthenticated base intent: W2c must reopen and authorize the exact base.
    Base {
        offset: u64,
        length: u64,
    },
    Zero {
        length: u64,
    },
    /// Full-block digest checked before copying this bounded local slice.
    Local(Vec<u8>),
}
impl fmt::Debug for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Base { offset, length } => f
                .debug_struct("BaseIntent")
                .field("offset", offset)
                .field("length", length)
                .finish(),
            Self::Zero { length } => f.debug_struct("Zero").field("length", length).finish(),
            Self::Local(bytes) => write!(f, "Local([private bytes; {}])", bytes.len()),
        }
    }
}
fn write_bounds(offset: u64, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() {
        return Err(Failure::Invalid.into());
    }
    if bytes.len() > MAX_RANGE_BYTES
        || offset
            .checked_add(bytes.len() as u64)
            .is_none_or(|n| n > MAX_FILE_BYTES)
    {
        return Err(Failure::Bounds.into());
    }
    Ok(())
}
fn resize_bounds(size: u64) -> Result<()> {
    if size > MAX_FILE_BYTES {
        Err(Failure::Bounds.into())
    } else {
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Boundary {
    BlockStored,
    ViewStored,
    OperationStored,
    HeadUpdated,
    Validated,
    Committed,
}
pub(super) fn store_record(
    db: &Connection,
    saved: &Saved,
    hook: &mut impl FnMut(Boundary) -> Result<()>,
) -> Result<()> {
    let r = &saved.record;
    if db.execute(
        "INSERT INTO sparse_views VALUES(?1,?2,?3,?4)",
        params![
            r.id.as_slice(),
            saved.target.as_slice(),
            &saved.raw,
            hash(b"s3drive-sparse-view-v1\0", r)?.as_slice()
        ],
    )? != 1
    {
        return Err(Failure::Corrupt.into());
    }
    hook(Boundary::ViewStored)?;
    if db.execute(
        "INSERT INTO sparse_view_operations VALUES(?1,?2,?3,?4)",
        params![
            r.operation.as_slice(),
            r.id.as_slice(),
            &saved.request,
            hash(b"s3drive-sparse-request-v1\0", &(r.operation, &r.request))?.as_slice()
        ],
    )? != 1
    {
        return Err(Failure::Corrupt.into());
    }
    hook(Boundary::OperationStored)?;
    Ok(())
}
pub struct Journal<'a> {
    pub(super) state: &'a mut State,
    pub(super) options: Options,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State, options: Options) -> Result<Self> {
        options.validate()?;
        state.verify_staging_storage()?;
        let tx = state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        isolation(&tx)?;
        inspect(&tx)?;
        tx.commit()?;
        Ok(Self { state, options })
    }
    /// Explicit manual retention. This token is not an automatic file handle.
    pub fn pin(&mut self, view: Id) -> Result<pins::Pin> {
        pins::acquire(self.state, view)
    }
    pub fn release_pin(&mut self, pin: &pins::Pin) -> Result<bool> {
        pins::release(self.state, pin)
    }
    /// Local pieces only; the caller still needs provider/history authorization.
    pub fn read_pinned(
        &mut self,
        pin: &pins::Pin,
        offset: u64,
        length: usize,
    ) -> Result<Vec<Piece>> {
        pins::check(self.state, pin)?;
        self.read(pin.view_id(), offset, length)
    }
    pub fn enable_scoped(&mut self) -> Result<()> {
        scoped::enable(self.state)
    }
    pub fn begin(&mut self, operation: Id, binding: Binding, size: u64) -> Result<View> {
        self.begin_inner(operation, None, binding, size)
    }
    pub fn begin_scoped(
        &mut self,
        token: ScopedOperation,
        binding: Binding,
        size: u64,
    ) -> Result<View> {
        self.begin_inner(token.id()?, Some(token), binding, size)
    }
    fn begin_inner(
        &mut self,
        operation: Id,
        scope: Option<ScopedOperation>,
        binding: Binding,
        size: u64,
    ) -> Result<View> {
        let record = Record::new(
            operation,
            Request {
                format: 1,
                binding,
                size,
                edit: None,
            },
        )?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        isolation(&tx)?;
        let before = inspect(&tx)?;
        if let Some(old) = before.operation(operation) {
            scoped::known(&tx, scope, operation, &old.request.binding)?;
            if old.request != record.request {
                return Err(Failure::Conflict.into());
            }
            let result = View(old.clone());
            tx.commit()?;
            return Ok(result);
        }
        preparation::publication::settlement::guard_operation(&tx, operation)?;
        scoped::admit(&tx, scope, operation, &record.request.binding)?;
        let target = record.request.binding.target()?;
        if before.files.contains_key(&target) {
            return Err(Failure::Conflict.into());
        }
        let raw = encode(&record.request.binding)?;
        if raw.len() > BINDING_BYTES {
            return Err(Failure::Bounds.into());
        }
        let saved = Saved::new(record.clone())?;
        let mut expected = before.clone();
        expected.files.insert(
            target,
            File {
                binding: record.request.binding.clone(),
                raw: raw.clone(),
                head: record.id,
            },
        );
        expected.records.insert(record.id, saved.clone());
        expected.admit(self.options)?;
        if before.files.is_empty()
            && tx.execute(
                "INSERT INTO sparse_view_role VALUES(1,1,?1)",
                [role_hash()?.as_slice()],
            )? != 1
        {
            return Err(Failure::Corrupt.into());
        }
        if tx.execute(
            "INSERT INTO sparse_view_files VALUES(?1,?2,?3,?4)",
            params![
                target.as_slice(),
                raw,
                record.id.as_slice(),
                hash(b"s3drive-sparse-binding-v1\0", &record.request.binding)?.as_slice()
            ],
        )? != 1
        {
            return Err(Failure::Corrupt.into());
        }
        store_record(&tx, &saved, &mut |_| Ok(()))?;
        if inspect(&tx)? != expected {
            return Err(Failure::Corrupt.into());
        }
        tx.commit()?;
        Ok(View(record))
    }
    pub fn write(&mut self, operation: Id, parent: Id, offset: u64, bytes: &[u8]) -> Result<View> {
        self.write_inner(operation, parent, offset, bytes, |_| Ok(()))
    }
    pub(super) fn write_inner(
        &mut self,
        operation: Id,
        parent: Id,
        offset: u64,
        bytes: &[u8],
        hook: impl FnMut(Boundary) -> Result<()>,
    ) -> Result<View> {
        self.write_scoped_inner(operation, None, parent, offset, bytes, hook)
    }
    pub fn write_scoped(
        &mut self,
        token: ScopedOperation,
        parent: Id,
        offset: u64,
        bytes: &[u8],
    ) -> Result<View> {
        self.write_scoped_inner(token.id()?, Some(token), parent, offset, bytes, |_| Ok(()))
    }
    fn write_scoped_inner(
        &mut self,
        operation: Id,
        scope: Option<ScopedOperation>,
        parent: Id,
        offset: u64,
        bytes: &[u8],
        hook: impl FnMut(Boundary) -> Result<()>,
    ) -> Result<View> {
        write_bounds(offset, bytes)?;
        let mut data = BTreeMap::new();
        let mut blocks = Vec::new();
        for chunk in bytes.chunks(PATCH_BYTES) {
            let block = digest(chunk);
            blocks.push(BlockRef {
                block,
                length: chunk.len() as u64,
            });
            if let Some(old) = data.insert(block, chunk.to_vec())
                && old != chunk
            {
                return Err(Failure::Corrupt.into());
            }
        }
        self.change(
            operation,
            scope,
            Change::Write {
                parent,
                offset,
                blocks,
            },
            None,
            data,
            hook,
        )
    }
    pub fn resize(&mut self, operation: Id, parent: Id, size: u64) -> Result<View> {
        resize_bounds(size)?;
        self.change(
            operation,
            None,
            Change::Resize { parent },
            Some(size),
            BTreeMap::new(),
            |_| Ok(()),
        )
    }
    pub fn resize_scoped(&mut self, token: ScopedOperation, parent: Id, size: u64) -> Result<View> {
        resize_bounds(size)?;
        self.change(
            token.id()?,
            Some(token),
            Change::Resize { parent },
            Some(size),
            BTreeMap::new(),
            |_| Ok(()),
        )
    }
    fn change(
        &mut self,
        operation: Id,
        scope: Option<ScopedOperation>,
        change: Change,
        size: Option<u64>,
        data: BTreeMap<Id, Vec<u8>>,
        mut hook: impl FnMut(Boundary) -> Result<()>,
    ) -> Result<View> {
        if operation == [0; 32] {
            return Err(Failure::Invalid.into());
        }
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        isolation(&tx)?;
        let before = inspect(&tx)?;
        // Known operations are compared before head admission or new quotas.
        if let Some(old) = before.operation(operation) {
            scoped::known(&tx, scope, operation, &old.request.binding)?;
            if old.request.edit.as_ref() != Some(&change)
                || size.is_some_and(|n| old.request.size != n)
                || data
                    .iter()
                    .any(|(id, bytes)| before.blocks.get(id) != Some(bytes))
            {
                return Err(Failure::Conflict.into());
            }
            let result = View(old.clone());
            tx.commit()?;
            return Ok(result);
        }
        preparation::publication::settlement::guard_operation(&tx, operation)?;
        let parent = before.record(change.parent())?;
        scoped::admit(&tx, scope, operation, &parent.request.binding)?;
        let target = parent.request.binding.target()?;
        if before.files.get(&target).ok_or(Failure::Corrupt)?.head != parent.id {
            return Err(Failure::Conflict.into());
        }
        let size = match &change {
            Change::Rebase { .. } | Change::Observe { .. } => return Err(Failure::Invalid.into()),
            Change::Resize { .. } => size.ok_or(Failure::Invalid)?,
            Change::Write { offset, blocks, .. } => parent
                .request
                .size
                .max(*offset + blocks.iter().map(|b| b.length).sum::<u64>()),
        };
        let record = Record::child(
            operation,
            Request {
                format: 2,
                binding: parent.request.binding.clone(),
                size,
                edit: Some(change),
            },
            parent,
        )?;
        let saved = Saved::new(record.clone())?;
        let mut expected = before.clone();
        for (block, bytes) in &data {
            if let Some(old) = expected.blocks.get(block) {
                if old != bytes {
                    return Err(Failure::Corrupt.into());
                }
            } else {
                expected.blocks.insert(*block, bytes.clone());
            }
        }
        if expected.records.insert(record.id, saved.clone()).is_some() {
            return Err(Failure::Corrupt.into());
        }
        expected
            .files
            .get_mut(&target)
            .ok_or(Failure::Corrupt)?
            .head = record.id;
        expected.admit(self.options)?;
        for (block, bytes) in &data {
            if !before.blocks.contains_key(block)
                && tx.execute(
                    "INSERT INTO sparse_view_blocks VALUES(?1,?2)",
                    params![block.as_slice(), bytes],
                )? != 1
            {
                return Err(Failure::Corrupt.into());
            }
        }
        hook(Boundary::BlockStored)?;
        store_record(&tx, &saved, &mut hook)?;
        if tx.execute(
            "UPDATE sparse_view_files SET head=?1 WHERE target=?2 AND head=?3",
            params![
                record.id.as_slice(),
                target.as_slice(),
                parent.id.as_slice()
            ],
        )? != 1
        {
            return Err(Failure::Corrupt.into());
        }
        hook(Boundary::HeadUpdated)?;
        if inspect(&tx)? != expected {
            return Err(Failure::Corrupt.into());
        }
        hook(Boundary::Validated)?;
        tx.commit()?;
        hook(Boundary::Committed)?;
        Ok(View(record))
    }
    /// Scope-bound metadata lookup across locally acknowledged base epochs.
    /// This grants no permission to read the returned immutable source.
    /// All retained current heads, bounded by the existing file quota. No network authority.
    pub fn heads(&mut self) -> Result<Vec<View>> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        isolation(&tx)?;
        let data = inspect(&tx)?;
        let heads = data
            .files
            .values()
            .map(|f| data.record(f.head).cloned().map(View))
            .collect::<Result<Vec<_>>>()?;
        tx.commit()?;
        Ok(heads)
    }
    pub fn current_head(&mut self, binding: &Binding) -> Result<View> {
        binding.validate()?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let before = inspect(&tx)?;
        let file = before
            .files
            .get(&binding.target()?)
            .ok_or(Failure::NotFound)?;
        if !binding.same_scope(&file.binding)? {
            return Err(Failure::Conflict.into());
        }
        let view = View(before.record(file.head)?.clone());
        tx.commit()?;
        Ok(view)
    }
    pub fn view(&mut self, id: Id) -> Result<View> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        isolation(&tx)?;
        let ledger = inspect(&tx)?;
        let result = View(ledger.record(id)?.clone());
        tx.commit()?;
        Ok(result)
    }
    /// Explicit inspection only: write/resize still require the returned exact ID.
    pub fn head(&mut self, binding: &Binding) -> Result<View> {
        binding.validate()?;
        let target = binding.target()?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        isolation(&tx)?;
        let ledger = inspect(&tx)?;
        let file = ledger.files.get(&target).ok_or(Failure::NotFound)?;
        if &file.binding != binding {
            return Err(Failure::Conflict.into());
        }
        let result = View(ledger.record(file.head)?.clone());
        tx.commit()?;
        Ok(result)
    }
    /// Local pieces are structurally bound and digest-checked; this method does
    /// not authorize a filesystem read or fetch any remote base content.
    pub fn read(&mut self, id: Id, offset: u64, length: usize) -> Result<Vec<Piece>> {
        if length > MAX_RANGE_BYTES {
            return Err(Failure::Bounds.into());
        }
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        isolation(&tx)?;
        let ledger = inspect(&tx)?;
        let mut out = Vec::new();
        for range in View(ledger.record(id)?.clone()).range(offset, length)? {
            out.push(match range {
                Range::Base { offset, length } => Piece::Base { offset, length },
                Range::Zero { length } => Piece::Zero { length },
                Range::Patch {
                    block,
                    offset,
                    length,
                } => {
                    let bytes = ledger.blocks.get(&block).ok_or(Failure::Corrupt)?;
                    let end = offset.checked_add(length).ok_or(Failure::Corrupt)?;
                    let piece = bytes
                        .get(offset as usize..end as usize)
                        .ok_or(Failure::Corrupt)?;
                    Piece::Local(piece.to_vec())
                }
            });
        }
        tx.commit()?;
        Ok(out)
    }
}
#[derive(Clone)]
pub struct Client {
    pub(super) owner: Owner,
    pub(super) options: Options,
}
impl Client {
    pub(super) fn release_guard(&self, pin: pins::Pin) {
        self.owner.release_read_pin(pin, self.options);
    }
    /// All clones share one local retention pin; last drop queues bounded cleanup.
    pub async fn guard(&self, id: Id) -> Result<super::ReadGuard> {
        let client = self.clone();
        let options = self.options;
        self.owner
            .call(move |s| {
                let mut journal = Journal::new(&mut s.state, options)?;
                let view = journal.view(id)?;
                let pin = journal.pin(id)?;
                Ok(super::guards::ReadGuard::new(client, view, pin))
            })
            .await
    }
    /// Same private owner as the immutable views; no second SQLite connection.
    pub fn preparations(&self, options: preparation::Options) -> Result<preparation::Client> {
        preparation::Client::new(self.owner.clone(), options)
    }

    pub(crate) fn new(owner: Owner, options: Options) -> Result<Self> {
        options.validate()?;
        Ok(Self { owner, options })
    }
    pub async fn begin(&self, operation: Id, binding: Binding, size: u64) -> Result<View> {
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.begin(operation, binding, size))
            .await
    }
    pub async fn view(&self, id: Id) -> Result<View> {
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.view(id))
            .await
    }
    pub async fn head(&self, binding: Binding) -> Result<View> {
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.head(&binding))
            .await
    }
    /// An unknown reply preserves any admitted pin; this manual API does not
    /// make canceled file handles or clones automatically release retention.
    pub async fn pin(&self, view: Id) -> Result<pins::Pin> {
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.pin(view))
            .await
    }
    pub async fn release_pin(&self, pin: pins::Pin) -> Result<bool> {
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.release_pin(&pin))
            .await
    }
    pub async fn read_pinned(
        &self,
        pin: pins::Pin,
        offset: u64,
        length: usize,
    ) -> Result<Vec<Piece>> {
        if length > MAX_RANGE_BYTES {
            return Err(Failure::Bounds.into());
        }
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.read_pinned(&pin, offset, length))
            .await
    }
    pub async fn enable_scoped(&self) -> Result<()> {
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.enable_scoped())
            .await
    }
    pub async fn begin_scoped(
        &self,
        token: ScopedOperation,
        binding: Binding,
        size: u64,
    ) -> Result<View> {
        binding.validate()?;
        token.id()?;
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.begin_scoped(token, binding, size))
            .await
    }
    pub async fn write_scoped(
        &self,
        token: ScopedOperation,
        parent: Id,
        offset: u64,
        bytes: Arc<[u8]>,
    ) -> Result<View> {
        write_bounds(offset, &bytes)?;
        token.id()?;
        let options = self.options;
        self.owner
            .call(move |s| {
                Journal::new(&mut s.state, options)?.write_scoped(token, parent, offset, &bytes)
            })
            .await
    }
    pub async fn resize_scoped(
        &self,
        token: ScopedOperation,
        parent: Id,
        size: u64,
    ) -> Result<View> {
        resize_bounds(size)?;
        token.id()?;
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.resize_scoped(token, parent, size))
            .await
    }
    pub async fn heads(&self) -> Result<Vec<View>> {
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.heads())
            .await
    }
    pub async fn current_head(&self, binding: Binding) -> Result<View> {
        binding.validate()?;
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.current_head(&binding))
            .await
    }
    pub async fn write(
        &self,
        operation: Id,
        parent: Id,
        offset: u64,
        bytes: Arc<[u8]>,
    ) -> Result<View> {
        write_bounds(offset, &bytes)?;
        let options = self.options;
        self.owner
            .call(move |s| {
                Journal::new(&mut s.state, options)?.write(operation, parent, offset, &bytes)
            })
            .await
    }
    pub async fn resize(&self, operation: Id, parent: Id, size: u64) -> Result<View> {
        resize_bounds(size)?;
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.resize(operation, parent, size))
            .await
    }
    pub async fn read(&self, id: Id, offset: u64, length: usize) -> Result<Vec<Piece>> {
        if length > MAX_RANGE_BYTES {
            return Err(Failure::Bounds.into());
        }
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.read(id, offset, length))
            .await
    }
}
