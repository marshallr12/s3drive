# S3Drive handoff

Updated2026-10-21. Goal ACTIVE. Assistant Mira; no voice-context handoff capability
is available. Read this and [implementation plan](docs/src/implementation-plan.md)
before implementation. Research and evidence live in docs/src.

## Current decisions and permissions

- marshallr12/s3drive is intentionally PUBLIC. Never change visibility without a
  new direct request; all visibility checks are GET-only. This supersedes the
  original private objective and historical publication notes.
- Managed immutable chunks/manifests, normal coordinated filenames, shared Rust
  metadata authority and private client SQLite outside the mounted tree. Both
  hosting options and optional Turso remain open; neither is provisioned.
- MX Linux without systemd and Windows, sign-in startup plus same-account SSH/
  tmux continuity are required. Actual workstation reboot/logon is not certified.
- No new Wasabi mutations, credentials, root/startup installation or hosting
  changes are authorized by fixture tests. Never dump a config on error.
- Run bash scripts/gate.sh; OS/provider/recovery acceptance is separate. Preserve
  dirty bytes and unknown identities; no unconditional overwrite fallback.

## Accepted runtime and publication

Runtime d95a6343d1c232dfc7f7e63ca958e70cee819bf0 is schema41/all366 inputs,14
runtime deltas from accepted66d78db/all362. Own full local gate5 and hosted
CI39351215634 attempt1 all3 passed. [Contract and complete evidence](docs/src/native-chunk-writes.md).
Canonical publication will be recorded in /tmp/s3drive-canonical-native-chunk-publication.json.
Pre-promotion main is1ce239479fbd0f272297a5caff9948265327bcef; verify current
HEAD/remote/runtime digests rather than copying that historical SHA.

Version8 explicitly selects canonical create plus sparse read/write/resize, exact
view flush, immutable create-only S3 objects, root CAS and local acknowledgment.
Stable native inodes expose dirty sizes. Old whole-object/metadata-empty files
remain readable, never silently promoted. Versions3–7 retain their behavior and
older writable profiles'8MiB limit. Source/store/credential changes conflict rather
than concealing dirty heads. Captured-view ack preserves newer edits.

Four permits are acquired before spawning owned Tokio operations; abort-on-drop
requests cancellation. Fresh format/open selection is on the worker. Thin
creation selects separate sparse/legacy futures before constructing async state.
No callback stack limit was increased. Same-owner typed chunk capability still
binds namespace/open journals, but their driver mutation/lifecycle paths remain
unsupported. No schema or dependency upgrade was introduced.

## Verification

Local gate5 CLOSED0 at2026-10-20T01:14:51Z,707.85s, all693 unchanged, logSHA
332e1065cbd921b084278059cd8cd249612e84bcc5cf9decb0d24cfa2d564e89. State539+1
ignored/publication35+1nativeignored/reader26+1nativeignored/filesystem14, fmt/
Clippy/build/95links/book91 passed. The own source ZIP matches all693 inputs.
Own Windows118395571162/Linux118395571627/book118395571467 all success. Strict
source/API ZIP/21+14/project/all35 raw fields/types passed; all17 slow notices and
15 interleaved completions resolved. Exactly3 known disposable Windows profile
locks: dummy credentials removed, deletion deferred to VM disposal; pass lines
reviewed. No hidden test failure. Tooling warnings retained.

Real v8 chunk fixture: Linux54.48s, WinFsp57.69s;11MiB logical/2MiB binary/Unicode
create, sparse read, exact flush, shrink/regrow and remount, six unique immutable
PUTs. Windows128KiB control4.76s and full-suite repeat passed. Existing FUSE9/
77.78s, WinFsp7/88.43s, manifest remount1.62s/2.30s, coordinator startup24.13s
Linux and paired Windows77.28s passed. These are owned loopback fixtures, not
Wasabi uploads or physical reboot tests. Images Ubuntu24.04/20261012.334.1 and
WindowsServer2025/windows-2025-vs2026/20261007.264.1.

## Rejections and durable records

Sourcea6a44fb failed old startup stacks; gate1 stopped149.15s/exit-15 unchanged.
Source390cf00 passed local717.08s but Linux CI exposed a partial ready marker and
Windows new native create overflowed. Source8f1fb7b atomically renames its marker;
20 repetitions exercised100 real child kills. Local736.78s and Linux native
passed; Windows callback still overflowed. Source428581f owned tasks passed local
702.59s and both real native fixtures, but Windows128KiB control failed. Only
finald95a634 passed complete own cross-platform acceptance. Final portable
pressure fixture reproduced the old production overflow before repaired success.
Temporary production probes were removed; thresholds and acceptance were not weakened.

Four indexed archives in docs/src/evidence, all members verified; one-use writers
DONE, never rerun: pre-dispatch local413/1116225B SHA72f5252595f226750c9424ff5d4e2cc7d76e4933041fe1c2310925d293f0fe75;
rejected dispatch local435/1200163B SHAa198baa95c5b68eab8908bf1da97f89ea434badc7f84bbbe58110ae46ee489c8;
accepted local443/1278283B SHAf86234d37c5db6ed160081ad6a55dcd45d73f1cd87ff06e8885d8ffc83770c40;
hosted84/451636B SHA05ca1a0e5dbee6f1439c0ec58e2d256ba915cc06278c613e0c9097534a85d752.
Detailed /tmp/s3drive-native-chunk-checkpoint.json and frozen runtime map
/tmp/s3drive-native-chunk-candidate5-frozen.json. Previous canonical handoff
archived verbatim in docs/archive/HANDOFF-before-native-chunk-writer-refresh-20261021.md.

## Next work and acceptance boundaries

1. Declare native chunk namespace/lifecycle phases. Directory creation, rename/
   delete/save-replace, deferred canonical creation and owned-unlinked publication
   remain unsupported. Use exact creator provenance, typed same-owner journals and
   original removal owner/fence/event; never fabricate admission from a path. See
   [deferred review](docs/src/deferred-manifest-integration.md). Native FUSE/WinFsp
   gates must cover those operations before enabling flags.
2. Complete cold metadata-only namespace/open recovery and full retry-forest
   retention/GC. Current64-file/512-view/64MiB dirty and creation/root/authority
   history quotas are prototype limits; no reclamation is enabled.
3. Verify version8 with startup supervisors, actual MX and Windows10/11 sign-in/
   reboot/SSH/tmux, independent-host partitions/fencing/deployment and writable
   restore/failover. Snapshot create/inspect is implemented; restore is not.
4. Prepare a fresh bounded managed Wasabi plan before requesting live mutation
   approval. Existing six-key/14-PUT authorization expired/consumed on2026-10-16
   at23:36:18Z and fixtures were cleaned. Earlier three-object Unicode approval
   was also completed. Never replay/extend either approval or the unapproved
   optional plan pointer. Local config s3drive.local.toml stays ignored; bucket
   s3drivetest/us-east-2/profilewasabi-s3drive. Keys stay outside Git/chat.
5. Packaging, licensing, usable policy renewal and complete application workflows
   remain required. No user drive, writable provider policy or service installed.

First continuation: inspect canonical status, publication proof and accepted366
digests, then declare the next independently testable phase in docs/src. One
local Cargo at a time with the shared /home/marshall/src/s3drive/target. Preserve
all failed/passing evidence and unrelated user edits.
