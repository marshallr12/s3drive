# S3Drive handoff

Updated 2026-10-18. Goal ACTIVE. Mira is the assistant; no voice handoff capability
was available. Read docs/src/implementation-plan.md before implementation.
Research, decisions, acceptance and plans belong in docs/src/.

## Checkouts and immediate next action

- Private local/remote main is now accepted diagnostic f8c4f5832e399c6c54567c122c875e2a438ce681.
  Topic CI39097275707 is terminal all-three SUCCESS, raw Linux/Windows native/
  startup logs and strict21/14 artifacts checked before guarded FF from a7994e6.
  Remote/main SHA and repository PRIVATE verified after push.
- Diagnostic worktree /home/marshall/src/s3drive-windows-transport remains clean
  at f8, branch codex/windows-transport-followup.
- Main publication automatically triggered same-source CI39099187117. Revalidate
  that exact live handle; a repeat failure is new evidence, not erased by the
  accepted topic run. Never restart live jobs on observer timeout/log404.
- Root codex/authority-snapshots was safely rebased onto f8. Current local commits
  f4c37bb/e218381/01af668 correspond to the old UNPUSHED defc8b9/43bfd46/e0a4353.
  Backup ref codex/authority-snapshots-before-transport retains old history.
  Config/plan/report private hashes stayed identical through rebase. Rust
  snapshot code and all f8 diagnostics were preserved; only docs conflicted.
- Current local changes record f8 acceptance and update source status. Snapshot
  code is NOT yet in main or pushed. Combined-source gate PASSED exit0 at
  /tmp/s3drive-snapshot-integrated-gate1.log (274 state, snapshot CLI2,
  Hyper2/HTTP10/TLS6/SDK17/managed7 plus lint/build/57-doc).
- Both checkouts share /home/marshall/src/s3drive/target. Builds replace its CLI,
  so verify/rebuild the intended source before experiments. The last CLI was
  old defc8b9 for the SIGINT/busy checks; a new integrated build will replace it.

Next: inspect main-repeat CI, validate/checkpoint/push the prepared ordinary-
account Windows snapshot acceptance candidate, then inspect actual runtime and unchanged
native/reference/startup gates before any snapshot main promotion. Keep restore
unsupported. No new provider/user startup/root/hosting mutations authorized.
Publication always requires exact-source all-three terminal CI, raw native/
startup and strict21/14 artifacts, guarded private FF and post-push SHA/privacy.
No force push or native shortcuts.

## Windows investigation and evidence

Main a7994e6 passed primary CI39090568625 before promotion: Linux117566235370,
Windows117566235393, book117566235255. Both full paired recoveries and native
suites passed; strict21/14 artifacts12123120389/12123325993 matched every typed
field and source/platform. Raw /tmp/s3drive-wire-fixture-{linux,windows,book}.log;
/tmp/s3drive-a799-reference-check.ps1 and artifacts; durable record
[authority-startup acceptance](docs/src/evidence/authority-startup-20261017.json).
This historical success does not prove repeatable Windows reliability.

Same-source main repeat CI39092631904 FAILED Windows117572408577 during
store-apply seed before mounting. Client Request/UnexpectedEof and server
HttpTimeout while the coordinator stayed alive did not establish the affected
RPC or commit outcome. Raw /tmp/s3drive-main-repeat-windows.log. Preserve exact
plans/unknown IDs; no mutation retry, new IDs, rebase or deadline/TLS weakening.

Diagnostic9b403bb CI39095464132 passed Linux117580869237/book117580869021 and
Windows early TLS, replacement and native lifecycle. Windows117580869114 then
failed the new blanket zero-server-error assertion AFTER every client complete
reply, exact receipt and sequence check passed. Its one untyped HttpTransport
connection214 logged2ms,321 bytes read,481 written, five flushes, dispatch=true.
Raw /tmp/s3drive-transport-windows1.log. Later startup/pair/broad steps were
SKIPPED. The original header timeout remains a separate unexplained failure.

Accepted f8 adds precise Hyper is_shutdown categorization, permitting ONLY
that phase with positive counters AFTER all caller reply/receipt assertions.
Generic transport, truncation and timeout still fail. The deterministic driver
independently read a complete Content-Length reply before injected shutdown error.

Topic CI39097275707 completed Linux117586223366, Windows117586223172 and
book117586223439 successfully. Raw /tmp/s3drive-transport-f8-{linux,windows}.log:
Linux8 native/pair PASS04:37:25UTC and both no-systemd HTTP/TLS coordinator profiles;
Windows standalone PASS04:55:30, pair04:56:52, lifecycle plus seven broad cases.
Supervisor9176 held child4740→7124; paired supervisor5032 held5460→4692. Original
volume/receipts, independent AuthenticationIds, exact unknown IDs/no repeated
PUT, Unicode/binary/empty data, dirty conflict and independent removal passed.
Locked disposable profiles deferred only after all assertions/owned-process and
dummy-credential cleanup, under explicit VM-destruction policy.

Current managed-engine event connection260 was HttpShutdown (1ms,224 read,247
written,four flushes,dispatch=true) while caller tests passed. This is not proof
of the old untyped event's cause or the separate header timeout. The broader
native Connect timeout follows its deliberate coordinator stop and is expected.
No actual priority values were printed; scheduling remains unmeasured.

Strict source/platform21/14 artifacts12124529619/12125676595 independently
matched every typed field. /tmp/s3drive-f8-artifacts/independent-check.json and
/tmp/s3drive-f8-reference-check.ps1 passed; zips /tmp/s3drive-f8-{replacement,
disposition}.zip. Four pinned upstream legacy cases passed. Proof
/tmp/s3drive-f8-publication.json; durable docs/src/evidence/authority-transport-20261018.json.

Local final Rust gate /tmp/s3drive-transport-shutdown-gate1.log passed (265 state,
two I/O/Hyper,ten HTTP,six TLS,seven CLI restart,17 SDK,seven managed and57-doc).
Subsequent PS-only read-only priority logging passed fresh AST and four child
policy cases at /tmp/s3drive-transport-priority-policy1.log; final doc checks
passed. No priority, retry, deadline or TLS policy changed.

## Unpublished offline snapshot prototype

Rebased local f4c37bb (old defc8b9) implements crates/s3drive-state/src/authority_snapshot{.rs,
/native.rs}, additive SnapshotDirectory platform helpers, and CLI
crates/s3drive-cli/src/snapshot.rs plus tests/authority_snapshots.rs. Root
Cargo.toml adds rusqlite backup/hooks/limits features; versions remain pinned.
No snapshot changes are in private main or the diagnostic candidate.

The create|inspect CLI requires explicit volume/policy and offline exclusive
ownership. It performs no migration, credentials lookup, provider I/O, listener
or service installation. Output must be new/private/outside source and mounted
trees. Distinct snapshot.lock/sqlite3/json filenames prevent live-state adoption.
Source stays under a read transaction; copy steps32 pages with size/time/cancel
bounds, destination reopen/SQLite integrity/identity checks, then sync/hash and
manifest-last publication. Post-marker errors are CompletionUnknown; preserve
and inspect that same artifact. No automatic cleanup/retry/new destination.

Validation profile sqlite-integrity-authority-identity-v1 is NOT a full semantic
history audit or freshness witness. Inspection never authorizes restore. Volume
identity alone accepts stale history: the dated rollback reproducer measured
that at7962058. Restoration/failover stays unsupported until independently
proved continuity and fencing; generations do not recover lost acknowledgments.

Resolved rusqlite0.40.2/libsqlite3-sys0.38.2 source inspected: checked backup DONE
follows destination commit; finish maps DONE to OK. No raw SQLite FFI. Never
raw-open/close a DB file while a SQLite connection in that process is active
(POSIX locks). Hash/recheck after close. Windows0.61.3 file-identity signatures
were inspected; actual Windows ACL/reparse/handle acceptance remains pending.

Pre-rebase root full gate PASSED exit0 /tmp/s3drive-snapshot-gate1.log:274 state, two new
snapshot CLI tests, ten HTTP, six TLS,17 SDK, seven managed, lint/build/57-doc.
Nine focused cases (eight plus guarded worker) passed snapshot-tests4.log:
WAL-only commit, exact schema8 pretty-printed receipt bytes, SQLITE_FULL,
cancellation, eight killed-process boundaries, private modes/links/aliases and
strict manifest/digest checks. Process death is not power-loss proof.

NEW real Linux CLI SIGINT proof passed at root defc8b9:
/tmp/s3drive-snapshot-sigint1.json and empty .stderr. A disposable valid authority
with128MiB of freelist pages received SIGINT after snapshot directory creation.
Cancellation exited2.919ms later in this one observation, without success stdout
or completion manifest. Source SHA-256 stayed identical; partial inspection and
same-destination retry refused. Owned processes were reaped before fixture
cleanup. Dated proof/reproducer: docs/src/evidence/authority-snapshot-sigint*
with binary and source hashes. This is not Windows console, physical power-cut,
restoration, mount or provider acceptance. A separate source-busy CLI check passed at the same binary/source:109.197ms
refusal under an independent SQLite EXCLUSIVE writer, no output/source changes,
then successful create/inspection after release. Proof/reproducer are
scope-limited docs/src/evidence/authority-snapshot-busy*. This is not Windows
locking or destination contention. Latest rebuild log:
/tmp/s3drive-snapshot-cancel-build1.log. No local Rust source changed after the
full gate. Pre-rebase57-doc links/build/tests passed at snapshot-cancel-final-book{,-tests}.log.

Combined-source gate passed. Prepared scripts/test-windows-snapshot-worker.ps1
is invoked from the existing ordinary-account AuthorityRemove workflow before
and after normal removal. Tests live-owner refusal, create/inspect, exact source
bytes, duplicate output, widened ACL restoration, hardlink/junction/case aliases
and concurrent read handle. AST validation passed for all three changed scripts and the exact embedded
worker at /tmp/s3drive-snapshot-windows-ast1.log. Updated57-doc links/build/
tests passed at /tmp/s3drive-snapshot-prepush-book{,tests}1.log.
Local parsing is not actual Windows acceptance.
Actual Windows CLI/ACL/reparse/sharing and native/reference/startup gates remain;
Windows console cancellation is also separate. Remaining snapshot gaps include
broader permission/destination-busy faults and writer-version field validation (currently
nonempty/length only). Do not mark snapshot phase or overall goal complete.

## Requirements, invariants and authorization

User chose managed layout/stronger cross-machine coordination. The Rust service
owns shared authoritative SQLite namespace metadata; clients keep private local
SQLite journals/cache and move immutable payloads directly to S3. Hosting is
undecided: retain SQLite-host and optional Turso choices. Turso is researched,
not enabled/provisioned; offline/last-push-wins replication cannot decide ownership.

Schema23/config7. Preserve dirty bytes, exact unknown logical/physical IDs,
guards, receipts, explicit conflicts and original data. No unconditional
overwrite/rebase fallback. Only online authoritative commit establishes shared
ownership. S3 upload alone does not acknowledge filesystem flush. Keep explicit
unsupported paths until their phase criteria pass; no fake retention ack.

Sign-in startup must support MX Linux WITHOUT systemd and same-account incoming
SSH/tmux sessions after disconnect, plus Windows. Per-account supervisors exist;
no real user startup entry was installed. Workstation NoNewPrivs=1 blocks native
FUSE here: NEVER bypass it through root, SSH or another launcher. Linux native
proof is hosted CI. Windows proof is hosted Server2025/WinFsp2.1.25156.ddca7bd,
not this Linux workstation or actual Windows10/11. Disposable CI accounts,
rights, hooks and owned fixtures are separate from real user provisioning.

Real Wasabi config remains read-only: alias wasabi, bucket s3drivetest,
region us-east-2, endpoint https://s3.us-east-2.wasabisys.com,
profile wasabi-s3drive. Credentials stay in local AWS mechanism; never print or
commit real config/keys or its private credential-generation digest. Preserve
ignored local config/plan/report. No new live provider mutation is authorized.

Earlier three-object Unicode/read fixture scope was cleaned. The later six-key
scope s3drive-conformance/d147830ba5257d762e14c3962cde60f9/ ran ONCE at
2026-10-16T23:36:18Z, source8eeda5b, all10 checks passed and six keys were deleted;
prefix verified empty. All14 allowed PUT attempts are consumed. Plan digest
98914e5210466c6c1e074ea36184ea580d20c93749363e18650e5ddf14906de6.
Source-bound evidence: docs/src/evidence/wasabi-conditional-put-2026-10-16.json.
No rerun, multipart or managed-provider mutations under that old approval.
New live mutation needs a concrete prepared scope and explicit authorization.
No root, user credentials, hosting, Turso account or real startup changes.

## Full-goal work still required

- Explain/correct classified Windows transport failures with direct evidence.
- Snapshot Windows/platform acceptance, history continuity, fencing and restore
  drills; independent-host partitions and secure actual deployment.
- Physical reboot/PAM/real SSH and actual target Windows10/11 acceptance.
- Approved real managed Wasabi acceptance; large/multipart files, reference/pin
  aware reclamation, explicit conflict/uncertain recovery and application saves.
- Product packaging/distribution, including WinFsp licensing decisions already
  recorded in docs/src/decisions.md. Preserve measured platform limitations.

Use bash scripts/gate.sh per checkout; native OS, provider and recovery gates
remain separate. Fetch completed jobs with gh api --allow-escape-sequences
repos/marshallr12/s3drive/actions/jobs/JOB/logs. For publication require exact
source/all-three terminal CI plus raw native/startup and strict21/14 artifacts,
then guarded FF/push main and reverify privacy/SHA. Do not force push or stop
at a regression-only pass.

## Handoff archive

The previous 860-line handoff was archived byte-identically before
refreshing this 200-line version. No historical decisions/evidence were deleted:
[2026-10-18 archive](docs/archive/HANDOFF-before-snapshot-cancel-20261018.md).
Archive SHA-256: 8d6eec5434162ea07035bb829ceec5b04936cc352f4939a574b6fe43c5580060.
Earlier archive: docs/archive/HANDOFF-before-flush-receipts.md.
