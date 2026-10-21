//! Sessions bind to an exclusive State acquisition, never a caller-supplied PID.
use super::*;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenProof {
    pub id: String,
    pub hash: Vec<u8>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryInput {
    pub id: String,
    pub session: String,
    pub volume: String,
    pub node: String,
    pub opens: Vec<OpenProof>,
    pub cancel: Option<Box<Input>>,
}
impl std::fmt::Debug for RecoveryInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionRecovery([private])")
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    closure: Option<Box<ActivationProof>>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryRecord {
    pub input: RecoveryInput,
    pub finished: bool,
}
pub(super) fn runtime(state: &State) -> Result<String> {
    state.verify_staging_storage()?;
    state
        .incarnation
        .clone()
        .ok_or_else(|| "session requires exclusive runtime ownership".into())
}
pub(super) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT session_id,volume_id,runtime_id,owner_id,state_hash FROM windows_mount_sessions LIMIT 0")?;
    db.prepare("SELECT recovery_id,volume_id,node_id,input,proof,state_hash FROM windows_session_recoveries LIMIT 0")?;
    db.prepare("SELECT open_id,recovery_id FROM recovered_windows_opens LIMIT 0")?;
    Ok(())
}
pub(super) fn binding(db: &Connection, session: &str, volume: &str) -> Result<String> {
    if !id(session) || !id(volume) {
        return Err("invalid mount session binding".into());
    }
    let shape:bool=db.query_row("SELECT length(CAST(volume_id AS BLOB))=32 AND length(CAST(runtime_id AS BLOB))=32 AND length(CAST(owner_id AS BLOB))=32 AND length(state_hash)=32 FROM windows_mount_sessions WHERE session_id=?",[session],|r|r.get(0))?;
    if !shape {
        return Err("mount session metadata bound failed".into());
    }

    let (v, r, owner, hash): (String,String,String,Vec<u8>) = db.query_row("SELECT volume_id,runtime_id,owner_id,state_hash FROM windows_mount_sessions WHERE session_id=?", [session], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))?;
    if !id(session)
        || !id(&r)
        || !id(&owner)
        || !id(&v)
        || v != volume
        || identity::read(db)?.as_ref() != Some(&owner)
        || hash != digest(b"s3drive-mount-session-v1\0", &(session, &v, &r, &owner))?
    {
        return Err("mount session binding changed".into());
    }
    Ok(r)
}
pub(super) fn current(db: &Connection, runtime: &str, session: &str, volume: &str) -> Result<()> {
    if binding(db, session, volume)? != runtime {
        return Err("mount session belongs to a different runtime owner".into());
    }
    Ok(())
}
fn record_hash(input: &RecoveryInput, proof: &Option<Proof>) -> Result<Vec<u8>> {
    digest(b"s3drive-session-recovery-v1\0", &(input, proof))
}
fn proof_for(db: &Connection, input: &Input) -> Result<ActivationProof> {
    let saved = super::super::load(db, &input.request.operation)?
        .ok_or("recovery closure proof missing")?;
    if saved.input != *input || !matches!(saved.outcome, Some(Ok(_))) {
        return Err("recovery closure is not acknowledged".into());
    }
    Ok(ActivationProof {
        input: Box::new(input.clone()),
        outcome_hash: super::super::hash(&saved)?,
    })
}
fn recovery(db: &Connection, key: &str) -> Result<(RecoveryInput, Option<Proof>)> {
    let shape:bool=db.query_row("SELECT length(CAST(volume_id AS BLOB))=32 AND length(CAST(node_id AS BLOB))=32 AND typeof(input)='blob' AND length(input) BETWEEN 1 AND 65536 AND (proof IS NULL OR (typeof(proof)='blob' AND length(proof) BETWEEN 1 AND 32768)) AND length(state_hash)=32 FROM windows_session_recoveries WHERE recovery_id=?",[key],|r|r.get(0))?;
    if !shape {
        return Err("session recovery metadata bound failed".into());
    }

    let (volume,node,bytes,proof,hash):(String,String,Vec<u8>,Option<Vec<u8>>,Vec<u8>)=db.query_row("SELECT volume_id,node_id,input,proof,state_hash FROM windows_session_recoveries WHERE recovery_id=?",[key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
    let input: RecoveryInput = serde_json::from_slice(&bytes)?;
    let proof: Option<Proof> = proof.map(|b| serde_json::from_slice(&b)).transpose()?;
    if bytes.len() > 65536
        || input.id != key
        || !id(key)
        || input.volume != volume
        || input.node != node
        || !id(&node)
        || input.opens.is_empty()
        || input.opens.len() > OPEN_LIMIT
        || record_hash(&input, &proof)? != hash
    {
        return Err("session recovery integrity failure".into());
    }
    binding(db, &input.session, &input.volume)?;
    let members = db.prepare("SELECT o.open_id,j.state_hash FROM recovered_windows_opens o JOIN windows_open_journal j USING(open_id) WHERE o.recovery_id=?")?
        .query_map([key], |r| Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?)))?
        .collect::<rusqlite::Result<std::collections::HashMap<_,_>>>()?;
    if members.len() != input.opens.len() {
        return Err("session recovery lost its frozen open index".into());
    }
    let mut seen = std::collections::HashSet::new();
    for open in &input.opens {
        creation::check_recovery_open(db, &open.id)?;
        if !id(&open.id)
            || open.hash.len() != 32
            || members.get(&open.id) != Some(&open.hash)
            || !seen.insert(&open.id)
        {
            return Err("recovery's frozen open history changed".into());
        }
    }
    if let Some(cancel) = &input.cancel {
        cancel.validate()?;
        if cancel.request.volume != input.volume
            || !matches!(&cancel.request.mutation,Mutation::CancelRemoval{node,..} if node==&input.node)
        {
            return Err("session recovery changed cancellation target".into());
        }
    }
    if let Some(proof) = &proof {
        if let Some(closure) = &proof.closure
            && (proof_for(db, &closure.input)? != **closure
                || closure.input.request.volume != input.volume
                || target(&closure.input) != Some(input.node.as_str())
                || !matches!(
                    closure.input.request.mutation,
                    Mutation::CancelRemoval { .. } | Mutation::CommitRemoval { .. }
                ))
        {
            return Err("session recovery closing proof changed".into());
        }
        if let Some(cancel) = &input.cancel
            && proof.closure.as_ref().map(|p| p.input.as_ref()) != Some(cancel.as_ref())
        {
            return Err("session recovery lost exact cancellation proof".into());
        }
    }
    Ok((input, proof))
}
pub(super) fn closed(db: &Connection, row: &Row, record: &OpenRecord) -> Result<bool> {
    let key: Option<String> = db
        .query_row(
            "SELECT recovery_id FROM recovered_windows_opens WHERE open_id=?",
            [&row.input.id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(key) = key else { return Ok(false) };
    let (input, proof) = recovery(db, &key)?;
    if !record.input.controlled
        || !record.input.session_bound
        || input.volume != record.input.volume
        || input.node != record.input.node
        || !input.opens.iter().any(|o| o.id == row.input.id)
    {
        return Err("recovered open changed its retained binding".into());
    }
    if input
        .opens
        .iter()
        .find(|o| o.id == row.input.id)
        .map(|o| &o.hash)
        != Some(&checksum(row)?)
    {
        return Err("recovered open checksum changed".into());
    }
    Ok(proof.is_some())
}
pub(super) fn frozen(db: &Connection, key: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM recovered_windows_opens WHERE open_id=?)",
        [key],
        |r| r.get(0),
    )?)
}
pub(super) fn check_index(db: &Connection) -> Result<()> {
    let bad:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM recovered_windows_opens o LEFT JOIN windows_session_recoveries r USING(recovery_id) LEFT JOIN windows_open_journal j USING(open_id) WHERE r.recovery_id IS NULL OR j.open_id IS NULL)",[],|r|r.get(0))?;
    if bad {
        return Err("session recovery index lost its evidence".into());
    }
    let ids = db
        .prepare("SELECT recovery_id FROM windows_session_recoveries")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for key in ids {
        recovery(db, &key)?;
    }
    Ok(())
}
impl Journal<'_> {
    pub fn start_session(&mut self, session: &str, volume: &str) -> Result<()> {
        self.chunk_check()?;
        if let Some(key) = &self.chunk {
            crate::chunk_mount::verify_key(&self.state.connection, key)?.volume(volume)?;
        }
        if !id(session) || !id(volume) {
            return Err("invalid mount session identity".into());
        }
        let runtime = runtime(self.state)?;
        let owner = identity::ensure(self.state)?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(key) = &self.chunk {
            crate::chunk_mount::verify_key(&tx, key)?;
        }
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM windows_mount_sessions WHERE session_id=?)",
            [session],
            |r| r.get(0),
        )?;
        if exists {
            return current(&tx, &runtime, session, volume);
        }
        let hash = digest(
            b"s3drive-mount-session-v1\0",
            &(session, volume, &runtime, &owner),
        )?;
        if tx.execute(
            "INSERT INTO windows_mount_sessions VALUES(?1,?2,?3,?4,?5)",
            params![session, volume, runtime, owner, hash],
        )? != 1
        {
            return Err("mount session was not durably registered".into());
        }
        current(&tx, &runtime, session, volume)?;
        tx.commit()?;
        Ok(())
    }
    pub fn current_session(&self, session: &str, volume: &str) -> Result<()> {
        self.chunk_check()?;
        if let Some(key) = &self.chunk {
            crate::chunk_mount::verify_key(&self.state.connection, key)?.volume(volume)?;
        }
        current(
            &self.state.connection,
            &runtime(self.state)?,
            session,
            volume,
        )
    }
    pub fn obsolete_session(&self, session: &str, volume: &str) -> Result<bool> {
        self.chunk_check()?;
        if let Some(key) = &self.chunk {
            crate::chunk_mount::verify_key(&self.state.connection, key)?.volume(volume)?;
        }
        Ok(binding(&self.state.connection, session, volume)? != runtime(self.state)?)
    }
    pub fn pending_recovery(&self, volume: &str, node: &str) -> Result<Option<RecoveryRecord>> {
        self.chunk_check()?;
        if let Some(key) = &self.chunk {
            crate::chunk_mount::verify_key(&self.state.connection, key)?.volume(volume)?;
        }
        let key:Option<String>=self.state.connection.query_row("SELECT recovery_id FROM windows_session_recoveries WHERE volume_id=?1 AND node_id=?2 AND proof IS NULL",params![volume,node],|r|r.get(0)).optional()?;
        key.map(|k| {
            let (input, proof) = recovery(&self.state.connection, &k)?;
            Ok(RecoveryRecord {
                input,
                finished: proof.is_some(),
            })
        })
        .transpose()
    }
    pub fn recovered(&self, key: &str) -> Result<bool> {
        self.chunk_check()?;
        let (row, record) = load(&self.state.connection, key)?.ok_or("open missing")?;
        closed(&self.state.connection, &row, &record)
    }
    pub fn prepare_recovery(
        &mut self,
        session: &str,
        volume: &str,
        node: &str,
        cancel: Option<Input>,
    ) -> Result<RecoveryRecord> {
        self.chunk_check()?;
        if let Some(key) = &self.chunk {
            crate::chunk_mount::verify_key(&self.state.connection, key)?.volume(volume)?;
        }
        self.current_session(session, volume)?;
        if self.pending_recovery(volume, node)?.is_some() {
            return Err("resume original session recovery request".into());
        }
        let runtime = runtime(self.state)?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(key) = &self.chunk {
            crate::chunk_mount::verify_key(&tx, key)?;
        }
        let records = all(&tx)?
            .into_iter()
            .filter(|(_, r)| r.input.volume == volume && r.input.node == node)
            .collect::<Vec<_>>();
        if records.is_empty() {
            return Err("no old open records to recover".into());
        }
        let mut opens = Vec::new();
        let first = &records[0].1.input;
        if removals::blocks_for_node(&tx, volume, node, None)? {
            return Err("recover exact namespace outcomes before session abandonment".into());
        }
        for (row, r) in &records {
            creation::recoverable(&tx, &r.input)?;
            if !r.input.session_bound
                || !r.input.controlled
                || binding(&tx, &r.input.session, volume)? == runtime
                || r.input.store != first.store
                || r.input.source != first.source
                || r.input.fingerprint != first.fingerprint
                || !r.pending().is_empty()
                || frozen(&tx, &r.input.id)?
                || r.events.iter().any(|e| {
                    matches!(e.action, Action::Callback(_))
                        && !r.completed(&e.id)
                        && !(matches!(e.action, Action::Callback(Callback::Close))
                            && r.cleanup().is_none())
                })
            {
                return Err("recover original obsolete session work before abandonment".into());
            }
            if let Some(admission) = &r.input.admission {
                let saved = super::super::load(&tx, &admission.request.operation)?
                    .ok_or("original admission not recovered")?;
                if saved.input != **admission || saved.outcome.is_none() {
                    return Err("original admission remains unknown".into());
                }
            }
            opens.push(OpenProof {
                id: r.input.id.clone(),
                hash: checksum(row)?,
            });
        }
        if let Some(cancel) = &cancel {
            cancel.validate()?;
            let owned = removals::owned(&tx, volume, node)?
                .ok_or("recovery cancellation lacks acknowledged owner")?;
            if !control(first, cancel)
                || !matches!(&cancel.request.mutation,Mutation::CancelRemoval{node:n,fence} if n==node && *fence==owned.fence)
            {
                return Err("recovery cancellation changed owner or frozen binding".into());
            }
        } else if removals::owned(&tx, volume, node)?.is_some()
            || removals::blocks_for_node(&tx, volume, node, None)?
        {
            return Err("session recovery must retain its exact closing work".into());
        }
        let key = digest(
            b"s3drive-session-recovery-id-v1\0",
            &(session, volume, node),
        )?[..16]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let input = RecoveryInput {
            id: key.clone(),
            session: session.into(),
            volume: volume.into(),
            node: node.into(),
            opens,
            cancel: cancel.map(Box::new),
        };
        let bytes = serde_json::to_vec(&input)?;
        if bytes.len() > 65536 {
            return Err("session recovery input exceeds bound".into());
        }
        if tx.execute(
            "INSERT INTO windows_session_recoveries VALUES(?1,?2,?3,?4,NULL,?5)",
            params![key, volume, node, bytes, record_hash(&input, &None)?],
        )? != 1
        {
            return Err("session recovery was not durably prepared".into());
        }
        for open in &input.opens {
            if tx.execute(
                "INSERT INTO recovered_windows_opens VALUES(?1,?2)",
                params![open.id, key],
            )? != 1
            {
                return Err("recovery open was not frozen".into());
            }
        }
        if recovery(&tx, &key)?.0 != input {
            return Err("prepared session recovery changed".into());
        }
        tx.commit()?;
        Ok(RecoveryRecord {
            input,
            finished: false,
        })
    }
    pub fn finish_recovery(&mut self, session: &str, key: &str) -> Result<RecoveryRecord> {
        self.chunk_check()?;
        let runtime = runtime(self.state)?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(key) = &self.chunk {
            crate::chunk_mount::verify_key(&tx, key)?;
        }
        let (input, old) = recovery(&tx, key)?;
        current(&tx, &runtime, session, &input.volume)?;
        if old.is_some() {
            return Ok(RecoveryRecord {
                input,
                finished: true,
            });
        }
        if removals::owned(&tx, &input.volume, &input.node)?.is_some()
            || removals::blocks_for_node(&tx, &input.volume, &input.node, None)?
        {
            return Err("session ownership or namespace outcome remains unresolved".into());
        }
        for open in &input.opens {
            let (_, r) = load(&tx, &open.id)?.ok_or("recovery open missing")?;
            if binding(&tx, &r.input.session, &input.volume)? == runtime {
                return Err("cannot abandon a current runtime file object".into());
            }
        }
        let closure = if let Some(cancel) = &input.cancel {
            Some(Box::new(proof_for(&tx, cancel)?))
        } else {
            let mut candidates = removals::records(&tx)?
                .into_iter()
                .filter(|r| {
                    r.input.request.volume == input.volume
                        && target(&r.input) == Some(input.node.as_str())
                        && matches!(
                            r.input.request.mutation,
                            Mutation::CancelRemoval { .. } | Mutation::CommitRemoval { .. }
                        )
                        && matches!(r.outcome, Some(Ok(_)))
                })
                .collect::<Vec<_>>();
            candidates.sort_by_key(|r| r.outcome.as_ref().unwrap().as_ref().unwrap().sequence);
            candidates
                .last()
                .map(|r| proof_for(&tx, &r.input).map(Box::new))
                .transpose()?
        };
        let proof = Some(Proof { closure });
        if tx.execute("UPDATE windows_session_recoveries SET proof=?1,state_hash=?2 WHERE recovery_id=?3 AND proof IS NULL",params![serde_json::to_vec(proof.as_ref().unwrap())?,record_hash(&input,&proof)?,key])?!=1 || recovery(&tx,key)?.1!=proof {
            return Err("session abandonment was not durably acknowledged".into());
        }
        tx.commit()?;
        Ok(RecoveryRecord {
            input,
            finished: true,
        })
    }
}

pub(super) fn chunk_scope(db: &Connection, b: &crate::chunk_mount::Binding) -> Result<()> {
    let bad:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM windows_mount_sessions WHERE length(CAST(session_id AS BLOB))!=32 OR length(CAST(volume_id AS BLOB))!=32 OR length(CAST(owner_id AS BLOB))!=32 OR length(CAST(runtime_id AS BLOB))!=32 OR length(state_hash)!=32) OR EXISTS(SELECT 1 FROM windows_session_recoveries WHERE length(CAST(recovery_id AS BLOB))!=32 OR length(CAST(volume_id AS BLOB))!=32 OR length(CAST(node_id AS BLOB))!=32 OR typeof(input)!='blob' OR length(input) NOT BETWEEN 1 AND 65536 OR (proof IS NOT NULL AND (typeof(proof)!='blob' OR length(proof) NOT BETWEEN 1 AND 32768)) OR length(state_hash)!=32)",[],|r|r.get(0))?;
    if bad {
        return Err("chunk session metadata bound failed".into());
    }
    let mut q = db.prepare("SELECT session_id,volume_id,owner_id FROM windows_mount_sessions")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (session, volume, owner) = row?;
        b.volume(&volume)?;
        if owner != b.owner {
            return Err("chunk session owner changed".into());
        }
        binding(db, &session, &volume)?;
    }
    let mut q = db.prepare("SELECT recovery_id FROM windows_session_recoveries")?;
    for row in q.query_map([], |r| r.get::<_, String>(0))? {
        let (input, _) = recovery(db, &row?)?;
        b.volume(&input.volume)?;
        if let Some(cancel) = input.cancel {
            b.namespace(&cancel)?;
        }
    }
    Ok(())
}
