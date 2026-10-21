# S3Drive handoff

Updated 2026-10-21. Goal ACTIVE/incomplete. Assistant Mira. No voice-context handoff
capability is available. Read this and docs/src/implementation-plan.md first.

## Decisions and permissions

- marshallr12/s3drive is intentionally PUBLIC. Never mutate visibility without a
  new direct request. Checks GET-only; original private goal is superseded.
- Managed immutable chunks/manifests, ordinary mounted filenames, Rust authority,
  private local SQLite outside the mount. Optional Turso and both hosting choices
  remain open; none provisioned.
- MX Linux without systemd and Windows; sign-in plus same-account SSH/tmux
  continuity required. Physical workstation reboot/logon remains unverified.
- No new provider writes, credentials, root/startup installation, hosting or Turso
  changes authorized. Old Wasabi3-object and6-key/14-PUT scopes exhausted/cleaned
  on2026-10-16 at23:36:18Z. Never replay them. Keep config/keys out of Git/chat.
- Run bash scripts/gate.sh; native/provider/recovery gates are separate. Preserve
  dirty bytes and exact unknown identities; no unconditional overwrite fallback.

## Accepted runtime and evidence

Runtime3c38ad1bdf3aad9f41a202af1e6120ed2e6cfd57, schema44/all377;15 runtime deltas
from accepted main25fc926. [Contract](docs/src/creator-provenance-import.md).
Own full local gate1 CLOSED0/874.84s/all723 unchanged, logSHA
541307c3c948433d62f7222799b5bc58fceb2dbb4ae59a899871417ea0f258d7.
Own CI39599329985 attempt1 Linux/Windows/book all success. Exact source/API ZIP,
strict21+14/project/all35 raw field/type and full raw-log review passed.
Publication proof: /tmp/s3drive-canonical-creator-import-publication.json;
verify actual HEAD/remote and runtime377 instead of copying an older main.

Namespace::import_creation accepts only the original operation ID on a retained
chunk capability. It reads a verified acknowledged format2 creator in that same
SQLite worker, checks owner/source/store/proof, atomically writes the exact
complete namespace row and checks it again on load. Generic begin refuses this
kind; no path/caller receipt/metadata-empty surrogate. Targeted reads keep the
complete creator-role metadata projection plus the exact selected physical
witness; full-role APIs still validate all physical witnesses. Historical body
bytes/credentials are unnecessary; damaged provenance fails without repair.

Imported creation supports local open admission/activation facts with exact
original owner/fence/current acknowledged local event. Known cancellations and
successor epochs cannot be rewound. Obsolete sessions cannot admit fresh handles.
Local activation does not prove current remote ownership or grant a kernel
handle; native live authority checks remain required. No new OS flag is enabled.

Schema44 has no new table/DDL. Valid43 roles upgrade atomically with unchanged
rows/hashes; imported shapes under old markers fail before relabeling. Failures
roll back the marker. Keep original-version guards before transaction-local
marker changes in future upgrades. Actual43 Linux binary refused44 unchanged;
Cargo restored actual44 with identical binary hash. Shared-target switches need
state-artifact invalidation plus actual schema checks, not Cargo freshness alone.

All14 state/3 SDK cases passed in both hosted jobs: seven worker kill points,
realSQL_FULL at import/open/activation, quota closing slots, source/owner/receipt
corruption, unknown local receipts after authority commit, no-credential damaged-
body recovery and migration. Existing v8 Unicode folders and chunk file I/O/
flush/resize/remount,128KiB callback and old native/startup regressions passed.
Linux: Ubuntu24.04/20261018.339.1; WindowsServer2025/windows-2025-vs2026/20261007.264.1.
Owned loopback fixtures do not certify physical MX/Windows10/11 or live Wasabi.
Old profiles3–7 and whole-generation8MiB bounds remain unchanged.

Indexed local/hosted archives in docs/src/evidence, every member verified. Local
412members/SHAe1aa32aa57c40f392edaeb2f0f955a64baf4a29bc28ceedc11a4d6adfdd39655;
hosted37members/SHA3f4591648357e92070349af60667cba15e94520aadcb4c13190a0467609d13eb.
Archive writers DONE; never rerun. /tmp/s3drive-creator-import-checkpoint.json,
candidate1-frozen.json and hosted1-* hold continuation pointers. Development
fixture lifetime/parent-directory/session/pending-read/Debug-format failures are
retained; corrected tests preserve session and owned-node guards. No subagents
were authorized/spawned. No user database, provider or startup installation touched.

## Next work

1. Reserve local namespace/open closing headroom BEFORE native S3/authority file
   creation. Historical import can currently refuse capacity after remote create;
   preserve its creator and support recovery of such existing records. Then bind
   sparse owned-unlinked reservations/publication to original fence/current event,
   preserve dirty/newer views and unresolved-effect barriers before final removal.
   Enable native delete-on-close/save-replace/rename/delete only after exact
   state/SDK/FUSE+WinFsp gates. See docs/src/deferred-manifest-integration.md.
2. Cold metadata-only namespace/open recovery, retry-forest closure and retention/
   GC remain missing.64-file/512-view/64MiB dirty and history quotas are prototypes;
   no reclamation enabled. Snapshot create/inspect exists; restore not implemented.
3. v8 startup supervisors, physical MX/Windows10/11 reboot/sign-in/SSH/tmux,
   independent-host partitions/fencing/deployment/restore, packaging/licensing,
   policy renewal and application workflows remain.
4. Prepare a fresh bounded managed Wasabi plan before requesting live mutations.
   Local ignored s3drive.local.toml: bucket s3drivetest, us-east-2/profilewasabi-s3drive.
   No user mount or writable policy is installed.

One local Cargo at a time; shared /home/marshall/src/s3drive/target, threads4.
Preserve unrelated/dirty work and every failed or uncertain verification result.
Prior handoff archived verbatim in
`docs/archive/HANDOFF-before-creator-import-refresh-20261021.md`.

Archived source SHA25612adb445ad37144b8c63bb231066541ec73b3f8204871f9c6dcacb9738aa0870.
