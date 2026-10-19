//! One exact-view data candidate; no upload receipt, persisted tree or send permit.
use super::*;

enum Content {
    Inherited(ChunkProof),
    Hole,
    Bytes { bytes: Vec<u8>, sha256: [u8; 32] },
}

/// Read-only classification of an authenticated preparation result. An inherited
/// proof with no data reference establishes a base hole, not a missing object.
pub enum ChunkContent<'a> {
    Inherited(&'a ChunkProof),
    Hole,
    Bytes {
        bytes: &'a [u8],
        sha256: &'a [u8; 32],
    },
}

/// Bounded candidate from one immutable local view and its authorized base.
/// Inherited objects have reachability proofs, not current data-health or upload
/// receipts. Persistence/publication must revalidate provenance and exact view.
/// No public constructor or mutable bytes can adopt arbitrary references.
pub struct PreparedChunk {
    view: Id,
    binding: Binding,
    offset: u64,
    length: u64,
    content: Content,
}
impl fmt::Debug for PreparedChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ExactViewPreparedChunk([private])")
    }
}
impl PreparedChunk {
    pub(super) fn into_candidate(self) -> s3drive_state::sparse_views::preparation::Candidate {
        use s3drive_state::sparse_views::preparation::Candidate;
        match self.content {
            Content::Inherited(proof) => Candidate::InheritedChunk(proof.encode()),
            Content::Hole => Candidate::Bytes(vec![0; self.length as usize]),
            Content::Bytes { bytes, .. } => Candidate::Bytes(bytes),
        }
    }

    pub fn view_id(&self) -> Id {
        self.view
    }
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    pub fn offset(&self) -> u64 {
        self.offset
    }
    pub fn length(&self) -> u64 {
        self.length
    }
    pub fn content(&self) -> ChunkContent<'_> {
        match &self.content {
            Content::Inherited(proof) => ChunkContent::Inherited(proof),
            Content::Hole => ChunkContent::Hole,
            Content::Bytes { bytes, sha256 } => ChunkContent::Bytes { bytes, sha256 },
        }
    }
}
impl EditedFile {
    /// Prepare at most one MiB; unchanged complete chunks need no data GET.
    /// Every result is freshly authorized, including holes and all-local data.
    /// Partial-final length changes always materialize and verify old bytes.
    pub async fn prepare_chunk(&self, chunk: u64) -> Result<PreparedChunk> {
        let offset = chunk
            .checked_mul(manifest::CHUNK_BYTES)
            .filter(|offset| *offset < self.size())
            .ok_or(ReadFailure::Bounds)?;
        let length = manifest::CHUNK_BYTES.min(self.size() - offset);
        let _active = self
            .base
            .0
            .reader
            .0
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        bounded(
            self.base.0.reader.0.options,
            Box::pin(self.prepare_inner(chunk, offset, length)),
        )
        .await
    }
    pub(super) async fn prepare_inner(
        &self,
        chunk: u64,
        offset: u64,
        length: u64,
    ) -> Result<PreparedChunk> {
        // Full local ledger and dirty-block verification ends before network I/O.
        let pieces = self.view.read(offset, length as usize).await?;
        Box::pin(self.base.authorize_sparse()).await?;
        self.prepare_material(chunk, offset, length, pieces).await
    }
    async fn prepare_material(
        &self,
        chunk: u64,
        offset: u64,
        length: u64,
        pieces: Vec<Piece>,
    ) -> Result<PreparedChunk> {
        let unchanged = matches!(pieces.as_slice(), [Piece::Base { offset: start, length: size }]
            if *start == offset && *size == length)
            && offset < self.base.size()
            && length == manifest::CHUNK_BYTES.min(self.base.size() - offset);
        let content = if unchanged {
            // The family permit/deadline already covers proof traversal; do not
            // acquire a nested reader slot or reset the operation deadline.
            let proof = Box::pin(self.base.prove_chunk_inner(chunk, offset)).await?;
            if proof.offset() != offset || proof.length() != length {
                return Err(ReadFailure::Integrity.into());
            }
            Content::Inherited(proof)
        } else {
            let bytes = Box::pin(self.compose(pieces, length as usize)).await?;
            if bytes.iter().all(|b| *b == 0) {
                Content::Hole
            } else {
                let sha256 = Sha256::digest(&bytes).into();
                Content::Bytes { bytes, sha256 }
            }
        };
        Ok(PreparedChunk {
            view: self.view.id(),
            binding: self.view.binding().clone(),
            offset,
            length,
            content,
        })
    }
}

// Only the private, freshly authorized exact-preparation scope can use this
// path without repeating history/root authorization for each page child.
impl super::planning::DependencySession<'_> {
    pub(super) async fn prepare_chunk(
        &self,
        chunk: u64,
        offset: u64,
        length: u64,
    ) -> Result<PreparedChunk> {
        let edited = self.edited();
        let pieces = edited.view.read(offset, length as usize).await?;
        edited.prepare_material(chunk, offset, length, pieces).await
    }
}
