//! Root/subtree reachability from an authorized snapshot, never an upload or
//! data-health receipt. Root and child objects share the Page key namespace.
use super::*;

const PAGE_MAGIC: &[u8; 8] = b"S3DPREF1";
const PAGE_HEADER: usize = 50;
pub const MAX_PAGE_PROOF_BYTES: usize =
    PAGE_HEADER + MAX_ROOT_LEVEL as usize * (2 + manifest::MAX_PAGE_BYTES);

#[derive(Clone)]
pub struct PageProof {
    binding: [u8; 32],
    reference: Reference,
    page: VerifiedPage,
    path: Vec<Vec<u8>>,
}
impl fmt::Debug for PageProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthenticatedPageProof")
            .field("offset", &self.offset())
            .field("level", &self.level())
            .finish()
    }
}
impl PageProof {
    pub fn base_binding(&self) -> &[u8; 32] {
        &self.binding
    }
    pub fn reference(&self) -> &Reference {
        &self.reference
    }
    pub fn offset(&self) -> u64 {
        self.page.page().start()
    }
    pub fn length(&self) -> u64 {
        self.page.page().length()
    }
    pub fn level(&self) -> u8 {
        self.page.page().level()
    }
    pub fn page(&self) -> &VerifiedPage {
        &self.page
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut out =
            Vec::with_capacity(PAGE_HEADER + self.path.iter().map(|p| p.len() + 2).sum::<usize>());
        out.extend_from_slice(PAGE_MAGIC);
        out.extend_from_slice(&self.binding);
        out.extend_from_slice(&self.offset().to_le_bytes());
        out.push(self.level());
        out.push(self.path.len() as u8);
        for p in &self.path {
            out.extend_from_slice(&(p.len() as u16).to_le_bytes());
            out.extend_from_slice(p);
        }
        out
    }
}
impl BaseFile {
    fn root_page_reference(&self) -> Result<Reference> {
        let hint = self
            .0
            .payload
            .as_ref()
            .and_then(|p| p.reservation.manifest.as_ref())
            .ok_or(ReadFailure::UnsupportedContent)?;
        Ok(hint
            .root()
            .map_err(|_| ReadFailure::Integrity)?
            .page()
            .clone())
    }
    fn page_position(&self, offset: u64, level: u8) -> Result<()> {
        let Content::Manifest { root, .. } = &self.0.content else {
            return Err(ReadFailure::UnsupportedContent.into());
        };
        if level > root.page().level()
            || level > MAX_ROOT_LEVEL
            || (offset >= self.0.size && !(self.0.size == 0 && offset == 0))
        {
            return Err(ReadFailure::Bounds.into());
        }
        Ok(())
    }
    /// The already verified root needs no metadata fetch. Still shares active
    /// admission with reads/other proofs; does not confer a send permission.
    pub async fn prove_root(&self) -> Result<PageProof> {
        let Content::Manifest { root, .. } = &self.0.content else {
            return Err(ReadFailure::UnsupportedContent.into());
        };
        self.prove_page_containing(0, root.page().level()).await
    }
    /// At most two verified child pages, no data-object HEAD or GET. Returns the
    /// entire containing page, whose exact span must match intended reuse.
    pub async fn prove_page_containing(&self, offset: u64, level: u8) -> Result<PageProof> {
        self.page_position(offset, level)?;
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
            Box::pin(self.prove_page_inner(offset, level)),
        )
        .await
    }
    async fn prove_page_inner(&self, offset: u64, level: u8) -> Result<PageProof> {
        let Content::Manifest {
            root,
            handle,
            namespace,
            store,
        } = &self.0.content
        else {
            return Err(ReadFailure::UnsupportedContent.into());
        };
        let mut reference = self.root_page_reference()?;
        let mut page = root.clone();
        let mut path = Vec::new();
        for _ in 0..=MAX_ROOT_LEVEL {
            if page.page().level() == level {
                return Ok(PageProof {
                    binding: self.proof_binding()?,
                    reference,
                    page,
                    path,
                });
            }
            if path.len() >= MAX_ROOT_LEVEL as usize {
                return Err(ReadFailure::Bounds.into());
            }
            let found = page.page().lookup(offset).ok_or(ReadFailure::Integrity)?;
            let index = page
                .page()
                .entries()
                .iter()
                .position(|e| std::ptr::eq(e, found.entry))
                .ok_or(ReadFailure::Integrity)?;
            let Entry::Child { object, .. } = found.entry else {
                return Err(ReadFailure::Integrity.into());
            };
            reference = object.clone();
            let key = namespace.key(Role::Page, object)?;
            let child = Box::pin(handle.open_related(key.as_str())).await?;
            let size = child.metadata().size;
            if !(64..=manifest::MAX_PAGE_BYTES as u64).contains(&size) {
                return Err(ReadFailure::Integrity.into());
            }
            let body = Box::pin(child.read_range(0, size as usize)).await?;
            page = page.verify_child(index, &body)?;
            page_scope(&page, store)?;
            path.push(body);
        }
        Err(ReadFailure::Integrity.into())
    }
    /// Pure bounded revalidation against this version's exact base. No new authorized snapshot,
    /// old-root reopening, network authorization or byte-health proof is made.
    pub fn verify_page_proof(&self, encoded: &[u8]) -> Result<PageProof> {
        if encoded.len() < PAGE_HEADER
            || encoded.len() > MAX_PAGE_PROOF_BYTES
            || encoded.get(..8) != Some(PAGE_MAGIC.as_slice())
        {
            return Err(ReadFailure::Integrity.into());
        }
        let binding = self.proof_binding()?;
        if encoded[8..40] != binding {
            return Err(ReadFailure::Binding.into());
        }
        let start = u64::from_le_bytes(
            encoded[40..48]
                .try_into()
                .map_err(|_| ReadFailure::Integrity)?,
        );
        let level = encoded[48];
        let count = usize::from(encoded[49]);
        self.page_position(start, level)?;
        if count > MAX_ROOT_LEVEL as usize {
            return Err(ReadFailure::Bounds.into());
        }
        let Content::Manifest { root, store, .. } = &self.0.content else {
            return Err(ReadFailure::UnsupportedContent.into());
        };
        let mut reference = self.root_page_reference()?;
        let mut page = root.clone();
        let mut path = Vec::with_capacity(count);
        let mut at = PAGE_HEADER;
        for _ in 0..=MAX_ROOT_LEVEL {
            if page.page().level() == level {
                if page.page().start() != start || at != encoded.len() || path.len() != count {
                    return Err(ReadFailure::Integrity.into());
                }
                return Ok(PageProof {
                    binding,
                    reference,
                    page,
                    path,
                });
            }
            if path.len() >= count {
                return Err(ReadFailure::Integrity.into());
            }
            let found = page.page().lookup(start).ok_or(ReadFailure::Integrity)?;
            let index = page
                .page()
                .entries()
                .iter()
                .position(|e| std::ptr::eq(e, found.entry))
                .ok_or(ReadFailure::Integrity)?;
            let Entry::Child { object, .. } = found.entry else {
                return Err(ReadFailure::Integrity.into());
            };
            reference = object.clone();
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
            path.push(body.to_vec());
        }
        Err(ReadFailure::Integrity.into())
    }
}
