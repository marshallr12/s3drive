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
    // Absent old field stays absent on serialization: initial IDs/hashes remain
    // byte-compatible with schema29/30. Child intent has its own format/domain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) edit: Option<Change>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BlockRef {
    pub(super) block: Id,
    pub(super) length: u64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(super) enum Change {
    Write {
        parent: Id,
        offset: u64,
        blocks: Vec<BlockRef>,
    },
    Resize {
        parent: Id,
    },
}
impl Change {
    pub(super) fn parent(&self) -> Id {
        match self {
            Self::Write { parent, .. } | Self::Resize { parent } => *parent,
        }
    }
}
impl Request {
    pub(super) fn validate(&self) -> Result<()> {
        self.binding.validate()?;
        if self.size > MAX_FILE_BYTES {
            return Err(Failure::Invalid.into());
        }
        match &self.edit {
            None if self.format == 1 => (),
            Some(change) if self.format == 2 && change.parent() != [0; 32] => {
                if let Change::Write { offset, blocks, .. } = change {
                    if blocks.is_empty() || blocks.len() > MAX_RANGE_BYTES / PATCH_BYTES {
                        return Err(Failure::Invalid.into());
                    }
                    let mut total = 0;
                    for (i, b) in blocks.iter().enumerate() {
                        if b.length == 0
                            || b.length > PATCH_BYTES as u64
                            || (i + 1 < blocks.len() && b.length != PATCH_BYTES as u64)
                        {
                            return Err(Failure::Invalid.into());
                        }
                        total += b.length;
                    }
                    if offset
                        .checked_add(total)
                        .is_none_or(|end| end > MAX_FILE_BYTES)
                    {
                        return Err(Failure::Invalid.into());
                    }
                }
            }
            _ => return Err(Failure::Invalid.into()),
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(super) enum Span {
    Base { offset: u64, length: u64 },
    Zero { length: u64 },
    Patch { block: Id, offset: u64, length: u64 },
}
impl Span {
    pub(super) fn length(&self) -> u64 {
        match self {
            Self::Base { length, .. } | Self::Zero { length } | Self::Patch { length, .. } => {
                *length
            }
        }
    }
    fn slice(&self, skip: u64, length: u64) -> Self {
        match self {
            Self::Base { offset, .. } => Self::Base {
                offset: offset + skip,
                length,
            },
            Self::Zero { .. } => Self::Zero { length },
            Self::Patch { block, offset, .. } => Self::Patch {
                block: *block,
                offset: offset + skip,
                length,
            },
        }
    }
}
fn push(out: &mut Vec<Span>, span: Span) {
    if span.length() == 0 {
        return;
    }
    if let Some(last) = out.last_mut() {
        match (last, &span) {
            (Span::Zero { length: a }, Span::Zero { length: b }) => {
                *a += b;
                return;
            }
            (
                Span::Base {
                    offset: a,
                    length: n,
                },
                Span::Base {
                    offset: b,
                    length: m,
                },
            ) if *a + *n == *b => {
                *n += m;
                return;
            }
            (
                Span::Patch {
                    block: a,
                    offset: x,
                    length: n,
                },
                Span::Patch {
                    block: b,
                    offset: y,
                    length: m,
                },
            ) if a == b && *x + *n == *y => {
                *n += m;
                return;
            }
            _ => (),
        }
    }
    out.push(span);
}
fn slice(spans: &[Span], start: u64, end: u64, out: &mut Vec<Span>) {
    let mut at = 0;
    for s in spans {
        let stop = at + s.length();
        let lo = at.max(start);
        let hi = stop.min(end);
        if lo < hi {
            push(out, s.slice(lo - at, hi - lo));
        }
        at = stop;
        if at >= end {
            break;
        }
    }
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
        if operation == [0; 32] || request.edit.is_some() {
            return Err(Failure::Invalid.into());
        }
        let id = hash(b"s3drive-sparse-initial-view-v1\0", &(operation, &request))?;
        let base = request.size.min(request.binding.base_size);
        let mut spans = Vec::new();
        push(
            &mut spans,
            Span::Base {
                offset: 0,
                length: base,
            },
        );
        push(
            &mut spans,
            Span::Zero {
                length: request.size - base,
            },
        );
        Ok(Self {
            format: 1,
            id,
            operation,
            request,
            parent: None,
            spans,
        })
    }
    pub(super) fn child(operation: Id, request: Request, parent: &Record) -> Result<Self> {
        request.validate()?;
        if operation == [0; 32] || request.binding != parent.request.binding {
            return Err(Failure::Invalid.into());
        }
        let change = request.edit.as_ref().ok_or(Failure::Invalid)?;
        if change.parent() != parent.id {
            return Err(Failure::Invalid.into());
        }
        let mut spans = Vec::new();
        match change {
            Change::Resize { .. } => {
                slice(
                    &parent.spans,
                    0,
                    request.size.min(parent.request.size),
                    &mut spans,
                );
                if request.size > parent.request.size {
                    push(
                        &mut spans,
                        Span::Zero {
                            length: request.size - parent.request.size,
                        },
                    );
                }
            }
            Change::Write { offset, blocks, .. } => {
                let end = *offset + blocks.iter().map(|b| b.length).sum::<u64>();
                if request.size != parent.request.size.max(end) {
                    return Err(Failure::Invalid.into());
                }
                slice(
                    &parent.spans,
                    0,
                    (*offset).min(parent.request.size),
                    &mut spans,
                );
                if *offset > parent.request.size {
                    push(
                        &mut spans,
                        Span::Zero {
                            length: *offset - parent.request.size,
                        },
                    );
                }
                for b in blocks {
                    push(
                        &mut spans,
                        Span::Patch {
                            block: b.block,
                            offset: 0,
                            length: b.length,
                        },
                    );
                }
                slice(&parent.spans, end, parent.request.size, &mut spans);
            }
        }
        if spans.len() > MAX_VIEW_EXTENTS {
            return Err(Failure::Quota.into());
        }
        let id = hash(b"s3drive-sparse-child-view-v1\0", &(operation, &request))?;
        Ok(Self {
            format: 2,
            id,
            operation,
            request,
            parent: Some(parent.id),
            spans,
        })
    }
    pub(super) fn validate(&self, parent: Option<&Record>) -> Result<()> {
        let expected = match parent {
            None => Self::new(self.operation, self.request.clone()),
            Some(p) => Self::child(self.operation, self.request.clone(), p),
        }
        .map_err(|_| Failure::Corrupt)?;
        if self != &expected {
            return Err(Failure::Corrupt.into());
        }
        Ok(())
    }
}
/// Immutable local intent, never a provider authorization or clean receipt.
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
    Patch { block: Id, offset: u64, length: u64 },
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
        let mut spans = Vec::new();
        slice(&self.0.spans, offset, end, &mut spans);
        Ok(spans
            .into_iter()
            .map(|s| match s {
                Span::Base { offset, length } => Range::Base { offset, length },
                Span::Zero { length } => Range::Zero { length },
                Span::Patch {
                    block,
                    offset,
                    length,
                } => Range::Patch {
                    block,
                    offset,
                    length,
                },
            })
            .collect())
    }
}
