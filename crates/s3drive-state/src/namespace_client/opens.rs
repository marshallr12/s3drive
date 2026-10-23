//! Durable per-file-object facts, not a substitute for kernel deletion policy.
//! Namespace effects keep their exact existing journal and proof boundary.
use super::*;
pub(crate) mod creation;
mod retirement;
pub mod session;
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub controlled: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub session_bound: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub posix: bool,
    pub admission: Option<Box<Input>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creation: Option<String>,
}
fn is_false(value: &bool) -> bool {
    !*value
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
    Reply {
        callback: String,
        error: Option<namespace::Error>,
        proof: Option<Box<ActivationProof>>,
    },
    Completed {
        callback: String,
    },
    /// Durable opt-in by the original current native file object.
    BindSparse,
    /// Historical clean-head witness recorded atomically with Close completion.
    CompleteSparse {
        callback: String,
        view: Option<crate::sparse_views::Id>,
        frozen: crate::sparse_views::Id,
    },
    /// Local native publisher identity, minted only with a bound source edit.
    OwnSparseEpoch {
        binding: Box<crate::sparse_views::Binding>,
        operation: crate::sparse_views::Id,
        base_root: Vec<u8>,
        base_page: crate::sparse_views::Id,
    },
    /// Exact plans admitted before this Close in the same state transaction.
    FreezeSparse {
        callback: String,
        plan: crate::sparse_views::Id,
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
impl OpenRecord {
    pub fn completed(&self, callback: &str) -> bool {
        self.events
            .iter()
            .any(|e| matches!(&e.action, Action::Completed { callback: c } if c == callback))
    }
    pub fn reply(&self, callback: &str) -> Option<Option<namespace::Error>> {
        self.events.iter().find_map(|e| match &e.action {
            Action::Reply {
                callback: c, error, ..
            } if c == callback => Some(*error),
            _ => None,
        })
    }
    pub fn links(&self, callback: &str) -> Vec<&Input> {
        self.events
            .iter()
            .filter_map(|e| match &e.action {
                Action::Link { callback: c, input } if c == callback => Some(input.as_ref()),
                _ => None,
            })
            .collect()
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
        Mutation::CreateDeferredEntry { .. } | Mutation::CreateDeferredEmptyManifest { .. } => {
            Some(&input.request.operation)
        }
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
        || (input.session_bound && !input.controlled)
        || (input.posix && !input.session_bound)
        || !input.store.valid()
        || !namespace::publication::digest(&input.fingerprint)
        || (input.creation.is_some()
            && (!input.session_bound || !input.controlled || input.admission.is_some()))
    {
        return Err("invalid per-open binding".into());
    }
    if let Some(admission) = &input.admission {
        admission.validate()?;
        if !input.delete_on_close
            || !control(input, admission)
            || target(admission) != Some(&input.node)
            || !matches!(&admission.request.mutation,Mutation::PrepareRemoval{owner:who,..}|Mutation::CreateDeferredEntry{owner:who,..}|Mutation::CreateDeferredEmptyManifest{owner:who,..} if who==owner)
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
    db.prepare("SELECT open_id,state_hash FROM retired_windows_opens LIMIT 0")?;
    session::check_schema(db)?;
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
    if let Action::Link { input, .. } = &event.action {
        super::creator_import::check_link(db, input)?;
    }
    match &event.action {
        Action::BindSparse if record.active() && record.input.controlled && record.input.session_bound => {
            if record.close().is_some() || record.events.iter().any(|e| matches!(e.action, Action::BindSparse)) {
                return Err("duplicate or closed native sparse binding".into());
            }
        }
        Action::CompleteSparse { callback, view, frozen } if record.active() => {
            if !record.events.iter().any(|e| matches!(e.action, Action::BindSparse))
                || record.close().map(|e| &e.id) != Some(callback) || record.completed(callback)
                || *view == Some([0; 32]) || *frozen == [0; 32]
                || record.events.iter().any(|e| matches!(&e.action, Action::CompleteSparse { callback: old, .. } if old == callback))
            { return Err("invalid native sparse completion witness".into()); }
        }
        Action::OwnSparseEpoch { binding, operation, base_root, base_page }
            if record.active() && record.input.controlled && record.input.session_bound =>
        {
            let base = binding.base()?;
            let epoch = binding.epoch()?;
            let root = s3drive_core::manifest::objects::RootDescriptor::decode(base_root)?;
            let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
            if *base_page == [0; 32]
                || root.length() != binding.base_size()
                || root.page().store() != binding.store()
                || hex(root.volume()) != base.volume()
                || hex(root.node()) != base.node()
                || record.close().is_some()
                || !record.events.iter().any(|e| matches!(e.action, Action::BindSparse))
                || *operation == [0; 32]
                || base.volume() != record.input.volume
                || base.node() != record.input.node
                || record.events.iter().any(|e| {
                    matches!(&e.action,
                    Action::OwnSparseEpoch { binding: b, .. } if b.epoch().ok() == Some(epoch))
                })
            {
                return Err("invalid original native source epoch".into());
            }
        }
        Action::FreezeSparse { callback, plan } if record.active() => {
            let adjacent = record.events.last().is_some_and(|e| match &e.action {
                Action::Callback(Callback::Close) => &e.id == callback,
                Action::FreezeSparse { callback: c, .. } => c == callback,
                _ => false,
            });
            if !adjacent
                || !record.events.iter().any(|e| matches!(e.action, Action::BindSparse))
                || *plan == [0; 32]
                || record.close().map(|e| &e.id) != Some(callback)
                || record.events.iter().any(|e| {
                    matches!(&e.action,
                    Action::FreezeSparse { plan: p, .. } if p == plan)
                })
            {
                return Err("invalid native Close frozen-plan sequence".into());
            }
        }
        Action::Activate { proof } if record.events.is_empty() => {
            if let Some(proof) = proof {
                let saved = super::load(db, &proof.input.request.operation)?
                    .ok_or("activation namespace proof missing")?;
                let (node, deferred) = match &proof.input.request.mutation {
                    Mutation::CreateDeferredEntry { .. }
                    | Mutation::CreateDeferredEmptyManifest { .. } => {
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
                    || record.input.creation.as_ref().is_some_and(|origin| {
                        removals::fence(&proof.input).as_ref().map(|f| &f.intent) != Some(origin)
                    })
                    || super::hash(&saved)? != proof.outcome_hash
                {
                    return Err("activation namespace proof changed".into());
                }
            } else if record.input.delete_on_close || record.input.creation.is_some() {
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
        Action::Reply {
            callback,
            error,
            proof,
        } if record.input.controlled => {
            let delete = record
                .events
                .iter()
                .find_map(|e| match &e.action {
                    Action::Callback(Callback::SetDelete { delete }) if &e.id == callback => {
                        Some(*delete)
                    }
                    _ => None,
                })
                .ok_or("reply lacks disposition callback")?;
            if record.reply(callback).is_some() || record.completed(callback) {
                return Err("disposition response already planned".into());
            }
            if let Some(proof) = proof {
                let saved = super::load(db, &proof.input.request.operation)?
                    .ok_or("reply namespace proof missing")?;
                let mode = match &proof.input.request.mutation {
                    Mutation::ArmRemoval { .. } => Some(RemovalMode::Pending),
                    Mutation::PrepareRemoval { .. }
                    | Mutation::CreateDeferredEntry { .. }
                    | Mutation::CreateDeferredEmptyManifest { .. }
                    | Mutation::MoveDeferredEntry { .. } => Some(RemovalMode::Deferred),
                    Mutation::TransitionRemoval { mode, .. } => Some(*mode),
                    _ => None,
                };
                let node = if let Mutation::MoveDeferredEntry { node, .. } =
                    &proof.input.request.mutation
                {
                    Some(node.id.as_str())
                } else {
                    target(&proof.input)
                };
                if error.is_some()
                    || !control(&record.input, &proof.input)
                    || node != Some(record.input.node.as_str())
                    || mode
                        != Some(if delete {
                            RemovalMode::Pending
                        } else {
                            RemovalMode::Deferred
                        })
                    || removals::fence(&proof.input).as_ref().map(|f| &f.owner)
                        != Some(&record.owner)
                    || saved.input != *proof.input
                    || !matches!(saved.outcome, Some(Ok(_)))
                    || super::hash(&saved)? != proof.outcome_hash
                {
                    return Err("disposition reply changed namespace proof".into());
                }
            } else if error.is_none() && (delete || !record.links(callback).is_empty()) {
                return Err("accepted disposition lacks exact proof".into());
            }
            if error.is_none()
                && record
                    .links(callback)
                    .iter()
                    .any(|i| record.pending().contains(i))
            {
                return Err("disposition result remains unknown".into());
            }
        }
        Action::Completed { callback } if record.input.controlled => {
            let kind = record
                .events
                .iter()
                .find_map(|e| match &e.action {
                    Action::Callback(c) if &e.id == callback => Some(c),
                    _ => None,
                })
                .ok_or("completion lacks callback")?;
            if matches!(kind, Callback::Close)
                && record.events.iter().any(|e| matches!(e.action, Action::BindSparse))
                && !record.events.last().is_some_and(|e| matches!(&e.action,
                    Action::CompleteSparse { callback: c, .. } if c == callback))
            { return Err("native Close completion lacks atomic clean-head witness".into()); }
            if record.completed(callback)
                || record
                    .links(callback)
                    .iter()
                    .any(|i| record.pending().contains(i))
            {
                return Err("callback completion remains unknown or duplicate".into());
            }
            if let Callback::SetDelete { .. } = kind {
                let reply = record
                    .reply(callback)
                    .ok_or("disposition response not planned")?;
                if reply.is_some() {
                    let links = record.links(callback);
                    let mut successes = Vec::new();
                    for input in &links {
                        let saved = super::load(db, &input.request.operation)?
                            .ok_or("callback namespace proof missing")?;
                        if matches!(saved.outcome, Some(Ok(_))) {
                            successes.push(*input);
                        }
                    }
                    if let Some(first) = successes.first() {
                        let expected = match &first.request.mutation {
                            Mutation::ArmRemoval { .. } => RemovalMode::Deferred,
                            Mutation::TransitionRemoval { expected, .. } => *expected,
                            _ => return Err("rejected disposition has no mode restoration".into()),
                        };
                        let last = successes.last().unwrap();
                        if !matches!(&last.request.mutation, Mutation::TransitionRemoval{mode,..} if *mode==expected)
                            || removals::fence(first) != removals::fence(last)
                        {
                            return Err("rejected disposition not restored with same fence".into());
                        }
                    }
                }
            }
            if matches!(kind, Callback::Close) && record.cleanup().is_none()
                && !record.events.iter().any(|e| matches!(e.action, Action::BindSparse)) {
                return Err("close has no cleanup evidence".into());
            }
        }
        _ => return Err("invalid per-open event ordering".into()),
    }
    record.events.push(event.clone());
    Ok(())
}
type RawRow = (String, Vec<u8>, i64, Option<Vec<u8>>, Vec<u8>);
#[cfg(test)]
thread_local! { static LOADS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
pub(crate) fn load_count() -> usize {
    LOADS.get()
}
fn load(db: &Connection, key: &str) -> Result<Option<(Row, OpenRecord)>> {
    #[cfg(test)]
    LOADS.set(LOADS.get() + 1);
    let bad:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM windows_open_journal WHERE open_id=?1 AND (length(CAST(owner_id AS BLOB))!=32 OR typeof(input)!='blob' OR length(input) NOT BETWEEN 1 AND 16384 OR event_count NOT BETWEEN 0 AND 4096 OR length(state_hash)!=32 OR (head_hash IS NOT NULL AND length(head_hash)!=32))) OR EXISTS(SELECT 1 FROM windows_open_events WHERE open_id=?1 AND (typeof(action)!='blob' OR length(action) NOT BETWEEN 1 AND 16384 OR length(state_hash)!=32 OR length(CAST(event_id AS BLOB))!=32 OR (previous_hash IS NOT NULL AND length(previous_hash)!=32)))",[key],|r|r.get(0))?;
    if bad {
        return Err("per-open bounded metadata shape changed".into());
    }
    let raw:Option<RawRow>=db.query_row("SELECT owner_id,input,event_count,head_hash,state_hash FROM windows_open_journal WHERE open_id=?",[key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
    let retained: Option<Vec<u8>> = db
        .query_row(
            "SELECT CASE WHEN length(input_hash)=32 THEN input_hash END FROM retained_windows_opens WHERE open_id=?",
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
    super::creator_import::check_admission(db, &input)?;
    creation::check_input(db, &input, &owner)?;
    if input.session_bound {
        session::binding(db, &input.session, &input.volume)?;
    }
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
    retirement::check(db, &row, &record)?;
    Ok(Some((row, record)))
}
pub(super) fn metadata_records(db: &Connection) -> Result<Vec<OpenRecord>> {
    Ok(all(db)?.into_iter().map(|(_, r)| r).collect())
}
fn all(db: &Connection) -> Result<Vec<(Row, OpenRecord)>> {
    session::check_index(db)?;
    retirement::check_index(db)?;
    let ids=db.prepare("SELECT open_id FROM windows_open_journal WHERE open_id NOT IN (SELECT open_id FROM retired_windows_opens) UNION SELECT open_id FROM retained_windows_opens WHERE open_id NOT IN (SELECT open_id FROM retired_windows_opens) LIMIT 257")?.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    if ids.len() > OPEN_LIMIT {
        return Err("per-open journal limit or integrity failure".into());
    }
    let records = ids
        .iter()
        .map(|key| load(db, key)?.ok_or_else(|| "per-open row missing".into()))
        .collect::<Result<Vec<_>>>()?;
    let count: i64 = db.query_row("SELECT count(*) FROM windows_open_events", [], |r| r.get(0))?;
    let expected: i64 = db.query_row(
        "SELECT coalesce(sum(event_count),0) FROM windows_open_journal",
        [],
        |r| r.get(0),
    )?;
    if count != expected {
        return Err("orphaned per-open event history".into());
    }
    Ok(records)
}
fn capacity(db: &Connection, records: &[(Row, OpenRecord)]) -> Result<()> {
    // Facts remain recordable even when ordinary callback/effect history fills.
    // Namespace closing capacity is independently reserved by its own journal.
    let slots = records
        .iter()
        .map(|(_, r)| {
            if r.input.controlled {
                return controlled_slots(r);
            }
            r.events.len()
                + usize::from(!r.active())
                + usize::from(r.cleanup().is_none())
                + usize::from(r.close().is_none())
                + r.pending().len()
        })
        .sum::<usize>();
    let native = crate::sparse_views::preparation::native::reserved_events(
        db,
        records.iter().map(|(_, r)| r),
    )?;
    if slots.checked_add(native).is_none_or(|n| n > EVENT_LIMIT) {
        return Err(
            "per-open journal preserves activation, cleanup, close and acknowledgment slots".into(),
        );
    }
    Ok(())
}
fn controlled_slots(r: &OpenRecord) -> usize {
    let mut slots = r.events.len() + usize::from(!r.active());
    for event in &r.events {
        let Action::Callback(kind) = &event.action else {
            continue;
        };
        if r.completed(&event.id) {
            continue;
        }
        let budget = match kind {
            Callback::SetDelete { .. } => 7usize,
            Callback::Cleanup { .. } => 6,
            Callback::Close => 4,
        };
        let links = r.links(&event.id);
        let used = 1
            + links.len()
            + links.iter().filter(|i| !r.pending().contains(i)).count()
            + usize::from(r.reply(&event.id).is_some());
        slots += budget.saturating_sub(used);
    }
    if r.cleanup().is_none() {
        slots += 6;
    }
    if r.close().is_none() {
        slots += 4;
    }
    slots
}

fn insert_row(db: &Connection, row: &Row, record: &OpenRecord) -> Result<()> {
    let mut records = all(db)?;
    if records.len() >= OPEN_LIMIT {
        return Err("per-open limit reached; history retained".into());
    }
    records.push((row.clone(), record.clone()));
    capacity(db, &records)?;
    let a = db.execute(
        "INSERT INTO windows_open_journal VALUES(?1,?2,?3,0,NULL,?4)",
        params![
            row.input.id,
            row.owner,
            serde_json::to_vec(&row.input)?,
            checksum(row)?
        ],
    )?;
    let b = db.execute(
        "INSERT INTO retained_windows_opens VALUES(?1,?2)",
        params![row.input.id, anchor(row)?],
    )?;
    if a != 1 || b != 1 || load(db, &row.input.id)?.as_ref().map(|(_, r)| r) != Some(record) {
        return Err("per-open preparation was not durably recorded".into());
    }
    Ok(())
}

pub struct Journal<'a> {
    state: &'a mut State,
    chunk: Option<crate::chunk_mount::Key>,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State) -> Result<Self> {
        state.verify_staging_storage()?;
        crate::chunk_publication::require_legacy(&state.connection)?;
        Ok(Self { state, chunk: None })
    }
    pub fn for_chunks(state: &'a mut State, key: &crate::chunk_mount::Key) -> Result<Self> {
        state.verify_staging_storage()?;
        crate::chunk_mount::require(&state.connection, key)?;
        Ok(Self {
            state,
            chunk: Some(key.clone()),
        })
    }
    fn chunk_check(&self) -> Result<()> {
        if let Some(key) = &self.chunk {
            crate::chunk_mount::verify_key(&self.state.connection, key)?;
            bounded(&self.state.connection)?;
        }
        Ok(())
    }
    pub fn prepare(&mut self, input: OpenInput) -> Result<OpenRecord> {
        self.chunk_check()?;
        let owner = identity::ensure(self.state)?;
        valid_input(&input, &owner)?;
        super::creator_import::check_admission(&self.state.connection, &input)?;
        if let Some(key) = &self.chunk {
            crate::chunk_mount::verify_key(&self.state.connection, key)?.open(&input)?;
        }
        if input.session_bound {
            self.current_session(&input.session, &input.volume)?;
            for (_, old) in all(&self.state.connection)? {
                if old.input.volume == input.volume
                    && old.input.node == input.node
                    && (!old.input.session_bound
                        || old.input.session != input.session
                        || old.input.posix != input.posix)
                {
                    return Err("recover previous mount session before admitting this node".into());
                }
            }
        }
        if let Some((_, old)) = load(&self.state.connection, &input.id)? {
            if retirement::is_retired(&self.state.connection, &input.id)? {
                return Err("retired open identity cannot be admitted again".into());
            }
            if old.input != input {
                return Err("per-open input changed; recover original identity".into());
            }
            return Ok(old);
        }
        if input.creation.is_some() {
            return Err("prospective open requires atomic creator admission".into());
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
        if let Some(key) = &self.chunk {
            crate::chunk_mount::verify_key(&tx, key)?;
        }
        insert_row(&tx, &row, &record)?;
        let imported = record
            .input
            .admission
            .as_ref()
            .is_some_and(|a| super::creator_import::canonical(&a.request.mutation));
        if imported {
            super::creator_import::checkpoint("CreatorOpenStored")?;
        }
        tx.commit()?;
        if imported {
            super::creator_import::checkpoint("CreatorOpenCommitted")?;
        }
        Ok(record)
    }
    pub fn get(&self, key: &str) -> Result<OpenRecord> {
        self.chunk_check()?;
        let record = load(&self.state.connection, key)?
            .map(|(_, r)| r)
            .ok_or("per-open record not found")?;
        if let Some(key) = &self.chunk {
            let b = crate::chunk_mount::verify_key(&self.state.connection, key)?;
            scope_record(&b, &record)?;
        }
        Ok(record)
    }
    pub fn records(&self) -> Result<Vec<OpenRecord>> {
        self.chunk_check()?;
        let records = all(&self.state.connection)?;
        if let Some(key) = &self.chunk {
            let b = crate::chunk_mount::verify_key(&self.state.connection, key)?;
            for (_, record) in &records {
                scope_record(&b, record)?;
            }
        }
        Ok(records.into_iter().map(|(_, r)| r).collect())
    }
    pub fn abandoned(&self, current_session: &str) -> Result<Vec<OpenRecord>> {
        self.chunk_check()?;
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
        self.chunk_check()?;
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
        if record.input.creation.is_some() {
            self.current_session(&record.input.session, &record.input.volume)?;
            creation::activation(&self.state.connection, &record.input)?;
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
        self.chunk_check()?;
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
        self.chunk_check()?;
        if let Some(key) = &self.chunk {
            crate::chunk_mount::verify_key(&self.state.connection, key)?.namespace(&input)?;
        }
        let linked = Event {
            id: event.into(),
            action: Action::Link {
                callback: callback.into(),
                input: Box::new(input.clone()),
            },
        };
        // Exact replay is bookkeeping even if a later effect is unknown.
        // Do not let that barrier prevent recovering the later effect itself.
        if self.get(key)?.events.iter().any(|old| old.id == event) {
            return self.append(key, linked);
        }
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
        self.append(key, linked)
    }
    pub fn acknowledge(&mut self, key: &str, event: &str, operation: &str) -> Result<OpenRecord> {
        self.chunk_check()?;
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
    pub fn reply(
        &mut self,
        key: &str,
        event: &str,
        callback: &str,
        error: Option<namespace::Error>,
    ) -> Result<OpenRecord> {
        self.chunk_check()?;
        let record = self.get(key)?;
        if let Some(old) = record.events.iter().find(|e| e.id == event) {
            if !matches!(&old.action,Action::Reply{callback:c,error:old_error,..} if c==callback && *old_error==error)
            {
                return Err("callback reply identity changed".into());
            }
            return self.append(key, old.clone());
        }
        let proof = if error.is_none() {
            if let Some(owned) = removals::owned(
                &self.state.connection,
                &record.input.volume,
                &record.input.node,
            )? {
                let saved = super::load(&self.state.connection, &owned.event)?
                    .ok_or("reply event missing")?;
                Some(Box::new(ActivationProof {
                    outcome_hash: super::hash(&saved)?,
                    input: Box::new(saved.input),
                }))
            } else {
                None
            }
        } else {
            None
        };
        self.append(
            key,
            Event {
                id: event.into(),
                action: Action::Reply {
                    callback: callback.into(),
                    error,
                    proof,
                },
            },
        )
    }
    pub fn complete(&mut self, key: &str, event: &str, callback: &str) -> Result<OpenRecord> {
        self.chunk_check()?;
        self.append(
            key,
            Event {
                id: event.into(),
                action: Action::Completed {
                    callback: callback.into(),
                },
            },
        )
    }
    fn append(&mut self, key: &str, event: Event) -> Result<OpenRecord> {
        append_existing(
            &mut self.state.connection,
            self.chunk.as_ref(),
            key,
            event,
            false,
        )
    }
}

fn bounded(db: &Connection) -> Result<()> {
    let bad:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM windows_open_journal WHERE length(CAST(open_id AS BLOB))!=32 OR length(CAST(owner_id AS BLOB))!=32 OR typeof(input)!='blob' OR length(input) NOT BETWEEN 1 AND 16384 OR event_count NOT BETWEEN 0 AND 4096 OR length(state_hash)!=32 OR (head_hash IS NOT NULL AND length(head_hash)!=32)) OR EXISTS(SELECT 1 FROM windows_open_events WHERE length(CAST(open_id AS BLOB))!=32 OR length(CAST(event_id AS BLOB))!=32 OR typeof(action)!='blob' OR length(action) NOT BETWEEN 1 AND 16384 OR length(state_hash)!=32) OR EXISTS(SELECT 1 FROM retained_windows_opens WHERE length(CAST(open_id AS BLOB))!=32 OR length(input_hash)!=32)",[],|r|r.get(0))?;
    if bad {
        return Err("chunk open metadata bound failed".into());
    }
    Ok(())
}
fn scope_record(b: &crate::chunk_mount::Binding, r: &OpenRecord) -> Result<()> {
    if r.owner != b.owner {
        return Err("chunk open owner changed".into());
    }
    b.open(&r.input)?;
    for e in &r.events {
        match &e.action {
            Action::Link { input, .. } => b.namespace(input)?,
            Action::Activate { proof: Some(p) } | Action::Reply { proof: Some(p), .. } => {
                b.namespace(&p.input)?
            }
            _ => {}
        }
    }
    Ok(())
}
pub(super) fn chunk_scope(db: &Connection, b: &crate::chunk_mount::Binding) -> Result<()> {
    bounded(db)?;
    all(db)?;
    let mut q = db.prepare("SELECT open_id FROM windows_open_journal ORDER BY open_id")?;
    for row in q.query_map([], |r| r.get::<_, String>(0))? {
        let key = row?;
        let (_, record) = load(db, &key)?.ok_or("chunk open history missing")?;
        scope_record(b, &record)?;
    }
    session::chunk_scope(db, b)
}

#[cfg(test)]
mod tests;

/// Independent retained-open witness used by sparse admission and recovery.
pub(crate) fn sparse_open(db: &Connection, key: &str) -> Result<OpenRecord> {
    load(db, key)?
        .map(|(_, record)| record)
        .ok_or_else(|| "unknown original sparse open".into())
}
pub(crate) fn sparse_current(db: &Connection, runtime: &str, open: &OpenRecord) -> Result<()> {
    if open.close().is_some() {
        return Err("sparse admission requires a live original session-bound open".into());
    }
    sparse_original_session(db, runtime, open)
}
/// Original session validity only. This is not fresh-work permission for a closed open.
pub(crate) fn sparse_original_session(
    db: &Connection,
    runtime: &str,
    open: &OpenRecord,
) -> Result<()> {
    if !open.input.session_bound
        || !open.active()
        || !open.pending().is_empty()
        || session::frozen(db, &open.input.id)?
    {
        return Err("sparse admission requires the original unfrozen session-bound open".into());
    }
    session::current(db, runtime, &open.input.session, &open.input.volume)
}
pub(crate) fn sparse_records(db: &Connection) -> Result<Vec<OpenRecord>> {
    all(db).map(|rows| rows.into_iter().map(|(_, record)| record).collect())
}

// Shared transaction implementation; restricted recovery permits only acknowledgments.
pub(super) fn append_existing(
    connection: &mut Connection,
    chunk: Option<&crate::chunk_mount::Key>,
    key: &str,
    event: Event,
    restricted: bool,
) -> Result<OpenRecord> {
    if matches!(
        event.action,
        Action::BindSparse
            | Action::CompleteSparse { .. }
            | Action::OwnSparseEpoch { .. }
            | Action::FreezeSparse { .. }
    ) {
        return Err("native sparse events require atomic typed admission".into());
    }
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let acknowledgment = matches!(event.action, Action::Acknowledge { .. });
    let appended = append_in_transaction(&tx, chunk, key, event, restricted)?;
    if appended.changed {
        tx.commit()?;
        if acknowledgment {
            super::recovery::checkpoint("OpenCommitted");
        }
        if appended.imported {
            super::creator_import::checkpoint("CreatorActivationCommitted")?;
        }
    }
    Ok(appended.record)
}

/// Private composition primitive. The caller owns the immediate transaction and
/// must commit the complete higher-level transition, never a partial event set.
pub(crate) struct Appended {
    pub(crate) record: OpenRecord,
    pub(crate) changed: bool,
    imported: bool,
}
pub(crate) fn append_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    chunk: Option<&crate::chunk_mount::Key>,
    key: &str,
    event: Event,
    restricted: bool,
) -> Result<Appended> {
    if let Some(key) = &chunk {
        crate::chunk_mount::verify_key(tx, key)?;
        if restricted {
            super::recovery::metadata(tx, key)?;
        }
    }
    let (mut row, mut record) = load(tx, key)?.ok_or("per-open record not found")?;
    if let Some(old) = record.events.iter().find(|e| e.id == event.id) {
        if *old != event {
            return Err("per-open event identity changed".into());
        }
        return Ok(Appended {
            record,
            changed: false,
            imported: false,
        });
    }
    if retirement::is_retired(tx, key)? {
        return Err("retired open history is immutable".into());
    }
    if session::frozen(tx, key)? {
        return Err("session recovery froze this exact callback history".into());
    }
    let old_hash = checksum(&row)?;
    let previous = row.head.clone();
    let hash = event_hash(key, row.count + 1, &event, &previous)?;
    reduce(tx, &mut record, &event)?;
    if let Some(key) = &chunk {
        scope_record(&crate::chunk_mount::verify_key(tx, key)?, &record)?;
    }
    row.count += 1;
    row.head = Some(hash.clone());
    let mut records = all(tx)?;
    *records
        .iter_mut()
        .find(|(r, _)| r.input.id == key)
        .ok_or("per-open row missing")? = (row.clone(), record.clone());
    capacity(tx, &records)?;
    let action = serde_json::to_vec(&event.action)?;
    if action.len() > 16384 {
        return Err("per-open event too large".into());
    }
    #[cfg(test)]
    crate::sparse_views::preparation::native::full_probe(tx)?;
    let a = tx.execute(
        "INSERT INTO windows_open_events VALUES(?1,?2,?3,?4,?5,?6)",
        params![event.id, key, row.count as i64, action, previous, hash],
    )?;
    let b=tx.execute("UPDATE windows_open_journal SET event_count=?1,head_hash=?2,state_hash=?3 WHERE open_id=?4 AND state_hash=?5",params![row.count as i64,row.head,checksum(&row)?,key,old_hash])?;
    if a != 1 || b != 1 || load(tx, key)?.as_ref().map(|(_, r)| r) != Some(&record) {
        return Err("per-open event was not durably committed".into());
    }
    let imported = matches!(event.action, Action::Activate { .. })
        && record
            .input
            .admission
            .as_ref()
            .is_some_and(|a| super::creator_import::canonical(&a.request.mutation));
    if imported {
        super::creator_import::checkpoint("CreatorActivationStored")?;
    }
    if restricted {
        super::recovery::metadata(
            tx,
            chunk.ok_or("restricted acknowledgment requires chunk role")?,
        )?;
    }
    if matches!(event.action, Action::Acknowledge { .. }) {
        super::recovery::checkpoint("OpenStored");
    }
    Ok(Appended {
        record,
        changed: true,
        imported,
    })
}

pub(crate) fn sparse_capacity(db: &Connection) -> Result<()> {
    capacity(db, &all(db)?)
}

/// Exact durable abandonment proof; never fresh original-open authority.
pub(crate) fn sparse_recovered(db: &Connection, id: &str) -> Result<bool> {
    let (row, record) = load(db, id)?.ok_or("original open missing")?;
    session::closed(db, &row, &record)
}

/// Exact sealed retirement, used only to release an obsolete runtime token.
pub(crate) fn sparse_retired(db: &Connection, id: &str) -> Result<bool> {
    let (row, record) = load(db, id)?.ok_or("original open missing")?;
    retirement::check(db, &row, &record)?;
    retirement::is_retired(db, id)
}
