//! Durable local managed edits. Local targets cannot be mistaken for an S3
//! generation binding; metadata publication and logical settlement are separate.
use crate::{
    Result, State,
    journal::{self, GenerationId, Target},
    managed,
    namespace::{self, Kind, Node, Receipt, Store},
    remote,
};
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
const LIMIT: usize = 4096;
const MAX_BYTES: u64 = 8 * 1024 * 1024;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub volume: String,
    pub node: Node,
    pub store: Store,
    pub source: [u8; 32],
    pub fingerprint: String,
}
impl std::fmt::Debug for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LogicalBinding([private])")
    }
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn id(s: &str) -> bool {
    s.len() == 32
        && s != namespace::ROOT
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
fn hash<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut h = Sha256::new();
    h.update(b"s3drive-logical-v1\0");
    h.update(serde_json::to_vec(value)?);
    Ok(h.finalize().to_vec())
}
impl Binding {
    fn validate(&self) -> Result<()> {
        let n = &self.node;
        if !id(&self.volume)
            || !id(&n.id)
            || n.parent
                .as_ref()
                .is_none_or(|p| p != namespace::ROOT && !id(p))
            || n.deleted
            || n.revision == 0
            || n.revision >= i64::MAX as u64
            || !self.store.valid()
            || !digest(&self.fingerprint)
            || n.name.is_empty()
            || n.name.len() > 255
            || [".", ".."].contains(&n.name.as_str())
            || n.name
                .chars()
                .any(|c| c.is_control() || c == '/' || c == '\\')
        {
            return Err("invalid managed logical binding".into());
        }
        match (&n.kind, &n.payload) {
            (Kind::EmptyFile, None) => {}
            (Kind::File, Some(p))
                if p.valid()
                    && p.reservation.volume == self.volume
                    && p.reservation.node == n.id
                    && p.reservation.store == self.store => {}
            _ => return Err("logical base is not a bound file snapshot".into()),
        }
        Ok(())
    }
    /// A domain-separated local identity, never an authorized S3 target.
    pub fn target(&self) -> Result<Target> {
        self.validate()?;
        let source: [u8; 32] = hash(self)?
            .try_into()
            .map_err(|_| "invalid logical digest")?;
        Target::new(
            source,
            &hex(&source),
            &format!("logical/{}/{}", self.volume, self.node.id),
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub id: String,
    pub volume: String,
    pub node: String,
    pub parent: Option<GenerationId>,
    pub size: u64,
    pub sha256: String,
}
impl Input {
    pub fn from_bytes(
        id: String,
        binding: &Binding,
        parent: Option<GenerationId>,
        bytes: &[u8],
    ) -> Self {
        Self {
            id,
            volume: binding.volume.clone(),
            node: binding.node.id.clone(),
            parent,
            size: bytes.len() as u64,
            sha256: hex(&Sha256::digest(bytes)),
        }
    }
    fn validate(&self) -> Result<()> {
        if !id(&self.id)
            || !id(&self.volume)
            || !id(&self.node)
            || self.size > MAX_BYTES
            || !digest(&self.sha256)
        {
            return Err("invalid logical edit input".into());
        }
        Ok(())
    }
    fn operation_key(&self, binding: &Binding) -> Result<[u8; 32]> {
        Ok(hash(&("stage", binding, self))?
            .try_into()
            .map_err(|_| "invalid edit digest")?)
    }
}
#[derive(Debug, Clone)]
pub struct Record {
    pub input: Input,
    pub generation: Option<GenerationId>,
    pub publication: Option<managed::Input>,
    pub receipt: Option<Receipt>,
}
#[derive(Debug, Clone)]
pub struct File {
    pub binding: Binding,
    pub head: Option<GenerationId>,
    pub settled: Option<GenerationId>,
    pub base: Node,
    pub base_observed: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct FileRow {
    binding: Binding,
    settled: Option<GenerationId>,
    publication: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    observed: Option<Node>,
}
fn record_hash(r: &Record) -> Result<Vec<u8>> {
    hash(&(&r.input, &r.generation, &r.publication, &r.receipt))
}
type EncodedFileRow = (
    Vec<u8>,
    Option<i64>,
    Option<String>,
    Vec<u8>,
    Option<Vec<u8>>,
);
fn file_row(db: &Connection, volume: &str, node: &str) -> Result<FileRow> {
    let shape:bool=db.query_row("SELECT length(input)<=32768 AND length(state_hash)=32 AND coalesce(length(settled_publication),0)<=32 AND coalesce(length(observed_base),0)<=32768 FROM logical_files WHERE volume=?1 AND node=?2",params![volume,node],|r|r.get(0))?;
    if !shape {
        return Err("invalid logical file record shape".into());
    }
    let (input,generation,publication,checksum,observed):EncodedFileRow=db.query_row("SELECT input,settled_generation,settled_publication,state_hash,observed_base FROM logical_files WHERE volume=?1 AND node=?2",params![volume,node],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
    let f = FileRow {
        binding: serde_json::from_slice(&input)?,
        settled: generation.map(GenerationId::from_database).transpose()?,
        publication,
        observed: observed.map(|b| serde_json::from_slice(&b)).transpose()?,
    };
    f.binding.validate()?;
    if let Some(node) = &f.observed {
        let mut binding = f.binding.clone();
        binding.node = node.clone();
        binding.validate()?;
        if node.id != f.binding.node.id {
            return Err("observed logical identity changed".into());
        }
    }
    if f.binding.volume != volume
        || f.binding.node.id != node
        || f.settled.is_some() != f.publication.is_some()
        || hash(&f)? != checksum
    {
        return Err("logical file integrity failure".into());
    }
    Ok(f)
}
type EncodedEditRow = (
    String,
    String,
    Vec<u8>,
    Option<i64>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Vec<u8>,
);
fn load(db: &Connection, id: &str) -> Result<Record> {
    let shape:bool=db.query_row("SELECT length(input)<=32768 AND coalesce(length(publication),0)<=32768 AND coalesce(length(receipt),0)<=32768 AND length(state_hash)=32 FROM logical_edits WHERE operation_id=?",[id],|r|r.get(0))?;
    if !shape {
        return Err("invalid logical edit record shape".into());
    }
    let (volume,node,input,generation,publication,receipt,checksum):EncodedEditRow=db.query_row("SELECT volume,node,input,generation_id,publication,receipt,state_hash FROM logical_edits WHERE operation_id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?)))?;
    let r = Record {
        input: serde_json::from_slice(&input)?,
        generation: generation.map(GenerationId::from_database).transpose()?,
        publication: publication
            .map(|v| serde_json::from_slice(&v))
            .transpose()?,
        receipt: receipt.map(|v| serde_json::from_slice(&v)).transpose()?,
    };
    r.input.validate()?;
    if r.input.id != id
        || r.input.volume != volume
        || r.input.node != node
        || record_hash(&r)? != checksum
    {
        return Err("logical edit integrity failure".into());
    }
    let f = file_row(db, &volume, &node)?;
    if let Some(g) = r.generation {
        let info = journal::load(db, g)?;
        if info.target != f.binding.target()?
            || info.parent != r.input.parent
            || info.size != r.input.size
            || info.base != journal::Base::Missing
        {
            return Err("logical generation binding failure".into());
        }
        if info.phase == journal::Phase::Sealed {
            verify_bytes(db, &r, &info)?;
        }
    } else if r.publication.is_some() || r.receipt.is_some() {
        return Err("logical edit lacks staged bytes".into());
    }
    if let Some(p) = &r.publication {
        let reserve = p.reservation()?;
        let links:i64=db.query_row("SELECT count(*) FROM logical_publication_links WHERE operation_id IN (?1,?2) AND edit_id=?3",params![p.id(),p.publish_operation,id],|r|r.get(0))?;
        if links != 2 {
            return Err("logical publication operation ownership changed".into());
        }
        if p.source != f.binding.source
            || p.fingerprint != f.binding.fingerprint
            || reserve.store != f.binding.store
            || reserve.volume != volume
            || reserve.node != node
            || reserve.size != r.input.size
            || reserve.sha256 != r.input.sha256
        {
            return Err("logical publication binding failure".into());
        }
    }
    if r.receipt.is_some() && r.publication.is_none() {
        return Err("logical receipt lacks publication".into());
    }
    Ok(r)
}
fn verify_bytes(db: &Connection, r: &Record, g: &journal::Generation) -> Result<()> {
    let bytes = journal::read_data(
        db,
        g.id,
        0,
        g.size as usize,
        g.phase == journal::Phase::Building,
    )?;
    if bytes.len() as u64 != r.input.size || hex(&Sha256::digest(bytes)) != r.input.sha256 {
        return Err("logical bytes differ from durable intent".into());
    }
    Ok(())
}
fn save(db: &Connection, r: &Record) -> Result<()> {
    let changed=db.execute("UPDATE logical_edits SET generation_id=?1,publication=?2,receipt=?3,state_hash=?4 WHERE operation_id=?5",params![r.generation.map(|g|g.value()),r.publication.as_ref().map(serde_json::to_vec).transpose()?,r.receipt.as_ref().map(serde_json::to_vec).transpose()?,record_hash(r)?,r.input.id])?;
    if changed != 1 {
        return Err("logical edit update was not committed".into());
    }
    Ok(())
}
fn ancestor(
    db: &Connection,
    binding: &Binding,
    older: GenerationId,
    mut newer: GenerationId,
) -> Result<bool> {
    for _ in 0..LIMIT {
        let g = journal::load(db, newer)?;
        if g.target != binding.target()? || g.phase != journal::Phase::Sealed {
            return Err("logical ancestry binding failure".into());
        }
        if newer == older {
            return Ok(true);
        }
        let Some(parent) = g.parent else {
            return Ok(false);
        };
        newer = parent;
    }
    Err("logical ancestry exceeds retained bounds".into())
}
pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT volume,node,input,settled_generation,settled_publication,state_hash,observed_base FROM logical_files LIMIT 0")?;
    db.prepare("SELECT operation_id,volume,node,input,generation_id,publication,receipt,state_hash FROM logical_edits LIMIT 0")?;
    db.prepare("SELECT operation_id,edit_id FROM logical_publication_links LIMIT 0")?;
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    InputPersisted,
    BuilderCreated,
    BuilderLinked,
    SealPending,
    Sealed,
    PublicationPending,
    PublicationPrepared,
    SettlementPending,
    Settled,
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
    pub fn bind(&mut self, binding: Binding) -> Result<File> {
        binding.validate()?;
        let exists: bool = self.state.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM logical_files WHERE volume=?1 AND node=?2)",
            params![binding.volume, binding.node.id],
            |r| r.get(0),
        )?;
        if exists {
            let f = self.file(&binding.volume, &binding.node.id)?;
            let mut original = binding.clone();
            original.node = f.binding.node.clone();
            if original != f.binding || (binding.node != f.binding.node && binding.node != f.base) {
                return Err("logical base or credential binding changed; edits retained".into());
            }
            return Ok(f);
        }
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: i64 = tx.query_row("SELECT count(*) FROM logical_files", [], |r| r.get(0))?;
        if count >= LIMIT as i64 {
            return Err("logical file limit reached".into());
        }
        let row = FileRow {
            binding: binding.clone(),
            settled: None,
            publication: None,
            observed: None,
        };
        tx.execute(
            "INSERT INTO logical_files(volume,node,input,settled_generation,settled_publication,state_hash) VALUES(?1,?2,?3,NULL,NULL,?4)",
            params![
                binding.volume,
                binding.node.id,
                serde_json::to_vec(&binding)?,
                hash(&row)?
            ],
        )?;
        tx.commit()?;
        self.file(&binding.volume, &binding.node.id)
    }
    pub fn lookup(&mut self, volume: &str, node: &str) -> Result<Option<File>> {
        let exists: bool = self.state.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM logical_files WHERE volume=?1 AND node=?2)",
            params![volume, node],
            |r| r.get(0),
        )?;
        if exists {
            self.file(volume, node).map(Some)
        } else {
            Ok(None)
        }
    }
    pub fn for_generation(&mut self, g: GenerationId) -> Result<Record> {
        let id: String = self.state.connection.query_row(
            "SELECT operation_id FROM logical_edits WHERE generation_id=?",
            [g.value()],
            |r| r.get(0),
        )?;
        self.get(&id)
    }
    /// Caller supplies a freshly authorized immutable snapshot. Never discard
    /// a draft or adopt different remote content over any dirty local head.
    pub fn observe(&mut self, binding: Binding) -> Result<File> {
        binding.validate()?;
        let Some(file) = self.lookup(&binding.volume, &binding.node.id)? else {
            return self.bind(binding);
        };
        let mut original = binding.clone();
        original.node = file.binding.node.clone();
        if original != file.binding {
            return Err("logical credential binding changed".into());
        }
        if binding.node == file.base {
            return Ok(file);
        }
        if binding.node.revision <= file.base.revision {
            return Err("logical observation regressed".into());
        }
        let changed = namespace::payload_reference(&binding.node.payload)
            != namespace::payload_reference(&file.base.payload);
        if changed {
            if file.head != file.settled {
                return Err("external content conflicts with dirty head".into());
            }
            let ids = self
                .state
                .connection
                .prepare("SELECT operation_id FROM logical_edits WHERE volume=?1 AND node=?2")?
                .query_map(params![binding.volume, binding.node.id], |r| {
                    r.get::<_, String>(0)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for id in ids {
                let r = self.get(&id)?;
                let covered = match (r.generation, file.settled) {
                    (Some(g), Some(s)) => ancestor(&self.state.connection, &file.binding, g, s)?,
                    _ => false,
                };
                if !covered {
                    return Err("external content conflicts with retained draft".into());
                }
            }
        }
        let mut row = file_row(&self.state.connection, &binding.volume, &binding.node.id)?;
        row.observed = Some(binding.node.clone());
        let rows = self.state.connection.execute(
            "UPDATE logical_files SET observed_base=?1,state_hash=?2 WHERE volume=?3 AND node=?4",
            params![
                serde_json::to_vec(&binding.node)?,
                hash(&row)?,
                binding.volume,
                binding.node.id
            ],
        )?;
        if rows != 1 {
            return Err("logical observation not persisted".into());
        }
        self.file(&binding.volume, &binding.node.id)
    }
    pub fn file(&mut self, volume: &str, node: &str) -> Result<File> {
        let row = file_row(&self.state.connection, volume, node)?;
        let head = journal::Journal::new(self.state, self.options)?
            .head(&row.binding.target()?)?
            .map(|g| g.id);
        if let Some(g) = head {
            let id: String = self.state.connection.query_row(
                "SELECT operation_id FROM logical_edits WHERE generation_id=?",
                [g.value()],
                |r| r.get(0),
            )?;
            self.get(&id)?;
        }
        let base = if let Some(g) = row.settled {
            if !ancestor(
                &self.state.connection,
                &row.binding,
                g,
                head.ok_or("settled file lacks head")?,
            )? {
                return Err("logical settlement outside head lineage".into());
            }
            let id: String = self.state.connection.query_row(
                "SELECT operation_id FROM logical_edits WHERE generation_id=?",
                [g.value()],
                |r| r.get(0),
            )?;
            let r = self.get(&id)?;
            if r.publication.as_ref().map(|p| p.id()) != row.publication.as_deref() {
                return Err("logical settlement publication changed".into());
            }
            r.receipt.ok_or("logical settlement receipt missing")?.node
        } else {
            row.binding.node.clone()
        };
        Ok(File {
            binding: row.binding,
            head,
            settled: row.settled,
            base_observed: row.observed.is_some(),
            base: row.observed.unwrap_or(base),
        })
    }
    pub fn get(&mut self, id: &str) -> Result<Record> {
        let r = load(&self.state.connection, id)?;
        if let Some(receipt) = &r.receipt {
            if &self.checked_receipt(&r)? != receipt {
                return Err("logical receipt changed".into());
            }
            let f = file_row(&self.state.connection, &r.input.volume, &r.input.node)?;
            if !ancestor(
                &self.state.connection,
                &f.binding,
                r.generation.ok_or("settled generation missing")?,
                f.settled.ok_or("logical settlement marker missing")?,
            )? {
                return Err("logical receipt is outside settlement lineage".into());
            }
        }
        Ok(r)
    }
    pub fn begin(&mut self, input: Input) -> Result<Record> {
        self.begin_inner(input, |_| Ok(()))
    }
    fn begin_inner(
        &mut self,
        input: Input,
        mut hook: impl FnMut(Point) -> Result<()>,
    ) -> Result<Record> {
        input.validate()?;
        let exists: bool = self.state.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM logical_edits WHERE operation_id=?)",
            [&input.id],
            |r| r.get(0),
        )?;
        let mut record = if exists {
            let r = self.get(&input.id)?;
            if r.input != input {
                return Err("logical operation input changed".into());
            }
            r
        } else {
            let file = self.file(&input.volume, &input.node)?;
            if file.head != input.parent {
                return Err(journal::Failure::LocalConflict.into());
            }
            let tx = self
                .state
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            let count: i64 =
                tx.query_row("SELECT count(*) FROM logical_edits", [], |r| r.get(0))?;
            if count >= LIMIT as i64 {
                return Err("logical edit limit reached".into());
            }
            let r = Record {
                input: input.clone(),
                generation: None,
                publication: None,
                receipt: None,
            };
            tx.execute(
                "INSERT INTO logical_edits VALUES(?1,?2,?3,?4,NULL,NULL,NULL,?5)",
                params![
                    input.id,
                    input.volume,
                    input.node,
                    serde_json::to_vec(&input)?,
                    record_hash(&r)?
                ],
            )?;
            tx.commit()?;
            hook(Point::InputPersisted)?;
            r
        };
        if record.generation.is_none() {
            let f = file_row(&self.state.connection, &input.volume, &input.node)?;
            let g = journal::Journal::new(self.state, self.options)?.begin_operation(
                &input.operation_key(&f.binding)?,
                &f.binding.target()?,
                journal::Base::Missing,
                input.parent,
                input.size,
            )?;
            hook(Point::BuilderCreated)?;
            record.generation = Some(g);
            let tx = self
                .state
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            save(&tx, &record)?;
            tx.commit()?;
            hook(Point::BuilderLinked)?;
        }
        Ok(record)
    }
    pub fn seal(&mut self, id: &str) -> Result<Record> {
        self.seal_inner(id, |_| Ok(()))
    }
    fn seal_inner(
        &mut self,
        id: &str,
        mut hook: impl FnMut(Point) -> Result<()>,
    ) -> Result<Record> {
        let r = self.get(id)?;
        let g = r
            .generation
            .ok_or("logical builder not linked; replay begin")?;
        journal::Journal::new(self.state, self.options)?.seal_linked(g, |db, info| {
            let current = load(db, id)?;
            if current.input != r.input || current.generation != Some(g) {
                return Err("logical input changed while sealing".into());
            }
            verify_bytes(db, &current, info)?;
            hook(Point::SealPending)
        })?;
        hook(Point::Sealed)?;
        self.get(id)
    }
    pub fn prepare(&mut self, id: &str, input: managed::Input) -> Result<Record> {
        self.prepare_inner(id, input, |_| Ok(()))
    }
    fn prepare_inner(
        &mut self,
        id: &str,
        input: managed::Input,
        mut hook: impl FnMut(Point) -> Result<()>,
    ) -> Result<Record> {
        let mut r = self.get(id)?;
        if let Some(old) = &r.publication {
            if old != &input {
                return Err("logical publication input changed".into());
            }
            return Ok(r);
        }
        let file = self.file(&r.input.volume, &r.input.node)?;
        let g = r.generation.ok_or("logical bytes not staged")?;
        let metadata = journal::load(&self.state.connection, g)?;
        if metadata.phase != journal::Phase::Sealed
            || !ancestor(
                &self.state.connection,
                &file.binding,
                g,
                file.head.ok_or("missing head")?,
            )?
        {
            return Err("logical generation is not sealed in current lineage".into());
        }
        let reserve = input.reservation()?;
        if input.source != file.binding.source
            || input.fingerprint != file.binding.fingerprint
            || reserve.volume != r.input.volume
            || reserve.node != r.input.node
            || reserve.store != file.binding.store
            || reserve
                .guard
                .as_ref()
                .is_some_and(|g| g.expected != namespace::payload_reference(&file.base.payload))
            || reserve.expected_revision != file.base.revision
            || reserve.size != r.input.size
            || reserve.sha256 != r.input.sha256
        {
            return Err("publication must bind exact logical bytes and base revision".into());
        }
        if file.settled.is_some_and(|s| s.value() >= g.value()) {
            return Err("logical generation already covered by settlement".into());
        }
        r.publication = Some(input);
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let publication = r.publication.as_ref().unwrap();
        for operation in [publication.id(), publication.publish_operation.as_str()] {
            tx.execute(
                "INSERT INTO logical_publication_links VALUES(?1,?2)",
                params![operation, id],
            )?;
        }
        save(&tx, &r)?;
        hook(Point::PublicationPending)?;
        tx.commit()?;
        hook(Point::PublicationPrepared)?;
        self.get(id)
    }
    fn checked_receipt(&mut self, r: &Record) -> Result<Receipt> {
        let input = r
            .publication
            .as_ref()
            .ok_or("logical edit lacks publication")?;
        let physical = managed::Journal::new(self.state, self.options)?.get(input.id())?;
        if &physical.input != input {
            return Err("logical/physical publication mismatch".into());
        }
        let g = physical.generation.ok_or("physical bytes not staged")?;
        let flush = remote::Journal::new(self.state, remote::Options::default())?
            .flush_receipt(g)?
            .ok_or("physical flush not acknowledged")?;
        let info = flush.info();
        if info.target != input.target()?
            || info.guard != remote::Guard::Missing
            || info.size != r.input.size
            || hex(&info.payload_sha256) != r.input.sha256
        {
            return Err("physical receipt differs from logical bytes".into());
        }
        physical
            .receipt
            .ok_or_else(|| "metadata publication not acknowledged".into())
    }
    pub fn acknowledge(&mut self, id: &str) -> Result<Record> {
        self.acknowledge_inner(id, |_| Ok(()))
    }
    fn acknowledge_inner(
        &mut self,
        id: &str,
        mut hook: impl FnMut(Point) -> Result<()>,
    ) -> Result<Record> {
        let mut r = self.get(id)?;
        let receipt = self.checked_receipt(&r)?;
        if r.receipt.is_some() {
            return Ok(r);
        }
        let mut row = file_row(&self.state.connection, &r.input.volume, &r.input.node)?;
        let g = r.generation.ok_or("logical generation missing")?;
        if row.settled.is_none_or(|s| s.value() < g.value()) {
            let file = self.file(&r.input.volume, &r.input.node)?;
            if receipt.node.revision <= file.base.revision {
                return Err("logical acknowledgment would regress metadata revision".into());
            }
            if let Some(s) = row.settled
                && !ancestor(&self.state.connection, &row.binding, s, g)?
            {
                return Err("settlement would change lineage".into());
            }
            row.observed = None;
            row.settled = Some(g);
            row.publication = Some(r.publication.as_ref().unwrap().id().into());
        } else if !ancestor(
            &self.state.connection,
            &row.binding,
            g,
            row.settled.unwrap(),
        )? {
            return Err("historical settlement outside lineage".into());
        }
        r.receipt = Some(receipt);
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        save(&tx, &r)?;
        let changed=tx.execute("UPDATE logical_files SET settled_generation=?1,settled_publication=?2,state_hash=?3,observed_base=?6 WHERE volume=?4 AND node=?5",params![row.settled.map(|g|g.value()),row.publication,hash(&row)?,row.binding.volume,row.binding.node.id,row.observed.as_ref().map(serde_json::to_vec).transpose()?])?;
        if changed != 1 {
            return Err("logical settlement was not durably updated".into());
        }
        hook(Point::SettlementPending)?;
        tx.commit()?;
        hook(Point::Settled)?;
        self.get(id)
    }
    pub fn pending(&mut self, after: Option<&str>, limit: u16) -> Result<Vec<Record>> {
        if !(1..=1000).contains(&limit) {
            return Err("invalid logical pending limit".into());
        }
        let ids = self
            .state
            .connection
            .prepare("SELECT operation_id FROM logical_edits ORDER BY operation_id LIMIT 4097")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.len() > LIMIT {
            return Err("logical edit limit or integrity failure".into());
        }
        let mut out = Vec::new();
        for id in ids {
            let r = self.get(&id)?;
            let file = self.file(&r.input.volume, &r.input.node)?;
            let covered = match (r.generation, file.settled) {
                (Some(g), Some(s)) => ancestor(&self.state.connection, &file.binding, g, s)?,
                _ => false,
            };
            if !covered && after.is_none_or(|a| id.as_str() > a) && out.len() < usize::from(limit) {
                out.push(r);
            }
        }
        Ok(out)
    }
}
#[cfg(test)]
mod tests;
