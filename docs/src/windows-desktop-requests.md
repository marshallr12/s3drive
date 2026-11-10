# Windows desktop application requests

Dated 2026-11-09. This page records the follow-up to the
[first physical Windows pass](wasabi-physical-windows-plan.md), which found that
Office and `CopyFile` could not create files on the drive and that many small
writes stalled. It covers one implemented adapter change, the measurements of
the small-write defect, the write-coalescing decision that addresses it (now
implemented; see [write coalescing](write-coalescing.md)), and one remaining
**proposal** for stored modification times.

**Status.** The adapter change, write coalescing and their real-mount test
passed locally on MiniAir (Windows 11 Pro 10.0.26300, WinFsp 2.1, loopback
provider and authority). Neither has hosted exact-source CI evidence yet, and
Word and Excel have not been rerun against them. No provider write was made for
this work.

## Implemented: create, attribute and time requests

The WinFsp adapter previously refused three things that ordinary Windows
applications request as a matter of course.

| Request | Before | Now |
| --- | --- | --- |
| Access mask containing `WRITE_DAC` or `WRITE_OWNER` (also what `MAXIMUM_ALLOWED` and `GENERIC_ALL` resolve to) | `STATUS_MEDIA_WRITE_PROTECTED` | Granted on a writable mount; still refused on a read-only mount |
| Create or overwrite with `FILE_ATTRIBUTE_TEMPORARY`, `HIDDEN`, `SYSTEM` or `NOT_CONTENT_INDEXED` | `STATUS_NOT_SUPPORTED` | Accepted |
| Set basic information (attributes, times) | `STATUS_INVALID_DEVICE_REQUEST` (callback absent) | Accepted for the same attribute set and for all four times |

Granting the two access bits changes nothing by itself. The mount still has one
fixed owner-only security descriptor, and the adapter still implements no
set-security callback, so an actual attempt to change an ACL or owner fails
explicitly. `READONLY`, `ENCRYPTED`, `COMPRESSED`, reparse points, extended
attributes and alternate streams remain refused.

### Per-mount overlay and its limits

The managed layout stores neither Windows attributes nor timestamps. Accepted
values therefore live in a bounded in-memory overlay inside the mount process
(`crates/s3drive-winfsp/src/overlay.rs`, at most 65,536 nodes):

- `HIDDEN`, `SYSTEM` and `TEMPORARY` are remembered and reported back.
  `ARCHIVE`, `NORMAL` and `NOT_CONTENT_INDEXED` are accepted and ignored.
- Creation stamps all four times with the current time. A write, truncate or
  overwrite advances the write and change times. An explicit time set by an
  application replaces the recorded value; zero leaves it unchanged, and an
  attribute value of `INVALID_FILE_ATTRIBUTES` leaves attributes unchanged, as
  the WinFsp `SetBasicInfo` contract specifies (resolved source
  `winfsp-sys 0.12.1+winfsp-2.1`, `inc/winfsp/winfsp.h`, read 2026-11-09).

**Nothing in the overlay is persisted.** After a remount, or for any file not
touched during the current mount, attributes revert to their defaults and every
time again reports as the Unix epoch (shown as 1969-12-31 in US time zones).
A hidden Office lock file is hidden only while the mount that created it is
running. This narrows defect 3 of the physical pass but does not close it; see
the timestamp proposal below. An application that depends on a stored
`READONLY`, hidden or timestamp value across remounts is not supported.

### Evidence (local, 2026-11-09)

`desktop_application_actual_create_copy_attributes_times_and_replace` in
`crates/s3drive-cli/tests/chunk_publication/native_desktop.rs` mounts the real
driver with the version 8 `component-retention-v1` profile and performs, through
ordinary Win32 calls from Rust's standard library:

- creates with `MAXIMUM_ALLOWED`, `GENERIC_ALL` and
  `GENERIC_WRITE | WRITE_DAC | WRITE_OWNER`, with a dirty read-back;
- a `FILE_ATTRIBUTE_TEMPORARY` create named like Excel's save file and a
  `FILE_ATTRIBUTE_HIDDEN` create named like a Word lock file, both reported back;
- `CopyFileEx` into the mount and within it (what Explorer paste and `copy` use);
- a current write time, an explicit `SetFileTime` read back exactly, and an
  explicit refusal of `READONLY`;
- an Office-style save: write a temporary file, then rename it over the document;
- a clean unmount, cold remount and byte-exact read of every surviving file.

It passed in 18–21 s (release build) and 62 s (debug build, as CI runs it)
with exactly 24 immutable PUTs to the loopback provider, none repeated. (The
test has since gained a small-write save and makes 27 PUTs; see
[write coalescing](write-coalescing.md).) As a negative control, the same test against
the unchanged `feea0a9` adapter failed at its first create with
`ERROR_WRITE_PROTECT` (19), the failure seen on the physical pass. The six
overlay unit tests and workspace Clippy with warnings denied also passed, as
did the local Windows workspace test run (1,144 passed, 0 failed, 46 ignored in
40 suites). The step is added to the Windows `rust` CI job; that hosted run,
with Linux, is the pending acceptance gate.

This is not Office GUI acceptance. The earlier physical probes used direct
`CreateFileW` calls against the installed `feea0a9` build; Word and Excel must
be rerun on a build containing this change before defect 2 is called closed.

## Per-write cost and a 127-write ceiling (defect 1)

This section records the defect as measured **before**
[write coalescing](write-coalescing.md), which now addresses it for saves made
of adjacent small writes. The journal behavior described here is unchanged.

The physical pass used the version 7 whole-generation profile, where each write
stores a fresh copy of every 1 MiB chunk it touches. The copied evidence
database confirms it: 292 staged chunks holding 274.9 MB for one 3.4 MB Word
document, 300 generations, nothing reclaimed.

`managed configure` could only generate versions 6 and 7. It now accepts
`--sparse-lifecycle`, which selects version 8 with the named lifecycle profile
(see [setup](managed-bootstrap.md)). Version 8 stores only the bytes of each
write and reclaims closed history under `component-retention-v1`. **By itself it
does not remove the defect.** An ignored diagnostic,
`component_retention_shared_save_pattern_probe`, measured the shared engine
directly (release build, no driver, loopback provider, MiniAir, 2026-11-09):

| Pattern | Result |
| --- | --- |
| 128 writes of 4 KiB, no flush | Write time grew from 21 ms to 375 ms; 19.4 s for the first 112 writes (448 KiB); **write 127 failed with a disk-full error** at 128 retained views |
| 120 writes of 4 KiB, flush every 30 | Completed in 25.7 s; write time still grew to about 270 ms; each flush took about 2.5 s |
| 16 writes of 256 KiB (4 MiB), no flush | Writes 1.6 s in total (23 ms rising to 160 ms each); close and publication 4.6 s |

The cause is by design and documented in [sparse patches](sparse-patches.md):
every mutation loads and validates the whole retained local ledger, including
all patch bytes, before and after the change. Cost per write is therefore
proportional to retained history, so a save is quadratic in its number of
writes, and a file cannot receive more than about 127 unflushed writes through
one open. A flush did not reset the per-write cost; whether it resets the
ceiling was not measured, and local history is reclaimed only at the last close.
Word produced about 143 generations (writes and resizes) for each 3.4 MB
temporary file, so the same save was expected to fail on this profile as well;
that expectation was not tested with Word itself. The diagnostic stays in the
tree, ignored, as a timing probe.

### Decision: coalesce writes above the journal

The owner approved this on 2026-11-09 and it is implemented; behavior, commit
points, the durability change and evidence are in
[write coalescing](write-coalescing.md). In short: the validated journal is
unchanged, and a bounded in-memory buffer in the shared engine merges adjacent
small writes to one open file and commits them as one journal mutation.

Alternatives considered and not chosen: enabling the Windows cache manager
(same exposure, less control and "delayed write failed" reporting), and making
ledger validation incremental (keeps per-write durability, but is a deep change
to the proof core and does not lift the view ceiling by itself).

### Proposal: stored modification times

*Proposal, not implemented.* Persisting times needs a home that every machine
sees. The smallest change is an optional modification time on the authority's
node record, set by the publishing client when a file's content reference is
committed and by directory creation, defaulting to absent for existing nodes.
That is an authority wire and schema addition with its own compatibility gate,
so it is not folded into this change.

## Remaining before the next physical pass

1. Hosted exact-source CI for the adapter change and write coalescing on
   standard runners.
2. Passwordless startup (the separate drive-letter work) for the sign-in and
   reboot checks.
3. A new MSI build and the owner's interactive install; then Word and Excel on
   a version 8 `component-retention-v1` configuration. Whether a real Word save
   stays inside the journal's limits is not known until that run.
