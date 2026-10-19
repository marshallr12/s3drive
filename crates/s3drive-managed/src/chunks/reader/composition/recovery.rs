//! Observation-only recovery from the exact local mixed plan. Construction
//! does not reopen source metadata, and no method can claim or send a PUT.
use super::*;
use s3drive_s3::uploads::Executor;
use s3drive_state::{remote, sparse_views::preparation as plans};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixedRecoveryFailure {
    /// No durable claim exists. Fresh source/write admission is still required.
    Unclaimed,
    /// The exact historical attempt remains uncertain, rejected or cancelled.
    Unverified,
}
impl fmt::Display for MixedRecoveryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unclaimed => "mixed object has no claimed attempt; recovery cannot send it",
            Self::Unverified => "mixed object remains unverified; all local data retained",
        })
    }
}
impl std::error::Error for MixedRecoveryFailure {}
#[derive(Clone)]
pub struct MixedRecovery {
    client: plans::Client,
    executor: Executor,
}
impl fmt::Debug for MixedRecovery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MixedPhysicalRecovery([private; observation only])")
    }
}
impl MixedRecovery {
    /// The client and executor must share their exact private SQL owner. This
    /// type has no Reader/Preparation/authority handle and grants no fresh send.
    pub fn new(client: plans::Client, executor: Executor) -> Result<Self> {
        if !client.same_storage(&executor.storage()) {
            return Err(ReadFailure::Binding.into());
        }
        Ok(Self { client, executor })
    }
    /// Return only this exact physical object's historical receipt. Inherited
    /// dependencies/holes cannot acquire fake receipts. A parent receipt here
    /// does not certify its children: complete-tree validation remains separate.
    pub async fn recover_object(
        &self,
        plan: Id,
        p: plans::Position,
    ) -> Result<remote::FlushReceipt> {
        bounded(
            ReadOptions::default(),
            Box::pin(self.recover_inner(plan, p)),
        )
        .await
    }
    async fn recover_inner(&self, plan: Id, p: plans::Position) -> Result<remote::FlushReceipt> {
        let t = self
            .client
            .transfer(plan, p)
            .await?
            .ok_or(MixedRecoveryFailure::Unclaimed)?;
        if self.executor.recovery_target(t.target().object_key())? != *t.target() {
            return Err(ReadFailure::Binding.into());
        }
        let g = t.generation().ok_or(MixedRecoveryFailure::Unclaimed)?;
        let uploads = self.executor.storage().uploads();
        let original = uploads
            .for_generation(g)
            .await?
            .ok_or(MixedRecoveryFailure::Unclaimed)?;
        if original.phase == remote::Phase::Prepared {
            return Err(MixedRecoveryFailure::Unclaimed.into());
        }
        if !matches_info(&t, &original) {
            return Err(ReadFailure::Integrity.into());
        }
        // Reconcile is intrinsically observation-only, including if a prepared
        // phase were ever observed on a concurrent recheck. Never flush/submit.
        let observed = self.executor.reconcile(g).await?;
        if observed.operation != original.operation || !matches_info(&t, &observed) {
            return Err(ReadFailure::Integrity.into());
        }
        if observed.phase != remote::Phase::Verified {
            return Err(MixedRecoveryFailure::Unverified.into());
        }
        let receipt = uploads.acknowledge(observed.operation).await?;
        if receipt.info().operation != original.operation || !t.receipt_matches(&receipt) {
            return Err(ReadFailure::Integrity.into());
        }
        Ok(receipt)
    }
}
fn matches_info(t: &plans::Transfer, i: &remote::Info) -> bool {
    t.generation() == Some(i.generation)
        && t.target() == &i.target
        && t.size() == i.size
        && t.payload_sha256() == &i.payload_sha256
        && i.guard == remote::Guard::Missing
        && i.anchor.is_none()
}
