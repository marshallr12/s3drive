# S3Drive handoff

Updated2026-10-21. Goal ACTIVE/incomplete. Assistant Mira. No voice-context handoff
capability is available. Read this and docs/src/implementation-plan.md first.

## Decisions and permissions

- marshallr12/s3drive is intentionally PUBLIC. Never mutate visibility without a
  new direct request; checks GET-only. This supersedes the original private goal.
- Managed immutable chunks/manifests, ordinary mounted filenames, Rust shared
  authority, private local SQLite outside the mount. Optional Turso and both
  hosting choices remain open; none provisioned.
- MX Linux without systemd and Windows; sign-in startup plus same-account SSH/tmux
  continuity required. Physical workstation reboot/logon remains unverified.
- No new provider writes, credentials, root/startup installation, hosting or Turso
  changes authorized. Old Wasabi3-object and6-key/14-PUT scopes exhausted/cleaned
  on2026-10-16 at23:36:18Z. Never replay them. Keep config/keys out of Git/chat.
- Run bash scripts/gate.sh. Native/provider/recovery gates are separate. Preserve
  dirty bytes and exact unknown identities; no unconditional overwrite fallback.

## Accepted runtime and evidence

Runtime9381456210c1154e9e7291249f38e5d6f3d52691, schema43/all374; exactly15 runtime
paths from accepted8601973. [Client contract](docs/src/deferred-creation-client.md).
Own full local gate1 CLOSED0/841.84s/all716 unchanged, logSHA
3f2edc0f7f7010c9b8aa73cd04352f2474ce38c9e0307924176a3479a6f18673.
Own CI39587227416 attempt1 Linux/Windows/book all success. Exact source/API ZIP,
strict21+14/project/all35 raw fields/types and full raw-log review passed.
Publication proof: /tmp/s3drive-canonical-deferred-client-publication.json;
verify current HEAD/remote and runtime374 instead of assuming an older main.

Creator::create_deferred durably binds the original operation/owner before S3.
Format2 uses a distinct staging key; format1 JSON/hash/staging bytes stay intact.
The exact saved CreateDeferredEmptyManifest request requires the combined
PublishedRemoval receipt with the original fence. Unknown physical work observes
before fresh admission; restricted metadata recovery needs no credentials/body.
Historical metadata/fence grants no active open right. The authority still
records caller upload claims without holding S3 credentials. No import, sparse
owned-unlinked publisher or new native lifecycle flag is enabled.

Seven state/six SDK cases, eight kill points, actualSQL_FULL, source/owner/receipt
forgery, Unicode collisions and no-credential damaged-body recovery passed in
both hosted Rust jobs. Both dispatcher cancellation cases passed. The boxed
creation/join boundary fixed E0275 without increasing compiler/stack/deadline or
quota limits. Existing four owned Tokio slots and abort-on-drop remain.

Existing v8 chunk files create/read/write/resize/exact-view flush/remount and
nested Unicode folders/listing remain natively verified. Hosted Linux Ubuntu24.04
20261012.334.1 and WindowsServer2025/windows-2025-vs2026/20261007.264.1 use owned
loopback fixtures. They do not prove physical MX/Windows10/11 or live managed
Wasabi acceptance. Old profiles3–7 and older writable8MiB bounds remain unchanged.

Indexed local/hosted archives in docs/src/evidence, every member verified. Local
404members/SHAa0ef5ea5ea27aee72b664bd84757dfa7043e01814753d96fff308a99b474b185;
hosted36members/SHAfb4eb0683ca46c861d7bc44b6af355e83e71d54d4ade0c23033dd65c5e8803c5.
Archive writers DONE; never rerun. /tmp/s3drive-deferred-client-checkpoint.json,
candidate1-frozen.json and hosted1-* hold continuation pointers. Actual schema42
Linux binary refused43 unchanged; Cargo restored actual43 with identical binary
hash. No user database or startup installation was touched. Earlier compile
checks1/2 failed E0275; check3/full gate passed, failures retained in local archive.

## Next work and boundaries

1. Implement the [typed same-owner provenance import](docs/src/creator-provenance-import.md)
   from the exact verified creation journal into namespace/open admission. No
   caller receipt/path/owner substitution or generic-begin bypass. Validate the
   independent physical witness on load, original fence and current event. Then
   implement sparse owned-unlinked publication and native lifecycle with their
   state/SDK/FUSE/WinFsp recovery gates. See docs/src/deferred-manifest-integration.md.
2. Cold metadata-only namespace/open recovery, retry-forest closure and retention/
   GC remain missing.64-file/512-view/64MiB dirty and creation/root/authority history
   quotas are prototypes; no reclamation is enabled.
3. v8 startup supervisors, physical MX/Windows10/11 reboot/sign-in/SSH/tmux,
   independent-host partitions/fencing/deployment and writable restore/failover.
   Snapshot create/inspect exists; restore is not implemented.
4. Prepare a fresh bounded managed Wasabi plan before requesting live mutations.
   Local ignored s3drive.local.toml: bucket s3drivetest, us-east-2/profilewasabi-s3drive.
   No user mount or writable policy is installed. Packaging/licensing and policy
   renewal/application workflows remain.

One local Cargo at a time; shared /home/marshall/src/s3drive/target, threads4.
Switching worktrees reused an old state library despite a successful rebuild:
invalidate affected local artifacts and verify the actual binary schema. Cargo
freshness is not runtime-version proof. No subagents authorized/spawned here.
Preserve unrelated/dirty work and all failed or uncertain verification evidence.

Prior handoff archived verbatim in
`docs/archive/HANDOFF-before-deferred-client-refresh-20261021.md`.

Archived source SHA25606ed9afa4a1eeae32e357766cac0604af65441fbe8b047381750a4a413edccd8.
