//! Cross-role checks intentionally avoid recursively loading open event history.
use super::*;
use crate::manifest_creation::{self as creators, Input as CreatorInput};

fn expected(input: &CreatorInput) -> Result<(OpenInput, String)> {
    let claim = input
        .open
        .as_ref()
        .ok_or("creator has no prospective open")?;
    let owner = input.owner.clone().ok_or("creator has no owner")?;
    let open = OpenInput {
        id: claim.id.clone(),
        session: claim.session.clone(),
        volume: input.volume.clone(),
        node: input.operation.clone(),
        store: input.store.clone(),
        source: input.source,
        fingerprint: input.fingerprint.clone(),
        delete_on_close: claim.delete_on_close,
        controlled: true,
        session_bound: true,
        posix: claim.posix,
        admission: None,
        creation: Some(input.operation.clone()),
    };
    valid_input(&open, &owner)?;
    Ok((open, owner))
}

/// Only input/owner/head seals, not reduce/load/session recovery (which recurse).
fn raw(db: &Connection, id: &str) -> Result<(Row, OpenInput)> {
    let shape: bool = db.query_row("SELECT length(CAST(open_id AS BLOB))=32 AND length(CAST(owner_id AS BLOB))=32 AND typeof(input)='blob' AND length(input) BETWEEN 1 AND 16384 AND event_count BETWEEN 0 AND 4096 AND length(state_hash)=32 AND (head_hash IS NULL OR length(head_hash)=32) FROM windows_open_journal WHERE open_id=?",[id],|r|r.get(0))?;
    if !shape {
        return Err("prospective open metadata shape changed".into());
    }
    let (owner, bytes, count, head, hash): RawRow = db.query_row(
        "SELECT owner_id,CASE WHEN typeof(input)='blob' AND length(input) BETWEEN 1 AND 16384 THEN input END,event_count,head_hash,state_hash FROM windows_open_journal WHERE open_id=?", [id],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
    if !(0..=EVENT_LIMIT as i64).contains(&count)
        || hash.len() != 32
        || head.as_ref().is_some_and(|h| h.len() != 32)
        || (count == 0) != head.is_none()
    {
        return Err("prospective open bounded head changed".into());
    }
    let input: OpenInput = serde_json::from_slice(&bytes)?;
    valid_input(&input, &owner)?;
    let row = Row {
        input: input.clone(),
        owner,
        count: count.try_into()?,
        head,
    };
    let retained: Vec<u8> = db.query_row(
        "SELECT CASE WHEN length(input_hash)=32 THEN input_hash END FROM retained_windows_opens WHERE open_id=?",
        [id],
        |r| r.get(0),
    )?;
    if input.id != id
        || checksum(&row)? != hash
        || anchor(&row)? != retained
        || identity::read(db)?.as_ref() != Some(&row.owner)
    {
        return Err("prospective open retained anchor changed".into());
    }
    session::binding(db, &input.session, &input.volume)?;
    Ok((row, input))
}

pub(crate) fn check_anchor(db: &Connection, input: &CreatorInput) -> Result<()> {
    let (expected, owner) = expected(input)?;
    let (row, actual) = raw(db, &expected.id)?;
    if expected != actual || row.owner != owner {
        return Err("creator/open original binding changed".into());
    }
    Ok(())
}
pub(crate) fn check_input(db: &Connection, input: &OpenInput, owner: &str) -> Result<()> {
    if let Some(operation) = &input.creation {
        let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 45 {
            return Err("prospective open requires schema45".into());
        }
        let original = creators::admission::original(db, operation)?;
        let (expected, who) = expected(original.input())?;
        if expected != *input || who != owner {
            return Err("prospective open lost original creator".into());
        }
    }
    Ok(())
}
pub(crate) fn runtime(state: &State) -> Result<String> {
    session::runtime(state)
}
pub(crate) fn check_session(
    db: &Connection,
    runtime: &str,
    session: &str,
    volume: &str,
) -> Result<()> {
    session::current(db, runtime, session, volume)
}
pub(crate) fn insert(db: &Connection, creator: &CreatorInput, runtime: &str) -> Result<()> {
    let (input, owner) = expected(creator)?;
    session::current(db, runtime, &input.session, &input.volume)?;
    for (_, old) in all(db)? {
        if old.input.id == input.id
            || (old.input.volume == input.volume && old.input.node == input.node)
        {
            return Err("prospective open collides with retained handle".into());
        }
    }
    if db.query_row(
        "SELECT EXISTS(SELECT 1 FROM retained_windows_opens WHERE open_id=?)",
        [&input.id],
        |r| r.get::<_, bool>(0),
    )? {
        return Err("prospective open identity is retained".into());
    }
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
    insert_row(db, &row, &record)?;
    Ok(())
}

/// Unknown creator effects cannot become a no-effect abandoned session.
pub(crate) fn recoverable(db: &Connection, input: &OpenInput) -> Result<()> {
    let Some(operation) = &input.creation else {
        return Ok(());
    };
    let original = creators::admission::original(db, operation)?;
    check_input(
        db,
        input,
        original
            .input()
            .owner
            .as_deref()
            .ok_or("creator owner missing")?,
    )?;
    if original.rejection().is_some() {
        return Ok(());
    }
    let saved = super::super::load(db, operation)?
        .ok_or("recover exact creator result and import before abandonment")?;
    if original.request() != Some(&saved.input.request)
        || !matches!(&saved.outcome,Some(Ok(r)) if original.receipt()==Some(r))
    {
        return Err("prospective creator remains unknown".into());
    }
    Ok(())
}
pub(crate) fn check_recovery_open(db: &Connection, id: &str) -> Result<()> {
    let (_, input) = raw(db, id)?;
    recoverable(db, &input)
}
pub(crate) fn activation(db: &Connection, input: &OpenInput) -> Result<()> {
    recoverable(db, input)?;
    let operation = input
        .creation
        .as_ref()
        .ok_or("prospective creator missing")?;
    let saved = super::super::load(db, operation)?.ok_or("creator import missing")?;
    if !matches!(saved.outcome, Some(Ok(_))) {
        return Err("creator not successfully imported".into());
    }
    let owned = removals::owned(db, &input.volume, &input.node)?
        .ok_or("creator original owner no longer deferred")?;
    if owned.fence.intent != *operation || owned.mode != RemovalMode::Deferred {
        return Err("creator original fence is no longer deferred".into());
    }
    Ok(())
}

/// Bulk valid history to exercise row and event quotas independently. Physical
/// handles are modeled; admission itself uses the real creator transaction.
#[cfg(test)]
pub(crate) fn quota_fixture(
    db: &Connection,
    creator: &CreatorInput,
    owner: &str,
    count: usize,
    eventful: bool,
) -> Result<()> {
    for n in 0..count {
        let (mut input, _) = expected(creator)?;
        input.id = format!("{:032x}", 10000 + n);
        input.node = input.id.clone();
        input.creation = None;
        input.delete_on_close = false;
        input.session_bound = false;
        input.controlled = eventful;
        input.posix = false;
        let events = if eventful {
            vec![
                Event {
                    id: format!("{:032x}", 20000 + n),
                    action: Action::Activate { proof: None },
                },
                Event {
                    id: format!("{:032x}", 30000 + n),
                    action: Action::Callback(Callback::SetDelete { delete: false }),
                },
            ]
        } else {
            Vec::new()
        };
        let mut previous = None;
        let mut stored_events = Vec::new();
        for (index, event) in events.iter().enumerate() {
            let hash = event_hash(&input.id, index + 1, event, &previous)?;
            stored_events.push((index, event, previous.clone(), hash.clone()));
            previous = Some(hash);
        }
        let row = Row {
            input,
            owner: owner.into(),
            count: events.len(),
            head: previous,
        };
        db.execute(
            "INSERT INTO windows_open_journal VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                row.input.id,
                row.owner,
                serde_json::to_vec(&row.input)?,
                row.count as i64,
                row.head,
                checksum(&row)?
            ],
        )?;
        db.execute(
            "INSERT INTO retained_windows_opens VALUES(?1,?2)",
            params![row.input.id, anchor(&row)?],
        )?;
        for (index, event, previous, hash) in stored_events {
            db.execute(
                "INSERT INTO windows_open_events VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    event.id,
                    row.input.id,
                    (index + 1) as i64,
                    serde_json::to_vec(&event.action)?,
                    previous,
                    hash
                ],
            )?;
        }
    }
    let records = all(db)?;
    capacity(&records)?;
    Ok(())
}
