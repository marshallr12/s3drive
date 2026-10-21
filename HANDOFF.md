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

Accepted runtime `7c980745c6d2a2bd55805a3536b15d874d6b6b22`, schema41/all368, exactly7 runtime paths from main
6417ee7. [Directory contract/evidence](docs/src/native-chunk-directories.md).
Own full local gate3 CLOSED0/712.93s/all701 unchanged, logSHA
14d5fa41bf5ea5fc6ea2ee4fc35622a164bbf59dceaf40c014b7f438662d58b6.
Own CI39568987445 attempt1 Linux/Windows/book all success. Exact source ZIP/all701,
API ZIP digest/strict21+14/project/all35 raw field/type/full raw-log review passed.
Canonical promotion proof will be /tmp/s3drive-canonical-chunk-directories-publication.json;
verify current HEAD/remote/runtime368 rather than treating pre-promotion6417 as current.

Version8 canonical file create/read/write/resize/exact-view flush plus ordinary
folder create/list works through FUSE/WinFsp. Folder intent precedes RPC; exact
saved outcomes replay under narrowed/expired fresh admission. Directory-only
operations make zero S3 PUTs, do not flush unrelated files or resolve their
unknown roots, and preserve newer dirty edits. Fresh parent revision, portable
names, feature/default-store policy required. Four permits before owned Tokio
spawn,120s directory deadline, abort-on-drop; thin chunk/legacy future selection,
no stack increase.128KiB callback control passed. No schema/dependency upgrade.

Native fixture creates/lists nested Unicode directories, verifies metadata,
creates11MiB sparse/2MiB binary file, flushes/resizes/remounts; six unique immutable
PUTs. Both real native fixtures and existing startup/regressions pass. Hosted
Ubuntu24.04/WindowsServer2025 are owned loopback fixtures, not live Wasabi or
physical MX/Windows10/11 startup evidence. Old profiles3–7 retain behavior and
old writable8MiB limits. Legacy whole-object/metadata-empty origins remain read
only in v8; never silently promote. Source/store/credential changes conflict.

## Evidence and superseded runs

Indexed local/hosted archives in docs/src/evidence; every member verified.
Local archive401/1126835B SHAd24d53df92f9d415ad9c9d18081513d76a16aef83addb13ed5cc981ec1c92303.
Hosted archive counts/hash and complete warning review in the directory record.
/tmp/s3drive-chunk-directories-checkpoint.json and candidate3-frozen.json retain
continuation pointers. One-use archive writers DONE, never rerun.
SDK1 private-field compile mistake; SDK3 invalid sibling-revision race; SDK4
fixture mode not reset. SDK5 corrected controls passed3. Candidate3004479 full
gate1 failed Clippy38.76s unchanged; hosted1 both platforms lint rejected.
Candidate6471af9 gate2 deliberately stopped229.29s/exit-15 unchanged to add actual
native Unicode enumeration; its CI cannot replace final7c980745 acceptance.
Earlier file-writer stack/ready-marker rejections and accepted d95a634 remain
fully preserved in docs/src/native-chunk-writes.md and its four indexed archives.
No test timeout, stack threshold or unsupported lifecycle path was weakened.

## Next work and boundaries

1. Declare rename/delete/save-replace, deferred canonical creation and owned-
   unlinked publication phases. Use exact creator provenance, same-owner journals,
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

First continuation: inspect canonical clean status/publication proof, runtime368
map and latest CI, then declare one independently testable phase in docs/src.
One local Cargo at a time, shared /home/marshall/src/s3drive/target, threads4.
No subagents were authorized/spawned for this phase. Preserve unrelated edits.

Before refresh handoff archived verbatim in
[archive](docs/archive/HANDOFF-before-chunk-directory-refresh-20261021.md).
Pruned126→91 lines; archived SHA256
bb030bdd0dbf16f4a965ef48cee0d81cfa57f0648ad7758d9762318697edb5ab.
