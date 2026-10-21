//! Atomic metadata creation backed by the canonical empty-manifest identity.
//! S3 upload fields remain a caller claim; this service has no provider client.
use super::*;
use s3drive_core::manifest::objects::RootDescriptor;
fn is_empty(request: &Apply) -> bool {
    matches!(
        request.mutation,
        Mutation::CreateEmptyManifest { .. } | Mutation::CreateDeferredEmptyManifest { .. }
    )
}
pub(in crate::namespace) fn valid_proof(
    operation: &str,
    etag: &str,
    version: Option<&str>,
) -> bool {
    super::valid_proof(operation, etag, version)
}
pub(in crate::namespace) fn preflight(db: &Connection, request: &Apply) -> Result<()> {
    if !is_empty(request) {
        return Ok(());
    }
    let version: i32 = sql(db.pragma_query_value(None, "user_version", |r| r.get(0)))?;
    let fk: bool = sql(db.pragma_query_value(None, "foreign_keys", |r| r.get(0)))?;
    let trigger: bool = sql(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger')",
        [],
        |r| r.get(0),
    ))?;
    if version < 39 || !fk || trigger {
        return Err(Error::Internal);
    }
    Ok(())
}
/// Old opaque records stay byte-identical; a decodable new operation cannot be
/// laundered through a pre39 version marker. Bounded rows avoid huge allocation.
pub(crate) fn require_pre39(db: &Connection) -> crate::Result<()> {
    let mut q=db.prepare("SELECT CASE WHEN length(request) BETWEEN 1 AND 16384 THEN request END FROM namespace_operations")?;
    let mut rows = q.query([])?;
    while let Some(row) = rows.next()? {
        if let Some(raw) = row.get::<_, Option<Vec<u8>>>(0)?
            && let Ok(request) = serde_json::from_slice::<Apply>(&raw)
            && is_empty(&request)
        {
            return Err("atomic empty-manifest records require schema39".into());
        }
    }
    Ok(())
}
fn canonical(request: &Apply, store: Store, sequence: u64) -> Result<Payload> {
    let Mutation::CreateEmptyManifest {
        upload_operation,
        etag,
        version,
        ..
    } = &request.mutation
    else {
        return Err(Error::Invalid);
    };
    let raw_id = |s: &str| -> Result<[u8; 16]> {
        if !valid_id(s) {
            return Err(Error::Invalid);
        }
        let mut b = [0; 16];
        for (i, byte) in b.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|_| Error::Invalid)?;
        }
        Ok(b)
    };
    let op = raw_id(&request.operation)?;
    let (root, body) = RootDescriptor::empty(raw_id(&request.volume)?, op, raw_id(&store.id)?, op)
        .map_err(|_| Error::Invalid)?;
    let manifest = ManifestRoot::new(root)?;
    let mut reservation = Reservation {
        id: request.operation.clone(),
        volume: request.volume.clone(),
        node: request.operation.clone(),
        expected_revision: sequence,
        store,
        key: String::new(),
        size: body.len() as u64,
        sha256: Sha256::digest(&body)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        guard: Some(ContentGuard {
            expected: None,
            detached: false,
            removal: None,
        }),
        manifest: Some(manifest),
    };
    reservation.key = reservation
        .manifest
        .as_ref()
        .ok_or(Error::Internal)?
        .key(&reservation)?;
    let p = Payload {
        reservation,
        upload_operation: upload_operation.clone(),
        etag: etag.clone(),
        version: version.clone(),
    };
    if !p.valid() {
        return Err(Error::Invalid);
    }
    Ok(p)
}
pub(super) fn mutate(
    db: &Connection,
    request: &Apply,
    sequence: u64,
) -> Result<(String, Artifact)> {
    preflight(db, request)?;
    let Mutation::CreateEmptyManifest {
        parent,
        parent_revision,
        name,
        store: store_id,
        ..
    } = &request.mutation
    else {
        return Err(Error::Invalid);
    };
    if volume(db)?.name_policy != names::PORTABLE {
        return Err(Error::Invalid);
    }
    if directory(db, parent)?.revision != *parent_revision {
        return Err(Error::Conflict);
    }
    let uploads: i64 =
        sql(db.query_row("SELECT count(*) FROM namespace_uploads", [], |r| r.get(0)))?;
    if uploads >= 4096 {
        return Err(Error::Limit);
    }
    let p = canonical(request, store(db, store_id)?, sequence)?;
    let collision:bool=sql(db.query_row("SELECT EXISTS(SELECT 1 FROM namespace_uploads WHERE reservation_id=?1) OR EXISTS(SELECT 1 FROM namespace_nodes WHERE node_id=?1)",[&request.operation],|r|r.get(0)))?;
    if collision {
        return Err(Error::Internal);
    }
    create(
        db,
        request,
        parent,
        name,
        Kind::EmptyFile,
        Some(p.reservation.store.clone()),
        sequence as i64,
    )?;
    checkpoint(request, "NodeCreated")?;
    let changed=sql(db.execute("UPDATE namespace_nodes SET payload=?1 WHERE node_id=?2 AND kind='empty-file' AND payload IS NULL",params![encode(&p)?,request.operation]))?;
    if changed != 1 {
        return Err(Error::Internal);
    }
    checkpoint(request, "PayloadStored")?;
    if sql(db.execute(
        "INSERT INTO namespace_uploads VALUES(?1,?2,?3)",
        params![p.reservation.id, encode(&p.reservation)?, encode(&p)?],
    ))? != 1
    {
        return Err(Error::Internal);
    }
    checkpoint(request, "HistoryStored")?;
    Ok((request.operation.clone(), Artifact::Published(p)))
}
pub(super) fn replay(db: &Connection, request: &Apply, receipt: &Receipt) -> Result<()> {
    preflight(db, request)?;
    let Mutation::CreateEmptyManifest {
        parent,
        name,
        store: store_id,
        ..
    } = &request.mutation
    else {
        return Err(Error::Internal);
    };
    if receipt.sequence <= request.expected_sequence {
        return Err(Error::Internal);
    }
    let expected =
        canonical(request, store(db, store_id)?, receipt.sequence).map_err(|_| Error::Internal)?;
    let node = Node {
        id: request.operation.clone(),
        parent: Some(parent.clone()),
        name: name.clone(),
        kind: Kind::File,
        revision: receipt.sequence,
        deleted: false,
        payload: Some(expected.clone()),
        placement: Some(expected.reservation.store.clone()),
    };
    if receipt.node != node
        || receipt.artifact != Some(Artifact::Published(expected.clone()))
        || reservation(db, &request.operation)? != expected.reservation
    {
        return Err(Error::Internal);
    }
    let raw: Option<Vec<u8>> = sql(db.query_row(
        "SELECT publication FROM namespace_uploads WHERE reservation_id=?1",
        [&request.operation],
        |r| r.get(0),
    ))?;
    if raw.as_deref() != Some(encode(&expected)?.as_slice()) {
        return Err(Error::Internal);
    }
    Ok(())
}
pub(in crate::namespace) fn checkpoint(_request: &Apply, _point: &str) -> Result<()> {
    #[cfg(test)]
    if is_empty(_request) && std::env::var("S3DRIVE_EMPTY_CREATE_POINT").as_deref() == Ok(_point) {
        use std::io::Read;
        if let Some(ready) = std::env::var_os("S3DRIVE_EMPTY_CREATE_READY") {
            let tmp = std::path::PathBuf::from(&ready).with_extension("tmp");
            std::fs::write(&tmp, b"ready").map_err(|_| Error::Internal)?;
            std::fs::rename(tmp, ready).map_err(|_| Error::Internal)?;
            let _ = std::io::stdin().read(&mut [0u8]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
