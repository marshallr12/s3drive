//! Lossless retirement, not evidence deletion or abandoned-session cleanup.
use super::*;
fn seal(state_hash: &[u8]) -> Vec<u8> {
    let mut h = ProofSha256::new();
    h.update(b"s3drive-open-retirement-v1\0");
    h.update(state_hash);
    h.finalize().to_vec()
}
pub(super) fn is_retired(db: &Connection, key: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM retired_windows_opens WHERE open_id=?)",
        [key],
        |r| r.get(0),
    )?)
}
fn terminal(db: &Connection, row: &Row, record: &OpenRecord) -> Result<bool> {
    creation::recoverable(db, &record.input)?;
    Ok((record.input.controlled
        && record.active()
        && (record.cleanup().is_some_and(|e| record.completed(&e.id))
            || (record.cleanup().is_none()
                && record
                    .events
                    .iter()
                    .any(|e| matches!(e.action, Action::BindSparse))))
        && record.close().is_some_and(|e| record.completed(&e.id))
        && record.pending().is_empty()
        && record
            .events
            .iter()
            .all(|e| !matches!(e.action, Action::Callback(_)) || record.completed(&e.id)))
        || session::closed(db, row, record)?)
}
pub(super) fn check(db: &Connection, row: &Row, record: &OpenRecord) -> Result<()> {
    let seal: Option<Vec<u8>> = db
        .query_row(
            "SELECT CASE WHEN length(state_hash)=32 THEN state_hash END FROM retired_windows_opens WHERE open_id=?",
            [&row.input.id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(saved) = seal
        && (saved != self::seal(&checksum(row)?) || !terminal(db, row, record)?)
    {
        return Err("retired open proof changed or is incomplete".into());
    }
    Ok(())
}
pub(super) fn check_index(db: &Connection) -> Result<()> {
    let bad: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM retired_windows_opens r LEFT JOIN windows_open_journal j USING(open_id) LEFT JOIN retained_windows_opens a USING(open_id) WHERE j.open_id IS NULL OR a.open_id IS NULL OR length(r.state_hash)<>32 OR length(j.state_hash)<>32)", [], |r| r.get(0))?;
    if bad {
        return Err("retired open index lost its retained proof".into());
    }
    let mut stmt = db.prepare("SELECT r.state_hash,j.state_hash FROM retired_windows_opens r JOIN windows_open_journal j USING(open_id)")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let saved: Vec<u8> = row.get(0)?;
        if saved != seal(&row.get::<_, Vec<u8>>(1)?) {
            return Err("retired open seal changed".into());
        }
    }
    Ok(())
}
impl Journal<'_> {
    /// Free live capacity only after every recorded callback is complete.
    /// Complete original evidence remains available through `get` and exact
    /// callback replay. This never infers cleanup from a different session ID.
    pub fn retire(&mut self, key: &str) -> Result<()> {
        self.chunk_check()?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(key) = &self.chunk {
            crate::chunk_mount::verify_key(&tx, key)?;
        }
        let (row, record) = load(&tx, key)?.ok_or("per-open record not found")?;
        if !terminal(&tx, &row, &record)? {
            return Err("unfinished open cannot be retired".into());
        }
        if is_retired(&tx, key)? {
            return Ok(());
        }
        // Retirement is a new local capacity release, unlike replaying an
        // already sealed historical retirement. Retain every open while any
        // preparation for its node has an unresolved effect/source settlement.
        if self.chunk.is_some() {
            crate::sparse_views::preparation::publication::namespace_pending(
                &tx,
                &record.input.volume,
                &record.input.node,
            )?;
        }
        all(&tx)?;
        if tx.execute(
            "INSERT INTO retired_windows_opens VALUES(?1,?2)",
            params![key, seal(&checksum(&row)?)],
        )? != 1
            || !is_retired(&tx, key)?
            || load(&tx, key)?.as_ref().map(|(_, r)| r) != Some(&record)
        {
            return Err("open retirement was not durably recorded".into());
        }
        tx.commit()?;
        Ok(())
    }
}

/// Fresh linked-native slice plus fully settled no-effect recovery groups.
/// Publishers recovered without an observed Close, creation and namespace
/// ownership retain their original journals until compact proofs exist.
pub(crate) fn native_candidates(
    db: &Connection,
    volume: &str,
    node: &str,
) -> Result<Vec<OpenRecord>> {
    if !crate::chunk_mount::retry_enabled(db)? {
        return Err("native retention requires fresh retry contract".into());
    }
    for table in ["namespace_intents", "manifest_creations"] {
        if db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| {
            r.get::<_, bool>(0)
        })? {
            return Err(
                "native retention preserves namespace, creator and recovery evidence".into(),
            );
        }
    }
    let ids = db
        .prepare("SELECT open_id FROM windows_open_journal")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut records = Vec::new();
    for id in ids {
        let (row, record) = load(db, &id)?.ok_or("native open disappeared")?;
        if record.input.volume != volume || record.input.node != node {
            continue;
        }
        if records.len() >= OPEN_LIMIT
            || !is_retired(db, &id)?
            || !terminal(db, &row, &record)?
            || record.input.admission.is_some()
            || record.input.creation.is_some()
            || record.input.delete_on_close
            || !record.input.controlled
            || !record.input.session_bound
            || !record
                .events
                .iter()
                .any(|e| matches!(e.action, Action::BindSparse))
            || record.events.iter().any(|e| {
                matches!(
                    e.action,
                    Action::Link { .. } | Action::Acknowledge { .. } | Action::Reply { .. }
                )
            })
        {
            return Err("native retention requires exact completed linked opens".into());
        }
        records.push(record);
    }
    session::no_effect_retirement_groups(db, &records)?;
    Ok(records)
}
pub(crate) fn erase_native(db: &Connection, records: &[OpenRecord], runtime: &str) -> Result<()> {
    let recoveries = session::no_effect_retirement_groups(db, records)?;
    // Check every sealed record while its complete recovery proof still exists.
    for record in records {
        if load(db, &record.input.id)?.as_ref().map(|(_, r)| r) != Some(record)
            || !is_retired(db, &record.input.id)?
        {
            return Err("native retirement proof changed".into());
        }
    }
    for recovery in recoveries {
        db.execute(
            "DELETE FROM recovered_windows_opens WHERE recovery_id=?1",
            [&recovery],
        )?;
        if db.execute(
            "DELETE FROM windows_session_recoveries WHERE recovery_id=?1",
            [&recovery],
        )? != 1
        {
            return Err("native recovery retirement lost its exact group".into());
        }
    }
    for record in records {
        for table in [
            "windows_open_events",
            "retired_windows_opens",
            "retained_windows_opens",
            "windows_open_journal",
        ] {
            db.execute(
                &format!("DELETE FROM {table} WHERE open_id=?1"),
                [&record.input.id],
            )?;
        }
    }
    session::prune_unused_sessions(db, runtime)?;
    check_index(db)?;
    Ok(())
}

/// Exact final Close suffix, independently sealed by the mount's terminal
/// witness. Earlier ordinary events are represented only by their chain hash.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminalSuffix {
    ordinal: usize,
    previous: Option<Vec<u8>>,
    events: Vec<Event>,
    retired: Vec<u8>,
}
impl TerminalSuffix {
    pub(crate) fn capture(db: &Connection, record: &OpenRecord) -> Result<Self> {
        let (row, checked) = load(db, &record.input.id)?.ok_or("terminal open missing")?;
        if &checked != record || !is_retired(db, &record.input.id)? {
            return Err("terminal open changed".into());
        }
        let at = record
            .events
            .iter()
            .position(|e| e.action == Action::Callback(Callback::Close))
            .ok_or("terminal Close missing")?;
        let mut previous = None;
        for (index, event) in record.events[..at].iter().enumerate() {
            previous = Some(event_hash(&record.input.id, index + 1, event, &previous)?);
        }
        Ok(Self {
            ordinal: at + 1,
            previous,
            events: record.events[at..].to_vec(),
            retired: seal(&checksum(&row)?),
        })
    }
    pub(crate) fn validate(
        &self,
        input: &OpenInput,
        owner: &str,
        close: &str,
        completion: &str,
        frozen: &[crate::sparse_views::Id],
        view: crate::sparse_views::Id,
    ) -> Result<Vec<u8>> {
        let count = self
            .ordinal
            .checked_add(self.events.len())
            .and_then(|n| n.checked_sub(1))
            .ok_or("terminal suffix count overflow")?;
        if self.ordinal == 0
            || self.ordinal > EVENT_LIMIT
            || self.events.len() < 3
            || count > EVENT_LIMIT
            || self.previous.as_ref().is_some_and(|h| h.len() != 32)
            || (self.ordinal == 1) != self.previous.is_none()
            || self.retired.len() != 32
        {
            return Err("terminal Close suffix shape changed".into());
        }
        let mut previous = self.previous.clone();
        let mut plans = std::collections::BTreeSet::new();
        let mut witnessed = false;
        for (index, event) in self.events.iter().enumerate() {
            if !id(&event.id) {
                return Err("terminal event ID invalid".into());
            }
            match &event.action {
                Action::Callback(Callback::Close) if index == 0 && event.id == close => (),
                Action::FreezeSparse { callback, plan }
                    if index > 0 && !witnessed && callback == close && plans.insert(*plan) => {}
                Action::CompleteSparse {
                    callback,
                    view: Some(v),
                    frozen: hash,
                } if index == self.events.len() - 2
                    && callback == close
                    && *v == view
                    && hash.as_slice()
                        == digest(b"s3drive-native-sparse-completion-plans-v1\0", &plans)? =>
                {
                    witnessed = true;
                }
                Action::Completed { callback }
                    if index == self.events.len() - 1
                        && witnessed
                        && callback == close
                        && event.id == completion => {}
                _ => return Err("terminal Close suffix ordering changed".into()),
            }
            previous = Some(event_hash(
                &input.id,
                self.ordinal + index,
                event,
                &previous,
            )?);
        }
        if plans.into_iter().collect::<Vec<_>>() != frozen {
            return Err("terminal frozen set changed".into());
        }
        let row = Row {
            input: input.clone(),
            owner: owner.into(),
            count,
            head: previous,
        };
        if seal(&checksum(&row)?) != self.retired {
            return Err("terminal original retired seal changed".into());
        }
        Ok(self.retired.clone())
    }
}

#[cfg(test)]
pub(crate) fn rewrite_epoch_for_test(db: &Connection, key: &str) -> Result<()> {
    let (mut row, mut record) = load(db, key)?.ok_or("missing test open")?;
    let mut changed = false;
    for event in &mut record.events {
        if let Action::OwnSparseEpoch { operation, .. } = &mut event.action {
            *operation = [222; 32];
            changed = true;
            break;
        }
    }
    assert!(changed);
    let mut previous = None;
    for (index, event) in record.events.iter().enumerate() {
        let digest = event_hash(key, index + 1, event, &previous)?;
        db.execute("UPDATE windows_open_events SET action=?1,previous_hash=?2,state_hash=?3 WHERE open_id=?4 AND event_id=?5", params![serde_json::to_vec(&event.action)?, previous, digest, key, event.id])?;
        previous = Some(digest);
    }
    row.head = previous;
    let checksum = checksum(&row)?;
    db.execute(
        "UPDATE windows_open_journal SET head_hash=?1,state_hash=?2 WHERE open_id=?3",
        params![row.head, checksum, key],
    )?;
    db.execute(
        "UPDATE retired_windows_opens SET state_hash=?1 WHERE open_id=?2",
        params![seal(&checksum), key],
    )?;
    assert!(
        load(db, key)?.is_some(),
        "forgery must pass structural event/row/seal checks"
    );
    Ok(())
}

#[cfg(test)]
pub(crate) fn rewrite_session_for_test(
    db: &Connection,
    key: &str,
    wrong_volume: bool,
) -> Result<()> {
    let (mut row, _) = load(db, key)?.ok_or("missing test open")?;
    let unknown = "99999999999999999999999999999999".to_owned();
    if wrong_volume {
        row.input.volume = unknown;
    } else {
        row.input.session = unknown;
    }
    let checksum = checksum(&row)?;
    db.execute(
        "UPDATE windows_open_journal SET input=?1,state_hash=?2 WHERE open_id=?3",
        params![serde_json::to_vec(&row.input)?, checksum, key],
    )?;
    db.execute(
        "UPDATE retained_windows_opens SET input_hash=?1 WHERE open_id=?2",
        params![anchor(&row)?, key],
    )?;
    db.execute(
        "UPDATE retired_windows_opens SET state_hash=?1 WHERE open_id=?2",
        params![seal(&checksum), key],
    )?;
    Ok(())
}

/// Unlike linked anchors, a whole-forest cut leaves no source requiring an
/// original Close suffix. This only selects; the caller must independently
/// prove all source heads clean, roots settled, namespace closed and no readers.
pub(crate) fn quiescent_candidates(db: &Connection) -> Result<Option<Vec<OpenRecord>>> {
    if !crate::chunk_mount::quiescent_enabled(db)? {
        return Err("quiescent contract missing".into());
    }
    session::check_index(db)?;
    check_index(db)?;
    let ids = db
        .prepare("SELECT open_id FROM windows_open_journal LIMIT 257")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if ids.len() > OPEN_LIMIT {
        return Err("quiescent open capacity exceeded".into());
    }
    let mut records = Vec::new();
    for id in ids {
        let (row, record) = load(db, &id)?.ok_or("quiescent open missing")?;
        if !is_retired(db, &id)? || !terminal(db, &row, &record)? {
            return Ok(None);
        }
        if !record.input.controlled || !record.input.session_bound {
            return Err("quiescent open lacks original ownership".into());
        }
        records.push(record);
    }
    if !session::quiescent_groups(db, &records)? {
        return Ok(None);
    }
    Ok(Some(records))
}
