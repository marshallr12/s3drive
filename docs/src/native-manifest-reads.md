# Native manifest read integration

Implementation and source-review date: **2026-10-18**. The isolated draft on
`codex/native-manifest-reads` adds manifest reads to version-3 read-only managed
mounts. The original portable gate passed, but its Linux native control
failed. A repair is in progress on `codex/manifest-snapshot-inodes`; actual FUSE
and WinFsp acceptance of the repair is pending. No successful native chunk mount is claimed.
The prerequisite reader source `88d02f7` has passed its own Linux/Windows/book
cohort and strict reference-artifact checks at checkpoint `7c2b454`.

## Filesystem behavior

The read-only backend negotiates `manifest-root-v1`. A valid feature list without
that capability keeps the existing legacy reader; a failed or malformed exchange
is an error. The negotiated namespace reader validates live nodes and every
listed child, including identity, parent, store, placement, name policy, sequence
bounds and duplicate names/IDs. Four shared active-operation permits and the
reader's overall deadline also bound metadata operations. Listing fetches no
S3 file data and returns no partially validated result.

Manifest file sizes come from the validated logical descriptor rather than the
root object's physical length. A read-only open selects a reader from validated
live metadata, then independently obtains a fresh authorized pinned snapshot.
Read-only manifest inodes identify the immutable payload reference together with
its global file ID. Rename retains that inode; publishing another payload gets
a different inode. Attributes for an already discovered manifest inode come
from its validated lookup snapshot, including when FUSE omits the file handle.
This metadata does not authorize a new open or a provider read. Path lookup stays
online; open checks live authority state both before and after obtaining its fresh
snapshot and rejects a changed payload with a conflict. Existing handles keep
their pinned version. Versions count toward the existing bounded inode table;
exhaustion returns a resource error. Legacy and writable inode behavior remains
unchanged.
A concurrent format change can fail explicitly; it never causes a retry or write
fallback. Manifest OpenFile holds the bounded reader File; legacy files retain
their existing whole-byte FileSnapshot behavior. Empty metadata files and
published empty manifests retain their distinct authorization requirements.

Range reads use the [verified manifest reader](manifest-range-reader.md).
Handles keep their metadata/root across rename and newer publication. Bounds,
Busy, timeout, authority and provider errors retain explicit filesystem errors;
unsupported content, binding, protocol and integrity failures never produce
fabricated zeros or successful partial buffers. Write/truncate are refused for
read-only handles. Read-write configurations retain their existing unsupported
manifest paths until chunk mutation/recovery integration passes separately.
The legacy reader, write controller, configuration versions, database schema,
authority wire format and native mutation callbacks remain unchanged.

## Portable controls and actual mount gate

The focused suite passed six controls, with the separate native control explicitly
ignored. The filesystem scenario verifies mixed legacy/manifest and empty files,
Unicode, two mapped stores, logical sizes including sparse 64 GiB, cross-page
ranges, clipped holes/EOF, old roots after rename/publication, denied data,
corruption outside the requested slice, malformed nodes/listings/features and a
valid legacy-only capability fallback. Opening fetches only the bounded root;
read-only mutations remain refused. Existing reader budget/cancellation controls
now also check metadata operations share active capacity.

A dedicated ignored native control uses the actual CLI, OS filesystem calls and
FUSE or WinFsp. It checks the same mixed namespace and sizes, zero data-chunk GETs
at native open, bounded small-range transfer, old roots, fresh unread corrupt/
missing/denied files, read-only refusal and orderly remount. Its owned dummy S3
objects are seeded in loopback memory; no provider upload is implied. Blocking
filesystem checks, child readiness/logging, daemon teardown and worker reaping
are bounded. CI invokes this control explicitly on both platforms; existing
mounted-format tests cannot substitute for it.

The explicit local native attempt failed at readiness: fusermount3 reported
`Operation not permitted`. This session has `NoNewPrivs=1`. Its failed child was
reaped and findmnt showed no remaining FUSE mount. No privilege bypass, driver
change, actual Wasabi mutation, physical reboot or native success is claimed.

## Gate investigation and fixture framing

The first complete local gate exited 101 in an existing paired-startup setup.
Its seed child ran `managed store-plan` and received an Info TLS connect timeout
before any HTTP driver/body poll or changed filesystem path. The other seven
controls in that binary passed; all 257 source hashes were unchanged. High CPU
pressure was observed, but its causal role remains unproven. The unchanged failed
test passed alone in 2.69 seconds. The first failed log remains evidence.

The second gate, with top-level tests serialized, failed a paired fixture's
30-second worker-marker wait. Static stage diagnostics localized it to
`ENGINE_READY`. A negative control then caught the exact serial-libtest prefix
merged with that first marker in 2.54 seconds. This was a fixture framing defect,
not a demonstrated transport failure. Engine and producer workers now emit a
leading newline before their first markers; exact matching and the 30-second
bound remain. The same workspace-feature-graph paired recovery/conflict control
passed under serial execution in 4.60 seconds, and producer startup in 2.16
seconds. No production timeout, retry or recovery assertion was relaxed.

The fresh complete gate passed with `RUST_TEST_THREADS=2`. Tests' internal barriers,
concurrency and deadlines remain unchanged. The
[dated implementation evidence](evidence/native-manifest-reads-20261018.json)
retains the earlier results and their scope. Native acceptance still requires
this source's own completed FUSE and WinFsp CI controls. Chunk writes, migration,
garbage collection, live-provider conformance and physical reboot checks remain
separate work; the implementation plan records the next write-design boundaries.


The completed local gate passed 12 publication/source controls, six reader/filesystem
controls (one separate native control ignored), 62 S3 tests, 365 state tests / one
ignored, formatting, Clippy, build, 69 documentation files and 65 book chapters.
All 257 frozen source hashes stayed unchanged. Two long-running existing controls
passed; their notices remain in the log and support no performance conclusion.
The [27-member indexed local archive](evidence/native-manifest-reads-local-20261018.tar.gz)
retains the final gate, both earlier failures, marker proof, exact source and
self-review. Every indexed member was verified. Archive SHA-256:
`138761972263bdf707c249fccf8954a4766a1e4497153ff2b796fd4e71f2a2a6`.


Published source `d98ea2f` is on `codex/native-manifest-reads`. Its
[own Linux/Windows/book run](https://github.com/marshallr12/s3drive/actions/runs/39176574738)
contains the real native manifest control explicitly in both Rust jobs.
The Linux native test failed: after publishing a 4-byte replacement, `fstat`
on the old 130 MiB handle returned 4 bytes. The two versions shared an inode;
FUSE can request attributes without a file handle. This is an implementation
failure, distinct from the local mount restriction. The repair retains the old
handle size assertion and adds same-name replacement, rename identity, stale
open and publication-during-open checks. The original cohort cannot certify
the repaired source; a fresh local and hosted cohort is required. The clean exact remote and all 257 source hashes were verified;
repository visibility remains PUBLIC and main is unchanged. No own hosted/native
acceptance is claimed until its logs and source-bound artifacts are reviewed.

The [dated snapshot-inode repair evidence](evidence/manifest-snapshot-inodes-20261018.json)
records the failed native candidate, negative control and repaired portable checks.

The repair's full local gate passed with all 257 frozen inputs unchanged:
12 publication controls, six reader/filesystem controls (one native control
ignored), 62 S3 tests, 365 state tests (one ignored), formatting, Clippy,
build and book checks. The [13-member indexed repair archive](evidence/manifest-snapshot-inodes-local-20261018.tar.gz)
includes corrected source, the negative control, the completed local gate and
both earlier native job logs. Every indexed member was verified. Archive SHA-256:
`e243742e802539035a8efa36478ce339a6f1ce43646460592bade4feb887a2aa`.
This establishes portable acceptance only; the corrected source still needs its
own real FUSE and WinFsp controls.

Repaired source `c633d02` is published on `codex/manifest-snapshot-inodes`.
Its [own Linux/Windows/book cohort](https://github.com/marshallr12/s3drive/actions/runs/39178517491)
is running. Public visibility and the unchanged main tip were verified read-only;
all 257 working and committed source hashes match. This is a pending native gate,
not mounted-manifest acceptance.
