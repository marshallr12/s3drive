use super::*;

fn hash(owner: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(b"s3drive-client-identity-v1\0");
    h.update(owner.as_bytes());
    h.finalize().to_vec()
}
pub(super) fn read(db: &Connection) -> Result<Option<String>> {
    let identity: Option<(String, Vec<u8>)> = db
        .query_row(
            "SELECT owner_id,state_hash FROM client_identity WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let anchor: Option<Vec<u8>> = db
        .query_row(
            "SELECT state_hash FROM retained_client_identity WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match (identity, anchor) {
        (None, None) => Ok(None),
        (Some((owner, checksum)), Some(anchor))
            if namespace::RemovalFence {
                intent: owner.clone(),
                owner: owner.clone(),
            }
            .valid()
                && checksum == hash(&owner)
                && anchor == checksum =>
        {
            Ok(Some(owner))
        }
        _ => Err("client identity integrity failure; ownership retained".into()),
    }
}
pub(super) fn ensure(state: &mut State) -> Result<String> {
    state.verify_staging_storage()?;
    if let Some(owner) = read(&state.connection)? {
        return Ok(owner);
    }
    // Existing disposition records must never acquire a replacement identity.
    // Loading every retained row also validates the immutable journal checksums.
    super::removals::records(&state.connection)?;
    let opens: i64 =
        state
            .connection
            .query_row("SELECT (SELECT count(*) FROM retained_windows_opens)+(SELECT count(*) FROM windows_mount_sessions)", [], |r| {
                r.get(0)
            })?;
    if opens != 0 {
        return Err("client identity missing while per-open or session history is retained".into());
    }
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| "client identity generation failed")?;
    let owner: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    if owner == namespace::ROOT {
        return Err("client identity generation failed".into());
    }
    let checksum = hash(&owner);
    let tx = state
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let a = tx.execute(
        "INSERT INTO client_identity VALUES(1,?1,?2)",
        params![owner, checksum],
    )?;
    let b = tx.execute(
        "INSERT INTO retained_client_identity VALUES(1,?1)",
        [&checksum],
    )?;
    if a != 1 || b != 1 || read(&tx)?.as_ref() != Some(&owner) {
        return Err("client identity was not durably persisted".into());
    }
    tx.commit()?;
    Ok(owner)
}
