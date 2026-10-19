//! Offline reachability from the frozen root context. No fresh authority or S3
//! authentication. Root context must be checked again by the managed controller.
use super::*;
const CHUNK: &[u8; 8] = b"S3DCREF1";
const PAGE: &[u8; 8] = b"S3DPREF1";
const MAX_DEPTH: usize = 2;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Witness {
    pub header: Vec<u8>,
    pub pages: Vec<Id>,
}
pub(super) struct Resolved {
    pub level: i8,
    pub offset: u64,
    pub length: u64,
    pub reference: Option<Reference>,
}
pub(super) fn scope(page: &VerifiedPage, store: &[u8; 16]) -> Result<()> {
    for entry in page.page().entries() {
        if let Entry::Data { object, .. } | Entry::Child { object, .. } = entry
            && object.store() != store
        {
            return Err(Failure::Corrupt.into());
        }
    }
    Ok(())
}
fn header(raw: &[u8]) -> Result<(bool, usize, usize)> {
    if raw.get(..8) == Some(CHUNK.as_slice()) && raw.len() >= 49 {
        Ok((false, 49, usize::from(raw[48])))
    } else if raw.get(..8) == Some(PAGE.as_slice()) && raw.len() >= 50 {
        Ok((true, 50, usize::from(raw[49])))
    } else {
        Err(Failure::Corrupt.into())
    }
}
impl Witness {
    pub fn admit(raw: &[u8], page: bool, blobs: &mut BTreeMap<Id, Vec<u8>>) -> Result<Self> {
        let (kind, h, count) = header(raw)?;
        if kind != page
            || count > MAX_DEPTH
            || raw.len() > 50 + MAX_DEPTH * (2 + manifest::MAX_PAGE_BYTES)
        {
            return Err(Failure::Bounds.into());
        }
        let mut at = h;
        let mut pages = Vec::new();
        for _ in 0..count {
            let n = u16::from_le_bytes(
                raw.get(at..at + 2)
                    .ok_or(Failure::Corrupt)?
                    .try_into()
                    .map_err(|_| Failure::Corrupt)?,
            ) as usize;
            at += 2;
            if !(64..=manifest::MAX_PAGE_BYTES).contains(&n) {
                return Err(Failure::Corrupt.into());
            }
            let bytes = raw.get(at..at + n).ok_or(Failure::Corrupt)?.to_vec();
            at += n;
            let id = digest(&bytes);
            if let Some(old) = blobs.get(&id) {
                if old != &bytes {
                    return Err(Failure::Corrupt.into());
                }
            } else {
                blobs.insert(id, bytes);
            }
            pages.push(id);
        }
        if at != raw.len() {
            return Err(Failure::Corrupt.into());
        }
        Ok(Self {
            header: raw[..h].to_vec(),
            pages,
        })
    }
    pub fn resolve(
        &self,
        input: &Input,
        view: &View,
        blobs: &BTreeMap<Id, Vec<u8>>,
    ) -> Result<Resolved> {
        let (is_page, h, count) = header(&self.header)?;
        if self.header.len() != h || count != self.pages.len() || count > MAX_DEPTH {
            return Err(Failure::Corrupt.into());
        }
        let base = view.binding().base()?;
        let mut hash = Sha256::new();
        hash.update(b"s3drive-authenticated-chunk-reference-v1\0");
        hash.update(base.payload_reference().as_bytes());
        if self.header[8..40] != <[u8; 32]>::from(hash.finalize()) {
            return Err(Failure::Corrupt.into());
        }
        let position = u64::from_le_bytes(
            self.header[40..48]
                .try_into()
                .map_err(|_| Failure::Corrupt)?,
        );
        let offset = if is_page {
            position
        } else {
            position
                .checked_mul(manifest::CHUNK_BYTES)
                .ok_or(Failure::Corrupt)?
        };
        let base_size = view.binding().base_size();
        if offset >= base_size && !(is_page && offset == 0 && base_size == 0) {
            return Err(Failure::Corrupt.into());
        }
        let mut page = input.validate(view, blobs)?;
        let mut reference = RootDescriptor::decode(&input.base_root)?.page().clone();
        let wanted = if is_page { self.header[48] } else { 0 };
        if wanted > page.page().level() || wanted > 2 {
            return Err(Failure::Corrupt.into());
        }
        let mut at = 0;
        loop {
            if is_page && page.page().level() == wanted {
                if page.page().start() != offset || at != count {
                    return Err(Failure::Corrupt.into());
                }
                return Ok(Resolved {
                    level: wanted as i8,
                    offset,
                    length: page.page().length(),
                    reference: Some(reference),
                });
            }
            let found = page.page().lookup(offset).ok_or(Failure::Corrupt)?;
            let index = page
                .page()
                .entries()
                .iter()
                .position(|e| std::ptr::eq(e, found.entry))
                .ok_or(Failure::Corrupt)?;
            if let Entry::Child { object, .. } = found.entry {
                if at >= count {
                    return Err(Failure::Corrupt.into());
                }
                reference = object.clone();
                let bytes = blobs.get(&self.pages[at]).ok_or(Failure::Corrupt)?;
                if !(64..=manifest::MAX_PAGE_BYTES).contains(&bytes.len()) {
                    return Err(Failure::Corrupt.into());
                }
                page = page.verify_child(index, bytes)?;
                scope(&page, view.binding().store())?;
                at += 1;
            } else {
                if is_page || at != count {
                    return Err(Failure::Corrupt.into());
                }
                let length = manifest::CHUNK_BYTES.min(base_size - offset);
                let reference = match found.entry {
                    Entry::Hole { length: span } if *span - found.offset_in_entry >= length => None,
                    Entry::Data {
                        length: span,
                        object,
                    } if found.offset_in_entry == 0 && *span == length => Some(object.clone()),
                    _ => return Err(Failure::Corrupt.into()),
                };
                return Ok(Resolved {
                    level: -1,
                    offset,
                    length,
                    reference,
                });
            }
        }
    }
}
