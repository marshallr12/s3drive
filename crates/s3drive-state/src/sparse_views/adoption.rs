//! Local clean-version intent only. The managed layer must authenticate remote
//! observations separately. No reclamation, upload, global-head or read authority.
use super::*;
use mutations::{Boundary, store_record};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Adoption {
    /// No durable operation admitted; the nonce remains unused.
    Unchanged(View),
    /// Exact immutable local result; recover by the old parent/token after loss.
    Adopted(View),
}
fn known(
    db: &Connection,
    before: &ledger::Ledger,
    token: ScopedOperation,
    parent: Id,
) -> Result<Option<View>> {
    let old = before.record(parent)?;
    scoped::known(db, Some(token), token.id()?, &old.request.binding)?;
    let Some(record) = before.operation(token.id()?) else {
        preparation::publication::settlement::guard_operation(db, token.id()?)?;
        return Ok(None);
    };
    if record.request.edit != Some(Change::Observe { parent }) {
        return Err(Failure::Conflict.into());
    }
    Ok(Some(View(record.clone())))
}
impl Journal<'_> {
    /// Exact local recovery only; None makes no assertion about a remote RPC.
    pub fn recover_adoption(&mut self, token: ScopedOperation, parent: Id) -> Result<Option<View>> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        isolation(&tx)?;
        let before = inspect(&tx)?;
        let result = known(&tx, &before, token, parent)?;
        tx.commit()?;
        Ok(result)
    }
    /// Record externally supplied intent. Does NOT authenticate its remote base.
    pub fn adopt_clean_intent(
        &mut self,
        token: ScopedOperation,
        parent: Id,
        binding: Binding,
    ) -> Result<Adoption> {
        self.adopt_inner(token, parent, binding, |_| Ok(()))
    }
    pub(super) fn adopt_inner(
        &mut self,
        token: ScopedOperation,
        parent: Id,
        binding: Binding,
        mut hook: impl FnMut(Boundary) -> Result<()>,
    ) -> Result<Adoption> {
        let request = Request {
            format: 4,
            size: binding.base_size(),
            binding,
            edit: Some(Change::Observe { parent }),
        };
        request.validate()?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        isolation(&tx)?;
        let before = inspect(&tx)?;
        if let Some(view) = known(&tx, &before, token, parent)? {
            if view.0.request != request {
                return Err(Failure::Conflict.into());
            }
            tx.commit()?;
            return Ok(Adoption::Adopted(view));
        }
        let old = before.record(parent)?;
        scoped::admit(&tx, Some(token), token.id()?, &old.request.binding)?;
        let target = old.request.binding.target()?;
        if before.files.get(&target).ok_or(Failure::Corrupt)?.head != parent
            || !old.clean()
            || !old.request.binding.same_scope(&request.binding)?
        {
            return Err(Failure::Conflict.into());
        }
        let fence = preparation::adoption_fence(&tx, target)?;
        if old.request.binding == request.binding {
            tx.commit()?;
            return Ok(Adoption::Unchanged(View(old.clone())));
        }
        let record = Record::child(token.id()?, request, old)?;
        let saved = Saved::new(record.clone())?;
        let raw = encode(&record.request.binding)?;
        if raw.len() > BINDING_BYTES {
            return Err(Failure::Bounds.into());
        }
        let mut expected = before.clone();
        let file = expected.files.get_mut(&target).ok_or(Failure::Corrupt)?;
        file.binding = record.request.binding.clone();
        file.raw = raw.clone();
        file.head = record.id;
        if expected.records.insert(record.id, saved.clone()).is_some() {
            return Err(Failure::Corrupt.into());
        }
        expected.admit(self.options)?;
        store_record(&tx, &saved, &mut hook)?;
        if tx.execute(
            "UPDATE sparse_view_files SET binding=?1,head=?2,state_hash=?3 WHERE target=?4 AND head=?5",
            params![raw, record.id.as_slice(),
                hash(b"s3drive-sparse-binding-v1\0", &record.request.binding)?.as_slice(),
                target.as_slice(), parent.as_slice()],
        )? != 1 { return Err(Failure::Corrupt.into()); }
        hook(Boundary::HeadUpdated)?;
        if inspect(&tx)? != expected {
            return Err(Failure::Corrupt.into());
        }
        preparation::check_adoption_fence(&tx, target, &fence)?;
        hook(Boundary::Validated)?;
        tx.commit()?;
        hook(Boundary::Committed)?;
        Ok(Adoption::Adopted(View(record)))
    }
}
impl Client {
    pub async fn recover_adoption(
        &self,
        token: ScopedOperation,
        parent: Id,
    ) -> Result<Option<View>> {
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.recover_adoption(token, parent))
            .await
    }
    pub async fn adopt_clean_intent(
        &self,
        token: ScopedOperation,
        parent: Id,
        binding: Binding,
    ) -> Result<Adoption> {
        let options = self.options;
        self.owner
            .call(move |s| {
                Journal::new(&mut s.state, options)?.adopt_clean_intent(token, parent, binding)
            })
            .await
    }
}

#[cfg(test)]
mod tests;
