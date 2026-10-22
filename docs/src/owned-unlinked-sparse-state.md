# Typed owned sparse preparation state

Accepted state layer, 2026-10-22, at source
`499aa6378360f2bdc76bf020b58e73cb20c4437d`. Exact-source full Linux/Windows
suites, native regression artifacts and actual46/47 binary compatibility
checks have been reviewed. SDK integration and native lifecycle
activation remain deliberately excluded.

## Durable contract

Schema47 adds optional fields to existing canonical preparation/publication
records; no DDL or legacy BLOB rewrite occurs. Missing fields preserve old JSON
and hashes. Before advancing an older marker, bounded raw-field guards reject
new ownership or settlement fields, including explicit null. Old executables
must refuse47 without changing database or sidecar bytes.

The chunk capability exposes typed preparation admission. It captures the full
original session-bound open, retained client owner, original acknowledged removal
and its receipt, and the exact current acknowledged event, receipt and publication
claim in the same transaction as the preparation. Callers cannot substitute an
arbitrary in-memory ownership token. Fresh admission verifies original open
activity, current runtime session, source/store/volume control, pending namespace
work and the exact local event. These are local historical facts, not remote
permission or a guarantee of current authority ownership.

Current active and committed epochs use the existing event-bound authority wire
contract. Root requests preserve the frozen original open and exact event and
bind detached status, payload reference and revision to the request. Historical
replay retains its original identity after restart or close; fresh work cannot
resurrect an obsolete session. Metadata-only root recovery checks independent
namespace/open evidence and the exact preparation header without reading bodies.

Predecessors must be actual source-view ancestors with exact published payload,
source/store binding, original root and the same original open and ownership
epoch. Events may advance monotonically within that epoch, but a successor owner
or different original open cannot adopt the edge. Existing acknowledgment
semantics retain newer dirty views, rebase only an exact current published head,
and never let one resolved plan hide another unresolved plan on the same node.

## Explicit terminal resolution

No-effect settlement requires that no root request exists and no object has ever
been handed to physical staging. A staged object is conservatively unresolved,
even before a consumed send claim. Rejected settlement requires an exact terminal
Conflict on the saved reserve/publish request, with no successful publication or
source acknowledgment. Neither path deletes data, claims success or makes the
source clean. Unknown remote outcomes cannot be converted into rejection.

Resolution is persisted in the preparation's checksummed progress, with exact
idempotent operation identity. It blocks further preparation/materialization and
releases only that plan's namespace/adoption barrier. Existing source and object
bytes remain retained, including dirty and newer views. Recovery revalidates the
proof before recognizing resolution. Admission reserves bounded metadata space
for the later resolution record. There is no reclamation or provider deletion.

## Verification scope

Sixteen new state tests (including the crash-worker entry point) cover pending/committed admission, wrong owner/open/event,
obsolete sessions, exact restart replay, guarded root publication, newer dirty
heads, source lineage, quota, real SQLiteFull rollback, rejected/no-effect
resolution, and killed workers at admission and resolution transaction boundaries.
Remote receipts in these state fixtures are modeled, not an SDK or provider test.

The local cloud workstation compiles the implementation but its tmpfs/overlayfs
cannot satisfy the durable storage contract. The attempted focused suite fails
at the unchanged storage safeguard; it is not counted as passing. Standard hosted
runners execute the complete suites and unchanged native regression gates. A
separate hosted compatibility step builds the actual accepted schema46 CLI and
candidate47 CLI, checks complete legacy schema/table dumps, and checks old-reader
refusal bytes for upgraded and fresh47 databases. Local logs/archives remain
unpublished.

Still excluded: SDK sparse publisher integration, new FUSE/WinFsp owned-unlinked
lifecycle enablement, physical workstation sign-in/reboot, real provider writes,
independent-host deployment, retention and garbage collection.

### Local candidate checks

Workspace/all-target Clippy with warnings denied, a clean isolated workspace
build, 39 core tests, schema fixture checks and local documentation links pass.
The full local gate reaches authority fixtures and fails their unchanged durable
storage check. The owned state suite also requires hosted durable storage.

Actual schema46 and candidate47 executables pass a local tmpfs compatibility
rehearsal: forward migration preserves the complete legacy SQL dump, both fresh
and upgraded47 reopen, and the46 reader refuses both without changing database
or sidecar bytes. This is semantic verification, not crash/durability acceptance.
Old and new builds must use separate Cargo target directories; sharing outputs
can leave the wrong executable at the public binary path even after a build.
The hosted compatibility step uses isolated outputs and records both binary
hashes. Full hosted state acceptance is recorded below.


## Hosted acceptance

[Run39796709806](https://github.com/marshallr12/s3drive/actions/runs/39796709806)
is terminal green in all three jobs at the exact source above. Complete logs
show Linux949 passed/0 failed/15 ignored and Windows945 passed/0 failed/31 ignored,
37 workspace cohorts each. All16 new state tests pass on both, including actual
SQLite-full rollback and killed-worker recovery. Separate native FUSE/WinFsp,
Unicode chunk I/O/remount, disposition/replacement and startup/recovery gates pass.
The ignored workspace cases are exercised by their separate supported gates;
these regressions do not enable the new owned-unlinked native lifecycle.

Both Windows artifact JSON documents match all35 reference records (21 replacement,
14 disposition), including every field type/value and the corresponding raw log
records. Source commit, OS/runtime and archive SHA256 were verified. The source
ZIP matches all757 tracked files byte-for-byte (SHA256
`cf2c85e4e35adee0e37f175b583f4bc6aca4fd7d358ba8d009f1e6a6da899ebc`).

The actual46 CLI built from accepted main5343eb41 and separately built47 CLI pass
all7 compatibility invocations. The complete75-statement legacy schema/data dump
is unchanged through migration; both old-reader refusal cases preserve database
and sidecar bytes, and47 reopens fresh/upgraded databases. Binary hashes differ
and the candidate executable is checked unchanged after the isolated old build.
Artifact archive SHA256 values:

- replacement12434282601: `cbe2f289d948a0ee43b07b9d643cfc766af3719b02dcb311e150681f10b66d32`
- disposition12436391719: `84b4764e3600348d974c6c8bafaf3026bfeec0c9ac59d95bf93c176d6ec707c1`
- compatibility12435410634: `9e0d7011c47dd6047f19fc8d739e7722097a2eb90818d812cb6dfa8d4e6e9a79`

Earlier runs39794331799 and39795452958 caught test-fixture ordering mistakes:
writes must follow removal admission, and event advancement requires exact clean
source acknowledgment before subsequent writes. The fixes correct the fixtures;
no runtime safeguard was relaxed. Failed evidence is retained privately.
Integration must check fresh main/branch heads and verify main's full CI after
merging. Local validation archives remain unpublished.
