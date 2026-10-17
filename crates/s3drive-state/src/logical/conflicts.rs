//! Explicitly retain a confirmed failed publication without settling its bytes.
use super::*;
use rusqlite::OptionalExtension;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedConflict {
    pub edit: String,
    pub publication: String,
    pub rejection: managed::Rejection,
}
fn evidence(db: &Connection, r: &Record) -> Result<(RetainedConflict, Vec<u8>)> {
    if r.receipt.is_some() {
        return Err("settled edit cannot acquire a retained conflict".into());
    }
    let input = r.publication.as_ref().ok_or("edit has no publication")?;
    let physical = managed::load(db, input.id())?.ok_or("physical publication missing")?;
    if physical.input != *input {
        return Err("retained conflict changed publication input".into());
    }
    let rejection = physical
        .rejection
        .ok_or("publication outcome remains unresolved")?;
    let checksum = hash(&(
        "retained-conflict-v1",
        &r.input,
        r.generation,
        input,
        &rejection,
    ))?;
    Ok((
        RetainedConflict {
            edit: r.input.id.clone(),
            publication: input.id().into(),
            rejection,
        },
        checksum,
    ))
}
pub(super) fn checked(db: &Connection, r: &Record) -> Result<Option<RetainedConflict>> {
    let row: Option<(String, Vec<u8>)> = db
        .query_row(
            "SELECT publication_id,state_hash FROM logical_conflicts WHERE edit_id=?1",
            [&r.input.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((publication, checksum)) = row else {
        return Ok(None);
    };
    let (conflict, expected) = evidence(db, r)?;
    if publication != conflict.publication || checksum != expected {
        return Err("retained conflict integrity failure".into());
    }
    Ok(Some(conflict))
}
impl Journal<'_> {
    pub fn retained_conflict(&mut self, id: &str) -> Result<Option<RetainedConflict>> {
        let r = self.get(id)?;
        checked(&self.state.connection, &r)
    }
    pub fn retain_conflict(&mut self, id: &str) -> Result<RetainedConflict> {
        let r = self.get(id)?;
        if let Some(old) = checked(&self.state.connection, &r)? {
            return Ok(old);
        }
        let (conflict, checksum) = evidence(&self.state.connection, &r)?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rows = tx.execute(
            "INSERT INTO logical_conflicts VALUES(?1,?2,?3)",
            params![id, conflict.publication, checksum],
        )?;
        if rows != 1 || checked(&tx, &r)?.as_ref() != Some(&conflict) {
            return Err("conflict was not durably retained".into());
        }
        tx.commit()?;
        Ok(conflict)
    }
}
