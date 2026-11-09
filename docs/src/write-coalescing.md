# Write coalescing

Dated 2026-11-09. This page records an owner decision, the behavior it selects
and the local evidence for it. It closes the proposal made in
[Windows desktop application requests](windows-desktop-requests.md).

**Decision.** On 2026-11-09 the owner approved an in-memory write buffer above
the local journal, knowing that it changes what is durable when (see
[What is no longer durable](#what-is-no-longer-durable)). Before this change
Office-style saves failed on every available write profile.

**Status.** Implemented for the version 8 native path (every
`sparse-lifecycle` profile) and verified locally on MiniAir against loopback
fixtures. Hosted CI has not passed on it yet (see
[Hosted CI so far](#hosted-ci-so-far-pull-request-3-2026-11-09)), and Word and
Excel have not been rerun against it. No provider write was made for this work.

## Why

Every journal mutation loads and validates the whole retained local ledger,
before and after the change ([sparse patches](sparse-patches.md)). A save made
of hundreds of small writes therefore paid a cost that grew with each write,
and the 128th retained view failed with a disk-full error. The measurements are
in [Windows desktop application requests](windows-desktop-requests.md). The
validated journal is unchanged; the buffer only reduces how often it is asked.

## Behavior

Each open file has at most one buffered run: one contiguous range of bytes that
has been accepted from an application but not yet written to the journal.

- A write smaller than 1 MiB is merged into the run when it overlaps or touches
  it and the result still fits in 1 MiB. Later bytes replace earlier ones.
- A write that does not touch the run, or would grow it beyond 1 MiB, first
  sends the run to the journal as **one** mutation and then starts a new run.
- A write of 1 MiB or more, and an empty write, are never buffered. They are
  journaled before the call returns, after any run that precedes them.
- The run belongs to the handle that wrote it. A write through another handle
  on the same file journals the run first, so all handles see one order.
- File size, append position and size-limited (paging) writes use the size
  including the run. `stat` by path reports the same size.

Both limits are fixed in code (`crates/s3drive-fs/src/managed/chunk_writes/coalesce.rs`);
there is no configuration entry.

### Commit points

The run is written to the journal:

| When | Notes |
| --- | --- |
| Before any read of the file, through any handle | Reads always go through the validated journal reader |
| Before truncate or extend | |
| Before flush, and before `flush all` and unmount | Flush then publishes as before; a full journal is published first and the run retried |
| Before rename or delete of the file, and before it is replaced by a rename | These already publish first |
| At cleanup (Windows: the last user handle closes) | The handle stops buffering for good; later paging writes are journaled directly. A full journal is published first and the run retried |
| At Close/RELEASE, if the handle still owns a run or the file has kept bytes | See below |
| Between 1 and 2 s after the last write to the run | Idle timer |
| When another handle writes, or a write cannot be merged | |

**Close.** The journal refuses every write through a handle once that handle's
Close has been observed. A handle that reaches Close while it still owns a run,
or while the file has bytes kept from an earlier Close, therefore journals the
run first and only then lets Close be observed. For
every other handle, Close is observed at callback entry exactly as before. On
Windows the run is normally already empty, because cleanup comes first; this
path matters for FUSE RELEASE after a late mapped-page write and for callers
that close without a cleanup. What happens when the journal refuses the run at
this point is under [Failure reporting](#failure-reporting).

## What is no longer durable

Before this change, a write that had returned was in the local SQLite journal
and survived a kill of the mount process. Now up to 1 MiB of the most recent
small writes to a file exists only in the memory of the mount process until the
next commit point above. The idle timer fires between 1 and 2 s after the last
write to the run, so a file that keeps receiving adjacent small writes less
than a second apart can hold up to 1 MiB in memory for as long as that
continues; there is no separate age limit. If the process is killed or the
machine loses power before the commit point, those bytes are lost and the file
keeps its last journaled state. This is comparable to an ordinary
operating-system file cache. It does not change the
[flush contract](filesystem-contract.md): a successful flush still means
verified remote publication plus local acknowledgment.

Nothing is lost on close or on any path that reads the file back. A clean
unmount loses bytes only in the case the next section describes: the journal
still refuses them and the unmount reports it.

### Failure reporting

If the journal refuses a buffered run (for example a retained-history limit is
reached), the bytes stay in the buffer. They are never dropped while the mount
process lives. Second owner decision, 2026-11-09: bytes still refused at Close
are kept and retried, not discarded. The first implementation discarded them;
automated review of the pull request and the owner both rejected that.

**Making room.** Publishing a file frees room in the journal (measured below).
Flush, flush-all, unmount, cleanup and Close therefore do not stop at a full
journal: they publish what the journal already holds for the file and try the
buffered run once more. A write, a read, a truncate and the idle timer do not
publish; they report the refusal.

**Who sees a refusal.**

- A write, read or truncate that has to commit the run fails with the
  journal's error, on any handle of the file. An application can therefore see
  an error for bytes whose own write call had succeeded.
- A flush publishes the journaled state in every case. If the run is still
  refused after that, the flush returns the journal's error.
- The idle timer writes a failure to standard error. It tries again only after
  a later write has extended the run and it has gone idle again.
- Cleanup cannot return a status to the application on Windows. It reports the
  failure on standard error and the run stays buffered for Close.
- Publication that the disposition controller requests during another
  handle's cleanup, Close or delete does not fail because of buffered bytes.

**Kept after Close.** A closing handle can never write again. If its run is
still refused after making room, the run is detached from the handle and the
mount keeps it in memory with its file. Close then completes normally. From
then on:

- the file's size, by path and by handle, still includes the kept bytes;
- any handle of the file that is open or opened later journals them at its
  next commit point. A handle that closes takes the kept run over and journals
  it first, so reopening and closing the file is enough once the journal
  accepts the bytes;
- until then a read through a handle reports the journal's error, and so does
  a flush through a handle;
- flush-all and unmount publish every journaled file first and then return a
  conflict while kept bytes remain that no open handle can journal. They do
  not open a handle of their own to retry;
- rename and delete by path are refused with the same conflict while the file
  has kept bytes and no open handle.

The kept bytes are lost only when the mount process exits. An unmount that
reported the conflict is such an exit. If the file is deleted while its bytes
are still refused, they stay in memory until unmount.

On FUSE the application normally sees a refusal before Close, because
`close(2)` sends FLUSH and flush reports it. On Windows an application that
writes a final small piece and only closes sees no error; the bytes are either
journaled by cleanup or Close after making room, or kept as described.

## Not changed

- **Scattered small writes.** A workload that writes small pieces at
  non-adjacent offsets (a database file, for example) still costs one journal
  mutation per piece and still meets the retained-view limit. Writable
  databases remain unsupported ([filesystem contract](filesystem-contract.md)).
- **Journal limits.** The journal's retained-view and unpublished-byte limits
  remain. Without coalescing the journal refused write 127 of a sequential
  4 KiB series; with it, the journal refused after 91 scattered single-byte
  writes. Which limit each case reached was not determined. A sequential copy
  now uses about one view per MiB instead of one per write, so a large enough
  file written without a flush still reaches a limit; where was not measured.
- **Per-mutation cost.** Each journal mutation still validates the whole
  retained ledger. The buffer makes mutations rare; it does not make them cheap.
- **Other write paths.** Version 8 without a `sparse-lifecycle`, and versions 6
  and 7, do not use the buffer.

## Evidence (local, 2026-11-09)

All runs: MiniAir (Windows 11 Pro 10.0.26300, WinFsp 2.1), loopback provider
and authority fixtures, version 8 `component-retention-v1`. Hosted Linux and
Windows CI on this exact source is the pending acceptance gate.

### Small-write timing (shared engine, release build)

`component_retention_shared_save_pattern_probe`, an ignored diagnostic that
drives the shared engine without a driver. The "before" column repeats the
measurements in
[Windows desktop application requests](windows-desktop-requests.md), taken the
same day on the same machine and build profile.

| Pattern | Before | With coalescing |
| --- | --- | --- |
| 128 writes of 4 KiB, no flush | 19.4 s for the first 112 writes; write 127 failed with a disk-full error | All 128 writes in 21 ms; close with publication 0.9 s |
| 120 writes of 4 KiB, flush every 30 | 25.7 s; writes up to about 270 ms each; about 2.5 s per flush | 4.5 s in total; writes about 0.1 ms each; the last flush 1.4 s |
| 1,024 writes of 4 KiB (4 MiB), no flush | Not possible: ceiling at write 127 | Writes 0.34 s in total; close with publication 5.5 s |
| 16 writes of 256 KiB (4 MiB), no flush | Writes 1.6 s in total; close with publication 4.6 s | Writes 0.27 s in total; close with publication 5.2 s |

One run each; this is not a benchmark. Flush and close time is publication to
the loopback provider, which this change does not reduce.

### Shared-engine behavior

`component_retention_shared_write_coalescing_commit_points`
(`crates/s3drive-cli/tests/chunk_publication/native_coalescing.rs`) passed in
86 s on the final source (debug build; 104 s in an earlier run, 43 s before the
extended-run check was added). It runs in the existing
`component_retention_shared_` CI step on Linux and Windows. It checks that:

- 300 sequential 4 KiB writes add one journal view before any commit point, the
  size by handle and by path includes the buffered bytes, and the bytes read
  back exactly after a header rewrite;
- a handle closed directly after its writes keeps them;
- two handles on one file see each other's writes in order, and truncate,
  append and a size-limited write use the buffered size;
- a buffered write beyond the end of the file leaves a zero-filled gap;
- a write of at least 1 MiB after a small one keeps their order, and flush
  publishes buffered bytes;
- a small write adds no journal block at once, and one is present 2.5 s later
  (idle timer);
- a write that extends a run while its idle timer is waiting for the
  mount-wide lock does not end the timer: the extended run is journaled
  within 2.5 s. Automated review of the pull request found this defect; the
  check failed before the fix. It needs a flush of a second file to hold the
  lock past the idle period (5.6 s in the local run), and is an ordinary
  idle-timer check on a machine where the flush is faster than that;
- after a clean stop and a cold restart every file reads back exactly;
- after an abrupt stop, bytes committed by a read are in the journal, a later
  buffered write is not, and cold start refuses the retained dirty epoch as
  before.

Six unit tests cover merging, including a 2,000-step pseudo-random comparison
with the same writes applied directly, and the rule for what is buffered.

`write_coalescing_refused_run_is_reported_then_saved_at_close` forces the
journal to refuse a run by writing single bytes two apart, so that every write
commits the one before it. The journal refused after 91 accepted writes with a
disk-full error. The test then checks that the size still includes the
buffered byte, that a read reports the same error, that Close succeeds, that
the size by path still includes the byte afterwards, and that a reopened handle
reads every accepted byte, including the one that was buffered at the refusal.
It then writes again and reads everything back after a cold restart. It passed
in 21 s (release build). In a debug build it exceeds the
harness's three-minute limit, because Close now publishes before it retries.
It is ignored by default because it is slow, and has its own release-build CI
step.

`refused_bytes_survive_close_until_a_later_handle_journals_them`
(`crates/s3drive-fs/src/managed/chunk_writes/kept_tests.rs`) covers bytes that
stay refused after making room. Publication frees room, so a real journal
cannot be held in that state through the public interface; the test uses a
switch that exists only in the crate's own test build and makes the journal
refuse every buffered run. It checks that Close succeeds and keeps the bytes,
that the size by path and by handle counts them, that flush-all with no open
handle returns a conflict, that a read through a new handle reports the
refusal, that a second refused Close keeps them again, that the next Close
journals them once the switch is off, that unmount with kept bytes returns a
conflict, and that after a cold restart the first file reads back whole and
the second holds only its journaled bytes. It passed in 15 s and runs in the
workspace test step.

Does publication free journal room? In a one-off local run, 40 scattered
single-byte writes, one flush and then at least 60 more were accepted before
the harness time limit ended the run: at least 100 in total, against 91 without
a flush. The cost of each write kept rising after the flush (about 1.2 s
rising to 2.7 s in a debug build), so publication does not clear the retained
history that every mutation validates. Where the limit lies with repeated
flushes was not measured.

### Hosted CI so far (pull request 3, 2026-11-09)

No commit of this work has a complete passing hosted run yet.

- `54eff16`: the Linux job stopped at Clippy (a constant read only by the
  Windows adapter was dead code in the Linux test build). Fixed in `af30ad7`,
  where Clippy passed on Linux and Windows.
- `a63637d`: of the two runs of that commit, one failed in the Linux job
  `extended-retention-soak`, step "Additional actual 1000-save and 100-remount
  release soak" (real FUSE mount, `linked-retention-v1`). After the unmount that
  followed saves 801 to 810, the private database still held one row in
  `windows_open_journal`: one open record had not been closed. The other run
  of the same commit passed that step, as did both runs of `54eff16` and of
  `af30ad7`. It is the first failure of that job in the 30 runs GitHub lists.
- **Cause not established.** The log names no failing callback. One change of
  this work fits a rare race and has been removed: the idle timer held its
  file object strongly, so the object could outlive its last handle by up to a
  second and be reused by a reopen in that time, a state the engine never had
  before. The timer now holds the file weakly. Whether that was the cause is
  unproven; a single passing run would not show it either, at one failure in
  six runs.

### Real WinFsp mount

`desktop_application_actual_create_copy_attributes_times_and_replace` now also
saves a file as 200 separate 4 KiB writes and a header rewrite through one
Win32 handle, and reads it back before and after a cold remount. On the final
source it passed in 103 s (debug build; 90 s before keep-and-retry) with exactly
27 immutable PUTs for its nine files.

Negative control: the same test with the engine change removed did not
complete. The mount stalled, and the harness time limit ended it at small write
86 (`ERROR_OPERATION_ABORTED`, 995) after 124 s.

The other real-mount suites passed on the final source:
`native_lifecycle_actual_` (three tests, 141 s), `component_retention_actual_`
(three tests, 346 s) and
`actual_native_chunk_writes_unicode_sparse_flush_resize_and_remount` (48 s).
Each real-mount worker has a 120 s harness bound. In one earlier run of the
same source, made while the machine was slower, the unlink-replace lifecycle
test and two component retention tests crossed that bound (`ERROR_OPERATION_ABORTED`,
995, at the final reopen) and failed; the lifecycle sequence had taken 120 s
against 100 to 111 s in every other run, before and after this change. The
reruns above passed, so the bound is marginal on this machine, not a regression
this change can be shown to have caused.

### Tests changed because of the durability decision

Two existing tests wrote through a handle and then stopped the owner abruptly,
expecting the write to be in the journal: the shared-engine `cold_unknown` case
and the real-mount `mounted_crash` case
(`native_lifecycle_actual_killed_owner_unknown_epoch_refusal`). Without a
commit point `cold_unknown` failed as this page predicts: nothing had been
journaled, so cold start had no dirty epoch to refuse. Both now read the bytes
back before the stop. Their assertions about cold-start refusal are unchanged.

### Local gate

On the final source: `cargo fmt --all --check` and
`cargo clippy --workspace --all-targets --locked -- -D warnings` passed; the
Windows workspace test run passed with 1,155 passed, 0 failed and 47 ignored in
41 suites; the link check (121 files), `mdbook build`, `mdbook test` and the
schema-fixture check passed. The pieces were run by hand because
`scripts/gate.sh` calls `python3`, which is a stub on this machine.

### Not covered

- No passing hosted CI run yet, and no local Linux run. FUSE RELEASE with a
  buffered run is covered only by the shared-engine close check, not by a real
  FUSE mount.
- No Word or Excel run.
- Of the failure paths under [Failure reporting](#failure-reporting), refusal
  at a write, a read, Close, flush-all and unmount is tested on the shared
  engine. Making room is tested at Close only; flush, cleanup, flush-all and
  unmount share its code but have no assertion of their own. Refusal in the
  idle timer, at cleanup, and of rename or delete by path has no assertion
  either, and none of it has been exercised through a real mount.
- Kept bytes are tested only with the test-build switch, never with a
  journal that is really full.
