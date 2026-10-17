//! Lossless retirement, not evidence deletion or abandoned-session cleanup.
use super::*;
fn seal(state_hash: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
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
fn terminal(record: &OpenRecord) -> bool {
    record.input.controlled
        && record.active()
        && record.cleanup().is_some_and(|e| record.completed(&e.id))
        && record.close().is_some_and(|e| record.completed(&e.id))
        && record.pending().is_empty()
        && record
            .events
            .iter()
            .all(|e| !matches!(e.action, Action::Callback(_)) || record.completed(&e.id))
}
pub(super) fn check(db: &Connection, row: &Row, record: &OpenRecord) -> Result<()> {
    let seal: Option<Vec<u8>> = db
        .query_row(
            "SELECT state_hash FROM retired_windows_opens WHERE open_id=?",
            [&row.input.id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(saved) = seal
        && (saved != self::seal(&checksum(row)?) || !terminal(record))
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
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (row, record) = load(&tx, key)?.ok_or("per-open record not found")?;
        if !terminal(&record) {
            return Err("unfinished open cannot be retired".into());
        }
        if is_retired(&tx, key)? {
            return Ok(());
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
