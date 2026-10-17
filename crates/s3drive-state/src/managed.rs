//! Client-local managed publication intent. A physically verified blob is not a
//! logical file flush until metadata publication and this receipt are committed.
use crate::{
    Result, State,
    journal::{self, GenerationId, Target},
    namespace::{Apply, Artifact, Kind, Mutation, Payload, Receipt, Reservation, Store},
    remote,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub reserve: Apply,
    pub publish_operation: String,
    pub store: Store,
    pub source: [u8; 32],
    pub fingerprint: String,
}
impl std::fmt::Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ManagedInput([private local binding])")
    }
}
impl Input {
    pub fn id(&self) -> &str {
        &self.reserve.operation
    }
    pub fn reservation(&self) -> Result<Reservation> {
        self.reserve
            .validate()
            .map_err(|_| "invalid managed reservation")?;
        let (node, expected_revision, store, size, sha256, guard) = match &self.reserve.mutation {
            Mutation::Reserve {
                node,
                expected_revision,
                store,
                size,
                sha256,
            } => (node, expected_revision, store, size, sha256, None),
            Mutation::ReserveContent {
                node,
                expected_revision,
                store,
                size,
                sha256,
                expected_payload,
            }
            | Mutation::ReserveDetachedContent {
                node,
                expected_revision,
                store,
                size,
                sha256,
                expected_payload,
            } => (
                node,
                expected_revision,
                store,
                size,
                sha256,
                Some(crate::namespace::ContentGuard {
                    expected: expected_payload.clone(),
                    detached: matches!(
                        self.reserve.mutation,
                        Mutation::ReserveDetachedContent { .. }
                    ),
                }),
            ),
            _ => return Err("managed input must reserve a blob".into()),
        };
        let r = Reservation {
            id: self.id().into(),
            volume: self.reserve.volume.clone(),
            node: node.clone(),
            expected_revision: *expected_revision,
            store: self.store.clone(),
            key: format!(
                "{}v1/{}/{}/{}",
                self.store.root,
                self.reserve.volume,
                node,
                self.id()
            ),
            size: *size,
            sha256: sha256.clone(),
            guard,
        };
        let probe = Apply {
            volume: r.volume.clone(),
            operation: self.publish_operation.clone(),
            expected_sequence: 0,
            mutation: Mutation::Remove { node: node.clone() },
        };
        probe
            .validate()
            .map_err(|_| "invalid publication operation")?;
        if !r.valid() || store != &self.store.id || self.publish_operation == self.id() {
            return Err("managed reservation binding mismatch".into());
        }
        Ok(r)
    }
    pub fn target(&self) -> Result<Target> {
        let r = self.reservation()?;
        Target::new(self.source, &self.fingerprint, &r.key)
    }
    pub fn stage_key(&self) -> Result<[u8; 32]> {
        let mut h = Sha256::new();
        h.update(b"s3drive-managed-stage-v1\0");
        h.update(serde_json::to_vec(self)?);
        Ok(h.finalize().into())
    }
}
#[derive(Debug, Clone)]
pub struct Record {
    pub input: Input,
    pub generation: Option<GenerationId>,
    pub reservation: Option<Reservation>,
    pub publication: Option<Apply>,
    pub receipt: Option<Receipt>,
}
fn hash(record: &Record) -> Result<Vec<u8>> {
    let mut h = Sha256::new();
    h.update(b"s3drive-managed-record-v1\0");
    for bytes in [
        serde_json::to_vec(&record.input)?,
        serde_json::to_vec(&record.generation.map(|g| g.value()))?,
        serde_json::to_vec(&record.reservation)?,
        serde_json::to_vec(&record.publication)?,
        serde_json::to_vec(&record.receipt)?,
    ] {
        h.update((bytes.len() as u64).to_be_bytes());
        h.update(bytes);
    }
    Ok(h.finalize().to_vec())
}
fn decode<T: for<'de> Deserialize<'de>>(bytes: Vec<u8>) -> Result<T> {
    Ok(serde_json::from_slice(&bytes)?)
}
fn encoded<T: Serialize>(value: &Option<T>) -> Result<Option<Vec<u8>>> {
    value
        .as_ref()
        .map(serde_json::to_vec)
        .transpose()
        .map_err(Into::into)
}
pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT operation_id,input,generation_id,reservation,publication_request,receipt,state_hash FROM managed_publications LIMIT 0")?;
    Ok(())
}
fn load(db: &Connection, id: &str) -> Result<Option<Record>> {
    let mut stmt=db.prepare("SELECT input,generation_id,reservation,publication_request,receipt,state_hash FROM managed_publications WHERE operation_id=?")?;
    let row = stmt
        .query_row([id], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<Vec<u8>>>(2)?,
                r.get::<_, Option<Vec<u8>>>(3)?,
                r.get::<_, Option<Vec<u8>>>(4)?,
                r.get::<_, Vec<u8>>(5)?,
            ))
        })
        .optional()?;
    let Some((input, generation, reservation, publication, receipt, checksum)) = row else {
        return Ok(None);
    };
    let r = Record {
        input: decode(input)?,
        generation: generation.map(GenerationId::from_database).transpose()?,
        reservation: reservation.map(decode).transpose()?,
        publication: publication.map(decode).transpose()?,
        receipt: receipt.map(decode).transpose()?,
    };
    if r.input.id() != id || hash(&r)? != checksum {
        return Err("managed publication record integrity failure".into());
    }
    validate(&r)?;
    Ok(Some(r))
}
fn validate(r: &Record) -> Result<()> {
    r.input.target()?;
    if r.reservation
        .as_ref()
        .is_some_and(|v| r.input.reservation().ok().as_ref() != Some(v))
        || (r.reservation.is_some() && r.generation.is_none())
    {
        return Err("managed reservation was rebound".into());
    }
    if let Some(p) = &r.publication {
        p.validate().map_err(|_| "invalid managed publication")?;
        if p.volume != r.input.reserve.volume
            || p.operation != r.input.publish_operation
            || r.reservation.is_none()
        {
            return Err("managed publication binding mismatch".into());
        }
        let Mutation::Publish { reservation, .. } = &p.mutation else {
            return Err("invalid managed publication kind".into());
        };
        if reservation != r.input.id() {
            return Err("managed publication reservation mismatch".into());
        }
    }
    if let Some(receipt) = &r.receipt {
        check_receipt(r, receipt)?;
    }
    Ok(())
}
fn check_receipt(r: &Record, receipt: &Receipt) -> Result<()> {
    let p = r
        .publication
        .as_ref()
        .ok_or("publication was never prepared")?;
    let Mutation::Publish {
        upload_operation,
        etag,
        version,
        ..
    } = &p.mutation
    else {
        return Err("wrong publication kind".into());
    };
    let payload = Payload {
        reservation: r.reservation.clone().ok_or("missing reservation")?,
        upload_operation: upload_operation.clone(),
        etag: etag.clone(),
        version: version.clone(),
    };
    let content = r.input.reservation()?.guard.is_some();
    if (if content {
        receipt.sequence <= p.expected_sequence
    } else {
        receipt.sequence != p.expected_sequence + 1
    }) || receipt.node.revision != receipt.sequence
        || receipt.node.id != payload.reservation.node
        || receipt.node.deleted
            != r.input
                .reservation()?
                .guard
                .as_ref()
                .is_some_and(|g| g.detached)
        || receipt.node.kind != Kind::File
        || receipt.node.payload.as_ref() != Some(&payload)
        || receipt.artifact != Some(Artifact::Published(payload))
    {
        return Err("metadata receipt does not cover this managed file generation".into());
    }
    Ok(())
}
fn save(db: &Connection, r: &Record) -> Result<()> {
    validate(r)?;
    let changed = db.execute("UPDATE managed_publications SET generation_id=?1,reservation=?2,publication_request=?3,receipt=?4,state_hash=?5 WHERE operation_id=?6",
      params![r.generation.map(|g|g.value()),encoded(&r.reservation)?,encoded(&r.publication)?,encoded(&r.receipt)?,hash(r)?,r.input.id()])?;
    if changed != 1 {
        return Err("managed publication was not durably updated".into());
    }
    Ok(())
}
pub struct Journal<'a> {
    state: &'a mut State,
    options: journal::Options,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State, options: journal::Options) -> Result<Self> {
        state.verify_staging_storage()?;
        Ok(Self {
            state,
            options: options.validate()?,
        })
    }
    pub fn lookup(&self, id: &str) -> Result<Option<Record>> {
        load(&self.state.connection, id)
    }
    pub fn get(&self, id: &str) -> Result<Record> {
        load(&self.state.connection, id)?.ok_or_else(|| "unknown managed publication".into())
    }
    pub fn begin(&mut self, input: Input) -> Result<Record> {
        input.target()?;
        if let Some(r) = load(&self.state.connection, input.id())? {
            if r.input != input {
                return Err("managed operation input changed".into());
            }
            return Ok(r);
        }
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: i64 = tx.query_row("SELECT count(*) FROM managed_publications", [], |r| {
            r.get(0)
        })?;
        if count >= 4096 {
            return Err("managed publication record limit reached".into());
        }
        let r = Record {
            input,
            generation: None,
            reservation: None,
            publication: None,
            receipt: None,
        };
        tx.execute(
            "INSERT INTO managed_publications VALUES(?1,?2,NULL,NULL,NULL,NULL,?3)",
            params![r.input.id(), serde_json::to_vec(&r.input)?, hash(&r)?],
        )?;
        tx.commit()?;
        Ok(r)
    }
    pub fn link(&mut self, id: &str, generation: GenerationId) -> Result<Record> {
        let mut r = self.get(id)?;
        if let Some(old) = r.generation {
            if old != generation {
                return Err("managed generation changed".into());
            }
            return Ok(r);
        }
        let expected = r.input.reservation()?;
        let journal = journal::Journal::new(self.state, self.options)?;
        let g = journal.info(generation)?;
        if g.phase != journal::Phase::Sealed
            || g.target != r.input.target()?
            || g.parent.is_some()
            || g.base != journal::Base::Missing
            || g.size != expected.size
            || hex(&Sha256::digest(journal.read(
                generation,
                0,
                g.size as usize,
            )?)) != expected.sha256
        {
            return Err("managed blob must be a complete create-only immutable root".into());
        }
        r.generation = Some(generation);
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        save(&tx, &r)?;
        tx.commit()?;
        Ok(r)
    }
    pub fn recover_stage(&mut self, id: &str) -> Result<Record> {
        let r = self.get(id)?;
        if r.generation.is_some() {
            return Ok(r);
        }
        let generation: Option<i64> = self
            .state
            .connection
            .query_row(
                "SELECT generation_id FROM staged_operations WHERE operation_key=?",
                [r.input.stage_key()?.as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(g) = generation {
            let g = GenerationId::from_database(g)?;
            if journal::Journal::new(self.state, self.options)?
                .info(g)?
                .phase
                == journal::Phase::Sealed
            {
                return self.link(id, g);
            }
        }
        Ok(r)
    }
    pub fn reserved(&mut self, id: &str, reservation: Reservation) -> Result<Record> {
        let mut r = self.get(id)?;
        if r.generation.is_none() || reservation != r.input.reservation()? {
            return Err("authority reservation differs from local immutable input".into());
        }
        if r.reservation.is_some() {
            return Ok(r);
        }
        r.reservation = Some(reservation);
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        save(&tx, &r)?;
        tx.commit()?;
        Ok(r)
    }
    pub fn prepare(&mut self, id: &str, sequence: u64) -> Result<Record> {
        let mut r = self.get(id)?;
        if r.publication.is_some() {
            return Ok(r);
        }
        let generation = r.generation.ok_or("managed bytes are not sealed")?;
        let reservation = r.reservation.as_ref().ok_or("reservation not confirmed")?;
        let receipt = remote::Journal::new(self.state, remote::Options::default())?
            .flush_receipt(generation)?
            .ok_or("blob verification not acknowledged")?;
        let info = receipt.info();
        let revision = info.revision.as_ref().ok_or("missing verified revision")?;
        if info.guard != remote::Guard::Missing
            || info.target != r.input.target()?
            || info.size != reservation.size
            || hex(&info.payload_sha256) != reservation.sha256
        {
            return Err("verified blob differs from managed reservation".into());
        }
        r.publication = Some(Apply {
            volume: r.input.reserve.volume.clone(),
            operation: r.input.publish_operation.clone(),
            expected_sequence: sequence,
            mutation: Mutation::Publish {
                reservation: id.into(),
                upload_operation: hex(info.operation.as_bytes()),
                etag: revision.etag().into(),
                version: revision.version().map(Into::into),
            },
        });
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        save(&tx, &r)?;
        tx.commit()?;
        Ok(r)
    }
    pub fn acknowledge(&mut self, id: &str, receipt: Receipt) -> Result<Record> {
        let mut r = self.get(id)?;
        check_receipt(&r, &receipt)?;
        if r.receipt.as_ref().is_some_and(|old| old != &receipt) {
            return Err("managed receipt changed".into());
        }
        r.receipt = Some(receipt);
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        save(&tx, &r)?;
        tx.commit()?;
        Ok(r)
    }
    pub fn pending(&self, after: Option<&str>, limit: u16) -> Result<Vec<Record>> {
        if !(1..=1000).contains(&limit) {
            return Err("invalid managed pending limit".into());
        }
        let mut q = self.state.connection.prepare(
            "SELECT operation_id FROM managed_publications ORDER BY operation_id LIMIT 4097",
        )?;
        let ids = q
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.len() > 4096 {
            return Err("managed record limit or integrity failure".into());
        }
        let mut pending = Vec::new();
        for id in ids {
            let r = self.get(&id)?;
            if after.is_none_or(|a| id.as_str() > a)
                && r.receipt.is_none()
                && pending.len() < usize::from(limit)
            {
                pending.push(r);
            }
        }
        Ok(pending)
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests;
