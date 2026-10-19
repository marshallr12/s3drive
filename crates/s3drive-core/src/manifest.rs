//! Offline managed-manifest prototype. No key derivation, storage, authorization,
//! authority capability or native file-limit change is implied by this codec.
use sha2::{Digest, Sha256};
use std::fmt;

pub mod base;
pub mod objects;

pub const CHUNK_BYTES: u64 = 1024 * 1024;
pub const MAX_ENTRIES: usize = 128;
pub const MAX_LEVEL: u8 = 7;
pub const MAX_LENGTH: u64 = i64::MAX as u64;
pub const MAX_PAGE_BYTES: usize = 64 + MAX_ENTRIES * 81;
const MAGIC: &[u8; 8] = b"S3DMAN01";
const DOMAIN: &[u8] = b"s3drive-manifest-page-v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Bounds,
    Encoding,
    Shape,
    Integrity,
    Binding,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "managed manifest {:?}", self)
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;

/// A future role-specific key builder must bind store/generation/ordinal and
/// distinguish page objects from data objects. These are identities, not keys
/// or proof that this caller owns or may adopt an existing object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    store: [u8; 16],
    generation: [u8; 16],
    ordinal: u64,
    sha256: [u8; 32],
}
impl Reference {
    pub fn new(
        store: [u8; 16],
        generation: [u8; 16],
        ordinal: u64,
        sha256: [u8; 32],
    ) -> Result<Self> {
        if store == [0; 16] || generation == [0; 16] {
            return Err(Error::Binding);
        }
        Ok(Self {
            store,
            generation,
            ordinal,
            sha256,
        })
    }
    pub fn store(&self) -> &[u8; 16] {
        &self.store
    }
    pub fn generation(&self) -> &[u8; 16] {
        &self.generation
    }
    pub fn ordinal(&self) -> u64 {
        self.ordinal
    }
    pub fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.store);
        out.extend_from_slice(&self.generation);
        out.extend_from_slice(&self.ordinal.to_le_bytes());
        out.extend_from_slice(&self.sha256);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Hole { length: u64 },
    Data { length: u64, object: Reference },
    Child { length: u64, object: Reference },
}
impl Entry {
    pub fn length(&self) -> u64 {
        match self {
            Self::Hole { length } | Self::Data { length, .. } | Self::Child { length, .. } => {
                *length
            }
        }
    }
}

/// Structurally valid metadata only. It is not an authenticated file view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    volume: [u8; 16],
    node: [u8; 16],
    start: u64,
    length: u64,
    level: u8,
    entries: Vec<Entry>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPage(Page);

#[derive(Debug)]
pub struct Located<'a> {
    pub start: u64,
    pub offset_in_entry: u64,
    pub entry: &'a Entry,
}

impl Page {
    pub fn new(
        volume: [u8; 16],
        node: [u8; 16],
        start: u64,
        level: u8,
        entries: Vec<Entry>,
    ) -> Result<Self> {
        if volume == [0; 16] || node == [0; 16] {
            return Err(Error::Binding);
        }
        if entries.len() > MAX_ENTRIES || level > MAX_LEVEL || start > MAX_LENGTH {
            return Err(Error::Bounds);
        }
        if !start.is_multiple_of(CHUNK_BYTES) || entries.is_empty() && (start != 0 || level != 0) {
            return Err(Error::Shape);
        }
        let mut end = start;
        let mut previous_hole = false;
        for (index, entry) in entries.iter().enumerate() {
            let length = entry.length();
            if length == 0 {
                return Err(Error::Shape);
            }
            match entry {
                Entry::Hole { .. } if level == 0 && !previous_hole => {}
                Entry::Data { .. } if level == 0 && length <= CHUNK_BYTES => {}
                Entry::Child { .. } if level > 0 => {}
                _ => return Err(Error::Shape),
            }
            end = end
                .checked_add(length)
                .filter(|end| *end <= MAX_LENGTH)
                .ok_or(Error::Bounds)?;
            if index + 1 != entries.len() && !end.is_multiple_of(CHUNK_BYTES) {
                return Err(Error::Shape);
            }
            previous_hole = matches!(entry, Entry::Hole { .. });
        }
        Ok(Self {
            volume,
            node,
            start,
            length: end - start,
            level,
            entries,
        })
    }
    pub fn volume(&self) -> &[u8; 16] {
        &self.volume
    }
    pub fn node(&self) -> &[u8; 16] {
        &self.node
    }
    pub fn start(&self) -> u64 {
        self.start
    }
    pub fn length(&self) -> u64 {
        self.length
    }
    pub fn level(&self) -> u8 {
        self.level
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// At most MAX_PAGE_BYTES; all fields are immutable after validation.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(MAX_PAGE_BYTES);
        out.extend_from_slice(MAGIC);
        out.push(self.level);
        out.push(0);
        out.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&self.volume);
        out.extend_from_slice(&self.node);
        out.extend_from_slice(&self.start.to_le_bytes());
        out.extend_from_slice(&self.length.to_le_bytes());
        out.extend_from_slice(&(CHUNK_BYTES as u32).to_le_bytes());
        for entry in &self.entries {
            out.push(match entry {
                Entry::Hole { .. } => 0,
                Entry::Data { .. } => 1,
                Entry::Child { .. } => 2,
            });
            out.extend_from_slice(&entry.length().to_le_bytes());
            match entry {
                Entry::Hole { .. } => {}
                Entry::Data { object, .. } | Entry::Child { object, .. } => object.encode(&mut out),
            }
        }
        out
    }
    pub fn digest(&self) -> [u8; 32] {
        page_hash(&self.encode())
    }

    /// Rejects oversized input/count before allocation. Parsing is not trust.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if !(64..=MAX_PAGE_BYTES).contains(&bytes.len()) {
            return Err(Error::Bounds);
        }
        let mut c = Cursor { bytes, at: 0 };
        if &c.array::<8>()? != MAGIC {
            return Err(Error::Encoding);
        }
        let level = c.array::<1>()?[0];
        if c.array::<1>()? != [0] {
            return Err(Error::Encoding);
        }
        let count = usize::from(u16::from_le_bytes(c.array()?));
        if count > MAX_ENTRIES || level > MAX_LEVEL {
            return Err(Error::Bounds);
        }
        let volume = c.array()?;
        let node = c.array()?;
        let start = u64::from_le_bytes(c.array()?);
        let length = u64::from_le_bytes(c.array()?);
        if u32::from_le_bytes(c.array()?) != CHUNK_BYTES as u32 {
            return Err(Error::Encoding);
        }
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let tag = c.array::<1>()?[0];
            let length = u64::from_le_bytes(c.array()?);
            entries.push(match tag {
                0 => Entry::Hole { length },
                1 | 2 => {
                    let object = Reference::new(
                        c.array()?,
                        c.array()?,
                        u64::from_le_bytes(c.array()?),
                        c.array()?,
                    )?;
                    if tag == 1 {
                        Entry::Data { length, object }
                    } else {
                        Entry::Child { length, object }
                    }
                }
                _ => return Err(Error::Encoding),
            });
        }
        if c.at != bytes.len() {
            return Err(Error::Encoding);
        }
        let page = Self::new(volume, node, start, level, entries)?;
        if page.length != length {
            return Err(Error::Shape);
        }
        Ok(page)
    }

    /// The expected digest/identity/length must come from trusted authority
    /// metadata. Cryptographic verification does not confer S3 authorization.
    pub fn verify_root(
        bytes: &[u8],
        expected_hash: &[u8; 32],
        volume: &[u8; 16],
        node: &[u8; 16],
        length: u64,
    ) -> Result<VerifiedPage> {
        let page = Self::verified(bytes, expected_hash)?;
        if page.volume != *volume || page.node != *node || page.start != 0 || page.length != length
        {
            return Err(Error::Binding);
        }
        Ok(VerifiedPage(page))
    }
    fn verified(bytes: &[u8], hash: &[u8; 32]) -> Result<Self> {
        let page = Self::decode(bytes)?;
        if &page_hash(bytes) != hash {
            return Err(Error::Integrity);
        }
        Ok(page)
    }
    /// Allocation-free bounded scan; None means outside this page, including EOF.
    pub fn lookup(&self, offset: u64) -> Option<Located<'_>> {
        if offset < self.start || offset >= self.start + self.length {
            return None;
        }
        let mut start = self.start;
        for entry in &self.entries {
            if offset < start + entry.length() {
                return Some(Located {
                    start,
                    offset_in_entry: offset - start,
                    entry,
                });
            }
            start += entry.length();
        }
        None
    }
}
impl VerifiedPage {
    pub fn page(&self) -> &Page {
        &self.0
    }
    pub fn verify_child(&self, index: usize, bytes: &[u8]) -> Result<Self> {
        let Some(Entry::Child { length, object }) = self.0.entries.get(index) else {
            return Err(Error::Shape);
        };
        let page = Page::verified(bytes, &object.sha256)?;
        let start = self.0.start
            + self.0.entries[..index]
                .iter()
                .map(Entry::length)
                .sum::<u64>();
        if page.volume != self.0.volume
            || page.node != self.0.node
            || page.start != start
            || page.length != *length
            || page.level + 1 != self.0.level
        {
            return Err(Error::Binding);
        }
        Ok(Self(page))
    }
    /// Verify one complete bounded data object before exposing its subranges.
    pub fn verify_data(&self, index: usize, bytes: &[u8]) -> Result<()> {
        let Some(Entry::Data { length, object }) = self.0.entries.get(index) else {
            return Err(Error::Shape);
        };
        if bytes.len() as u64 != *length || <[u8; 32]>::from(Sha256::digest(bytes)) != object.sha256
        {
            return Err(Error::Integrity);
        }
        Ok(())
    }
}
fn page_hash(bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(bytes);
    h.finalize().into()
}
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Cursor<'_> {
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let end = self.at.checked_add(N).ok_or(Error::Bounds)?;
        let bytes = self.bytes.get(self.at..end).ok_or(Error::Encoding)?;
        self.at = end;
        bytes.try_into().map_err(|_| Error::Encoding)
    }
}

#[cfg(test)]
mod tests;
