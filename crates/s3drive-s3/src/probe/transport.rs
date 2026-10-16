//! Fixture-only preflight and deletion; kept outside the reusable transport.
use super::{Plan, payload};
#[cfg(test)]
pub(super) use crate::transport::MAX_PUT;
pub(super) use crate::transport::{Condition, Intent, Outcome, Session};
use crate::{Error, ErrorKind, Result, error::sdk_error};

impl Session {
    pub async fn preflight(&self, plan: &Plan) -> Result<()> {
        self.ensure_unversioned().await?;
        let page = self
            .reader
            .list_page(&plan.relative_prefix, None, 1000)
            .await?;
        if !page.objects.is_empty() || !page.directories.is_empty() || page.next_token.is_some() {
            return Err(Error::new(
                "probe-prefix-not-empty",
                ErrorKind::InvalidInput,
            ));
        }
        for index in 0..plan.keys.len() {
            if self.head(&plan.relative_key(index)).await?.is_some() {
                return Err(Error::new(
                    "probe-key-already-exists",
                    ErrorKind::InvalidInput,
                ));
            }
        }
        Ok(())
    }
    pub async fn ensure_unversioned(&self) -> Result<()> {
        let output = self
            .bounded("probe-versioning", async {
                Box::pin(
                    self.reader
                        .client
                        .get_bucket_versioning()
                        .bucket(&self.reader.source.bucket)
                        .send(),
                )
                .await
                .map_err(|e| sdk_error("probe-versioning", e))
            })
            .await?;
        // This disposable-object probe does not manage retained object versions.
        if output.status().is_some() {
            return Err(Error::new(
                "probe-unversioned-bucket-required",
                ErrorKind::Unsupported,
            ));
        }
        Ok(())
    }
    /// Test-fixture disposal only: exact plan key, known operation marker and
    /// verified bytes in a reserved, unversioned probe namespace. This is not
    /// the application's delete protocol and performs no conditional fallback.
    pub async fn cleanup(&self, plan: &Plan, index: usize) -> Result<()> {
        let key = plan.relative_key(index);
        let Some(head) = self.head(&key).await? else {
            return Ok(());
        };
        if head.revision.version_id.is_some() {
            return Err(Error::new(
                "probe-versioned-cleanup-refused",
                ErrorKind::Unsupported,
            ));
        }
        let mut matched = false;
        for variant in 0..4 {
            let expected = plan.intent(index, variant, Condition::Missing);
            if head.operation.as_deref() == Some(&expected.operation)
                && head.checksum.as_deref() == Some(&expected.checksum)
                && head.size == payload(index, variant).len() as u64
                && self.verify(&expected).await?.is_some()
            {
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(Error::new(
                "probe-unowned-cleanup-refused",
                ErrorKind::InvalidResponse,
            ));
        }
        let full_key = self.reader.full_key(&key)?;
        self.bounded("probe-cleanup", async {
            Box::pin(
                self.reader
                    .client
                    .delete_object()
                    .bucket(&self.reader.source.bucket)
                    .key(full_key)
                    .send(),
            )
            .await
            .map_err(|e| sdk_error("probe-cleanup", e))?;
            Ok(())
        })
        .await?;
        if self.head(&key).await?.is_some() {
            return Err(Error::new(
                "probe-cleanup-not-empty",
                ErrorKind::InvalidResponse,
            ));
        }
        Ok(())
    }
}
