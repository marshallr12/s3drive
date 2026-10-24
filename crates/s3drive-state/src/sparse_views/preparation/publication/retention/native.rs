//! Compact proof for a closed linked-native publisher, not an open capability.
use super::*;
use crate::namespace_client::opens::{self, Action};
use crate::sparse_views::preparation::native as publisher;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Terminal {
    target: Id,
    plan: Id,
    generation: [u8; 16],
    original: opens::OpenInput,
    owner: String,
    first_operation: Id,
    suffix: opens::TerminalSuffix,
    close: String,
    completion: String,
    frozen: Vec<Id>,
    final_view: Id,
    frontier: u64,
}
impl Terminal {
    pub(super) fn capture(
        db: &Connection,
        info: &Info,
        view: Id,
        before: &crate::sparse_views::preparation::ledger::Snapshot,
        records: &[opens::OpenRecord],
    ) -> Result<Self> {
        let native = info.native_ownership().ok_or(Failure::Corrupt)?;
        let record = records
            .iter()
            .find(|r| r.input.id == native.open())
            .ok_or(Failure::Incomplete)?;
        let close = record.close().ok_or(Failure::Incomplete)?;
        let completion = record
            .events
            .iter()
            .find_map(|e| match &e.action {
                Action::Completed { callback } if callback == &close.id => Some(e.id.clone()),
                _ => None,
            })
            .ok_or(Failure::Incomplete)?;
        let mut frozen = record
            .events
            .iter()
            .filter_map(|e| match e.action {
                Action::FreezeSparse { plan, .. } => Some(plan),
                _ => None,
            })
            .collect::<Vec<_>>();
        frozen.sort();
        if !record.events.iter().any(|e| {
            matches!(&e.action,
            Action::CompleteSparse {callback, view: Some(v), frozen: f}
            if callback == &close.id && *v == view
                && hash(b"s3drive-native-sparse-completion-plans-v1\0", &frozen).ok() == Some(*f))
        }) {
            return Err(Failure::Incomplete.into());
        }
        let operation = record
            .events
            .iter()
            .find_map(|e| match &e.action {
                Action::OwnSparseEpoch {
                    binding, operation, ..
                } if binding.as_ref() == info.view().binding() => Some(*operation),
                _ => None,
            })
            .ok_or(Failure::Incomplete)?;
        let first = before
            .views
            .records
            .values()
            .find(|v| v.record.operation == operation)
            .ok_or(Failure::Corrupt)?;
        let proof = Self {
            target: info.view().binding().target()?,
            plan: info.id()?,
            generation: *info.generation(),
            original: record.input.clone(),
            owner: record.owner.clone(),
            first_operation: first.record.operation,
            suffix: opens::TerminalSuffix::capture(db, record)?,
            close: close.id.clone(),
            completion,
            frozen,
            final_view: view,
            frontier: crate::chunk_mount::admission::frontier(db)?,
        };
        proof.validate(db, info, view)?;
        Ok(proof)
    }
    pub(super) fn digest(&self) -> Result<Id> {
        hash(b"s3drive-native-terminal-witness-v1\0", self)
    }
    pub(super) fn verify_witness(
        &self,
        mount: &crate::chunk_mount::Binding,
        target: &Id,
    ) -> Result<()> {
        if crate::chunk_mount::admission::witness_of(mount, target) != Some(self.digest()?) {
            return Err(Failure::Corrupt.into());
        }
        Ok(())
    }
    pub(super) fn validate(
        &self,
        db: &Connection,
        info: &Info,
        view: Id,
    ) -> Result<crate::chunk_mount::Binding> {
        self.validate_in(db, info, view, None)
    }
    pub(super) fn validate_scoped(
        &self,
        scope: &crate::chunk_mount::ReadScope<'_>,
        info: &Info,
        view: Id,
    ) -> Result<crate::chunk_mount::Binding> {
        self.validate_in(scope.connection(), info, view, Some(scope))
    }
    fn validate_in(
        &self,
        db: &Connection,
        info: &Info,
        view: Id,
        scope: Option<&crate::chunk_mount::ReadScope<'_>>,
    ) -> Result<crate::chunk_mount::Binding> {
        // This fresh fully validated owner proof is reused only in this invocation.
        let mount = match scope {
            Some(scope) => scope.binding().clone(),
            None => crate::chunk_mount::load(db)?.ok_or(Failure::Corrupt)?,
        };
        let valid_id = |id: &str| {
            id.len() == 32
                && id
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                && id.bytes().any(|c| c != b'0')
        };
        if self.target != info.view().binding().target()?
            || self.plan != info.id()?
            || self.generation != *info.generation()
            || !valid_id(&self.close)
            || !valid_id(&self.completion)
            || self.close == self.completion
            || self.final_view != view
            || self.frontier > crate::chunk_mount::admission::frontier_of(&mount)?
            || self.frozen.is_empty()
            || self.frozen.len() > 256
            || self.frozen.windows(2).any(|pair| pair[0] >= pair[1])
            || !self.frozen.contains(&info.id()?)
        {
            return Err(Failure::Corrupt.into());
        }
        info.native_ownership()
            .ok_or(Failure::Corrupt)?
            .validate_terminal(
                &mount,
                &self.original,
                &self.owner,
                info.view().binding(),
                self.first_operation,
            )?;
        self.suffix.validate(
            &self.original,
            &self.owner,
            &self.close,
            &self.completion,
            &self.frozen,
            self.final_view,
        )?;
        match scope {
            Some(scope) => publisher::verify_terminal_plan_scoped(scope, info)?,
            None => publisher::verify_terminal_plan(db, &mount, info)?,
        }
        Ok(mount)
    }
}
