//! Persistable intent to reopen a version. Decoding does not authorize a File.
use super::*;

use s3drive_core::manifest::base::{Descriptor, ENCODED_BYTES};

/// Exact historical version intent. Decode does not authorize a File; use the
/// Reader's authenticated historical lookup and fresh root verification.
#[derive(Clone, PartialEq, Eq)]
pub struct BaseDescriptor(Descriptor);
impl fmt::Debug for BaseDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ManifestBaseDescriptor([untrusted intent])")
    }
}
fn descriptor_error(error: manifest::Error) -> ReadFailure {
    match error {
        manifest::Error::Binding => ReadFailure::Binding,
        _ => ReadFailure::Integrity,
    }
}
impl BaseDescriptor {
    pub fn volume(&self) -> &str {
        self.0.volume()
    }
    pub fn node(&self) -> &str {
        self.0.node()
    }
    pub fn reservation(&self) -> &str {
        self.0.reservation()
    }
    pub fn payload_reference(&self) -> &str {
        self.0.payload_reference()
    }
    pub fn encode(&self) -> [u8; ENCODED_BYTES] {
        self.0.encode()
    }
    pub fn decode(encoded: &[u8]) -> Result<Self> {
        Ok(Self(Descriptor::decode(encoded).map_err(descriptor_error)?))
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
        let reference =
            s3drive_authority::payload_reference(&self.0.payload).ok_or(ReadFailure::Binding)?;
        Ok(BaseDescriptor(
            Descriptor::from_hex(
                &p.reservation.volume,
                &p.reservation.node,
                &p.reservation.id,
                &reference,
            )
            .map_err(descriptor_error)?,
        ))
    }
}
impl Reader {
    /// Reauthorize the exact persisted version, then independently HEAD/GET/hash
    /// its root with current provider credentials. Never substitute live content.
    pub async fn reopen_base(&self, base: &BaseDescriptor) -> Result<BaseFile> {
        if base.volume() != self.0.volume {
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
    pub(super) async fn reopen_inner(
        &self,
        base: &BaseDescriptor,
        snapshot: OwnedSemaphorePermit,
    ) -> Result<BaseFile> {
        let Reply::ManifestPayload(p) = reply(
            self.0
                .authority
                .request(&Request::ManifestPayload {
                    volume: base.volume().to_owned(),
                    node: base.node().to_owned(),
                    reservation: base.reservation().to_owned(),
                    reference: base.payload_reference().to_owned(),
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
            || r.volume != base.volume()
            || r.node != base.node()
            || r.id != base.reservation()
            || &r.store != store
            || s3drive_authority::payload_reference(&Some(p.clone())).as_deref()
                != Some(base.payload_reference())
        {
            return Err(ReadFailure::Binding.into());
        }
        Box::pin(self.open_version(Some(p), snapshot)).await
    }
}
