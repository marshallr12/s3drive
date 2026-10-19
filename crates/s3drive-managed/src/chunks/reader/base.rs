//! Persistable intent to reopen a version. Decoding does not authorize a File.
use super::*;

const MAGIC: &[u8; 8] = b"S3DBASE1";
const LENGTH: usize = 88;

/// Exact historical version identity, without credentials, S3 keys or cached
/// namespace attributes. Its reference is the original edit CAS expected value,
/// not the predecessor guard stored in the payload's reservation.
#[derive(Clone, PartialEq, Eq)]
pub struct BaseDescriptor {
    volume: String,
    node: String,
    reservation: String,
    reference: String,
}
impl fmt::Debug for BaseDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ManifestBaseDescriptor([untrusted intent])")
    }
}
impl BaseDescriptor {
    pub fn volume(&self) -> &str {
        &self.volume
    }
    pub fn node(&self) -> &str {
        &self.node
    }
    pub fn reservation(&self) -> &str {
        &self.reservation
    }
    pub fn payload_reference(&self) -> &str {
        &self.reference
    }
    pub fn encode(&self) -> [u8; LENGTH] {
        let mut out = [0; LENGTH];
        out[..8].copy_from_slice(MAGIC);
        let mut at = 8;
        for field in [&self.volume, &self.node, &self.reservation, &self.reference] {
            for pair in field.as_bytes().as_chunks::<2>().0 {
                // Private constructors establish canonical lowercase hex.
                let nibble = |b: u8| {
                    if b.is_ascii_digit() {
                        b - b'0'
                    } else {
                        b - b'a' + 10
                    }
                };
                out[at] = (nibble(pair[0]) << 4) | nibble(pair[1]);
                at += 1;
            }
        }
        out
    }
    pub fn decode(encoded: &[u8]) -> Result<Self> {
        if encoded.len() != LENGTH || &encoded[..8] != MAGIC {
            return Err(ReadFailure::Integrity.into());
        }
        let value = Self {
            volume: super::super::hex(&encoded[8..24]),
            node: super::super::hex(&encoded[24..40]),
            reservation: super::super::hex(&encoded[40..56]),
            reference: super::super::hex(&encoded[56..88]),
        };
        if [&value.volume, &value.node, &value.reservation]
            .iter()
            .any(|id| id.as_str() == s3drive_authority::ROOT)
        {
            return Err(ReadFailure::Binding.into());
        }
        Ok(value)
    }
}
impl BaseFile {
    /// Export requires the historical-read capability so a future dirty view
    /// cannot promise restart recovery against an older authority by accident.
    pub fn base_descriptor(&self) -> Result<BaseDescriptor> {
        if !self.0.reader.0.history {
            return Err(ReadFailure::UnsupportedContent.into());
        }
        let p = self
            .0
            .payload
            .as_ref()
            .filter(|p| p.reservation.manifest.is_some())
            .ok_or(ReadFailure::UnsupportedContent)?;
        Ok(BaseDescriptor {
            volume: p.reservation.volume.clone(),
            node: p.reservation.node.clone(),
            reservation: p.reservation.id.clone(),
            reference: s3drive_authority::payload_reference(&self.0.payload)
                .ok_or(ReadFailure::Binding)?,
        })
    }
}
impl Reader {
    /// Reauthorize the exact persisted version, then independently HEAD/GET/hash
    /// its root with current provider credentials. Never substitute live content.
    pub async fn reopen_base(&self, base: &BaseDescriptor) -> Result<BaseFile> {
        if base.volume != self.0.volume {
            return Err(ReadFailure::Binding.into());
        }
        if !self.0.history {
            return Err(ReadFailure::UnsupportedContent.into());
        }
        let _active = self
            .0
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        let snapshot = self
            .0
            .snapshots
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        bounded(self.0.options, Box::pin(self.reopen_inner(base, snapshot))).await
    }
    async fn reopen_inner(
        &self,
        base: &BaseDescriptor,
        snapshot: OwnedSemaphorePermit,
    ) -> Result<BaseFile> {
        let Reply::ManifestPayload(p) = reply(
            self.0
                .authority
                .request(&Request::ManifestPayload {
                    volume: base.volume.clone(),
                    node: base.node.clone(),
                    reservation: base.reservation.clone(),
                    reference: base.reference.clone(),
                })
                .await?,
        )?
        else {
            return Err(ReadFailure::Protocol.into());
        };
        let r = &p.reservation;
        let (store, _) = self.0.stores.get(&r.store.id).ok_or(ReadFailure::Binding)?;
        if !p.valid()
            || r.manifest.is_none()
            || r.volume != base.volume
            || r.node != base.node
            || r.id != base.reservation
            || &r.store != store
            || s3drive_authority::payload_reference(&Some(p.clone())).as_deref()
                != Some(base.reference.as_str())
        {
            return Err(ReadFailure::Binding.into());
        }
        Box::pin(self.open_version(Some(p), snapshot)).await
    }
}
