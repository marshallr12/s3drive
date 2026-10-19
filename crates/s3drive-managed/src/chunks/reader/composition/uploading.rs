//! Managed source checks around mixed physical uploads. No authority root CAS,
//! sparse clean acknowledgment, reclamation or native write activation.
use super::*;
use s3drive_s3::uploads::Executor;
use s3drive_state::{remote, sparse_views::preparation as plans};

#[derive(Clone)]
pub struct MixedUploader {
    preparation: Preparation,
    executor: Executor,
}
impl fmt::Debug for MixedUploader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MixedDependencyUploader([private; no root commit])")
    }
}
impl Preparation {
    /// One exact private owner. Neither construction nor an offline transfer is
    /// a provider capability; every fresh send still needs source/write checks.
    pub fn uploader(&self, executor: Executor) -> Result<MixedUploader> {
        if !self.client.same_storage(&executor.storage()) {
            return Err(ReadFailure::Binding.into());
        }
        Ok(MixedUploader {
            preparation: self.clone(),
            executor,
        })
    }
}
impl MixedUploader {
    pub fn plan(&self) -> Id {
        self.preparation.id
    }
    fn binding(&self, t: &plans::Transfer) -> Result<()> {
        if self.executor.recovery_target(t.target().object_key())? != *t.target() {
            return Err(ReadFailure::Binding.into());
        }
        Ok(())
    }
    /// Exact physical completion only. Inherited objects and proved holes
    /// return None, with no fabricated upload receipt. Claimed attempts observe
    /// their original identity before any fresh source authorization/composition.
    pub async fn upload_object(&self, p: plans::Position) -> Result<Option<remote::FlushReceipt>> {
        bounded(
            self.preparation.edited.base.0.reader.0.options,
            Box::pin(self.upload_inner(p)),
        )
        .await
    }
    async fn upload_inner(&self, p: plans::Position) -> Result<Option<remote::FlushReceipt>> {
        let client = &self.preparation.client;
        let plan = self.plan();
        let info = self.preparation.info().await?;
        info.root()?.ok_or(ReadFailure::Binding)?;
        let Some(t) = client.transfer(plan, p).await? else {
            self.preparation.dependency(p.level, p.slot).await?;
            return Ok(None);
        };
        self.binding(&t)?;
        let prior = if let Some(g) = t.generation() {
            self.executor.storage().uploads().for_generation(g).await?
        } else {
            None
        };
        if prior
            .as_ref()
            .is_some_and(|i| i.phase != remote::Phase::Prepared)
        {
            return MixedRecovery::new(client.clone(), self.executor.clone())?
                .recover_object(plan, p)
                .await
                .map(Some);
        }
        self.executor
            .check_create(t.target().object_key(), t.size())?;
        let checked = self.preparation.dependency(p.level, p.slot).await?;
        let plans::Dependency::New {
            role,
            reference,
            bytes,
        } = checked.content()
        else {
            return Err(ReadFailure::Integrity.into());
        };
        if *role != t.role()
            || reference != t.reference()
            || bytes.len() as u64 != t.size()
            || <[u8; 32]>::from(Sha256::digest(bytes)) != *t.payload_sha256()
        {
            return Err(ReadFailure::Integrity.into());
        }
        // A page's local graph is structural. Check its direct inherited/hole/
        // new-data semantics using the actual pinned source, then require exact
        // historical receipts before any fresh parent materialization/send.
        for child in client.children(plan, p).await? {
            self.preparation.dependency(child.level, child.slot).await?;
        }
        client.require_dependencies(plan, p).await?;
        let staged = client
            .materialize(plan, p, &self.executor.storage())
            .await?
            .ok_or(ReadFailure::Integrity)?;
        if staged.target() != t.target()
            || staged.reference() != t.reference()
            || staged.size() != t.size()
            || staged.payload_sha256() != t.payload_sha256()
        {
            return Err(ReadFailure::Integrity.into());
        }
        let receipt = self
            .executor
            .flush(staged.generation().ok_or(ReadFailure::Binding)?)
            .await?;
        if !staged.receipt_matches(&receipt) {
            return Err(ReadFailure::Integrity.into());
        }
        Ok(Some(receipt))
    }
    /// Walk only retained records. The resulting receipt describes historical
    /// physical uploads/provenance, never an authorized root commit or flush.
    pub async fn upload_tree(&self) -> Result<plans::TreeReceipt> {
        match self.preparation.client.tree_receipt(self.plan()).await {
            Ok(receipt) => return Ok(receipt),
            Err(e)
                if e.downcast_ref::<plans::TransferFailure>()
                    == Some(&plans::TransferFailure::Incomplete) => {}
            Err(e) => return Err(e),
        }
        let mut after = None;
        loop {
            let page = self
                .preparation
                .client
                .transfers(self.plan(), after, 128)
                .await?;
            if page.is_empty() {
                break;
            }
            for transfer in page {
                let p = transfer.position();
                self.upload_object(p).await?;
                after = Some(p);
                tokio::task::yield_now().await;
            }
        }
        self.preparation.client.tree_receipt(self.plan()).await
    }
}
