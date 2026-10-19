//! Historical metadata, never a new content guard or an upload receipt.
use super::*;

const MAX_RECORD_BYTES: i64 = 16 * 1024;
/// Selectors are fixed internal identifiers, never caller input. CASE prevents
/// an oversized SQLite value from being copied into a Rust Vec before refusal.
fn blob(
    db: &Connection,
    table: &str,
    column: &str,
    key: &str,
    id: &str,
) -> Result<Option<Vec<u8>>> {
    let query = format!(
        "SELECT CASE WHEN length({column}) BETWEEN 1 AND {MAX_RECORD_BYTES} THEN {column} END,
        length({column}) FROM {table} WHERE {key}=?1"
    );
    let (value, length): (Option<Vec<u8>>, Option<i64>) = sql(db
        .query_row(&query, [id], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional())?
    .ok_or(Error::NotFound)?;
    match (value, length) {
        (None, None) => Ok(None),
        (Some(bytes), Some(n)) if (1..=MAX_RECORD_BYTES).contains(&n) => Ok(Some(bytes)),
        _ => Err(Error::Internal),
    }
}

impl State {
    /// Resolve exactly one previously published manifest by its original
    /// content reference. Never consult or substitute the live node payload.
    pub fn namespace_manifest_payload(
        &mut self,
        volume_id: &str,
        node_id: &str,
        reservation_id: &str,
        expected_reference: &str,
    ) -> Result<Payload> {
        if !valid_id(node_id)
            || node_id == ROOT
            || !valid_id(reservation_id)
            || reservation_id == ROOT
            || !digest(expected_reference)
        {
            return Err(Error::Invalid);
        }
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred))?;
        let v = check_volume(&tx, volume_id)?;
        let r: Reservation = decode(
            blob(
                &tx,
                "namespace_uploads",
                "descriptor",
                "reservation_id",
                reservation_id,
            )?
            .ok_or(Error::Internal)?,
        )?;
        if !r.valid() || r.id != reservation_id || r.volume != volume_id {
            return Err(Error::Internal);
        }
        if r.node != node_id {
            return Err(Error::NotFound);
        }
        if r.manifest.is_none() {
            return Err(Error::Invalid);
        }
        let mapped: Store = decode(
            blob(
                &tx,
                "namespace_stores",
                "descriptor",
                "store_id",
                &r.store.id,
            )
            .map_err(|_| Error::Internal)?
            .ok_or(Error::Internal)?,
        )?;
        if !mapped.valid() || mapped != r.store {
            return Err(Error::Internal);
        }
        let encoded = blob(
            &tx,
            "namespace_operations",
            "request",
            "operation_id",
            reservation_id,
        )
        .map_err(|_| Error::Internal)?
        .ok_or(Error::Internal)?;
        let outcome = blob(
            &tx,
            "namespace_operations",
            "outcome",
            "operation_id",
            reservation_id,
        )
        .map_err(|_| Error::Internal)?
        .ok_or(Error::Internal)?;
        let original: Apply = decode(encoded)?;
        original.validate().map_err(|_| Error::Internal)?;
        if !matches!(
            original.mutation,
            Mutation::ReserveManifest { .. } | Mutation::CreateEmptyManifest { .. }
        ) || original.volume != volume_id
            || original.operation != reservation_id
        {
            return Err(Error::Internal);
        }
        let receipt: Result<Receipt> = decode(outcome)?;
        let receipt = receipt.map_err(|_| Error::Internal)?;
        validate_manifest_replay(&tx, &original, &receipt)?;
        if receipt.sequence > v.sequence
            || receipt.node.id != node_id
            || !match &receipt.artifact {
                Some(Artifact::Reservation(saved)) => {
                    saved == &r && matches!(original.mutation, Mutation::ReserveManifest { .. })
                }
                Some(Artifact::Published(p)) => {
                    p.reservation == r
                        && matches!(original.mutation, Mutation::CreateEmptyManifest { .. })
                }
                _ => false,
            }
        {
            return Err(Error::Internal);
        }
        let raw = blob(
            &tx,
            "namespace_uploads",
            "publication",
            "reservation_id",
            reservation_id,
        )?;
        let p: Payload = decode(raw.ok_or(Error::NotFound)?)?;
        if !p.valid() || p.reservation != r {
            return Err(Error::Internal);
        }
        if payload_reference(&Some(p.clone())).as_deref() != Some(expected_reference) {
            return Err(Error::Conflict);
        }
        sql(tx.commit())?;
        Ok(p)
    }
}
