//! Snapshot-bound reachability only. No data verification, upload receipt,
//! persistent ownership decision, or arbitrary reference constructor.
use super::*;
use s3drive_core::manifest::Reference;

const MAGIC: &[u8; 8] = b"S3DCREF1";
const HEADER: usize = 49;
pub const MAX_CHUNK_PROOF_BYTES: usize =
    HEADER + MAX_ROOT_LEVEL as usize * (2 + manifest::MAX_PAGE_BYTES);

/// An authenticated reference or hole for one logical chunk of a pinned base.
/// A data reference proves reachability, not current availability/authorization
/// or successful verification of its bytes. The encoded witness must be
/// revalidated with an authorized File before reuse after restart.
#[derive(Clone)]
pub struct ChunkProof {
    binding: [u8; 32],
    chunk: u64,
    length: u64,
    reference: Option<Reference>,
    pages: Vec<Vec<u8>>,
}
impl fmt::Debug for ChunkProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthenticatedChunkProof")
            .field("chunk", &self.chunk)
            .field("hole", &self.reference.is_none())
            .finish()
    }
}
impl ChunkProof {
    pub fn base_binding(&self) -> &[u8; 32] {
        &self.binding
    }
    pub fn offset(&self) -> u64 {
        self.chunk * manifest::CHUNK_BYTES
    }
    pub fn length(&self) -> u64 {
        self.length
    }
    /// None is a proven hole, never a missing or denied object.
    pub fn reference(&self) -> Option<&Reference> {
        self.reference.as_ref()
    }
    pub fn encode(&self) -> Vec<u8> {
        encode(self.binding, self.chunk, &self.pages)
    }
}
fn encode(binding: [u8; 32], chunk: u64, pages: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + pages.iter().map(|p| p.len() + 2).sum::<usize>());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&binding);
    out.extend_from_slice(&chunk.to_le_bytes());
    out.push(pages.len() as u8);
    for page in pages {
        out.extend_from_slice(&(page.len() as u16).to_le_bytes());
        out.extend_from_slice(page);
    }
    out
}
impl File {
    fn proof_binding(&self) -> Result<[u8; 32]> {
        let reference = s3drive_authority::payload_reference(&self.0.node.payload)
            .ok_or(ReadFailure::Binding)?;
        let mut hash = Sha256::new();
        hash.update(b"s3drive-authenticated-chunk-reference-v1\0");
        hash.update(reference.as_bytes());
        Ok(hash.finalize().into())
    }
    fn proof_offset(&self, chunk: u64) -> Result<u64> {
        chunk
            .checked_mul(manifest::CHUNK_BYTES)
            .filter(|offset| *offset < self.0.size)
            .ok_or_else(|| ReadFailure::Bounds.into())
    }
    /// Fetch at most two bounded child pages, sharing the reader's operation
    /// permits and overall deadline. Never HEAD or GET a data object.
    pub async fn prove_chunk(&self, chunk: u64) -> Result<ChunkProof> {
        let offset = self.proof_offset(chunk)?;
        let _active = self
            .0
            .reader
            .0
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        bounded(
            self.0.reader.0.options,
            Box::pin(self.prove_chunk_inner(chunk, offset)),
        )
        .await
    }
    async fn prove_chunk_inner(&self, chunk: u64, offset: u64) -> Result<ChunkProof> {
        let Content::Manifest {
            root,
            handle,
            namespace,
            store,
        } = &self.0.content
        else {
            return Err(ReadFailure::UnsupportedContent.into());
        };
        let mut page = root.clone();
        let mut pages = Vec::new();
        for _ in 0..=MAX_ROOT_LEVEL {
            let found = page.page().lookup(offset).ok_or(ReadFailure::Integrity)?;
            let index = page
                .page()
                .entries()
                .iter()
                .position(|e| std::ptr::eq(e, found.entry))
                .ok_or(ReadFailure::Integrity)?;
            let Entry::Child { object, .. } = found.entry else {
                return self.verify_chunk_proof(&encode(self.proof_binding()?, chunk, &pages));
            };
            if pages.len() >= MAX_ROOT_LEVEL as usize {
                return Err(ReadFailure::Bounds.into());
            }
            let key = namespace.key(Role::Page, object)?;
            let child = Box::pin(handle.open_related(key.as_str())).await?;
            let size = child.metadata().size;
            if !(64..=manifest::MAX_PAGE_BYTES as u64).contains(&size) {
                return Err(ReadFailure::Integrity.into());
            }
            let body = Box::pin(child.read_range(0, size as usize)).await?;
            page = page.verify_child(index, &body)?;
            page_scope(&page, store)?;
            pages.push(body);
        }
        Err(ReadFailure::Integrity.into())
    }
    /// Pure bounded validation against this authorized snapshot's root. This
    /// does not authorize a new File, verify data bytes or create a send permit.
    pub fn verify_chunk_proof(&self, encoded: &[u8]) -> Result<ChunkProof> {
        if encoded.len() < HEADER
            || encoded.len() > MAX_CHUNK_PROOF_BYTES
            || encoded.get(..8) != Some(MAGIC.as_slice())
        {
            return Err(ReadFailure::Integrity.into());
        }
        let binding = self.proof_binding()?;
        if encoded[8..40] != binding {
            return Err(ReadFailure::Binding.into());
        }
        let chunk = u64::from_le_bytes(
            encoded[40..48]
                .try_into()
                .map_err(|_| ReadFailure::Integrity)?,
        );
        let offset = self.proof_offset(chunk)?;
        let count = usize::from(encoded[48]);
        if count > MAX_ROOT_LEVEL as usize {
            return Err(ReadFailure::Bounds.into());
        }
        let Content::Manifest { root, store, .. } = &self.0.content else {
            return Err(ReadFailure::UnsupportedContent.into());
        };
        let mut at = HEADER;
        let mut pages = Vec::with_capacity(count);
        let mut page = root.clone();
        for _ in 0..=MAX_ROOT_LEVEL {
            let found = page.page().lookup(offset).ok_or(ReadFailure::Integrity)?;
            let index = page
                .page()
                .entries()
                .iter()
                .position(|e| std::ptr::eq(e, found.entry))
                .ok_or(ReadFailure::Integrity)?;
            if matches!(found.entry, Entry::Child { .. }) {
                if pages.len() >= count {
                    return Err(ReadFailure::Integrity.into());
                }
                let size = u16::from_le_bytes(
                    encoded
                        .get(at..at + 2)
                        .ok_or(ReadFailure::Integrity)?
                        .try_into()
                        .map_err(|_| ReadFailure::Integrity)?,
                ) as usize;
                at += 2;
                if !(64..=manifest::MAX_PAGE_BYTES).contains(&size) {
                    return Err(ReadFailure::Integrity.into());
                }
                let body = encoded.get(at..at + size).ok_or(ReadFailure::Integrity)?;
                at += size;
                page = page.verify_child(index, body)?;
                page_scope(&page, store)?;
                pages.push(body.to_vec());
            } else {
                if at != encoded.len() || pages.len() != count {
                    return Err(ReadFailure::Integrity.into());
                }
                let length = manifest::CHUNK_BYTES.min(self.0.size - offset);
                let reference = match found.entry {
                    Entry::Hole { length: span } if *span - found.offset_in_entry >= length => None,
                    Entry::Data {
                        object,
                        length: span,
                    } if found.offset_in_entry == 0 && *span == length => Some(object.clone()),
                    _ => return Err(ReadFailure::Integrity.into()),
                };
                return Ok(ChunkProof {
                    binding,
                    chunk,
                    length,
                    reference,
                    pages,
                });
            }
        }
        Err(ReadFailure::Integrity.into())
    }
}
