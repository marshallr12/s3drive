# S3Drive handoff

Updated2026-10-21. Goal ACTIVE/incomplete. Assistant Mira. No voice-context handoff
capability is available. Read this and docs/src/implementation-plan.md first.

## Decisions and permissions

- marshallr12/s3drive is intentionally PUBLIC. Never mutate visibility without a
  new direct request; all checks GET-only. This supersedes original private goal.
- Managed immutable chunks/manifests, coordinated ordinary filenames, Rust shared
  authority and private local SQLite outside the mount. Optional Turso and both
  hosting choices remain open, neither provisioned.
- MX Linux without systemd and Windows, sign-in startup plus same-account SSH/tmux
  continuity required. Physical workstation reboot/logon remains unverified.
- No new provider writes, credentials, root/startup installation, hosting or Turso
  changes authorized. Old Wasabi3-object and6-key/14-PUT scopes exhausted/cleaned
  on2026-10-16 at23:36:18Z. Never replay them. Keep config/keys out of Git/chat.
- Run bash scripts/gate.sh; native/provider/recovery acceptance separate. Preserve
  dirty bytes and exact unknown identities, no unconditional overwrite fallback.

## Accepted runtime and publication

Runtime `3e670915cdfe3dbafee9cd40c33d1a70cf979234`, schema42/all372,16 runtime deltas from main19ac76f.
[Authority/state contract and evidence](docs/src/atomic-deferred-manifest-create.md).
Full local gate3 CLOSED0/664.97s/all709 unchanged, logSHA
48a3dfd3767e37747ab70d9bbabbba70ed34f78055b382d07fcc34dd9ba04174.
Own CI39579144740 attempt1 Linux/Windows/book all success. Exact source/API ZIP,
strict21+14/project/all35 raw fields/types and full raw-log review passed.
Canonical publication proof will be /tmp/s3drive-canonical-deferred-canonical-publication.json;
verify current HEAD/remote and runtime372 rather than copying historical19ac.

CreateDeferredEmptyManifest commits canonical node/payload/upload history plus
original removal fence and initial event in one transaction. PublishedRemoval
contains payload and exact fence; replay checks immutable initial evidence,
keeps older receipts after content/mode/name/epoch changes, never adopts a newer
owner. Schema42 marker only;64-table snapshot bound, old BLOB/hash bytes retained.
The authority records upload claims, has no S3 credentials. No SDK creation/import,
sparse owned-unlinked or OS lifecycle callback is enabled by this phase.

Version8 canonical file create/read/write/resize/exact-view flush and nested
Unicode directory create/list/remount remain natively verified. Four bounded
owned Tokio tasks, abort-on-drop and128KiB callback control unchanged. Old
profiles3–7 retain behavior and older writable8MiB bounds. Dirty/newer edits and
unknown identities remain preserved. Hosted Ubuntu and WindowsServer2025 use
owned loopback fixtures; not physical MX/Windows10/11 or live managed Wasabi.

## Evidence and superseded runs

Indexed local/hosted deferred-canonical archives in docs/src/evidence; every member
verified. Exact hashes/counts, job/raw review and all inherited directory/file-I/O
archives are linked from the contract. /tmp/s3drive-deferred-canonical-checkpoint.json
and candidate2-frozen.json hold continuation pointers. Archive writers DONE,
never rerun. Real schema41 Linux binaryfea3b379 refused42 unchanged; Cargo restored
new binary. No user database or startup installation touched.

Development check1 visibility/tests2 signed-SQL conversion errors retained;
tests3 red missing-history NotFound normalized to integrity error. Focused2
13state/1HTTP/Clippy passed. Candidatefeead770 gate1 failed stale41 expectation
773.99s unchanged; both hosted Rust jobs failed the same fixture. Source3e670915
gate2 lost its supervisor after tests, child was waited to reaping without
inferring its exit. Only fresh isolated-supervisor gate3 counts as full acceptance.
Earlier native directory/file stack rejections remain in their own indexed
archives. No timeout, stack threshold or dirty-preservation barrier was weakened.

## Next work and boundaries

1. Implement the durable deferred creation SDK and exact verified creator-to-open
   journal import, then sparse owned-unlinked and native lifecycle phases. Use exact creator provenance, same-owner journals,
   original removal owner/fence/event; never derive admission from a path. Read
   docs/src/deferred-manifest-integration.md. Keep unsupported flags until own
   state/SDK/native FUSE+WinFsp recovery gates pass.
2. Cold metadata-only namespace/open recovery, complete retry forests and
   retention/GC remain missing.64-file/512-view/64MiB dirty and creation/root/
   authority history quotas are prototypes; no reclamation enabled.
3. v8 startup supervisors, physical MX/Windows10/11 sign-in/reboot/SSH/tmux,
   independent-host partitions/fencing/deployment, writable restore/failover.
   Snapshot create/inspect exists; restore is not implemented.
4. Prepare a fresh bounded managed Wasabi plan before asking for live mutation
   approval. Local ignored config s3drive.local.toml: bucket s3drivetest,
   us-east-2/profilewasabi-s3drive. No user mount/writable policy installed.
5. Packaging/licensing, policy renewal and complete application workflows remain.

First continuation: inspect canonical clean status/publication proof, runtime372
map and latest CI, then declare one independently testable phase in docs/src.
One local Cargo at a time, shared /home/marshall/src/s3drive/target, threads4.
No subagents were authorized/spawned for this phase. Preserve unrelated edits.


Shared target caveat: switching compatibility worktrees reused an old state
library despite a successful CLI rebuild. Invalidate affected local package
artifacts or use an isolated target, and verify actual binary schema. Keep one
local Cargo process; do not use Cargo freshness as runtime evidence.

Before refresh handoff archived verbatim in docs/archive/HANDOFF-before-deferred-canonical-refresh-20261021.md.

Handoff refresh105→96 lines; archived source SHA25614a1ef9456abba997d90221e46e2ef98b53f24179b0d96c38b27f42a42580b54.
