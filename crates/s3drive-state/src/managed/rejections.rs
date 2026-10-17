//! Terminal evidence is separate from successful receipts and old record hashes.
use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Rejection {
    Authority {
        request: Box<Apply>,
        error: crate::namespace::Error,
    },
    Physical {
        operation: [u8; 32],
        status: u16,
    },
}
fn hash(input: &Input, rejection: &Rejection) -> Result<Vec<u8>> {
    let mut h = Sha256::new();
    h.update(b"s3drive-managed-rejection-v1\0");
    h.update(serde_json::to_vec(&(input, rejection))?);
    Ok(h.finalize().to_vec())
}
fn validate(db: &Connection, r: &Record, rejection: &Rejection) -> Result<()> {
    if r.receipt.is_some() {
        return Err("successful publication cannot become a retained rejection".into());
    }
    match rejection {
        Rejection::Authority { request, error } => {
            let reserve = request.as_ref() == &r.input.reserve
                && r.reservation.is_none()
                && r.publication.is_none();
            if *error != crate::namespace::Error::Conflict
                || (!reserve && r.publication.as_ref() != Some(request.as_ref()))
            {
                return Err("only an exact permanent metadata conflict may be retained".into());
            }
        }
        Rejection::Physical { operation, status } => {
            let g = r
                .generation
                .ok_or("rejected publication lacks physical generation")?;
            let info = remote::load(db, g)?;
            let expected = r.input.reservation()?;
            if info.phase != remote::Phase::Rejected
                || info.operation.as_bytes() != operation
                || info.rejection_status != Some(*status)
                || !matches!(*status, 409 | 412)
                || info.target != r.input.target()?
                || info.size != expected.size
                || hex(&info.payload_sha256) != expected.sha256
                || info.guard != remote::Guard::Missing
            {
                return Err("physical rejection does not cover this exact immutable blob".into());
            }
        }
    }
    Ok(())
}
pub(super) fn load(db: &Connection, r: &Record) -> Result<Option<Rejection>> {
    let shape:Option<bool>=db.query_row("SELECT length(rejection)<=32768 AND length(state_hash)=32 FROM managed_rejections WHERE operation_id=?1",[r.input.id()],|row|row.get(0)).optional()?;
    if shape == Some(false) {
        return Err("managed rejection shape failure".into());
    }
    if shape.is_none() {
        return Ok(None);
    }
    let row: Option<(Vec<u8>, Vec<u8>)> = db
        .query_row(
            "SELECT rejection,state_hash FROM managed_rejections WHERE operation_id=?1",
            [r.input.id()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((bytes, checksum)) = row else {
        return Ok(None);
    };
    if bytes.len() > 32768 {
        return Err("managed rejection too large".into());
    }
    let rejection: Rejection = serde_json::from_slice(&bytes)?;
    if hash(&r.input, &rejection)? != checksum {
        return Err("managed rejection integrity failure".into());
    }
    validate(db, r, &rejection)?;
    Ok(Some(rejection))
}
impl Journal<'_> {
    /// Caller has received this exact authority rejection, or supplies the
    /// immutable local rejected-attempt identity. Unknown outcomes are refused.
    pub fn reject(&mut self, id: &str, rejection: Rejection) -> Result<Record> {
        let r = self.get(id)?;
        validate(&self.state.connection, &r, &rejection)?;
        if let Some(old) = &r.rejection {
            if old != &rejection {
                return Err("retained rejection changed".into());
            }
            return Ok(r);
        }
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rows = tx.execute(
            "INSERT INTO managed_rejections VALUES(?1,?2,?3)",
            params![
                id,
                serde_json::to_vec(&rejection)?,
                hash(&r.input, &rejection)?
            ],
        )?;
        if rows != 1 || load(&tx, &r)?.as_ref() != Some(&rejection) {
            return Err("managed rejection was not durably retained".into());
        }
        tx.commit()?;
        self.get(id)
    }
}
