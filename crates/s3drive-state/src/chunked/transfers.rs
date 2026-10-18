//! Exact physical staging handoff; no network or logical-file acknowledgment.
use super::*;
use crate::remote;

/// Validated immutable object expectation. A mapped generation is local state,
/// not evidence of a successful upload. Page digests and raw body SHA differ.
#[derive(Clone, PartialEq, Eq)]
pub struct Transfer {
    target: Target,
    reference: Reference,
    role: Role,
    size: u64,
    payload_sha256: [u8; 32],
    operation: [u8; 32],
    generation: Option<GenerationId>,
}
impl fmt::Debug for Transfer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChunkObjectTransfer([private])")
    }
}
impl Transfer {
    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn reference(&self) -> &Reference {
        &self.reference
    }
    pub fn role(&self) -> Role {
        self.role
    }
    pub fn size(&self) -> u64 {
        self.size
    }
    pub fn payload_sha256(&self) -> &[u8; 32] {
        &self.payload_sha256
    }
    pub fn generation(&self) -> Option<GenerationId> {
        self.generation
    }
    fn matches(&self, g: &journal::Generation) -> bool {
        g.target == self.target
            && g.base == journal::Base::Missing
            && g.parent.is_none()
            && g.size == self.size
    }
    fn receipt_matches(&self, receipt: &remote::FlushReceipt) -> bool {
        let i = receipt.info();
        Some(i.generation) == self.generation
            && i.target == self.target
            && i.guard == remote::Guard::Missing
            && i.size == self.size
            && i.payload_sha256 == self.payload_sha256
            && i.phase == remote::Phase::Verified
            && receipt.covered_generations() == 1
    }
}
impl Journal<'_> {
    /// Metadata-only lookup. None means an explicit hole, never a missing row.
    /// Requires a complete validated graph; does not authorize a network send.
    pub fn transfer(&self, id: [u8; 32], level: i8, slot: u64) -> Result<Option<Transfer>> {
        self.state.verify_staging_storage()?;
        let i = self.info(id)?;
        if i.root.is_none() {
            return Err(Failure::Invalid.into());
        }
        self.transfer_inner(&i, level, slot)
    }
    fn transfer_inner(&self, i: &Info, level: i8, slot: u64) -> Result<Option<Transfer>> {
        let o = object(&self.state.connection, &i.input, level, slot)?;
        if o.hole {
            return Ok(None);
        }
        let reference = o.reference(&i.input)?;
        let role = if level == -1 { Role::Data } else { Role::Page };
        let key = i.input.namespace()?.key(role, &reference)?;
        let payload_sha256 = o.body.as_ref().map_or(o.sha, |b| Sha256::digest(b).into());
        let size = o.body.as_ref().map_or(o.length, |b| b.len() as u64);
        let operation = hash(
            b"s3drive-chunk-object-staging-v1\0",
            &(
                i.input.input_hash()?,
                i.generation.ok_or(Failure::Corrupt)?,
                i.manifest.ok_or(Failure::Corrupt)?,
                o.hash(&i.input)?,
                payload_sha256,
            ),
        )?;
        let mapped: Option<i64> = self
            .state
            .connection
            .query_row(
                "SELECT generation_id FROM staged_operations WHERE operation_key=?1",
                [operation.as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        let t = Transfer {
            target: Target::new(i.input.source, &i.input.fingerprint, key.as_str())?,
            reference,
            role,
            size,
            payload_sha256,
            operation,
            generation: mapped.map(GenerationId::from_database).transpose()?,
        };
        if let Some(g) = t.generation {
            let g = journal::load(&self.state.connection, g)?;
            if !t.matches(&g) {
                return Err(Failure::Conflict.into());
            }
            journal::validate_manifest(&self.state.connection, &g, false)?;
        }
        Ok(Some(t))
    }
    /// Direct dependencies must have exact, committed historical receipts.
    /// This is inductive publication ordering, not a current remote-existence
    /// guarantee or authority root capability. No S3 request is performed.
    pub fn require_dependencies(&mut self, id: [u8; 32], level: i8, slot: u64) -> Result<()> {
        let i = self.info(id)?;
        if i.root.is_none() {
            return Err(Failure::Invalid.into());
        }
        let current = object(&self.state.connection, &i.input, level, slot)?;
        if current.hole || level == -1 {
            return Ok(());
        }
        let count = i.object_slots(level - 1)?;
        let start = slot.checked_mul(128).ok_or(Failure::Invalid)?;
        let end = start.checked_add(128).ok_or(Failure::Invalid)?.min(count);
        let mut children = Vec::new();
        for child in start..end {
            if let Some(t) = self.transfer_inner(&i, level - 1, child)? {
                children.push(t);
            }
        }
        let j = remote::Journal::new(self.state, remote::Options::default())?;
        for child in children {
            let g = child.generation.ok_or(Failure::Conflict)?;
            let receipt = j.flush_receipt(g)?.ok_or(Failure::Conflict)?;
            if !child.receipt_matches(&receipt) {
                return Err(Failure::Corrupt.into());
            }
        }
        Ok(())
    }
    /// Persist one exact physical operation before materializing its body.
    /// Repetition resumes the same builder/sealed generation. Each journal
    /// boundary commits separately; errors and lost replies retain all data.
    /// The caller must check policy/binding before invoking this local mutation.
    pub fn materialize(
        &mut self,
        id: [u8; 32],
        level: i8,
        slot: u64,
        staging_options: journal::Options,
    ) -> Result<Option<Transfer>> {
        self.require_dependencies(id, level, slot)?;
        let Some(t) = self.transfer(id, level, slot)? else {
            return Ok(None);
        };
        if let Some(g) = t.generation {
            let g = journal::load(&self.state.connection, g)?;
            if g.phase == journal::Phase::Sealed {
                self.verify_physical(&t, g.id)?;
                return Ok(Some(t));
            }
        }
        // The same freshly checked buffer supplies the physical builder.
        let b = self.bytes(id, level, slot)?.ok_or(Failure::Corrupt)?;
        if b.target != t.target
            || b.reference != t.reference
            || b.role != t.role
            || b.bytes.len() as u64 != t.size
            || <[u8; 32]>::from(Sha256::digest(&b.bytes)) != t.payload_sha256
        {
            return Err(Failure::Corrupt.into());
        }
        let mut j = journal::Journal::new(self.state, staging_options)?;
        let g = j.begin_operation(
            &t.operation,
            &t.target,
            journal::Base::Missing,
            None,
            t.size,
        )?;
        j.append_at(g, 0, &b.bytes)?;
        j.seal(g)?;
        let result = self.transfer(id, level, slot)?.ok_or(Failure::Corrupt)?;
        if result.generation != Some(g) {
            return Err(Failure::Corrupt.into());
        }
        self.verify_physical(&result, g)?;
        Ok(Some(result))
    }
    fn verify_physical(&self, t: &Transfer, g: GenerationId) -> Result<()> {
        let bytes = journal::read_data(&self.state.connection, g, 0, t.size as usize, false)?;
        if bytes.len() as u64 != t.size
            || <[u8; 32]>::from(Sha256::digest(&bytes)) != t.payload_sha256
        {
            return Err(Failure::Corrupt.into());
        }
        Ok(())
    }
}
impl Planner {
    pub async fn transfer(&self, id: [u8; 32], level: i8, slot: u64) -> Result<Option<Transfer>> {
        self.call(move |j| j.transfer(id, level, slot)).await
    }
    pub async fn require_dependencies(&self, id: [u8; 32], level: i8, slot: u64) -> Result<()> {
        self.call(move |j| j.require_dependencies(id, level, slot))
            .await
    }
    pub async fn materialize(
        &self,
        id: [u8; 32],
        level: i8,
        slot: u64,
    ) -> Result<Option<Transfer>> {
        let options = self.staging_options;
        self.call(move |j| j.materialize(id, level, slot, options))
            .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup(s: &mut State) -> (Input, GenerationId) {
        let i = Input {
            id: [7; 32],
            volume: [1; 16],
            node: [2; 16],
            store: [3; 16],
            generation: [4; 16],
            root: "managed/".into(),
            source: [8; 32],
            fingerprint: "a".repeat(64),
            parent: None,
            size: 5,
            expected_content: None,
        };
        Journal::new(s, Options::default())
            .unwrap()
            .freeze(i.clone())
            .unwrap();
        let mut j = journal::Journal::new(s, journal::Options::default()).unwrap();
        let g = j
            .begin_operation(
                &i.staging_operation().unwrap(),
                &i.local_target().unwrap(),
                journal::Base::Missing,
                None,
                i.size,
            )
            .unwrap();
        j.append(g, b"bytes").unwrap();
        j.seal(g).unwrap();
        let mut p = Journal::new(s, Options::default()).unwrap();
        p.attach(i.id, g).unwrap();
        let mut info = p.info(i.id).unwrap();
        while info.root.is_none() {
            info = p.step(i.id, info.cursor).unwrap();
        }
        (i, g)
    }
    fn materialize(s: &mut State, i: &Input, l: i8) -> Transfer {
        Journal::new(s, Options::default())
            .unwrap()
            .materialize(i.id, l, 0, journal::Options::default())
            .unwrap()
            .unwrap()
    }
    fn acknowledge(s: &mut State, t: &Transfer) -> remote::FlushReceipt {
        let mut j = remote::Journal::new(s, remote::Options::default()).unwrap();
        let info = j.prepare(t.generation.unwrap()).unwrap();
        let _permit = j.claim(info.operation, &info.target).unwrap();
        j.verified(remote::Verification {
            operation: info.operation,
            target: info.target,
            payload_sha256: info.payload_sha256,
            size: info.size,
            revision: remote::Revision::new("\"local-test\"", None).unwrap(),
        })
        .unwrap();
        j.acknowledge(info.operation).unwrap()
    }
    #[test]
    fn restart_at_every_physical_staging_boundary_recovers_same_generation() {
        for boundary in 0..=3 {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("state");
            let mut s = State::open_managed(&path).unwrap();
            let (i, logical) = setup(&mut s);
            let t = Journal::new(&mut s, Options::default())
                .unwrap()
                .transfer(i.id, -1, 0)
                .unwrap()
                .unwrap();
            let mut mapped = None;
            if boundary > 0 {
                let mut j = journal::Journal::new(&mut s, journal::Options::default()).unwrap();
                let g = j
                    .begin_operation(
                        &t.operation,
                        &t.target,
                        journal::Base::Missing,
                        None,
                        t.size,
                    )
                    .unwrap();
                mapped = Some(g);
                if boundary > 1 {
                    j.append_at(g, 0, b"bytes").unwrap();
                }
                if boundary > 2 {
                    j.seal(g).unwrap();
                }
            }
            drop(s);
            let mut s = State::open_managed(&path).unwrap();
            let actual = materialize(&mut s, &i, -1);
            if let Some(g) = mapped {
                assert_eq!(actual.generation, Some(g));
            }
            assert_eq!(materialize(&mut s, &i, -1), actual);
            acknowledge(&mut s, &actual);
            let page = materialize(&mut s, &i, 0);
            assert_ne!(page.payload_sha256, *page.reference.sha256());
            acknowledge(&mut s, &page);
            let j = remote::Journal::new(&mut s, remote::Options::default()).unwrap();
            assert!(j.flush_receipt(logical).unwrap().is_none());
        }
    }
    #[test]
    fn altered_mapping_and_physical_media_are_refused_with_original_rows_retained() {
        for corrupt in 0..3 {
            let tmp = tempfile::tempdir().unwrap();
            let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
            let (i, _logical) = setup(&mut s);
            let t = materialize(&mut s, &i, -1);
            let physical = t.generation.unwrap();
            match corrupt {
                0 => {
                    let foreign = journal::Journal::new(&mut s, journal::Options::default())
                        .unwrap()
                        .begin(
                            &Target::new(i.source, &i.fingerprint, "foreign").unwrap(),
                            journal::Base::Missing,
                            None,
                            5,
                        )
                        .unwrap();
                    s.connection
                        .execute(
                            "UPDATE staged_operations SET generation_id=?1 WHERE operation_key=?2",
                            params![foreign.value(), t.operation.as_slice()],
                        )
                        .unwrap();
                }
                1 => {
                    s.connection
                        .execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])
                        .unwrap();
                }
                _ => {
                    s.connection
                        .execute(
                            "DELETE FROM staged_operations WHERE operation_key=?1",
                            [t.operation.as_slice()],
                        )
                        .unwrap();
                }
            }
            assert!(
                Journal::new(&mut s, Options::default())
                    .unwrap()
                    .materialize(i.id, -1, 0, journal::Options::default())
                    .is_err()
            );
            assert_eq!(
                s.connection
                    .query_row(
                        "SELECT count(*) FROM staged_generations WHERE generation_id=?1",
                        [physical.value()],
                        |r| r.get::<_, i64>(0)
                    )
                    .unwrap(),
                1
            );
            assert_eq!(
                s.connection
                    .query_row("SELECT count(*) FROM staged_generations", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                if corrupt == 0 { 3 } else { 2 }
            );
        }
    }
    #[test]
    fn generation_quota_stops_materialization_without_changing_logical_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let (i, logical) = setup(&mut s);
        let options = journal::Options {
            max_generations: 1,
            ..journal::Options::default()
        };
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .materialize(i.id, -1, 0, options)
                .is_err()
        );
        let j = journal::Journal::new(&mut s, journal::Options::default()).unwrap();
        assert_eq!(j.read(logical, 0, 10).unwrap(), b"bytes");
        let p = Journal::new(&mut s, Options::default()).unwrap();
        assert!(
            p.transfer(i.id, -1, 0)
                .unwrap()
                .unwrap()
                .generation
                .is_none()
        );
    }
    #[test]
    fn parent_dependencies_refuse_deleted_receipt_or_changed_raw_hash() {
        for change in 0..2 {
            let tmp = tempfile::tempdir().unwrap();
            let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
            let (i, _) = setup(&mut s);
            let t = materialize(&mut s, &i, -1);
            acknowledge(&mut s, &t);
            if change == 0 {
                // Deliberate on-disk damage bypasses FK protection only in this test.
                s.connection
                    .pragma_update(None, "foreign_keys", false)
                    .unwrap();
                s.connection
                    .execute("DELETE FROM remote_flushes", [])
                    .unwrap();
            } else {
                s.connection
                    .execute("UPDATE remote_intents SET payload_sha256=zeroblob(32)", [])
                    .unwrap();
            }
            assert!(
                Journal::new(&mut s, Options::default())
                    .unwrap()
                    .materialize(i.id, 0, 0, journal::Options::default())
                    .is_err()
            );
            let p = Journal::new(&mut s, Options::default()).unwrap();
            assert!(
                p.transfer(i.id, 0, 0)
                    .unwrap()
                    .unwrap()
                    .generation
                    .is_none()
            );
        }
    }
    #[test]
    fn real_sqlite_full_rolls_back_new_physical_identity_and_resumes_exactly() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let (i, logical) = setup(&mut s);
        let before = Journal::new(&mut s, Options::default())
            .unwrap()
            .transfer(i.id, -1, 0)
            .unwrap()
            .unwrap();
        s.connection.execute_batch("CREATE TABLE pressure(data BLOB); CREATE TRIGGER pressure_object BEFORE INSERT ON staged_operations BEGIN INSERT INTO pressure VALUES(zeroblob(65536)); END").unwrap();
        let pages: i64 = s
            .connection
            .pragma_query_value(None, "page_count", |r| r.get(0))
            .unwrap();
        s.connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        loop {
            match s
                .connection
                .execute("INSERT INTO pressure VALUES(zeroblob(4096))", [])
            {
                Ok(_) => (),
                Err(e) => {
                    assert_eq!(e.sqlite_error_code(), Some(rusqlite::ErrorCode::DiskFull));
                    break;
                }
            }
        }
        let error = Journal::new(&mut s, Options::default())
            .unwrap()
            .materialize(i.id, -1, 0, journal::Options::default())
            .unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<rusqlite::Error>()
                .and_then(|e| e.sqlite_error_code()),
            Some(rusqlite::ErrorCode::DiskFull)
        );
        let after = Journal::new(&mut s, Options::default())
            .unwrap()
            .transfer(i.id, -1, 0)
            .unwrap()
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(
            s.connection
                .query_row("SELECT count(*) FROM staged_generations", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            journal::Journal::new(&mut s, journal::Options::default())
                .unwrap()
                .read(logical, 0, 10)
                .unwrap(),
            b"bytes"
        );
        s.connection
            .pragma_update(None, "max_page_count", 1000000)
            .unwrap();
        s.connection
            .execute_batch("DROP TRIGGER pressure_object")
            .unwrap();
        let t = materialize(&mut s, &i, -1);
        assert_eq!(t.operation, before.operation);
        assert!(t.generation.is_some());
    }
}
