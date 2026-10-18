//! Strict versioned root metadata. Structure and an authority reservation are
//! not independent proof that an authenticated client uploaded the S3 tree.
use super::*;
use s3drive_core::manifest::{
    self,
    objects::{Namespace, ROOT_BYTES, RootDescriptor},
};

pub const MAX_MANIFEST_LENGTH: u64 = 65_536 * manifest::CHUNK_BYTES;
const FORMAT: &str = "s3dman01";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Encoded")]
pub struct ManifestRoot {
    format: String,
    descriptor: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Encoded {
    format: String,
    descriptor: String,
}
impl TryFrom<Encoded> for ManifestRoot {
    type Error = &'static str;
    fn try_from(value: Encoded) -> std::result::Result<Self, Self::Error> {
        let m = Self {
            format: value.format,
            descriptor: value.descriptor,
        };
        m.root().map_err(|_| "invalid manifest root encoding")?;
        Ok(m)
    }
}
impl std::fmt::Debug for ManifestRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ManifestRoot([private])")
    }
}
fn bytes<const N: usize>(s: &str) -> Result<[u8; N]> {
    if s.len() != N * 2 {
        return Err(Error::Invalid);
    }
    let mut out = [0; N];
    fn nibble(b: u8) -> Result<u8> {
        match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            _ => Err(Error::Invalid),
        }
    }
    for (at, byte) in out.iter_mut().enumerate() {
        *byte = (nibble(s.as_bytes()[at * 2])? << 4) | nibble(s.as_bytes()[at * 2 + 1])?;
    }
    Ok(out)
}
impl ManifestRoot {
    pub fn new(root: RootDescriptor) -> Result<Self> {
        let descriptor = root.encode().iter().map(|b| format!("{b:02x}")).collect();
        let m = Self {
            format: FORMAT.into(),
            descriptor,
        };
        m.root()?;
        Ok(m)
    }
    /// Parsed expectation, not an S3 authorization or upload receipt.
    pub fn root(&self) -> Result<RootDescriptor> {
        if self.format != FORMAT {
            return Err(Error::Invalid);
        }
        let r = RootDescriptor::decode(&bytes::<ROOT_BYTES>(&self.descriptor)?)
            .map_err(|_| Error::Invalid)?;
        if r.length() > MAX_MANIFEST_LENGTH {
            return Err(Error::Invalid);
        }
        let mut pages = r
            .length()
            .div_ceil(manifest::CHUNK_BYTES)
            .div_ceil(128)
            .max(1);
        let mut level = 0u64;
        while pages > 1 {
            pages = pages.div_ceil(128);
            level += 1;
        }
        if r.page().ordinal() != (level + 1) << 56 {
            return Err(Error::Invalid);
        }
        Ok(r)
    }
    pub(in crate::namespace) fn context(
        &self,
        volume: &str,
        node: &str,
        store: &str,
        generation: &str,
        size: u64,
    ) -> bool {
        let Ok(r) = self.root() else {
            return false;
        };
        (64..=manifest::MAX_PAGE_BYTES as u64).contains(&size)
            && bytes::<16>(volume).is_ok_and(|id| r.volume() == &id)
            && bytes::<16>(node).is_ok_and(|id| r.node() == &id)
            && bytes::<16>(store).is_ok_and(|id| r.page().store() == &id)
            && bytes::<16>(generation).is_ok_and(|id| r.page().generation() == &id)
    }
    pub(super) fn key(&self, r: &Reservation) -> Result<String> {
        if !self.context(&r.volume, &r.node, &r.store.id, &r.id, r.size) {
            return Err(Error::Invalid);
        }
        let root = self.root()?;
        let ns = Namespace::new(
            r.store.root.clone(),
            *root.volume(),
            *root.node(),
            *root.page().store(),
        )
        .map_err(|_| Error::Invalid)?;
        Ok(ns
            .root_key(&root)
            .map_err(|_| Error::Invalid)?
            .as_str()
            .into())
    }
}
impl Reservation {
    /// Physical page bytes remain in size/sha256; only this accessor gives the
    /// logical file length. Invalid metadata never supplies a projected size.
    pub fn logical_size(&self) -> Result<u64> {
        if !self.valid() {
            return Err(Error::Invalid);
        }
        self.manifest
            .as_ref()
            .map_or(Ok(self.size), |m| m.root().map(|r| r.length()))
    }
}
#[cfg(test)]
mod tests;
