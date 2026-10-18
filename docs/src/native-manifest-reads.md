# Native manifest read integration

Implementation and source-review date: **2026-10-18**. The isolated draft on
`codex/native-manifest-reads` adds manifest reads to version-3 read-only managed
mounts. Focused controls and the fresh full local gate passed; actual FUSE
and WinFsp acceptance is pending. No successful native chunk mount is claimed.
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
