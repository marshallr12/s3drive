//! Policy-bound upload of a prepared immutable tree. No authority CAS, mounted
//! format activation, logical-file acknowledgment or payload reclamation.
use super::{Executor, Failure};
use s3drive_core::manifest::objects::RootDescriptor;
use s3drive_state::{Result, chunked, remote};
use std::fmt;

#[derive(Clone)]
pub struct Uploader {
    executor: Executor,
    planner: chunked::Planner,
}
/// Historical upload completion for this tree's objects. The root still needs
/// an authenticated authority commit before the file can be called flushed.
pub struct UploadedTree {
    receipt: chunked::TreeReceipt,
}
impl fmt::Debug for UploadedTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UploadedChunkTree([private])")
    }
}
impl UploadedTree {
    pub fn root(&self) -> &RootDescriptor {
        self.receipt.root()
    }
    pub fn root_receipt(&self) -> &remote::FlushReceipt {
        self.receipt.root_receipt()
    }
    /// Historical complete-tree inspection; never a logical file-flush receipt.
    pub fn tree_receipt(&self) -> &chunked::TreeReceipt {
        &self.receipt
    }
}
impl Uploader {
    /// Always obtains the planner from this executor's exact SQLite owner.
    pub fn new(executor: Executor, options: chunked::Options) -> Result<Self> {
        let planner = executor.storage().chunked(options)?;
        Ok(Self { executor, planner })
    }
    /// One object, with exact binding/policy checks before new physical staging.
    /// None means a freshly verified explicit hole. Unknown results are retained
    /// under the same identity; a retry never reissues a claimed PUT.
    pub async fn upload_object(
        &self,
        id: [u8; 32],
        level: i8,
        slot: u64,
    ) -> Result<Option<remote::FlushReceipt>> {
        let Some(t) = self.planner.transfer(id, level, slot).await? else {
            if self.planner.bytes(id, level, slot).await?.is_some() {
                return Err(Failure::Integrity.into());
            }
            return Ok(None);
        };
        if self.executor.recovery_target(t.target().object_key())? != *t.target() {
            return Err(Failure::BindingMismatch.into());
        }
        let prior = match t.generation() {
            Some(g) => self.executor.storage().uploads().for_generation(g).await?,
            None => None,
        };
        if prior
            .as_ref()
            .is_none_or(|i| i.phase == remote::Phase::Prepared)
        {
            self.executor
                .check_create(t.target().object_key(), t.size())?;
        }
        self.planner.require_dependencies(id, level, slot).await?;
        let staged = self
            .planner
            .materialize(id, level, slot)
            .await?
            .ok_or(Failure::Integrity)?;
        if staged.target() != t.target()
            || staged.reference() != t.reference()
            || staged.payload_sha256() != t.payload_sha256()
            || staged.size() != t.size()
        {
            return Err(Failure::Integrity.into());
        }
        let g = staged.generation().ok_or(Failure::MissingIntent)?;
        let receipt = self.executor.flush(g).await?;
        let i = receipt.info();
        if i.generation != g
            || i.target != *t.target()
            || i.size != t.size()
            || i.payload_sha256 != *t.payload_sha256()
            || i.guard != remote::Guard::Missing
            || i.phase != remote::Phase::Verified
            || receipt.covered_generations() != 1
        {
            return Err(Failure::Integrity.into());
        }
        Ok(Some(receipt))
    }
    /// Upload data, leaves, then parents; returns no logical file-flush receipt.
    /// Repeated calls recover each already-known operation. Other failures stop
    /// the walk immediately with its completed and uncertain objects retained.
    pub async fn upload_tree(&self, id: [u8; 32]) -> Result<UploadedTree> {
        let info = self.planner.info(id).await?;
        info.root().ok_or(chunked::Failure::Invalid)?;
        for level in -1..=info.root_level() {
            for slot in 0..info.object_slots(level)? {
                self.upload_object(id, level, slot).await?;
                tokio::task::yield_now().await;
            }
        }
        let receipt = self.planner.tree_receipt(id).await?;
        if receipt.plan() != &info {
            return Err(Failure::Integrity.into());
        }
        Ok(UploadedTree { receipt })
    }
}
#[cfg(test)]
mod tests;
