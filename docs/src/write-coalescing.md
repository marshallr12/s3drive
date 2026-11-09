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
fixtures. It has no hosted exact-source CI evidence yet, and Word and Excel have
not been rerun against it. No provider write was made for this work.

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
| Before flush, and before `flush all` and unmount | Flush then publishes as before |
| Before rename or delete of the file, and before it is replaced by a rename | These already publish first |
| At cleanup (Windows: the last user handle closes) | The handle stops buffering for good; later paging writes are journaled directly |
| At Close/RELEASE, if the handle still owns a run | See below |
| Between 1 and 2 s after the last write to the run | Idle timer |
| When another handle writes, or a write cannot be merged | |

**Close.** The journal refuses every write through a handle once that handle's
Close has been observed. A handle that reaches Close while it still owns a run
therefore journals the run first and only then lets Close be observed. For
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

Unless the journal refuses a run (next section), nothing is lost on a clean
unmount, on close, or on any path that reads the file back.

### Failure reporting

If the journal refuses a buffered run (for example the dirty-byte or
retained-view limit is reached), the bytes stay in the buffer and:

- the operation that tried to commit them fails with the journal's error. This
  can be a later write, a read, a truncate or a flush on any handle of the
  file, so an application can see an error for bytes whose own write call had
  succeeded;
- a failure in the idle timer is written to standard error and retried by the
  next operation;
- a failure at cleanup cannot be returned to the application on Windows.
  Cleanup still records its callback, reports the failure on standard error,
  and the run stays buffered for Close to retry;
- **at Close, a run the journal still refuses is discarded** and the number of
  bytes lost is written to standard error. Close then proceeds normally. The
  closing handle can never write again, so the only alternative is to leave the
  handle unable to close. Under the existing Close design a handle in that
  state makes its file refuse further work for the rest of the mount, and the
  bytes would still not be saved. That alternative was reasoned from the code,
  not exercised in a test.

This last point differs from the original proposal, which said the bytes would
stay attached and be retried. On FUSE the application normally sees the error
first, because `close(2)` sends FLUSH, and flush reports it. On Windows an
application that calls `FlushFileBuffers`, reads the file back, or keeps
writing sees it; one that writes a final small piece and only closes does not.

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
43 s (debug build). It runs in the existing `component_retention_shared_` CI
step on Linux and Windows. It checks that:

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
- after a clean stop and a cold restart every file reads back exactly;
- after an abrupt stop, bytes committed by a read are in the journal, a later
  buffered write is not, and cold start refuses the retained dirty epoch as
  before.

Six unit tests cover merging, including a 2,000-step pseudo-random comparison
with the same writes applied directly, and the rule for what is buffered.

`write_coalescing_refused_run_is_reported_then_discarded_at_close` forces the
journal to refuse a run by writing single bytes two apart, so that every write
commits the one before it. The journal refused after 91 accepted writes with a
disk-full error. The test then checks that the size still includes the
buffered byte, that a read and a flush each report the same error, that Close
succeeds and discards exactly that byte, and that the file can be reopened,
holds every journaled byte, accepts a new write and reads back exactly after a
cold restart. It passed in 25 s (release build; 132 s in a debug build). It is ignored by default because it is
slow, and has its own release-build CI step.

### Real WinFsp mount

`desktop_application_actual_create_copy_attributes_times_and_replace` now also
saves a file as 200 separate 4 KiB writes and a header rewrite through one
Win32 handle, and reads it back before and after a cold remount. On the final
source it passed in 90 s (debug build) with exactly 27 immutable PUTs for its
nine files.

Negative control: the same test with the engine change removed did not
complete. The mount stalled, and the harness time limit ended it at small write
86 (`ERROR_OPERATION_ABORTED`, 995) after 124 s.

The other real-mount suites passed on the final source:
`native_lifecycle_actual_` (three tests, 150 s), `component_retention_actual_`
(three tests, 321 s) and
`actual_native_chunk_writes_unicode_sparse_flush_resize_and_remount` (39 s).

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
Windows workspace test run passed with 1,151 passed, 0 failed and 47 ignored in
40 suites; the link check (121 files), `mdbook build`, `mdbook test` and the
schema-fixture check passed. The pieces were run by hand because
`scripts/gate.sh` calls `python3`, which is a stub on this machine.

### Not covered

- No hosted CI yet, and no local Linux run. FUSE RELEASE with a buffered run is
  covered only by the shared-engine close check, not by a real FUSE mount.
- No Word or Excel run.
- Of the failure paths under [Failure reporting](#failure-reporting), refusal
  at a write, a read, a flush and Close is tested on the shared engine.
  Refusal in the idle timer and at cleanup has no assertion of its own, and
  none of it has been exercised through a real mount.
- Whether a flush resets the retained-view ceiling is still unmeasured.
