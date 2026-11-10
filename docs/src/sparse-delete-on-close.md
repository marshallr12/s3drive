# Delete-on-close on the version 8 writer (proposal)

**Status: proposal, 2026-11-09. Nothing here is implemented or accepted.** The
adapter's refusal of delete-on-close on version 8 stays in place until the
acceptance criteria below pass. The owner asked for this design after the
physical Office rerun failed; see
[the run4 results](wasabi-physical-windows-plan.md#office-rerun-on-version-8-run4-approved-2026-11-09).

## Why it is needed

On 2026-11-09 Word and Excel both failed their first save on a version 8
`component-retention-v1` drive with "write protected". A `CreateFileW` probe
showed that a create with `FILE_FLAG_DELETE_ON_CLOSE` fails with
`ERROR_WRITE_PROTECT` while the same create without the flag succeeds. That
Office's failing call is such a create is inferred from the probe and the
identical error; no call trace of Word or Excel was taken.

Version 7 supports the flag but keeps the per-write cost that made a Word save
unusable, and [write coalescing](write-coalescing.md) applies to version 8
only. Office therefore needs the flag on version 8.

## What exists today (read from the source, 2026-11-09)

- **The refusal.** `crates/s3drive-winfsp/src/windows.rs`, `open` and `create`,
  return `STATUS_MEDIA_WRITE_PROTECTED` when the request carries
  `FILE_DELETE_ON_CLOSE` and `native_lifecycle_enabled()` is false. Only the
  version 7 writer reports true.
- **Version 7's mechanism.** A delete-on-close open is recorded in the open
  journal with a frozen removal request (`PrepareRemoval`, or
  `CreateDeferredEntry` for a create), so the obligation to delete survives a
  crash. `create` goes through `create_native_deferred`.
- **Version 8 already deletes open files.** Its native object goes through the
  same disposition controller, with `set_delete` and `cleanup(flags)` wired
  (`chunk_writes/native.rs`). Unlink and replace of open, dirty files pass on
  real WinFsp and FUSE mounts (`native_lifecycle_actual_`,
  `component_retention_actual_`).
- **Version 8 state excludes delete-on-close opens on purpose.** Every sparse
  native open is recorded with `delete_on_close: false`, and
  `sparse_views/preparation/native.rs` treats a retained open that carries
  `delete_on_close`, an admission or a creation as corrupt.
  [Deferred manifest integration](deferred-manifest-integration.md) lists the
  version 8 deferred and delete-on-close callbacks as unsupported until their
  recovery gates pass.
- **How the driver delivers it.** WinFsp does not call `SetDelete` for a
  delete-on-close open. The request shows up only as the delete flag on a
  cleanup callback, which cannot return an error, and that cleanup need not be
  on the handle that carried the flag. Recorded with sources in
  [the delete plan](windows-delete-plan.md) and
  [the disposition reference](windows-disposition-reference.md) (WinFsp 2.1,
  checked 2026-10-17, not rechecked for this proposal).

So the controller's deleting cleanup currently expects a removal that an
earlier `SetDelete` armed. A delete-on-close cleanup arrives without one.

## Options

### A. Delete at cleanup (recommended)

Accept the flag on version 8 and record nothing extra at open. A create with
the flag is an ordinary create. When a cleanup arrives with the delete flag and
no removal is armed for the node, the engine arms one and then runs the
existing deleting cleanup. That is the unlink-while-open path version 8 already
has.

- **Benefit.** Small. It reuses an accepted path and needs no schema, wire or
  journal format change. The sparse state layer keeps seeing only ordinary
  opens.
- **Cost.** The obligation is not durable. If the mount process dies between
  the create and the cleanup, the file stays in the namespace and is visible
  after remount. Nothing is lost; a temporary file is left over. This is
  believed to match NTFS, where delete-on-close is also carried out at handle
  cleanup (unverified; to be checked against the existing NTFS reference
  probes before acceptance).
- **Cost.** Cleanup cannot report failure. If arming or deleting fails (work
  slots busy, authority unreachable), the file stays, with a line on standard
  error. Again a leftover file, not lost data.

### B. Durable obligation at open

Mirror version 7: admit the open with a frozen removal request, add deferred
creation to the chunk writer, and teach the sparse state layer to accept and
recover such opens.

- **Benefit.** A crash cannot leave the temporary file behind.
- **Cost.** A schema and format change in the sparse state layer, new crash,
  disk-full and recovery gates, and deferred creation for chunk files. This is
  the remaining "native v8 deferred/delete-on-close" row of the deferred
  manifest plan: several independently green phases.

**Recommendation: A now, B only if leftover temporary files after a crash prove
to matter.** A does not block B later.

## Proposed phases for option A

Each phase is independently green and keeps the refusal until phase 2 passes.

1. **Engine.** In the sparse native object's cleanup, when the delete flag is
   set and the node has no armed removal, arm it before the controller's
   cleanup, inside the same order locks. Shared-engine tests, loopback only:
   - cleanup with the delete flag and no prior `SetDelete` removes the file;
   - the same with a second handle still open: the name goes, the other handle
     keeps reading its bytes until it closes;
   - the same with buffered (coalesced) bytes on the closing handle;
   - a forced failure while arming leaves the file present and readable, and
     is reported on standard error;
   - an explicit `SetDelete` followed by a deleting cleanup behaves as before.
2. **Adapter.** Replace the `native_lifecycle_enabled()` test with a capability
   that is also true for the version 8 native writer; keep version 8 creates
   ordinary. A delete-on-close open that would get no native object stays
   refused. Directories with the flag stay refused on version 8. Real WinFsp
   tests through `CreateFileW`:
   - create with the flag, write, close: the name is gone, and gone after a
     cold remount;
   - open an existing file with the flag and close it;
   - two handles, the flagged one closing first;
   - kill the mount between create and close: after remount the file is
     present and reads back what was journaled (the documented difference);
   - the existing lifecycle, retention and desktop suites unchanged.
3. **Physical rerun.** Word and Excel create, save, edit-save, Save As, close
   and reopen on a fresh version 8 drive, under a separately approved provider
   bound. Record the time and the PUT count of each save.

Hosted Linux and Windows CI must pass on the exact source of phases 1 and 2.
FUSE has no delete-on-close; its paths must be unchanged.

## Open questions to settle in phase 1, before code

- **Which opens carry a native object.** A version 8 read-only open of a clean,
  published file appears to get none (`lib.rs`, the `Opened::Chunks` path), and
  `set_delete` on such a handle returns "not supported". Whether a
  delete-on-close open with only `DELETE` access reaches that path has not
  been traced.
- **Cleanup order with POSIX unlink enabled.** The recorded driver facts
  describe the non-POSIX mode. Version 8 advertises POSIX unlink and rename;
  whether the deleting cleanup then arrives on the flagged handle or on the
  last one has not been checked.
- **Arming twice.** Whether arming when another handle already armed and then
  cleared a removal behaves correctly depends on the controller's repeated
  true/false handling, which is tested for `SetDelete` but not for this caller.
- **Office's actual calls.** A Process Monitor trace of one Word save would
  confirm the cause and show what else Office asks for. It is cheap and can be
  taken before phase 1.

## Known costs that this does not remove

- On 2026-11-09 one empty file created, closed and deleted on the run4 drive
  cost the mount about 7 CPU-seconds over 20 s and 3 MB of write-ahead log.
  An Office save uses several temporary files, so a save may still take tens
  of seconds. This needs its own measurement once saves complete.
- A temporary file that is written and then deleted may still be journaled and
  published before it is removed. Skipping that work is a possible later
  optimization, not part of this proposal.
- Scattered small writes and the journal's view and dirty-byte limits are
  unchanged ([write coalescing](write-coalescing.md)).
