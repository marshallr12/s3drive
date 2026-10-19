//! Structural intent and immutable initial base/zero ranges; never verified bytes.
use super::*;
use s3drive_core::manifest::{base::Descriptor, objects::Namespace};

pub const MAX_FILE_BYTES: u64 = 65_536 * s3drive_core::manifest::CHUNK_BYTES;
pub const MAX_RANGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub(super) base: Vec<u8>,
    pub(super) store: [u8; 16],
    pub(super) source: Id,
    pub(super) fingerprint: String,
    pub(super) root: String,
    pub(super) base_size: u64,
}
impl fmt::Debug for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SparseBaseBinding([untrusted intent])")
    }
}
fn bytes_id(s: &str) -> Result<[u8; 16]> {
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Failure::Invalid.into());
    }
    let mut out = [0; 16];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| Failure::Invalid)?;
    }
    Ok(out)
}
impl Binding {
    /// Record intent only. The future composer must reauthorize this exact base
    /// and independently compare its size, route and target before using bytes.
    pub fn new(
        base: Descriptor,
        store: [u8; 16],
        source: Id,
        fingerprint: String,
        root: String,
        base_size: u64,
    ) -> Result<Self> {
        let value = Self {
            base: base.encode().to_vec(),
            store,
            source,
            fingerprint,
            root,
            base_size,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn base(&self) -> Result<Descriptor> {
        Descriptor::decode(&self.base).map_err(|_| Failure::Invalid.into())
    }
    pub fn store(&self) -> &[u8; 16] {
        &self.store
    }
    pub fn source(&self) -> &Id {
        &self.source
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    pub fn root(&self) -> &str {
        &self.root
    }
    pub fn base_size(&self) -> u64 {
        self.base_size
    }
    pub(super) fn validate(&self) -> Result<()> {
        let base = self.base()?;
        if self.source == [0; 32]
            || self.root.is_empty()
            || self.root.len() > 128
            || self.base_size > MAX_FILE_BYTES
            || self.fingerprint.len() != 64
            || !self
                .fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Failure::Invalid.into());
        }
        Namespace::new(
            self.root.clone(),
            bytes_id(base.volume())?,
            bytes_id(base.node())?,
            self.store,
        )
        .map_err(|_| Failure::Invalid)?;
        Ok(())
    }
    pub(super) fn target(&self) -> Result<Id> {
        let base = self.base()?;
        // A store/route/base change must conflict with the same file's dirty
        // identity, never create another independent local head for it.
        hash(b"s3drive-sparse-target-v1\0", &(base.volume(), base.node()))
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub(super) format: u8,
    pub(super) binding: Binding,
    pub(super) size: u64,
}
impl Request {
    pub(super) fn validate(&self) -> Result<()> {
        self.binding.validate()?;
        if self.format != 1 || self.size > MAX_FILE_BYTES {
            return Err(Failure::Invalid.into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(super) enum Span {
    Base { offset: u64, length: u64 },
    Zero { length: u64 },
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub(super) format: u8,
    pub(super) id: Id,
    pub(super) operation: Id,
    pub(super) request: Request,
    pub(super) parent: Option<Id>,
    pub(super) spans: Vec<Span>,
}
impl Record {
    pub(super) fn new(operation: Id, request: Request) -> Result<Self> {
        request.validate()?;
        if operation == [0; 32] {
            return Err(Failure::Invalid.into());
        }
        let id = hash(b"s3drive-sparse-initial-view-v1\0", &(operation, &request))?;
        let base = request.size.min(request.binding.base_size);
        let mut spans = Vec::new();
        if base > 0 {
            spans.push(Span::Base {
                offset: 0,
                length: base,
            });
        }
        if request.size > base {
            spans.push(Span::Zero {
                length: request.size - base,
            });
        }
        Ok(Self {
            format: 1,
            id,
            operation,
            request,
            parent: None,
            spans,
        })
    }
    pub(super) fn validate(&self) -> Result<()> {
        let expected =
            Self::new(self.operation, self.request.clone()).map_err(|_| Failure::Corrupt)?;
        if self != &expected {
            return Err(Failure::Corrupt.into());
        }
        Ok(())
    }
}
/// Immutable local metadata. This is not an authorized content handle, upload
/// receipt or acknowledgment that any remote or local bytes are healthy.
#[derive(Clone, PartialEq, Eq)]
pub struct View(pub(super) Record);
impl fmt::Debug for View {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SparseView([private metadata])")
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Range {
    Base { offset: u64, length: u64 },
    Zero { length: u64 },
}
impl View {
    pub fn id(&self) -> Id {
        self.0.id
    }
    pub fn operation(&self) -> Id {
        self.0.operation
    }
    pub fn binding(&self) -> &Binding {
        &self.0.request.binding
    }
    pub fn size(&self) -> u64 {
        self.0.request.size
    }
    pub fn range(&self, offset: u64, length: usize) -> Result<Vec<Range>> {
        if length > MAX_RANGE_BYTES {
            return Err(Failure::Bounds.into());
        }
        if offset >= self.size() || length == 0 {
            return Ok(Vec::new());
        }
        let end = offset + (length as u64).min(self.size() - offset);
        let mut at = 0;
        let mut out = Vec::new();
        for span in &self.0.spans {
            let length = match span {
                Span::Base { length, .. } | Span::Zero { length } => *length,
            };
            let stop = at + length;
            let start = offset.max(at);
            let finish = end.min(stop);
            if start < finish {
                out.push(match span {
                    Span::Base { offset, .. } => Range::Base {
                        offset: offset + (start - at),
                        length: finish - start,
                    },
                    Span::Zero { .. } => Range::Zero {
                        length: finish - start,
                    },
                });
            }
            at = stop;
            if at >= end {
                break;
            }
        }
        Ok(out)
    }
}
