//! Explicit epoch-bound mutation identities. No byte reclamation or S3 rights.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    epoch: Id,
    nonce: Id,
}
impl fmt::Debug for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ScopedSourceOperation([private])")
    }
}
impl Operation {
    pub fn new(binding: &Binding, nonce: Id) -> Result<Self> {
        Self::from_epoch(binding.epoch()?, nonce)
    }
    pub fn from_epoch(epoch: Id, nonce: Id) -> Result<Self> {
        if epoch == [0; 32] || nonce == [0; 32] {
            return Err(Failure::Invalid.into());
        }
        Ok(Self { epoch, nonce })
    }
    pub fn epoch(&self) -> Id {
        self.epoch
    }
    pub fn id(&self) -> Result<Id> {
        Self::from_epoch(self.epoch, self.nonce)?;
        hash(
            b"s3drive-scoped-source-operation-v1\0",
            &(self.epoch, self.nonce),
        )
    }
}
fn mode_hash() -> Result<Id> {
    hash(b"s3drive-source-scoped-mode-v1\0", &1u8)
}
pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT singleton,format,state_hash FROM sparse_scoped_mode LIMIT 0")?;
    Ok(())
}
pub(crate) fn enabled(db: &Connection) -> Result<bool> {
    // Disabled mode is the common legacy path. One bounded typed query avoids
    // rescanning sqlite_schema during every full source/transfer validation.
    let mut query = match db
        .prepare("SELECT singleton,format,state_hash FROM sparse_scoped_mode LIMIT 2")
    {
        Ok(query) => query,
        Err(error) => {
            let exists: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='sparse_scoped_mode')",
                [], |r| r.get(0),
            )?;
            if !exists {
                let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
                return if version <= 35 {
                    Ok(false)
                } else {
                    Err(Failure::Corrupt.into())
                };
            }
            return Err(error.into());
        }
    };
    let mut rows = query.query([])?;
    let Some(row) = rows.next()? else {
        return Ok(false);
    };
    let (singleton, format, sha): (i64, i64, Vec<u8>) = (row.get(0)?, row.get(1)?, row.get(2)?);
    if rows.next()?.is_some() {
        return Err(Failure::Corrupt.into());
    }
    let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 36 {
        return Err(Failure::Corrupt.into());
    }
    if singleton != 1 || format != 1 || sha != mode_hash()? {
        return Err(Failure::Corrupt.into());
    }
    // An attached trigger could remove the permanent mode during an otherwise
    // valid source transaction. Refuse it before any scoped mutation or replay.
    if db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Failure::Corrupt.into());
    }
    isolation(db)?;
    Ok(true)
}
pub(crate) fn require_legacy(db: &Connection) -> Result<()> {
    if enabled(db)? {
        return Err(Failure::Isolated.into());
    }
    Ok(())
}
pub(super) fn enable(state: &mut State) -> Result<()> {
    enable_inner(state, |_| Ok(()))
}
pub(in crate::sparse_views) fn enable_inner(
    state: &mut State,
    mut hook: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    let tx = state
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    isolation(&tx)?;
    let before = inspect(&tx)?;
    if enabled(&tx)? {
        return Ok(());
    }
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Failure::Corrupt.into());
    }
    if tx.execute(
        "INSERT INTO sparse_scoped_mode VALUES(1,1,?1)",
        [mode_hash()?.as_slice()],
    )? != 1
    {
        return Err(Failure::Corrupt.into());
    }
    hook("Stored")?;
    if !enabled(&tx)? || inspect(&tx)? != before {
        return Err(Failure::Corrupt.into());
    }
    hook("Validated")?;
    tx.commit()?;
    hook("Committed")?;
    Ok(())
}
/// A retained result may replay; caller epoch must describe its original input.
pub(crate) fn known(
    db: &Connection,
    scope: Option<Operation>,
    raw: Id,
    binding: &Binding,
) -> Result<()> {
    if let Some(token) = scope {
        if !enabled(db)? {
            return Err(Failure::ScopedDisabled.into());
        }
        if token.id()? != raw || token.epoch != binding.epoch()? {
            return Err(Failure::EpochClosed.into());
        }
    }
    Ok(())
}
/// Unknown raw IDs never receive admission after explicit scoped-mode opt-in.
pub(crate) fn admit(
    db: &Connection,
    scope: Option<Operation>,
    raw: Id,
    binding: &Binding,
) -> Result<()> {
    let on = enabled(db)?;
    match scope {
        None if on => Err(Failure::ScopedRequired.into()),
        Some(_) if !on => Err(Failure::ScopedDisabled.into()),
        Some(token) if token.id()? != raw || token.epoch != binding.epoch()? => {
            Err(Failure::EpochClosed.into())
        }
        _ => Ok(()),
    }
}
