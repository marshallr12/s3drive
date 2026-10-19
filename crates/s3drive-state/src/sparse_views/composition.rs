//! Metadata-only scope inspection and fresh namespace barriers.
use super::*;
use crate::namespace_client::{Control, Entry};
fn headers(db: &Connection) -> Result<Vec<(Id, Binding, Id)>> {
    let mut q=db.prepare("SELECT CASE WHEN length(target)=32 THEN target END,CASE WHEN typeof(binding)='blob' AND length(binding) BETWEEN 1 AND 4096 THEN binding END,CASE WHEN length(head)=32 THEN head END,CASE WHEN length(state_hash)=32 THEN state_hash END FROM sparse_view_files ORDER BY target LIMIT 257")?;
    let mut out = Vec::new();
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, Option<Vec<u8>>>(0)?,
            r.get::<_, Option<Vec<u8>>>(1)?,
            r.get::<_, Option<Vec<u8>>>(2)?,
            r.get::<_, Option<Vec<u8>>>(3)?,
        ))
    })? {
        let (target, raw, head, saved) = row?;
        let target: Id = target
            .ok_or(Failure::Corrupt)?
            .try_into()
            .map_err(|_| Failure::Corrupt)?;
        let binding: Binding = decode(&raw.ok_or(Failure::Corrupt)?)?;
        binding.validate()?;
        let head: Id = head
            .ok_or(Failure::Corrupt)?
            .try_into()
            .map_err(|_| Failure::Corrupt)?;
        if target != binding.target()?
            || saved.ok_or(Failure::Corrupt)? != hash(b"s3drive-sparse-binding-v1\0", &binding)?
            || out.len() >= HARD_FILES as usize
        {
            return Err(Failure::Corrupt.into());
        }
        out.push((target, binding, head));
    }
    Ok(out)
}
pub(crate) fn chunk_scope(db: &Connection, b: &crate::chunk_mount::Binding) -> Result<()> {
    for (_, binding, _) in headers(db)? {
        let base = binding.base()?;
        b.volume(base.volume())?;
        // The stable store route is validated in the composition registry; the
        // sparse binding retains its exact store/root/source/fingerprint.
        let store = b.store_for_sparse(
            binding.store(),
            binding.root(),
            binding.source(),
            binding.fingerprint(),
        )?;
        b.control(&Control {
            store,
            source: *binding.source(),
            fingerprint: binding.fingerprint().into(),
        })?;
    }
    Ok(())
}
pub(crate) fn namespace_guard(db: &Connection, volume: &str, entry: &Entry) -> Result<()> {
    let found = headers(db)?.into_iter().find(|(_, b, _)| {
        b.base()
            .is_ok_and(|base| base.volume() == volume && base.node() == entry.id)
    });
    let Some((target, binding, head)) = found else {
        return Ok(());
    };
    if entry.reference.as_deref() != Some(binding.base()?.payload_reference())
        || entry.placement.as_ref().is_none_or(|p| {
            p.id != binding
                .store()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                || p.root != binding.root()
        })
    {
        return Err("namespace reference conflicts with retained sparse base".into());
    }
    // Fresh effects require a fully validated clean head. This does not run
    // while replaying an already saved outcome, and never rewrites that head.
    let source = ledger::inspect(db)?;
    let file = source.files.get(&target).ok_or(Failure::Corrupt)?;
    let record = source.record(head)?;
    if file.head != head || !record.clean() {
        return Err("settle dirty sparse data before namespace mutation".into());
    }
    preparation::publication::namespace_pending(db, volume, &entry.id)?;
    Ok(())
}
