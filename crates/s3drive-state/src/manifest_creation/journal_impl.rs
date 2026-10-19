use super::*;

pub struct Journal<'a> {
    state: &'a mut State,
    limits: Limits,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State, limits: Limits) -> Result<Self> {
        let limits = limits.validate()?;
        state.verify_staging_storage()?;
        isolation(&state.connection)?;
        preflight(&state.connection)?;
        inspect(&state.connection)?;
        Ok(Self { state, limits })
    }
    pub fn get(&mut self, id: &str) -> Result<Record> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        preflight(&tx)?;
        isolation(&tx)?;
        let rows = inspect(&tx)?;
        let r = rows.get(id).cloned().ok_or(Failure::Missing)?;
        tx.commit()?;
        Ok(r)
    }
    pub fn begin(&mut self, input: Input) -> Result<Record> {
        self.begin_inner(input, checkpoint)
    }
    fn begin_inner(
        &mut self,
        input: Input,
        mut hook: impl FnMut(&str) -> Result<()>,
    ) -> Result<Record> {
        input.validate()?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        preflight(&tx)?;
        isolation(&tx)?;
        if let Some(b) = crate::chunk_mount::load(&tx)? {
            b.volume(&input.volume)?;
            b.control(&crate::namespace_client::Control {
                store: input.store.clone(),
                source: input.source,
                fingerprint: input.fingerprint.clone(),
            })?;
        }
        let mut rows = inspect(&tx)?;
        if let Some(old) = rows.get(&input.operation) {
            if old.input != input {
                return Err(Failure::Conflict.into());
            }
            let result = old.clone();
            tx.commit()?;
            return Ok(result);
        }
        if rows.len() >= self.limits.records
            || (rows.len() + 1) * RECORD_BYTES > self.limits.metadata_bytes
        {
            return Err(Failure::Quota.into());
        }
        tx.execute("INSERT INTO source_bindings(source_id,config_fingerprint) VALUES(?1,?2) ON CONFLICT(source_id) DO NOTHING",params![input.source.as_slice(),input.fingerprint])?;
        let actual: String = tx.query_row(
            "SELECT config_fingerprint FROM source_bindings WHERE source_id=?1",
            [input.source.as_slice()],
            |r| r.get(0),
        )?;
        if actual != input.fingerprint {
            return Err(Failure::Conflict.into());
        }
        let r = Record {
            format: 1,
            input,
            generation: None,
            physical: None,
            request: None,
            receipt: None,
            rejection: None,
        };
        save(&tx, &mut rows, r.clone(), true)?;
        hook("AdmissionStored")?;
        tx.commit()?;
        hook("AdmissionCommitted")?;
        Ok(r)
    }
    /// Link only the sealed, exact64-byte canonical body under this intent's
    /// idempotent staging key. A linked generation never changes after a claim.
    pub fn link(&mut self, id: &str, generation: journal::GenerationId) -> Result<Record> {
        self.change(
            id,
            |db, r| {
                if let Some(old) = r.generation {
                    if old != generation {
                        return Err(Failure::Conflict.into());
                    }
                    return Ok(());
                }
                check_staged(db, &r.input, generation, true)?;
                r.generation = Some(generation);
                Ok(())
            },
            checkpoint,
        )
    }
    /// Freeze exact metadata request only from the original linked generation's
    /// verified create-only upload. No RPC or caller-supplied proof is accepted.
    pub fn prepare_request(&mut self, id: &str) -> Result<Record> {
        self.change(
            id,
            |db, r| {
                if r.request.is_some() {
                    return Ok(());
                }
                let g = r.generation.ok_or(Failure::Incomplete)?;
                let physical = verified_physical(db, &r.input, g)?;
                r.request = Some(r.input.request(
                    physical.operation.clone(),
                    physical.etag.clone(),
                    physical.version.clone(),
                )?);
                r.physical = Some(physical);
                Ok(())
            },
            checkpoint,
        )
    }
    pub fn acknowledge(
        &mut self,
        id: &str,
        request: &ns::Apply,
        receipt: ns::Receipt,
    ) -> Result<Record> {
        self.change(
            id,
            |_, r| acknowledge_record(r, request, receipt),
            checkpoint,
        )
    }
    pub fn reject(&mut self, id: &str, request: &ns::Apply, error: ns::Error) -> Result<Record> {
        self.change(id, |_, r| reject_record(r, request, error), checkpoint)
    }
    pub fn records(&mut self) -> Result<Vec<Record>> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        preflight(&tx)?;
        isolation(&tx)?;
        let rows = inspect(&tx)?.into_values().collect();
        tx.commit()?;
        Ok(rows)
    }
    fn change(
        &mut self,
        id: &str,
        work: impl FnOnce(&Connection, &mut Record) -> Result<()>,
        mut hook: impl FnMut(&str) -> Result<()>,
    ) -> Result<Record> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        preflight(&tx)?;
        isolation(&tx)?;
        let mut rows = inspect(&tx)?;
        let mut r = rows.get(id).cloned().ok_or(Failure::Missing)?;
        let before = r.clone();
        work(&tx, &mut r)?;
        if r != before {
            save(&tx, &mut rows, r.clone(), false)?;
            hook("TransitionStored")?;
        }
        tx.commit()?;
        hook("TransitionCommitted")?;
        Ok(r)
    }
}
// Read only the bounded, independently checksummed upload metadata. The
// canonical source bytes are not needed after the physical proof is saved.
pub(super) fn verified_physical(
    db: &Connection,
    input: &Input,
    g: journal::GenerationId,
) -> Result<Physical> {
    check_staged(db, input, g, false)?;
    let info = remote::load(db, g)?;
    if info.phase != remote::Phase::Verified
        || info.target != input.target()?
        || info.size != 64
        || info.guard != remote::Guard::Missing
        || info.anchor.is_some()
        || info.payload_sha256 != <[u8; 32]>::from(Sha256::digest(input.body()?))
        || journal::load(db, g)?.manifest_hash != Some(info.manifest_hash)
    {
        return Err(Failure::Incomplete.into());
    }
    let revision = info.revision.ok_or(Failure::Corrupt)?;
    Ok(Physical {
        manifest: info.manifest_hash,
        payload: info.payload_sha256,
        operation: hex(info.operation.as_bytes()),
        etag: revision.etag().into(),
        version: revision.version().map(Into::into),
    })
}
pub(super) fn check_staged(
    db: &Connection,
    input: &Input,
    id: journal::GenerationId,
    bytes: bool,
) -> Result<()> {
    let g = journal::load(db, id)?;
    let key = input.stage_key()?;
    let staged: Option<i64> = db
        .query_row(
            "SELECT generation_id FROM staged_operations WHERE operation_key=?1",
            [key.as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    if staged != Some(id.value())
        || g.target != input.target()?
        || g.phase != journal::Phase::Sealed
        || g.size != 64
        || g.parent.is_some()
        || !matches!(g.base, journal::Base::Missing)
    {
        return Err(Failure::Conflict.into());
    }
    if bytes && journal::read_data(db, id, 0, 64, false)? != input.body()? {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}

#[derive(Clone)]
pub struct Client {
    owner: crate::worker::Owner,
    limits: Limits,
}
impl Client {
    pub(crate) fn new(owner: crate::worker::Owner, limits: Limits) -> Result<Self> {
        Ok(Self {
            owner,
            limits: limits.validate()?,
        })
    }
    pub fn same_storage(&self, storage: &crate::worker::Storage) -> bool {
        storage.same_owner(&self.owner)
    }
    async fn call<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Journal<'_>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let limits = self.limits;
        self.owner
            .call(move |store| work(&mut Journal::new(&mut store.state, limits)?))
            .await
    }
    pub async fn records(&self) -> Result<Vec<Record>> {
        self.call(|j| j.records()).await
    }
    pub async fn begin(&self, input: Input) -> Result<Record> {
        self.call(move |j| j.begin(input)).await
    }
    pub async fn get(&self, id: String) -> Result<Record> {
        self.call(move |j| j.get(&id)).await
    }
    pub async fn link(&self, id: String, g: journal::GenerationId) -> Result<Record> {
        self.call(move |j| j.link(&id, g)).await
    }
    pub async fn prepare_request(&self, id: String) -> Result<Record> {
        self.call(move |j| j.prepare_request(&id)).await
    }
    pub async fn acknowledge(
        &self,
        id: String,
        request: ns::Apply,
        receipt: ns::Receipt,
    ) -> Result<Record> {
        self.call(move |j| j.acknowledge(&id, &request, receipt))
            .await
    }
    pub async fn reject(&self, id: String, request: ns::Apply, error: ns::Error) -> Result<Record> {
        self.call(move |j| j.reject(&id, &request, error)).await
    }
}

#[cfg(test)]
mod tests;
