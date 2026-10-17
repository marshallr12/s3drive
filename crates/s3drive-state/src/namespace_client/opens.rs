//! Durable per-file-object facts, not a substitute for kernel deletion policy.
//! Namespace effects keep their exact existing journal and proof boundary.
use super::*;
use namespace::{ROOT, RemovalMode};
const OPEN_LIMIT: usize = 256;
const EVENT_LIMIT: usize = 4096;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenInput {
    pub id: String,
    pub session: String,
    pub volume: String,
    pub node: String,
    pub store: Store,
    pub source: [u8; 32],
    pub fingerprint: String,
    pub delete_on_close: bool,
    pub admission: Option<Box<Input>>,
}
impl std::fmt::Debug for OpenInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OpenInput([private])")
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum Callback {
    SetDelete { delete: bool },
    Cleanup { flags: u32 },
    Close,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum Action {
    Activate {
        proof: Option<Box<ActivationProof>>,
    },
    Callback(Callback),
    Link {
        callback: String,
        input: Box<Input>,
    },
    Acknowledge {
        operation: String,
        outcome_hash: Vec<u8>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationProof {
    pub input: Box<Input>,
    pub outcome_hash: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub id: String,
    pub action: Action,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRecord {
    pub input: OpenInput,
    pub owner: String,
    pub events: Vec<Event>,
}
impl OpenRecord {
    pub fn active(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e.action, Action::Activate { .. }))
    }
    pub fn cleanup(&self) -> Option<&Event> {
        self.events
            .iter()
            .find(|e| matches!(e.action, Action::Callback(Callback::Cleanup { .. })))
    }
    pub fn close(&self) -> Option<&Event> {
        self.events
            .iter()
            .find(|e| e.action == Action::Callback(Callback::Close))
    }
    pub fn pending(&self) -> Vec<&Input> {
        self.events.iter().filter_map(|e| match &e.action {
            Action::Link { input, .. } if !self.events.iter().any(|a| matches!(&a.action, Action::Acknowledge {operation,..} if operation == &input.request.operation)) => Some(input.as_ref()),
            _ => None,
        }).collect()
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Row {
    input: OpenInput,
    owner: String,
    count: usize,
    head: Option<Vec<u8>>,
}
fn digest<T: Serialize>(tag: &[u8], value: &T) -> Result<Vec<u8>> {
    let mut h = Sha256::new();
    h.update(tag);
    h.update(serde_json::to_vec(value)?);
    Ok(h.finalize().to_vec())
}
fn anchor(row: &Row) -> Result<Vec<u8>> {
    digest(b"s3drive-open-input-v1\0", &(&row.input, &row.owner))
}
fn checksum(row: &Row) -> Result<Vec<u8>> {
    digest(
        b"s3drive-open-row-v1\0",
        &(&row.input, &row.owner, row.count, &row.head),
    )
}
fn event_hash(
    open: &str,
    ordinal: usize,
    event: &Event,
    previous: &Option<Vec<u8>>,
) -> Result<Vec<u8>> {
    digest(
        b"s3drive-open-event-v1\0",
        &(open, ordinal, &event.id, &event.action, previous),
    )
}
fn id(value: &str) -> bool {
    value != ROOT
        && value.len() == 32
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn control(open: &OpenInput, input: &Input) -> bool {
    open.volume == input.request.volume
        && open.store == input.store
        && open.source == input.source
        && open.fingerprint == input.fingerprint
}
fn target(input: &Input) -> Option<&str> {
    match &input.request.mutation {
        Mutation::CreateDeferredEntry { .. } => Some(&input.request.operation),
        Mutation::PrepareRemoval { node, .. }
        | Mutation::ArmRemoval { node, .. }
        | Mutation::TransitionRemoval { node, .. }
        | Mutation::CommitRemoval { node, .. } => Some(&node.id),
        Mutation::CancelRemoval { node, .. } => Some(node),
        _ => None,
    }
}
fn valid_input(input: &OpenInput, owner: &str) -> Result<()> {
    if !id(&input.id)
        || !id(&input.session)
        || !id(&input.volume)
        || !id(&input.node)
        || !id(owner)
        || !input.store.valid()
        || !namespace::publication::digest(&input.fingerprint)
    {
        return Err("invalid per-open binding".into());
    }
    if let Some(admission) = &input.admission {
        admission.validate()?;
        if !input.delete_on_close
            || !control(input, admission)
            || target(admission) != Some(&input.node)
            || !matches!(&admission.request.mutation,Mutation::PrepareRemoval{owner:who,..}|Mutation::CreateDeferredEntry{owner:who,..} if who==owner)
        {
            return Err("per-open admission changed frozen identity or owner".into());
        }
    }
    if serde_json::to_vec(input)?.len() > 16384 {
        return Err("per-open input too large".into());
    }
    Ok(())
}
pub(super) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT open_id,owner_id,input,event_count,head_hash,state_hash FROM windows_open_journal LIMIT 0")?;
    db.prepare("SELECT open_id,input_hash FROM retained_windows_opens LIMIT 0")?;
    db.prepare("SELECT event_id,open_id,ordinal,action,previous_hash,state_hash FROM windows_open_events LIMIT 0")?;
    Ok(())
}
fn outcome(db: &Connection, input: &Input) -> Result<Vec<u8>> {
    let record =
        super::load(db, &input.request.operation)?.ok_or("linked namespace request missing")?;
    if record.input != *input || record.outcome.is_none() {
        return Err("linked namespace outcome remains unknown or changed".into());
    }
    super::hash(&record)
}
fn reduce(db: &Connection, record: &mut OpenRecord, event: &Event) -> Result<()> {
    if !id(&event.id) || record.events.iter().any(|e| e.id == event.id) {
        return Err("duplicate or invalid per-open event".into());
    }
    match &event.action {
        Action::Activate { proof } if record.events.is_empty() => {
            if let Some(proof) = proof {
                let saved = super::load(db, &proof.input.request.operation)?
                    .ok_or("activation namespace proof missing")?;
                let (node, deferred) = match &proof.input.request.mutation {
                    Mutation::CreateDeferredEntry { .. } => {
                        (proof.input.request.operation.as_str(), true)
                    }
                    Mutation::PrepareRemoval { node, .. }
                    | Mutation::MoveDeferredEntry { node, .. } => (node.id.as_str(), true),
                    Mutation::TransitionRemoval { node, mode, .. } => {
                        (node.id.as_str(), *mode == RemovalMode::Deferred)
                    }
                    _ => return Err("activation proof is not deferred".into()),
                };
                if !control(&record.input, &proof.input)
                    || node != record.input.node
                    || !deferred
                    || removals::fence(&proof.input).as_ref().map(|f| &f.owner)
                        != Some(&record.owner)
                    || saved.input != *proof.input
                    || !matches!(saved.outcome, Some(Ok(_)))
                    || super::hash(&saved)? != proof.outcome_hash
                {
                    return Err("activation namespace proof changed".into());
                }
            } else if record.input.delete_on_close {
                return Err("delete-on-close activation lacks a retained proof".into());
            }
        }
        Action::Callback(callback) if record.active() => match callback {
            Callback::SetDelete { .. }
                if record.cleanup().is_some() || record.close().is_some() =>
            {
                return Err("disposition callback after cleanup/close".into());
            }
            Callback::Cleanup { .. } if record.cleanup().is_some() => {
                return Err("cleanup already recorded; recover its exact event".into());
            }
            Callback::Close if record.close().is_some() => {
                return Err("close already recorded; recover its exact event".into());
            }
            _ => (),
        },
        Action::Link { callback, input } => {
            input.validate()?;
            if !record.events.iter().any(|e| &e.id==callback && matches!(e.action,Action::Callback(_)))
                || !control(&record.input,input) || target(input)!=Some(record.input.node.as_str())
                || removals::fence(input).as_ref().map(|f|&f.owner)!=Some(&record.owner)
                || record.events.iter().any(|e|matches!(&e.action,Action::Link{input:old,..} if old.request.operation==input.request.operation)) {
                return Err("per-open effect changed callback, binding or owner".into());
            }
            if !record.pending().is_empty() {
                return Err("recover exact per-open effect before another link".into());
            }
        }
        Action::Acknowledge {
            operation,
            outcome_hash,
        } => {
            let input = record
                .pending()
                .into_iter()
                .find(|i| &i.request.operation == operation)
                .ok_or("per-open effect is not pending")?;
            if outcome_hash.len() != 32 || outcome(db, input)? != *outcome_hash {
                return Err("per-open acknowledgment lacks exact namespace proof".into());
            }
        }
        _ => return Err("invalid per-open event ordering".into()),
    }
    record.events.push(event.clone());
    Ok(())
}
type RawRow = (String, Vec<u8>, i64, Option<Vec<u8>>, Vec<u8>);
fn load(db: &Connection, key: &str) -> Result<Option<(Row, OpenRecord)>> {
    let raw:Option<RawRow>=db.query_row("SELECT owner_id,input,event_count,head_hash,state_hash FROM windows_open_journal WHERE open_id=?",[key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
    let retained: Option<Vec<u8>> = db
        .query_row(
            "SELECT input_hash FROM retained_windows_opens WHERE open_id=?",
            [key],
            |r| r.get(0),
        )
        .optional()?;
    let Some((owner, bytes, count, head, hash)) = raw else {
        if retained.is_some() {
            return Err("retained per-open record missing".into());
        }
        return Ok(None);
    };
    let input: OpenInput = serde_json::from_slice(&bytes)?;
    valid_input(&input, &owner)?;
    let row = Row {
        input: input.clone(),
        owner: owner.clone(),
        count: count.try_into()?,
        head,
    };
    if input.id != key
        || row.count > EVENT_LIMIT
        || retained != Some(anchor(&row)?)
        || hash != checksum(&row)?
        || identity::read(db)?.as_ref() != Some(&owner)
    {
        return Err("per-open record integrity failure".into());
    }
    let mut record = OpenRecord {
        input,
        owner,
        events: Vec::new(),
    };
    let mut previous = None;
    let mut stmt=db.prepare("SELECT event_id,ordinal,action,previous_hash,state_hash FROM windows_open_events WHERE open_id=? ORDER BY ordinal LIMIT 4097")?;
    let mut rows = stmt.query([key])?;
    while let Some(r) = rows.next()? {
        let event = Event {
            id: r.get(0)?,
            action: serde_json::from_slice(&r.get::<_, Vec<u8>>(2)?)?,
        };
        let ordinal: i64 = r.get(1)?;
        let old: Option<Vec<u8>> = r.get(3)?;
        let hash: Vec<u8> = r.get(4)?;
        if ordinal != record.events.len() as i64 + 1
            || old != previous
            || hash != event_hash(key, ordinal as usize, &event, &previous)?
        {
            return Err("per-open event chain integrity failure".into());
        }
        reduce(db, &mut record, &event)?;
        previous = Some(hash);
    }
    if record.events.len() != row.count || previous != row.head {
        return Err("per-open event head missing or changed".into());
    }
    Ok(Some((row, record)))
}
fn all(db: &Connection) -> Result<Vec<(Row, OpenRecord)>> {
    let ids=db.prepare("SELECT open_id FROM windows_open_journal UNION SELECT open_id FROM retained_windows_opens LIMIT 257")?.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    if ids.len() > OPEN_LIMIT {
        return Err("per-open journal limit or integrity failure".into());
    }
    let records = ids
        .iter()
        .map(|key| load(db, key)?.ok_or_else(|| "per-open row missing".into()))
        .collect::<Result<Vec<_>>>()?;
    let count: i64 = db.query_row("SELECT count(*) FROM windows_open_events", [], |r| r.get(0))?;
    if count as usize != records.iter().map(|(r, _)| r.count).sum::<usize>() {
        return Err("orphaned per-open event history".into());
    }
    Ok(records)
}
fn capacity(records: &[(Row, OpenRecord)]) -> Result<()> {
    // Facts remain recordable even when ordinary callback/effect history fills.
    // Namespace closing capacity is independently reserved by its own journal.
    let slots = records
        .iter()
        .map(|(_, r)| {
            r.events.len()
                + usize::from(!r.active())
                + usize::from(r.cleanup().is_none())
                + usize::from(r.close().is_none())
                + r.pending().len()
        })
        .sum::<usize>();
    if slots > EVENT_LIMIT {
        return Err(
            "per-open journal preserves activation, cleanup, close and acknowledgment slots".into(),
        );
    }
    Ok(())
}
pub struct Journal<'a> {
    state: &'a mut State,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State) -> Result<Self> {
        state.verify_staging_storage()?;
        Ok(Self { state })
    }
    pub fn prepare(&mut self, input: OpenInput) -> Result<OpenRecord> {
        let owner = identity::ensure(self.state)?;
        valid_input(&input, &owner)?;
        if let Some((_, old)) = load(&self.state.connection, &input.id)? {
            if old.input != input {
                return Err("per-open input changed; recover original identity".into());
            }
            return Ok(old);
        }
        if input.admission.is_none() {
            if removals::blocks_for_node(&self.state.connection, &input.volume, &input.node, None)?
            {
                return Err("recover namespace effect before preparing another open".into());
            }
            let owned = removals::owned(&self.state.connection, &input.volume, &input.node)?;
            if owned
                .as_ref()
                .is_some_and(|o| o.mode == RemovalMode::Pending || !control(&input, &o.input))
                || (input.delete_on_close && owned.is_none())
            {
                return Err("per-open requires the acknowledged deferred owner".into());
            }
        }
        self.state
            .register_source(&input.source, &input.fingerprint)?;
        let row = Row {
            input: input.clone(),
            owner: owner.clone(),
            count: 0,
            head: None,
        };
        let record = OpenRecord {
            input,
            owner,
            events: Vec::new(),
        };
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut records = all(&tx)?;
        if records.len() >= OPEN_LIMIT {
            return Err("per-open limit reached; history retained".into());
        }
        records.push((row.clone(), record.clone()));
        capacity(&records)?;
        let a = tx.execute(
            "INSERT INTO windows_open_journal VALUES(?1,?2,?3,0,NULL,?4)",
            params![
                row.input.id,
                row.owner,
                serde_json::to_vec(&row.input)?,
                checksum(&row)?
            ],
        )?;
        let b = tx.execute(
            "INSERT INTO retained_windows_opens VALUES(?1,?2)",
            params![row.input.id, anchor(&row)?],
        )?;
        if a != 1 || b != 1 || load(&tx, &row.input.id)?.as_ref().map(|(_, r)| r) != Some(&record) {
            return Err("per-open preparation was not durably recorded".into());
        }
        tx.commit()?;
        Ok(record)
    }
    pub fn get(&self, key: &str) -> Result<OpenRecord> {
        load(&self.state.connection, key)?
            .map(|(_, r)| r)
            .ok_or_else(|| "per-open record not found".into())
    }
    pub fn records(&self) -> Result<Vec<OpenRecord>> {
        Ok(all(&self.state.connection)?
            .into_iter()
            .map(|(_, r)| r)
            .collect())
    }
    pub fn abandoned(&self, current_session: &str) -> Result<Vec<OpenRecord>> {
        if !id(current_session) {
            return Err("invalid mount session identity".into());
        }
        Ok(self
            .records()?
            .into_iter()
            .filter(|r| r.input.session != current_session && r.close().is_none())
            .collect())
    }
    pub fn activate(&mut self, key: &str, event: &str) -> Result<OpenRecord> {
        let record = self.get(key)?;
        if record.active() {
            return self.append(
                key,
                Event {
                    id: event.into(),
                    action: record.events[0].action.clone(),
                },
            );
        }
        let proof = {
            if let Some(admission) = &record.input.admission {
                let saved = super::load(&self.state.connection, &admission.request.operation)?
                    .ok_or("open admission not yet acknowledged")?;
                if saved.input != **admission || !matches!(saved.outcome, Some(Ok(_))) {
                    return Err("open admission not successfully acknowledged".into());
                }
            }
            let owned = removals::owned(
                &self.state.connection,
                &record.input.volume,
                &record.input.node,
            )?;
            if owned.as_ref().is_some_and(|o| {
                o.mode != RemovalMode::Deferred
                    || !control(&record.input, &o.input)
                    || record
                        .input
                        .admission
                        .as_ref()
                        .is_some_and(|a| o.fence.intent != a.request.operation)
            }) || (record.input.delete_on_close && owned.is_none())
                || removals::blocks_for_node(
                    &self.state.connection,
                    &record.input.volume,
                    &record.input.node,
                    None,
                )?
            {
                return Err("per-open admission remains unknown, pending or foreign".into());
            }
            if let Some(owned) = owned {
                let saved = super::load(&self.state.connection, &owned.event)?
                    .ok_or("activation event proof missing")?;
                Some(Box::new(ActivationProof {
                    outcome_hash: super::hash(&saved)?,
                    input: Box::new(saved.input),
                }))
            } else {
                None
            }
        };
        self.append(
            key,
            Event {
                id: event.into(),
                action: Action::Activate { proof },
            },
        )
    }
    pub fn callback(&mut self, key: &str, event: &str, kind: Callback) -> Result<OpenRecord> {
        self.append(
            key,
            Event {
                id: event.into(),
                action: Action::Callback(kind),
            },
        )
    }
    pub fn link(
        &mut self,
        key: &str,
        event: &str,
        callback: &str,
        input: Input,
    ) -> Result<OpenRecord> {
        for record in self.records()? {
            for old in record.pending() {
                if old.request.volume == input.request.volume
                    && target(old) == target(&input)
                    && old != &input
                {
                    return Err(
                        "recover exact linked effect before another node-wide change".into(),
                    );
                }
            }
        }
        self.append(
            key,
            Event {
                id: event.into(),
                action: Action::Link {
                    callback: callback.into(),
                    input: Box::new(input),
                },
            },
        )
    }
    pub fn acknowledge(&mut self, key: &str, event: &str, operation: &str) -> Result<OpenRecord> {
        let record = self.get(key)?;
        let input = record
            .events
            .iter()
            .find_map(|e| match &e.action {
                Action::Link { input, .. } if input.request.operation == operation => {
                    Some(input.as_ref())
                }
                _ => None,
            })
            .ok_or("linked effect not found")?;
        let hash = outcome(&self.state.connection, input)?;
        self.append(
            key,
            Event {
                id: event.into(),
                action: Action::Acknowledge {
                    operation: operation.into(),
                    outcome_hash: hash,
                },
            },
        )
    }
    fn append(&mut self, key: &str, event: Event) -> Result<OpenRecord> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (mut row, mut record) = load(&tx, key)?.ok_or("per-open record not found")?;
        if let Some(old) = record.events.iter().find(|e| e.id == event.id) {
            if *old != event {
                return Err("per-open event identity changed".into());
            }
            return Ok(record);
        }
        let old_hash = checksum(&row)?;
        let previous = row.head.clone();
        let hash = event_hash(key, row.count + 1, &event, &previous)?;
        reduce(&tx, &mut record, &event)?;
        row.count += 1;
        row.head = Some(hash.clone());
        let mut records = all(&tx)?;
        *records
            .iter_mut()
            .find(|(r, _)| r.input.id == key)
            .ok_or("per-open row missing")? = (row.clone(), record.clone());
        capacity(&records)?;
        let action = serde_json::to_vec(&event.action)?;
        if action.len() > 16384 {
            return Err("per-open event too large".into());
        }
        let a = tx.execute(
            "INSERT INTO windows_open_events VALUES(?1,?2,?3,?4,?5,?6)",
            params![event.id, key, row.count as i64, action, previous, hash],
        )?;
        let b=tx.execute("UPDATE windows_open_journal SET event_count=?1,head_hash=?2,state_hash=?3 WHERE open_id=?4 AND state_hash=?5",params![row.count as i64,row.head,checksum(&row)?,key,old_hash])?;
        if a != 1 || b != 1 || load(&tx, key)?.as_ref().map(|(_, r)| r) != Some(&record) {
            return Err("per-open event was not durably committed".into());
        }
        tx.commit()?;
        Ok(record)
    }
}
#[cfg(test)]
mod tests;
