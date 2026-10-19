//! Offline object naming and compact root descriptors. Structural validation
//! is not authority authentication, route admission or permission to adopt an
//! object. No wire capability, publication or migration is implemented here.
use super::{Cursor, Error, MAX_LENGTH, Page, Reference, Result, VerifiedPage};
use std::fmt;

const ROOT_MAGIC: &[u8; 8] = b"S3DROOT1";
pub const ROOT_BYTES: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Data,
    Page,
}

/// A structurally valid mapping supplied by the caller. The future publisher
/// must authenticate the store mapping and match its route/policy separately.
#[derive(Clone, PartialEq, Eq)]
pub struct Namespace {
    root: String,
    volume: [u8; 16],
    node: [u8; 16],
    store: [u8; 16],
}
impl fmt::Debug for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ManifestNamespace([private])")
    }
}

/// A deterministic key, not an object-ownership or create-only write permit.
#[derive(Clone, PartialEq, Eq)]
pub struct Key(String);
impl Key {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ManifestObjectKey([private])")
    }
}

fn nonzero(id: &[u8; 16]) -> bool {
    id != &[0; 16]
}
fn hex(id: &[u8; 16]) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}
impl Namespace {
    /// Roots follow the existing managed store's bounded ASCII component rules.
    /// They are object-key prefixes; do not URL-encode or path-normalize them.
    pub fn new(root: String, volume: [u8; 16], node: [u8; 16], store: [u8; 16]) -> Result<Self> {
        if !nonzero(&volume) || !nonzero(&node) || !nonzero(&store) {
            return Err(Error::Binding);
        }
        if root.is_empty()
            || root.len() > 128
            || !root.ends_with('/')
            || !root[..root.len() - 1].split('/').all(|component| {
                !component.is_empty()
                    && component != "."
                    && component != ".."
                    && component
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
        {
            return Err(Error::Shape);
        }
        Ok(Self {
            root,
            volume,
            node,
            store,
        })
    }

    /// Derive within this file's namespace. A Reference does not itself prove
    /// file provenance; reused references still need verified ancestry and
    /// retention in the future journal. Digests deliberately do not name keys:
    /// different bytes for the same identity must conflict, not pick a new key.
    pub fn key(&self, role: Role, object: &Reference) -> Result<Key> {
        if object.store() != &self.store {
            return Err(Error::Binding);
        }
        let role = match role {
            Role::Data => "data",
            Role::Page => "page",
        };
        Ok(Key(format!(
            "{}v2/{}/{}/{}/{}/{}/{:016x}",
            self.root,
            hex(&self.volume),
            hex(&self.node),
            hex(&self.store),
            hex(object.generation()),
            role,
            object.ordinal(),
        )))
    }

    pub fn root_key(&self, root: &RootDescriptor) -> Result<Key> {
        if root.volume != self.volume || root.node != self.node {
            return Err(Error::Binding);
        }
        self.key(Role::Page, &root.page)
    }
}

/// Parsed metadata only. Authenticate the descriptor through the future
/// authority capability before treating its hash/identity as a trusted root.
/// This prototype's magic selects S3DMAN01 pages, not legacy payload SHA fields.
#[derive(Clone, PartialEq, Eq)]
pub struct RootDescriptor {
    volume: [u8; 16],
    node: [u8; 16],
    length: u64,
    page: Reference,
}
impl fmt::Debug for RootDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ManifestRootDescriptor([private])")
    }
}
impl RootDescriptor {
    pub fn new(volume: [u8; 16], node: [u8; 16], length: u64, page: Reference) -> Result<Self> {
        if !nonzero(&volume) || !nonzero(&node) {
            return Err(Error::Binding);
        }
        if length > MAX_LENGTH {
            return Err(Error::Bounds);
        }
        Ok(Self {
            volume,
            node,
            length,
            page,
        })
    }
    /// Canonical zero-length root intent and its real page body. No upload,
    /// provider authorization or namespace publication follows from this helper.
    pub fn empty(
        volume: [u8; 16],
        node: [u8; 16],
        store: [u8; 16],
        generation: [u8; 16],
    ) -> Result<(Self, Vec<u8>)> {
        let page = Page::new(volume, node, 0, 0, Vec::new())?;
        let root = Self::new(
            volume,
            node,
            0,
            Reference::new(store, generation, 1 << 56, page.digest())?,
        )?;
        Ok((root, page.encode()))
    }
    pub fn volume(&self) -> &[u8; 16] {
        &self.volume
    }
    pub fn node(&self) -> &[u8; 16] {
        &self.node
    }
    pub fn length(&self) -> u64 {
        self.length
    }
    pub fn page(&self) -> &Reference {
        &self.page
    }
    pub fn encode(&self) -> [u8; ROOT_BYTES] {
        let mut out = [0; ROOT_BYTES];
        out[..8].copy_from_slice(ROOT_MAGIC);
        out[8..24].copy_from_slice(&self.volume);
        out[24..40].copy_from_slice(&self.node);
        out[40..48].copy_from_slice(&self.length.to_le_bytes());
        out[48..64].copy_from_slice(self.page.store());
        out[64..80].copy_from_slice(self.page.generation());
        out[80..88].copy_from_slice(&self.page.ordinal().to_le_bytes());
        out[88..].copy_from_slice(self.page.sha256());
        out
    }
    /// Exact fixed width; no allocation, trailing bytes or version fallback.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != ROOT_BYTES {
            return Err(Error::Bounds);
        }
        let mut c = Cursor { bytes, at: 0 };
        if &c.array::<8>()? != ROOT_MAGIC {
            return Err(Error::Encoding);
        }
        let volume = c.array()?;
        let node = c.array()?;
        let length = u64::from_le_bytes(c.array()?);
        let page = Reference::new(
            c.array()?,
            c.array()?,
            u64::from_le_bytes(c.array()?),
            c.array()?,
        )?;
        Self::new(volume, node, length, page)
    }

    /// Checks bytes against this descriptor only. It does NOT authenticate the
    /// descriptor, prove S3 authorization, or establish publication durability.
    pub fn verify_page(&self, bytes: &[u8]) -> Result<VerifiedPage> {
        Page::verify_root(
            bytes,
            self.page.sha256(),
            &self.volume,
            &self.node,
            self.length,
        )
    }
}

#[cfg(test)]
mod tests;
