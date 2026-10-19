# Native manifest read integration

Implementation and source-review date: **2026-10-18**. Source `c633d02` on
`codex/manifest-snapshot-inodes` has passed its own Linux/Windows/book cohort,
reviewed raw logs, strict typed reference artifacts and all35 raw-field/type
comparisons. This accepts the described version-3 read-only manifest controls.
The earlier draft `d98ea2f` failed its Linux native size check; the repair and
negative evidence are retained below. The prerequisite reader source `88d02f7`
was separately accepted at checkpoint `7c2b454`.

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
exhaustion returns a resource error. In prerequisite c633d02, legacy and writable
inode behavior remained unchanged; the current draft below extends the read-only
snapshot rule to empty and legacy files too.
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

The repaired source's Linux job `117826123544` completed successfully. Its
explicit real FUSE manifest-read/remount control passed in 1.22 seconds, including
same-name replacement and the retained old-handle size/bytes assertions. Nine
existing native controls passed in 63.27 seconds. No-systemd desktop/SSH and
coordinator HTTP/private-CA startup/recovery controls also passed. Raw logs are
hash-bound in the [repair evidence](evidence/manifest-snapshot-inodes-20261018.json).
Windows and combined artifact review remain pending. These timings identify
executions and support no performance claim. Provider mutations and a physical
reboot were not part of these loopback runner tests.


## Completed cross-platform native cohort

The [own source-c633d02 run](https://github.com/marshallr12/s3drive/actions/runs/39178517491)
completed all three jobs successfully. The explicit Linux FUSE control passed
in1.22s and Windows WinFsp control in1.79s, with the original old-handle assertion
retained and same-name publication before rename. The Windows runner was Windows
Server2025, image `windows-2025-vs2026` version `20261007.264.1`; testing was on
GitHub's disposable VM, not this Linux workstation.

The run's own finalized replacement/disposition artifacts passed strict21/14
record checks and project oracles. All35 raw JSON observations matched every
artifact field and type. API artifact ZIP digests, exact source and all257 frozen
inputs were verified. The [26-member hosted evidence archive](evidence/manifest-snapshot-inodes-c633-hosted-20261018.tar.gz)
retains all three raw logs, checkers, artifacts and exact tested source; every
indexed member was verified. SHA-256:
`800057c4b01f08120681438acc67ce0099fe083ffdc476af37e0e6ce5ee912fd`.
The long-running notices, expected injected failures, unused WinFsp delay-import
warnings and three disposable-profile cleanup deferrals remain visible in the
logs. Dummy credentials were removed before profile disposal was deferred.
These results accept the scoped read-only controls, not chunk writes, a physical
workstation reboot, restore/failover or new Wasabi mutations. The later W1a
reference-proof library has separate source and unfinished gates.


## Combined format-transition draft (2026-10-18)

The accepted c633 control started its old handle from a manifest. A new negative
control found that a previously opened metadata-empty inode acquired a later
manifest's4-byte size. The combined draft now gives every file in a read-only
chunk-capable backend a content-version identity and lookup attributes: absent
payload is an explicit empty version; legacy and manifest payloads use their
domain-separated content references. Rename keeps the identity. Reversing from
manifest to legacy or republishing legacy bytes cannot reuse a stale inode.
New opens check identity/kind/payload before dispatch and after either reader
returns its immutable snapshot. Snapshot attributes authorize no new data reads.
Direct, writable and valid capability-absence fallback behavior retain their
existing scope. This changes no schema or authority wire format.

[Draft transition evidence](evidence/snapshot-format-transitions-20261018.json)
records the negative control and eight passing portable controls. A first test
fixture held too many completed legacy snapshots; its scenario lifetimes now
respect the unchanged four-snapshot/32MiB legacy limit. The dedicated real mount
control adds empty/legacy/manifest transitions, legacy republication and rename
while retaining earlier old-size/bytes and remount assertions. These additional
native assertions require the combined source's own Linux/Windows cohort;
prerequisite c633's artifacts do not certify them. W1a proof exports are combined
in the same draft and likewise need their own final source-bound acceptance.

The combined full local gate passed with all259 frozen inputs unchanged:
eight reader/format controls (one native control ignored),12 publication controls,
62 S3 tests,365 state tests/one ignored,formatting,Clippy,build and book checks.
The first combined gate exposed an older readonly engine fixture opening a stale
inode to read replacement bytes. It now asserts a stale-open conflict, refreshes
path lookup, and checks distinct snapshot inodes with unchanged logical file IDs;
old-byte, rename and deletion assertions remain. Writable fixture cases are
unchanged. The isolated updated engine test passed before the fresh full gate.
The [indexed combined local archive](evidence/snapshot-format-transitions-local-20261018.tar.gz)
retains failures, controls, all input hashes and exact combined source. Every
indexed member was verified. This supports portable acceptance only; the new
real native assertions need a fresh exact-source Linux/Windows cohort.

Combined candidate `8c3448f` is pushed on `codex/manifest-reference-proofs`.
Its [own Linux/Windows/book cohort](https://github.com/marshallr12/s3drive/actions/runs/39184629344)
is running. All259 working and committed runtime inputs match; public visibility
and unchanged main were verified read-only. This cohort must validate the added
real-drive format assertions and proof API; earlier native artifacts do not
accept this source.
