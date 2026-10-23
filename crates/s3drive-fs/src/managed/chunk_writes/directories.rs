//! Directory effects do not alter or flush file content. Journal before RPC.
use super::*;
use s3drive_authority::{Apply, Kind, Mutation};
use s3drive_state::namespace_client::{Input, Record};
fn directory(input: &Input, volume: &str) -> Result<()> {
    if input.request.volume != volume
        || !input.entries.is_empty()
        || input.destination_control.is_some()
        || !matches!(
            &input.request.mutation,
            Mutation::CreateEntry {
                node_kind: Kind::Directory,
                store: None,
                ..
            }
        )
    {
        return Err(Error::Configuration);
    }
    Ok(())
}
impl Writes {
    async fn apply_directory(&self, input: Input) -> Result<RemoteNode> {
        directory(&input, &self.volume)?;
        let ledger = self._capability.namespace();
        let record = ledger.begin(input).await.map_err(error)?;
        let record = if record.outcome.is_some() {
            record
        } else {
            let reply = self
                .authority
                .request(&Request::Apply {
                    request: record.input.request.clone(),
                })
                .await
                .map_err(error)?;
            let outcome = match reply {
                Reply::Applied(receipt) => Ok(receipt),
                Reply::Error(e) => Err(e),
                _ => return Err(Error::Conflict),
            };
            ledger
                .acknowledge(record.input.request.operation.clone(), outcome)
                .await
                .map_err(error)?
        };
        Self::directory_result(record)
    }
    fn directory_result(record: Record) -> Result<RemoteNode> {
        record
            .outcome
            .ok_or(Error::Conflict)?
            .map(|r| r.node)
            .map_err(|e| error(s3drive_managed::Failure::Authority(e)))
    }
    pub(super) async fn recover_directories(&self) -> Result<()> {
        let ledger = self._capability.namespace();
        let mut after = None;
        loop {
            let page = ledger.pending(after, 128).await.map_err(error)?;
            if page.is_empty() {
                return Ok(());
            }
            after = page.last().map(|r| r.input.request.operation.clone());
            for r in page {
                // No fresh policy/feature lookup may replace or block a saved
                // request. The owner-bound journal validates retained controls.
                #[cfg(feature = "native-sparse-candidate")]
                if self.native_lifecycle {
                    self.native_saved_change(r.input).await?;
                    continue;
                }
                self.apply_directory(r.input).await?;
            }
        }
    }
    pub(super) async fn create_directory_inner(
        &self,
        parent: &str,
        name: &str,
    ) -> Result<RemoteNode> {
        let _serial = self.open_serial.lock().await;
        {
            let _recovery = self.recovery_serial.lock().await;
            self.recover_directories().await?;
        }
        if !self.permits_directory() {
            return Err(Error::ReadOnly);
        }
        if s3drive_core::names::collision_key(name).is_none() {
            return Err(Error::InvalidName);
        }
        let Reply::Features(features) = self
            .authority
            .request(&Request::Features {})
            .await
            .map_err(error)?
        else {
            return Err(Error::Configuration);
        };
        if !features.iter().any(|f| f == "namespace-create-v1") {
            return Err(Error::ReadOnly);
        }
        let Reply::Info(info) = self
            .authority
            .request(&Request::Info {})
            .await
            .map_err(error)?
        else {
            return Err(Error::Configuration);
        };
        if info.id != self.volume || info.name_policy != s3drive_core::names::PORTABLE {
            return Err(Error::Configuration);
        }
        let node = self.reader.node(parent).await.map_err(error)?;
        if node.deleted {
            return Err(Error::NotFound);
        }
        if node.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        let (store, executor, _) = self
            .stores
            .get(&self.default_store)
            .ok_or(Error::ReadOnly)?;
        executor
            .check_create(&format!("{}binding", store.root), 0)
            .map_err(error)?;
        let target = executor
            .recovery_target(&format!("{}binding", store.root))
            .map_err(error)?;
        let input = Input {
            request: Apply {
                volume: self.volume.clone(),
                operation: hex(&fresh()?[..16]),
                expected_sequence: info.sequence.max(node.revision),
                mutation: Mutation::CreateEntry {
                    parent: node.id,
                    parent_revision: node.revision,
                    store: None,
                    name: name.into(),
                    node_kind: Kind::Directory,
                },
            },
            store: store.clone(),
            source: *target.source(),
            fingerprint: target.fingerprint().into(),
            entries: Vec::new(),
            destination_control: None,
        };
        self.apply_directory(input).await
    }
}
