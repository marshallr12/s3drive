# Delete-on-close on the version 8 writer (proposal)

**Status: the owner chose option A on 2026-11-10. Phases 1 (engine) and 2
(adapter) are implemented and passed hosted Linux and Windows CI on their exact
commits. Phase 2 is unmerged and has no physical-machine or Office evidence.
Phase 3 (physical Office rerun) was approved by the owner on 2026-11-10 and
has not been run; its plan is
[run5](wasabi-physical-windows-plan.md#office-rerun-with-delete-on-close-run5-approved-2026-11-10).**
See [phase 2 as built](#phase-2-as-built-2026-11-10) and
[phase 1 as built](#phase-1-as-built-2026-11-10). The owner asked for this design after the
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

## Phase 1 as built (2026-11-10)

The arming is in the disposition controller's cleanup
(`crates/s3drive-managed/src/disposition.rs`, `arm_at_cleanup`), not in the
sparse native object as first sketched. When a cleanup carries the delete flag,
no removal is armed and the open is a native chunk open, the controller arms a
pending removal and continues into the existing deleting cleanup, all under the
controller's one lock. Whole-file (version 7) opens are unchanged: a deleting
cleanup with nothing armed is still a conflict there.

Two things differ from the proposal above:

- **A failed cleanup is retried, so the file is deleted later rather than
  kept.** A first attempt armed the removal from the native object before the
  cleanup. With the authority unreachable that left a recorded deleting cleanup
  that could never complete: Close failed and the next mount was refused.
  Inside the controller the recorded cleanup is re-driven by the recovery that
  already runs at Close and at mount, and it arms on the retry. While the
  authority is unreachable the file stays present and readable; the delete
  completes at Close once it is back. The "leftover file" outcome now applies
  only to a mount process that dies before the cleanup callback arrives.
- **No new request role.** The state layer admits callback requests only under
  a fixed list of roles, and a new one would be refused by an older binary
  reading the same state. The arm request therefore uses the existing "set"
  role under the cleanup's own callback identity. No schema, wire or journal
  format changed.

Shared-engine tests, loopback only, in both version 8 profiles
(`native_lifecycle_shared_deleting_cleanup_without_set_delete_removes_file`,
`component_retention_shared_deleting_cleanup_without_set_delete_removes_file`):

- one handle with buffered bytes, deleting cleanup with no `SetDelete`: the
  name is gone;
- two handles: the name is gone, the other handle reads its bytes until Close;
- authority unreachable during the cleanup: the cleanup reports an error, the
  file is present and readable, Close completes the delete once the authority
  is back;
- none of the three names exists after a cold remount.

The existing `native_lifecycle_shared_disposition_*` tests cover an explicit
`SetDelete` followed by a deleting cleanup and pass unchanged.

Local checks on MiniAir, 2026-11-10: formatting and clippy clean; the new
tests, the explicit-disposition tests and the whole `managed_publication`
suite (18 tests) pass on the final code. A full workspace run on an earlier
shape of the same change passed 1,154 tests and failed two:

- the whole-file controller test overflowed its debug-build stack, because
  the arming was then a branch inside the large cleanup function. Making it a
  separate boxed step fixed that; the test passes on the final code.
- `native_retention_runtime_reader_release_reopens_at_capacity` exceeded its
  900 s deadline, and did so again when run alone. It never calls cleanup, and
  its passing runs on 2026-11-09 took 726 to 898 s, so it is at its limit on
  this machine. Not passing locally; hosted CI decides.

The full workspace was not rerun locally after the final restructuring.

Hosted CI on `f8f5fa3` then failed one check, the Windows
`component_retention_actual_` step: the multiple-owners test hit the 120 s
worker bound in its sixth cycle, and the same test failed the same way
locally. The arming step ran a content flush on every deleting cleanup, also
for an explicit delete whose removal was already armed. It now inspects first
and flushes only when it has to arm. With that, the three
`component_retention_actual_` tests pass locally on real WinFsp (408 s, on a
day when this machine ran every call about a quarter slower than on
2026-11-09). One local run, so the cause is likely, not proven; the hosted
result on the fixed commit is the check.

Hosted CI on the fixed commit `e184cba` passed all 20 checks of
[PR 3](https://github.com/marshallr12/s3drive/pull/3) (runs 42785058671 and
42785051236, read from the check list on 2026-11-10; the logs were not
reviewed line by line).

Open questions, as far as phase 1 settles them:

- **Arming twice.** The controller arms only when nothing is armed, so an
  explicit `SetDelete` before the cleanup is left as it was. A removal that was
  armed and then cancelled leaves nothing armed and is armed again. Read from
  the source; the cancel-then-delete order has no test of its own.
- **Which opens carry a native object.** Confirmed from the source
  (`chunk_writes.rs`, `select_inner`): a read-only open of a clean, published
  file that has no local sparse state gets none. Phase 2 must keep the refusal
  for a flagged open on that path or route it through the writer.
- **Cleanup order with POSIX unlink enabled** and **Office's actual calls** are
  still open. Both need a real mount and belong to phase 2.

## Phase 2 as built (2026-11-10)

The adapter (`crates/s3drive-winfsp/src/windows.rs`, `open` and `create`) now
asks the engine `delete_on_close_enabled()`, which is true for the version 7
writer and for the version 8 native writer. Versions 1 to 6 and a version 8
mount without the native writer still refuse the flag.

- **Create of a file on version 8** is an ordinary create. The deferred
  creation path stays version 7 only.
- **Open of a file on version 8** is accepted only when the open carries a
  sparse native object. An open that gets none (a read-only open of a clean
  published file the writer does not track) is refused as before, in
  `native_file_object`.
- **Directories with the flag stay refused on version 8**, on both create and
  open.
- Nothing changed for FUSE, and no schema, wire or journal format changed.

Real WinFsp tests through `CreateFileW`, loopback only, in both version 8
profiles (`delete_on_close_actual_owned_unlinked_deletes_at_cleanup`,
`delete_on_close_actual_component_retention_deletes_at_cleanup`, in
`chunk_publication/native_delete_on_close.rs`), with their own Windows CI step:

- create with the flag, write, read back, close: the name is gone;
- open an existing file with the flag and close it: the name is gone;
- two handles, the flagged one closing first: the other handle reads its
  bytes, and the name is gone once it closes. Whether the name is still listed
  in between is printed, not asserted;
- a flagged directory create is refused and creates nothing;
- none of the three names is back after a cold remount;
- a flagged open of a published file after the cold remount: either refused
  with the file intact, or accepted and deleted. The outcome is printed;
- kill the mount process between a flagged create (written and flushed) and
  its close: after remount the file is present with the flushed bytes.

**Hosted evidence.** Commit `3ae3afd` passed all ten jobs of
[run 42839890487](https://github.com/marshallr12/s3drive/actions/runs/42839890487)
(2026-11-10), on standard Linux and Windows runners. The Windows job built the
adapter, passed clippy with warnings denied, and passed both new tests in
107 s, along with the unchanged lifecycle, retention, desktop, disposition and
workspace steps. The delete-on-close step's log was read; the other steps were
checked by their status only, not line by line.

The component-retention test printed:

```text
DELETE_ON_CLOSE create=deleted existing=deleted two_handles=deleted listed_between=true directory=refused clean_published=deleted killed_before_cleanup=present_exact cold_remount=absent
```

The owned-unlinked test passed the same assertions, but the harness did not
print its line in that run. Commit `6992084` makes it print; in
[run 42871136250](https://github.com/marshallr12/s3drive/actions/runs/42871136250)
(ten of ten jobs) both profiles printed that same line.

What the run settles, for the hosted WinFsp version and loopback backend only:

- **Cleanup order.** With two handles, the name is still listed after the
  flagged handle closes and goes when the last handle closes. The delete is
  carried out at the last cleanup, as on NTFS.
- **Which opens carry a native object.** A flagged read-only open of a
  published file after a cold remount was accepted and deleted the file, so
  that open did get a native object. The refusal for an open without one
  remains in the code but was not reached by any test.
- **Killed mount.** A mount process killed between a flagged create and its
  close leaves the file present with its flushed bytes after remount.

Not covered: Office's actual calls (untraced), a physical machine, a real
provider, and the cancel-then-delete order noted under phase 1. On the Linux
cloud workstation only formatting, clippy and the two phase 1 shared-engine
tests were run; the Windows-only code cannot be built there.

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
