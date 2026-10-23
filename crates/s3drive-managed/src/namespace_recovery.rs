//! Cold replay of already saved chunk namespace effects. No executor, provider
//! client, policy, source bodies, fresh requests or native callback fabrication.
use super::*;
use s3drive_state::namespace_client::{self as local, opens::OpenRecord};
use std::{path::Path, time::Duration};
struct Shared {
    log: local::recovery::RecoveryClient,
    authority: WireClient,
    slot: Semaphore,
    deadline: Duration,
}
#[derive(Clone)]
pub struct Recovery(Arc<Shared>);
fn id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && value != s3drive_authority::ROOT
}
impl Recovery {
    pub async fn open_existing(
        directory: &Path,
        authority: WireClient,
        volume: String,
        deadline: Duration,
    ) -> Result<Self> {
        if !id(&volume) || deadline.is_zero() || deadline > Duration::from_secs(300) {
            return Err(Failure::Binding.into());
        }
        let log = local::recovery::RecoveryClient::open(directory, volume).await?;
        Ok(Self(Arc::new(Shared {
            log,
            authority,
            slot: Semaphore::new(1),
            deadline,
        })))
    }
    /// Enumerate only bounded retained metadata, not source bodies or sessions.
    pub async fn open_records(&self) -> Result<Vec<OpenRecord>> {
        self.0.log.open_records().await
    }
    /// Reconcile only existing disposition links with their original deterministic
    /// bookkeeping identity. No callback, reply, completion or session is added.
    pub async fn recover_controller_links(&self) -> Result<()> {
        for record in self.open_records().await? {
            for event in &record.events {
                let local::opens::Action::Link { callback, input } = &event.action else {
                    continue;
                };
                if record.events.iter().any(|e| matches!(&e.action, local::opens::Action::Acknowledge { operation, .. } if operation == &input.request.operation)) { continue; }
                let role = ["set", "restore", "promote", "commit", "retain", "release"]
                    .into_iter()
                    .find(|role| {
                        crate::disposition::identity(&record.input.id, callback, role)
                            == input.request.operation
                            && crate::disposition::identity(
                                &record.input.id,
                                callback,
                                &format!("link-{role}"),
                            ) == event.id
                    })
                    .ok_or(Failure::Binding)?;
                self.recover_open_link(
                    record.input.id.clone(),
                    crate::disposition::identity(
                        &record.input.id,
                        callback,
                        &format!("ack-{role}"),
                    ),
                    input.request.operation.clone(),
                )
                .await?;
            }
        }
        Ok(())
    }
    pub async fn records(&self) -> Result<Vec<local::Record>> {
        self.0.log.records().await
    }
    pub async fn open_record(&self, open: String) -> Result<OpenRecord> {
        self.0.log.open_record(open).await
    }
    /// A retained rejection is returned as an explicit outcome, never success.
    /// Unknown/unavailable replies are not persisted as terminal rejection.
    pub async fn recover(&self, operation: String) -> Result<local::Record> {
        let _slot = self.0.slot.try_acquire().map_err(|_| Failure::Busy)?;
        tokio::time::timeout(self.0.deadline, self.recover_inner(operation))
            .await
            .map_err(|_| Failure::Incomplete)?
    }
    async fn recover_inner(&self, operation: String) -> Result<local::Record> {
        let r = self.0.log.get(operation).await?;
        if r.outcome.is_some() {
            return Ok(r);
        }
        let outcome = match self
            .0
            .authority
            .request(&Request::Apply {
                request: r.input.request.clone(),
            })
            .await?
        {
            Reply::Applied(receipt) => Ok(receipt),
            Reply::Error(error) => Err(error),
            _ => return Err(Failure::Protocol.into()),
        };
        self.0.log.acknowledge(r.input.request, outcome).await
    }
    /// Reconcile an existing open link, using the caller's original bookkeeping
    /// event ID. This does not infer a callback result, completion or live open.
    pub async fn recover_open_link(
        &self,
        open: String,
        event: String,
        operation: String,
    ) -> Result<OpenRecord> {
        if !id(&open) || !id(&event) || !id(&operation) {
            return Err(Failure::Binding.into());
        }
        let _slot = self.0.slot.try_acquire().map_err(|_| Failure::Busy)?;
        tokio::time::timeout(self.0.deadline, async {
            let r = self.0.log.open_record(open.clone()).await?;
            let linked = r
                .events
                .iter()
                .find_map(|e| match &e.action {
                    local::opens::Action::Link { input, .. }
                        if input.request.operation == operation =>
                    {
                        Some(input.as_ref())
                    }
                    _ => None,
                })
                .ok_or(Failure::Binding)?;
            // Check an independent saved intent BEFORE any network effect. A
            // Link alone does not prove namespace admission ever completed.
            let saved = self.0.log.get(operation.clone()).await?;
            if saved.input != *linked {
                return Err(Failure::Binding.into());
            }
            self.recover_inner(operation.clone()).await?;
            self.0.log.acknowledge_open(open, event, operation).await
        })
        .await
        .map_err(|_| Failure::Incomplete)?
    }
    pub async fn close(&self) -> Result<()> {
        self.0.log.close().await
    }
}
