//! Client-local metadata intents. Persist exact admission and request before RPC.
mod changes;
mod identity;
mod removals;
use crate::{
    Result, State,
    namespace::{self, Apply, Mutation, Receipt, Store},
};
pub use changes::Entry;
pub use removals::{OwnedRemoval, TransitionCancellation};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
const LIMIT: i64 = 4096;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub request: Apply,
    pub store: Store,
    pub source: [u8; 32],
    pub fingerprint: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<Entry>,
}
impl std::fmt::Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NamespaceInput([private])")
    }
}
impl Input {
    fn validate(&self) -> Result<()> {
        self.request
            .validate()
            .map_err(|_| "invalid namespace request")?;
        if !matches!(
            self.request.mutation,
            Mutation::CreateEntry { .. }
                | Mutation::CreateDeferredEntry { .. }
                | Mutation::MoveEntry { .. }
                | Mutation::RemoveEntry { .. }
                | Mutation::ArmRemoval { .. }
                | Mutation::PrepareRemoval { .. }
                | Mutation::TransitionRemoval { .. }
                | Mutation::CancelRemoval { .. }
                | Mutation::CommitRemoval { .. }
        ) || !self.store.valid()
            || !namespace::publication::digest(&self.fingerprint)
        {
            return Err("invalid namespace intent binding".into());
        }
        if let Mutation::CreateEntry {
            store: Some(store), ..
        }
        | Mutation::CreateDeferredEntry {
            store: Some(store), ..
        } = &self.request.mutation
            && *store != self.store.id
        {
            return Err("namespace placement changed".into());
        }
        changes::validate(self)?;
        if serde_json::to_vec(self)?.len() > 8192 {
            return Err("namespace intent too large".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub input: Input,
    pub outcome: Option<namespace::Result<Receipt>>,
}
fn hash(r: &Record) -> Result<Vec<u8>> {
    let mut h = Sha256::new();
    h.update(b"s3drive-namespace-intent-v1\0");
    h.update(serde_json::to_vec(&(&r.input, &r.outcome))?);
    Ok(h.finalize().to_vec())
}
fn checked_outcome(input: &Input, outcome: &namespace::Result<Receipt>) -> Result<()> {
    let receipt = match outcome {
        Err(
            namespace::Error::Internal
            | namespace::Error::DeletePending
            | namespace::Error::Unavailable
            | namespace::Error::OperationMismatch,
        ) => return Err("namespace outcome remains unknown; intent retained".into()),
        Err(_) => return Ok(()),
        Ok(r) => r,
    };
    if !matches!(
        input.request.mutation,
        Mutation::CreateEntry { .. } | Mutation::CreateDeferredEntry { .. }
    ) {
        changes::effects(input, receipt)?;
        return Ok(());
    }
    let placement = match &input.request.mutation {
        Mutation::CreateEntry { node_kind, .. }
        | Mutation::CreateDeferredEntry { node_kind, .. }
            if *node_kind == namespace::Kind::EmptyFile =>
        {
            Some(input.store.clone())
        }
        _ => None,
    };
    let expected = namespace::creation::expected(&input.request, placement, receipt.sequence)
        .map_err(|_| "invalid initial namespace receipt")?;
    if receipt.node != expected || receipt.artifact != namespace::creation::artifact(&input.request)
    {
        return Err("namespace receipt does not match frozen intent".into());
    }
    Ok(())
}

pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT operation_id,input,outcome,state_hash FROM namespace_intents LIMIT 0")?;
    db.prepare("SELECT singleton,owner_id,state_hash FROM client_identity LIMIT 0")?;
    db.prepare("SELECT singleton,state_hash FROM retained_client_identity LIMIT 0")?;
    Ok(())
}
fn load(db: &Connection, id: &str) -> Result<Option<Record>> {
    let shape:Option<bool>=db.query_row("SELECT length(input)<=8192 AND coalesce(length(outcome),0)<=65536 AND length(state_hash)=32 FROM namespace_intents WHERE operation_id=?",[id],|r|r.get(0)).optional()?;
    let Some(shape) = shape else { return Ok(None) };
    if !shape {
        return Err("namespace record shape changed".into());
    }
    type Row = (Vec<u8>, Option<Vec<u8>>, Vec<u8>);
    let (input, outcome, checksum): Row = db.query_row(
        "SELECT input,outcome,state_hash FROM namespace_intents WHERE operation_id=?",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let r = Record {
        input: serde_json::from_slice(&input)?,
        outcome: outcome.map(|v| serde_json::from_slice(&v)).transpose()?,
    };
    r.input.validate()?;
    removals::check_identity(db, &r.input)?;
    if r.input.request.operation != id || hash(&r)? != checksum {
        return Err("namespace intent integrity failure".into());
    }
    if let Some(o) = &r.outcome {
        checked_outcome(&r.input, o)?;
    }
    Ok(Some(r))
}
pub(crate) fn authorize_publication(db: &Connection, input: &crate::managed::Input) -> Result<()> {
    let reserve = input.reservation()?;
    if pending_for_node(db, &reserve.volume, &reserve.node)? {
        return Err("recover namespace outcome before preparing publication".into());
    }
    let owned = removals::owned(db, &reserve.volume, &reserve.node)?;
    match (
        owned,
        reserve.guard.as_ref().and_then(|g| g.removal.as_ref()),
    ) {
        (None, None) => Ok(()),
        (Some(owned), Some(fence))
            if owned.fence == *fence
                && owned.input.store == input.store
                && owned.input.source == input.source
                && owned.input.fingerprint == input.fingerprint
                && reserve.expected_revision >= owned.receipt.sequence =>
        {
            Ok(())
        }
        _ => Err("publication requires this client's exact acknowledged removal fence".into()),
    }
}
pub(crate) fn pending_for_node(db: &Connection, volume: &str, node: &str) -> Result<bool> {
    removals::blocks_for_node(db, volume, node, None)
}

pub struct Journal<'a> {
    state: &'a mut State,
    options: crate::journal::Options,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State) -> Result<Self> {
        Self::with_options(state, crate::journal::Options::default())
    }
    pub fn with_options(state: &'a mut State, options: crate::journal::Options) -> Result<Self> {
        state.verify_staging_storage()?;
        Ok(Self { state, options })
    }
    pub fn get(&self, id: &str) -> Result<Record> {
        load(&self.state.connection, id)?.ok_or_else(|| "unknown namespace intent".into())
    }
    pub fn client_identity(&mut self) -> Result<String> {
        identity::ensure(self.state)
    }
    pub fn owned_removal(&self, volume: &str, node: &str) -> Result<Option<OwnedRemoval>> {
        removals::owned(&self.state.connection, volume, node)
    }
    /// Inspect a verified cancellation without inventing the transition outcome.
    pub fn transition_cancellation(
        &self,
        operation: &str,
    ) -> Result<Option<TransitionCancellation>> {
        removals::transition_cancellation(&self.state.connection, operation)
    }
    pub fn begin(&mut self, input: Input) -> Result<Record> {
        input.validate()?;
        if let Some(r) = load(&self.state.connection, &input.request.operation)? {
            if r.input != input {
                return Err("namespace intent input changed".into());
            }
            return Ok(r);
        }
        removals::check_begin(&self.state.connection, &input)?;
        for entry in &input.entries {
            if removals::blocks_for_node(
                &self.state.connection,
                &input.request.volume,
                &entry.id,
                Some(&input),
            )? {
                return Err(
                    "recover pending namespace effect before another change to this node".into(),
                );
            }
        }
        for (i, entry) in input.entries.iter().enumerate() {
            crate::logical::check_namespace_entry(
                self.state,
                self.options,
                &input.request.volume,
                entry,
                i == 0 && matches!(input.request.mutation, Mutation::MoveEntry { .. }),
            )?;
        }
        self.state
            .register_source(&input.source, &input.fingerprint)?;
        let r = Record {
            input,
            outcome: None,
        };
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: i64 =
            tx.query_row("SELECT count(*) FROM namespace_intents", [], |r| r.get(0))?;
        if count >= LIMIT {
            return Err("namespace intent limit reached".into());
        }
        let inserted = tx.execute(
            "INSERT INTO namespace_intents VALUES(?1,?2,NULL,?3)",
            params![
                r.input.request.operation,
                serde_json::to_vec(&r.input)?,
                hash(&r)?
            ],
        )?;
        if inserted != 1
            || load(&tx, &r.input.request.operation)?
                .is_none_or(|saved| saved.input != r.input || saved.outcome.is_some())
        {
            return Err("namespace intent was not durably persisted".into());
        }
        tx.commit()?;
        Ok(r)
    }
    pub fn acknowledge(&mut self, id: &str, outcome: namespace::Result<Receipt>) -> Result<Record> {
        let mut r = self.get(id)?;
        checked_outcome(&r.input, &outcome)?;
        if let Some(old) = &r.outcome {
            if old != &outcome {
                return Err("namespace outcome changed".into());
            }
            return Ok(r);
        }
        let updates = if let Ok(receipt) = &outcome {
            if r.input.entries.is_empty() {
                Vec::new()
            } else {
                let nodes = changes::effects(&r.input, receipt)?;
                let mut updates = Vec::new();
                for (entry, after) in r.input.entries.iter().zip(nodes.iter()) {
                    if let Some(update) = crate::logical::namespace_observation(
                        self.state,
                        self.options,
                        &r.input.request.volume,
                        entry,
                        after,
                    )? {
                        updates.push(update);
                    }
                }
                updates
            }
        } else {
            Vec::new()
        };
        r.outcome = Some(outcome);
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rows=tx.execute("UPDATE namespace_intents SET outcome=?1,state_hash=?2 WHERE operation_id=?3 AND outcome IS NULL",params![serde_json::to_vec(r.outcome.as_ref().unwrap())?,hash(&r)?,id])?;
        if rows != 1 {
            return Err("namespace receipt was not durably updated".into());
        }
        for update in updates {
            update.apply(&tx)?;
        }
        removals::check_capacity(&removals::records(&tx)?)?;
        if load(&tx, id)?.as_ref().and_then(|r| r.outcome.as_ref()) != r.outcome.as_ref() {
            return Err("namespace acknowledgment readback mismatch".into());
        }
        tx.commit()?;
        self.get(id)
    }
    pub fn pending(&self, after: Option<&str>, limit: u16) -> Result<Vec<Record>> {
        if !(1..=1000).contains(&limit) {
            return Err("invalid namespace pending limit".into());
        }
        let ids = self
            .state
            .connection
            .prepare("SELECT operation_id FROM namespace_intents ORDER BY operation_id LIMIT 4097")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.len() > LIMIT as usize {
            return Err("namespace intent quota or integrity failure".into());
        }
        let mut out = Vec::new();
        for id in ids {
            let r = self.get(&id)?;
            if r.outcome.is_none()
                && after.is_none_or(|a| id.as_str() > a)
                && out.len() < usize::from(limit)
            {
                out.push(r)
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> Input {
        Input {
            request: Apply {
                volume: format!("{:032x}", 1),
                operation: format!("{:032x}", 10),
                expected_sequence: 0,
                mutation: Mutation::CreateEntry {
                    parent: namespace::ROOT.into(),
                    parent_revision: 0,
                    store: Some(format!("{:032x}", 200)),
                    name: "資料 🦀".into(),
                    node_kind: namespace::Kind::EmptyFile,
                },
            },
            store: Store {
                id: format!("{:032x}", 200),
                route: "a".repeat(64),
                root: "writable/.s3drive/".into(),
            },
            source: [1; 32],
            fingerprint: "b".repeat(64),
            entries: Vec::new(),
        }
    }
    fn receipt(i: &Input) -> Receipt {
        let Mutation::CreateEntry {
            parent,
            name,
            node_kind,
            ..
        } = &i.request.mutation
        else {
            panic!()
        };
        Receipt {
            sequence: 1,
            node: namespace::Node {
                id: i.request.operation.clone(),
                parent: Some(parent.clone()),
                name: name.clone(),
                kind: *node_kind,
                revision: 1,
                deleted: false,
                placement: Some(i.store.clone()),
                payload: None,
            },
            artifact: None,
        }
    }
    #[test]
    fn namespace_intent_replays_exact_admission_and_reopens_after_unknown_result() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("client");
        let mut s = State::open_managed(&path).unwrap();
        let i = input();
        let mut j = Journal::new(&mut s).unwrap();
        assert!(j.begin(i.clone()).unwrap().outcome.is_none());
        assert_eq!(j.begin(i.clone()).unwrap().input, i);
        let mut wrong = i.clone();
        wrong.fingerprint = "c".repeat(64);
        assert!(j.begin(wrong).is_err());
        assert!(
            j.acknowledge(&i.request.operation, Err(namespace::Error::Unavailable))
                .is_err()
        );
        assert!(
            j.acknowledge(&i.request.operation, Err(namespace::Error::DeletePending))
                .is_err()
        );
        assert!(j.pending(None, 10).unwrap()[0].outcome.is_none());
        drop(s);
        let mut s = State::open_managed(&path).unwrap();
        let mut j = Journal::new(&mut s).unwrap();
        assert_eq!(j.pending(None, 10).unwrap().len(), 1);
        let mut bad = receipt(&i);
        bad.node.name = "other".into();
        assert!(j.acknowledge(&i.request.operation, Ok(bad)).is_err());
        let r = j
            .acknowledge(&i.request.operation, Ok(receipt(&i)))
            .unwrap();
        assert_eq!(
            j.acknowledge(&i.request.operation, Ok(receipt(&i)))
                .unwrap(),
            r
        );
        assert!(j.pending(None, 10).unwrap().is_empty());
        assert!(
            j.acknowledge(&i.request.operation, Err(namespace::Error::Conflict))
                .is_err()
        );
        s.connection
            .execute("UPDATE namespace_intents SET state_hash=zeroblob(32)", [])
            .unwrap();
        assert!(Journal::new(&mut s).unwrap().pending(None, 10).is_err());
    }
    #[test]
    fn namespace_acknowledgment_update_failure_cannot_return_success() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("client")).unwrap();
        let i = input();
        Journal::new(&mut s).unwrap().begin(i.clone()).unwrap();
        s.connection.execute_batch("CREATE TRIGGER ignore_namespace_ack BEFORE UPDATE ON namespace_intents BEGIN SELECT RAISE(IGNORE); END").unwrap();
        assert!(
            Journal::new(&mut s)
                .unwrap()
                .acknowledge(&i.request.operation, Ok(receipt(&i)))
                .is_err()
        );
        assert_eq!(
            Journal::new(&mut s)
                .unwrap()
                .pending(None, 10)
                .unwrap()
                .len(),
            1
        );
        assert!(s.namespace_initialize().is_err());
    }
    #[test]
    fn sqlite_full_rolls_back_namespace_outcome_and_allows_exact_retry() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("client")).unwrap();
        let mut inputs = Vec::new();
        for n in 1..=64 {
            let mut i = input();
            i.request.operation = format!("{n:032x}");
            if let Mutation::CreateEntry { name, .. } = &mut i.request.mutation {
                *name = format!("{}-{n}", "é".repeat(120));
            }
            Journal::new(&mut s).unwrap().begin(i.clone()).unwrap();
            inputs.push(i);
        }
        s.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
        let pages: i64 = s
            .connection
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        s.connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        let mut failed = None;
        for i in inputs {
            match Journal::new(&mut s)
                .unwrap()
                .acknowledge(&i.request.operation, Ok(receipt(&i)))
            {
                Ok(_) => {}
                Err(e) => {
                    assert!(e.to_string().to_lowercase().contains("full"), "{e}");
                    assert!(
                        Journal::new(&mut s)
                            .unwrap()
                            .get(&i.request.operation)
                            .unwrap()
                            .outcome
                            .is_none()
                    );
                    failed = Some(i);
                    break;
                }
            }
        }
        let failed = failed.expect("bounded pages must reject growing namespace receipts");
        s.connection
            .pragma_update(None, "max_page_count", pages + 100)
            .unwrap();
        assert!(
            Journal::new(&mut s)
                .unwrap()
                .acknowledge(&failed.request.operation, Ok(receipt(&failed)))
                .unwrap()
                .outcome
                .is_some()
        );
    }
}
