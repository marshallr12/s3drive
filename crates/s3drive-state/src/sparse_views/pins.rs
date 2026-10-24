//! Explicit local retention tokens. No automatic reader lifetime or reclamation.
use super::*;
use rusqlite::OptionalExtension;
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_ACTIVE: usize = 256;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Corrupt,
    Stale,
    Quota,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "source read-pin {:?}; no reclamation authorized", self)
    }
}
impl std::error::Error for Failure {}
/// An explicit registry token, not a clone-sharing file handle. Release closes
/// it; subsequent reads using the token must fail. No public constructor.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct Pin {
    id: Id,
    owner: String,
    sequence: u64,
    view: Id,
}
impl fmt::Debug for Pin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SourceReadPin([private])")
    }
}
impl Pin {
    pub fn id(&self) -> Id {
        self.id
    }
    pub fn view_id(&self) -> Id {
        self.view
    }
    fn new(owner: String, sequence: u64, view: Id) -> Result<Self> {
        if !owner_valid(&owner) || sequence == 0 || sequence > i64::MAX as u64 || view == [0; 32] {
            return Err(Failure::Corrupt.into());
        }
        let id = hash(
            b"s3drive-source-read-pin-id-v1\0",
            &(&owner, sequence, view),
        )?;
        Ok(Self {
            id,
            owner,
            sequence,
            view,
        })
    }
    fn sha(&self) -> Result<Id> {
        hash(b"s3drive-source-read-pin-row-v1\0", self)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize)]
struct Role {
    owner: String,
    issued: u64,
    active: u16,
    entries: Id,
}
impl Role {
    fn sha(&self) -> Result<Id> {
        hash(b"s3drive-source-read-pin-role-v1\0", self)
    }
}
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Ledger {
    role: Option<Role>,
    pub(super) pins: BTreeMap<Id, Pin>,
}
fn owner_valid(s: &str) -> bool {
    s.len() == 32
        && s != "00000000000000000000000000000000"
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn entries(pins: &BTreeMap<Id, Pin>) -> Result<Id> {
    hash(
        b"s3drive-source-read-pin-entries-v1\0",
        &pins.values().collect::<Vec<_>>(),
    )
}
pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT singleton,format,owner,issued,active,entries_hash,state_hash FROM source_read_pin_session LIMIT 0")?;
    db.prepare("SELECT pin_id,owner,sequence,view_id,state_hash FROM source_read_pins LIMIT 0")?;
    Ok(())
}
pub(super) fn load(db: &Connection) -> Result<Ledger> {
    load_in(&mut IsolationCheck::new(db))
}
pub(super) fn load_in(scope: &mut IsolationCheck<'_>) -> Result<Ledger> {
    let db = scope.db;
    if check_schema(db).is_err() {
        let n:i64=db.query_row("SELECT count(*) FROM sqlite_schema WHERE type='table' AND name IN ('source_read_pin_session','source_read_pins')",[],|r|r.get(0))?;
        let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if n == 0 && version <= 36 {
            return Ok(Ledger::default());
        }
        return Err(Failure::Corrupt.into());
    }
    let mut out = Ledger::default();
    let mut query=db.prepare("SELECT singleton,format,owner,issued,active,entries_hash,state_hash FROM source_read_pin_session LIMIT 2")?;
    let mut rows = query.query([])?;
    if let Some(r) = rows.next()? {
        let (singleton, format, owner, issued, active, ent, sha): (
            i64,
            i64,
            String,
            i64,
            i64,
            Vec<u8>,
            Vec<u8>,
        ) = (
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
            r.get(6)?,
        );
        if singleton != 1
            || format != 1
            || !owner_valid(&owner)
            || issued < 0
            || !(0..=MAX_ACTIVE as i64).contains(&active)
            || rows.next()?.is_some()
        {
            return Err(Failure::Corrupt.into());
        }
        let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 37 {
            return Err(Failure::Corrupt.into());
        }
        let role = Role {
            owner,
            issued: issued as u64,
            active: active as u16,
            entries: ent.try_into().map_err(|_| Failure::Corrupt)?,
        };
        if sha != role.sha()? {
            return Err(Failure::Corrupt.into());
        }
        scope.require()?;
        out.role = Some(role);
    }
    let mut sequences = BTreeSet::new();
    let mut query=db.prepare("SELECT pin_id,owner,sequence,view_id,state_hash FROM source_read_pins ORDER BY pin_id LIMIT 257")?;
    let mut rows = query.query([])?;
    while let Some(r) = rows.next()? {
        let (id, owner, sequence, view, sha): (Vec<u8>, String, i64, Vec<u8>, Vec<u8>) =
            (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?);
        let role = out.role.as_ref().ok_or(Failure::Corrupt)?;
        if sequence <= 0
            || sequence as u64 > role.issued
            || owner != role.owner
            || !sequences.insert(sequence)
        {
            return Err(Failure::Corrupt.into());
        }
        let pin = Pin::new(
            owner,
            sequence as u64,
            view.try_into().map_err(|_| Failure::Corrupt)?,
        )?;
        if pin.id.as_slice() != id
            || pin.sha()?.as_slice() != sha
            || out.pins.insert(pin.id, pin).is_some()
            || out.pins.len() > MAX_ACTIVE
        {
            return Err(Failure::Corrupt.into());
        }
    }
    match &out.role {
        None if !out.pins.is_empty() => return Err(Failure::Corrupt.into()),
        Some(role)
            if role.active as usize != out.pins.len() || role.entries != entries(&out.pins)? =>
        {
            return Err(Failure::Corrupt.into());
        }
        _ => {}
    }
    if !out.pins.is_empty() && db.query_row("SELECT EXISTS(SELECT 1 FROM source_read_pins p LEFT JOIN sparse_views v ON v.view_id=p.view_id WHERE v.view_id IS NULL)",[],|r|r.get::<_,bool>(0))? {
        return Err(Failure::Corrupt.into());
    }
    Ok(out)
}
pub(crate) fn validate_headers(db: &Connection) -> Result<()> {
    load(db).map(|_| ())
}
pub(crate) fn require_legacy(db: &Connection) -> Result<()> {
    if load(db)?.role.is_some() {
        return Err(super::Failure::Isolated.into());
    }
    Ok(())
}
pub(crate) fn verify_owner(state: &State) -> Result<()> {
    if let Some(expected) = state.source_pin_witness {
        let actual: Option<Vec<u8>> = state
            .connection
            .query_row(
                "SELECT state_hash FROM source_read_pin_session WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if actual.as_deref() != Some(expected.as_slice()) {
            return Err(Failure::Corrupt.into());
        }
    }
    Ok(())
}
fn current_owner(state: &State) -> Result<String> {
    let owner = state.incarnation.as_ref().ok_or(Failure::Stale)?;
    if !owner_valid(owner) {
        return Err(Failure::Corrupt.into());
    }
    Ok(owner.clone())
}
fn no_triggers(db: &Connection) -> Result<()> {
    if db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
fn update_role(db: &Connection, previous: Option<&Role>, next: &Role) -> Result<()> {
    let n = if let Some(old) = previous {
        db.execute("UPDATE source_read_pin_session SET owner=?1,issued=?2,active=?3,entries_hash=?4,state_hash=?5 WHERE singleton=1 AND state_hash=?6",params![next.owner,next.issued as i64,next.active,next.entries.as_slice(),next.sha()?.as_slice(),old.sha()?.as_slice()])?
    } else {
        db.execute(
            "INSERT INTO source_read_pin_session VALUES(1,1,?1,?2,?3,?4,?5)",
            params![
                next.owner,
                next.issued as i64,
                next.active,
                next.entries.as_slice(),
                next.sha()?.as_slice()
            ],
        )?
    };
    if n != 1 {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Boundary {
    SessionCleared,
    PinStored,
    RoleStored,
    Validated,
    Committed,
}
pub(super) fn acquire(state: &mut State, view: Id) -> Result<Pin> {
    acquire_inner(state, view, |_| Ok(()))
}
pub(super) fn acquire_inner(
    state: &mut State,
    view: Id,
    mut hook: impl FnMut(Boundary) -> Result<()>,
) -> Result<Pin> {
    state.verify_staging_storage()?;
    let owner = current_owner(state)?;
    let tx = state
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let before = ledger::inspect_isolated(&tx)?;
    before.record(view)?;
    no_triggers(&tx)?;
    let mut expected = before.clone();
    let pins = &mut expected.read_pins;
    let rotate = pins.role.as_ref().is_some_and(|r| r.owner != owner);
    if rotate {
        if tx.execute("DELETE FROM source_read_pins", [])? != pins.pins.len() {
            return Err(Failure::Corrupt.into());
        }
        pins.pins.clear();
        hook(Boundary::SessionCleared)?;
    }
    if pins.pins.len() >= MAX_ACTIVE {
        return Err(Failure::Quota.into());
    }
    let issued = if rotate {
        0
    } else {
        pins.role.as_ref().map_or(0, |r| r.issued)
    };
    let sequence = issued
        .checked_add(1)
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or(Failure::Quota)?;
    let pin = Pin::new(owner.clone(), sequence, view)?;
    if pins.pins.insert(pin.id, pin.clone()).is_some() {
        return Err(Failure::Corrupt.into());
    }
    let role = Role {
        owner,
        issued: sequence,
        active: pins.pins.len() as u16,
        entries: entries(&pins.pins)?,
    };
    pins.role = Some(role.clone());
    if tx.execute(
        "INSERT INTO source_read_pins VALUES(?1,?2,?3,?4,?5)",
        params![
            pin.id.as_slice(),
            pin.owner,
            pin.sequence as i64,
            pin.view.as_slice(),
            pin.sha()?.as_slice()
        ],
    )? != 1
    {
        return Err(Failure::Corrupt.into());
    }
    hook(Boundary::PinStored)?;
    update_role(&tx, before.read_pins.role.as_ref(), &role)?;
    hook(Boundary::RoleStored)?;
    if inspect(&tx)? != expected {
        return Err(Failure::Corrupt.into());
    }
    hook(Boundary::Validated)?;
    let witness = role.sha()?;
    tx.commit()?;
    state.source_pin_witness = Some(witness);
    hook(Boundary::Committed)?;
    Ok(pin)
}
pub(super) fn release(state: &mut State, pin: &Pin) -> Result<bool> {
    release_inner(state, pin, |_| Ok(()))
}
pub(super) fn release_inner(
    state: &mut State,
    pin: &Pin,
    mut hook: impl FnMut(Boundary) -> Result<()>,
) -> Result<bool> {
    state.verify_staging_storage()?;
    if current_owner(state)? != pin.owner {
        return Err(Failure::Stale.into());
    }
    let tx = state
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let before = ledger::inspect_isolated(&tx)?;
    let old = before.read_pins.role.as_ref().ok_or(Failure::Stale)?;
    if old.owner != pin.owner
        || pin.sequence > old.issued
        || Pin::new(pin.owner.clone(), pin.sequence, pin.view)? != *pin
    {
        return Err(Failure::Stale.into());
    }
    let Some(saved) = before.read_pins.pins.get(&pin.id) else {
        return Ok(false);
    };
    if saved != pin {
        return Err(Failure::Corrupt.into());
    }
    no_triggers(&tx)?;
    let mut expected = before.clone();
    expected.read_pins.pins.remove(&pin.id);
    let role = Role {
        active: expected.read_pins.pins.len() as u16,
        entries: entries(&expected.read_pins.pins)?,
        ..old.clone()
    };
    expected.read_pins.role = Some(role.clone());
    if tx.execute(
        "DELETE FROM source_read_pins WHERE pin_id=?1 AND state_hash=?2",
        params![pin.id.as_slice(), pin.sha()?.as_slice()],
    )? != 1
    {
        return Err(Failure::Corrupt.into());
    }
    hook(Boundary::PinStored)?;
    update_role(&tx, Some(old), &role)?;
    hook(Boundary::RoleStored)?;
    if inspect(&tx)? != expected {
        return Err(Failure::Corrupt.into());
    }
    hook(Boundary::Validated)?;
    let witness = role.sha()?;
    tx.commit()?;
    state.source_pin_witness = Some(witness);
    hook(Boundary::Committed)?;
    Ok(true)
}
pub(super) fn check(state: &State, pin: &Pin) -> Result<()> {
    state.verify_staging_storage()?;
    if current_owner(state)? != pin.owner {
        return Err(Failure::Stale.into());
    }
    let ledger = load(&state.connection)?;
    if ledger.pins.get(&pin.id) != Some(pin) {
        return Err(Failure::Stale.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
