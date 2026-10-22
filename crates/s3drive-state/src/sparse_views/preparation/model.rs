//! Bounded version-one preparation format. Caller inputs remain offline intent.
use super::*;
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub plans: usize,
    pub objects: usize,
    pub metadata_bytes: u64,
    pub blob_bytes: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            plans: 16,
            objects: 32768,
            metadata_bytes: 8 * 1024 * 1024,
            blob_bytes: 64 * 1024 * 1024,
        }
    }
}
impl Options {
    pub(super) fn validate(self) -> Result<Self> {
        if !(1..=MAX_PLANS).contains(&self.plans)
            || !(1..=MAX_OBJECTS).contains(&self.objects)
            || !(1..=MAX_METADATA).contains(&self.metadata_bytes)
            || !(1..=MAX_BYTES).contains(&self.blob_bytes)
        {
            return Err(Failure::Invalid.into());
        }
        Ok(self)
    }
}
/// Freeze structural original-root intent; no remote authentication follows.
/// The managed producer must supply its actual authorized base, and a future
/// publication controller must independently revalidate it before any sends.
pub struct Request {
    pub operation: Id,
    pub view: Id,
    pub generation: [u8; 16],
    pub base_root: Vec<u8>,
    pub base_page: Vec<u8>,
}
impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MixedPreparationRequest([untrusted intent])")
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    pub operation: Id,
    pub view: Id,
    pub generation: [u8; 16],
    pub base_root: Vec<u8>,
    pub base_page: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned: Option<owned::Provenance>,
}
impl Input {
    pub fn id(&self) -> Result<Id> {
        hash(b"s3drive-mixed-preparation-input-v1\0", self)
    }
    pub fn validate(&self, view: &View, blobs: &BTreeMap<Id, Vec<u8>>) -> Result<VerifiedPage> {
        if self.operation == [0; 32] || self.generation == [0; 16] || self.view != view.id() {
            return Err(Failure::Corrupt.into());
        }
        let base = view.binding().base()?;
        let root = RootDescriptor::decode(&self.base_root)?;
        if hex(root.volume()) != base.volume()
            || hex(root.node()) != base.node()
            || root.length() != view.binding().base_size()
            || root.page().store() != view.binding().store()
        {
            return Err(Failure::Corrupt.into());
        }
        let page = root.verify_page(blobs.get(&self.base_page).ok_or(Failure::Corrupt)?)?;
        proof::scope(&page, view.binding().store())?;
        if page.page().level() > 2 {
            return Err(Failure::Corrupt.into());
        }
        Ok(page)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub offset: u64,
    /// -1 input coverage; 0..2 bottom-up page construction.
    pub level: i8,
    pub slot: u64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Progress {
    pub cursor: Cursor,
    pub root: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlement: Option<owned::Settlement>,
}
#[derive(Clone)]
pub struct Info {
    pub(super) input: Input,
    pub(super) view: View,
    pub(super) progress: Progress,
}
impl fmt::Debug for Info {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MixedPreparationInfo([private])")
    }
}
impl Info {
    pub fn ownership(&self) -> Option<&owned::Provenance> {
        self.input.owned.as_ref()
    }
    pub fn settlement(&self) -> Option<&owned::Settlement> {
        self.progress.settlement.as_ref()
    }
    pub fn id(&self) -> Result<Id> {
        self.input.id()
    }
    pub fn operation(&self) -> Id {
        self.input.operation
    }
    pub fn view(&self) -> &View {
        &self.view
    }
    pub fn generation(&self) -> &[u8; 16] {
        &self.input.generation
    }
    pub fn cursor(&self) -> Cursor {
        self.progress.cursor
    }
    pub fn root(&self) -> Result<Option<RootDescriptor>> {
        self.progress
            .root
            .as_deref()
            .map(RootDescriptor::decode)
            .transpose()
            .map_err(Into::into)
    }
    pub fn original_root(&self) -> Result<RootDescriptor> {
        RootDescriptor::decode(&self.input.base_root).map_err(Into::into)
    }
}
/// Caller-provided prepared content, never authenticated by construction. State
/// validates geometry/retained coverage/proofs and stores exact bytes; no network
/// or clean receipt may be inferred. Use the managed producer for file semantics.
pub enum Candidate {
    Bytes(Vec<u8>),
    /// A structural zero run, independently checked against immutable view spans.
    Zero {
        length: u64,
    },
    InheritedChunk(Vec<u8>),
    InheritedPage(Vec<u8>),
}
impl fmt::Debug for Candidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MixedCandidate([untrusted intent])")
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum Kind {
    Data { blob: Id },
    Zero,
    Inherited(proof::Witness),
    Page { blob: Id },
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Object {
    pub level: i8,
    pub slot: u64,
    pub length: u64,
    pub kind: Kind,
}
impl Object {
    pub fn offset(&self) -> u64 {
        self.slot * span(self.level)
    }
    pub fn end(&self) -> u64 {
        self.offset() + self.length
    }
    pub fn inherited(&self) -> bool {
        matches!(self.kind, Kind::Inherited(_))
    }
    pub fn blobs(&self) -> Vec<Id> {
        match &self.kind {
            Kind::Data { blob } | Kind::Page { blob } => vec![*blob],
            Kind::Inherited(p) => p.pages.clone(),
            Kind::Zero => Vec::new(),
        }
    }
}
pub(super) fn span(level: i8) -> u64 {
    manifest::CHUNK_BYTES * 128u64.pow((level + 1) as u32)
}
pub(super) fn count(size: u64, level: i8) -> u64 {
    let n = size.div_ceil(span(level));
    if level < 0 { n } else { n.max(1) }
}
pub(super) fn root_level(size: u64) -> i8 {
    let mut level = 0;
    while count(size, level) > 1 {
        level += 1;
    }
    level
}
pub(super) fn geometry(size: u64, level: i8, slot: u64) -> Result<(u64, u64)> {
    if !(-1..=root_level(size)).contains(&level) || slot >= count(size, level) {
        return Err(Failure::Corrupt.into());
    }
    let offset = slot * span(level);
    Ok((offset, (size - offset).min(span(level))))
}
pub(super) fn ordinal(level: i8, slot: u64) -> u64 {
    if level < 0 {
        slot
    } else {
        ((level as u64 + 1) << 56) | slot
    }
}

fn hex(v: &[u8; 16]) -> String {
    v.iter().map(|b| format!("{b:02x}")).collect()
}
