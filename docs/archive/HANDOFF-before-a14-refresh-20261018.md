# S3Drive handoff

Updated 2026-10-18. Goal ACTIVE. Mira is the assistant; no voice handoff capability
was available. Read docs/src/implementation-plan.md before implementation.
Research, decisions, acceptance and plans belong in docs/src/.

## Checkouts and immediate next action

- NEW latest private topic is a14ffa37b8e3c18a353c19c724bfe438b365ec6a, containing
  local7edc4f1 body diagnostic and a14 native32-flush gate. Both normally FF
  pushed together from e51; exact remote SHA/privacy and unchanged main f8 verified.
  CI39106656359 LIVE: Linux117613851099, Windows117613851164, book117613850947.
  Watch session72681, /tmp/s3drive-body-burst-a14-watch.log. First action: inspect
  this exact run/handle; do not restart old runs or infer death from timeout.
  Windows raw remote/TLS body-marker controls now run before standalone/burst,
  so validate true/false controls on Windows before interpreting native failures.
  New native burst runs once early after standalone, then all old lifecycle/
  workspace/pair/reference/broad gates remain; Linux expected9 native cases.
  Prepared NOT RUN /tmp/s3drive-check-a14-cohort.py and
  /tmp/s3drive-a14-reference-check.ps1 require terminal all-three SUCCESS and new
  strict21/14 artifacts in /tmp/s3drive-body-burst-a14-artifacts. Never use old
  e51/f8 artifacts. Main f8 stays until raw full native/startup plus strict proof.
  Only this post-push observation is uncommitted; bundle with later substantive
  source rather than docs-only native CI. The older paragraphs below are history.
- Previous pushed topic e51f7371dd9a4f0ce0ccdb91332ab1218cad50ac. Normal
  private FF push and exact remote/main SHA/privacy verified. CI39103791947 is
  TERMINAL FAILURE: Linux117605270625/book117605270620 passed; Windows117605270355
  passed early standalone snapshot/console, lifecycle, workspace, paired startup/
  snapshot/console and CI reference checks, then broad native6passed/1failed.
  Raw /tmp/s3drive-connection-lifetime-e51-{linux,windows,book}.log.
  Existing writes failed at managed.rs1708 file.sync_all(), native1117,
  Info/Request/UnexpectedEof (not client timeout); nearby empty HTTP TLS socket24
  timed out after5015ms. No exact RPC/socket correlation. Another List/EOF near
  socket228 occurred inside a passing mutation case; do NOT assume intended.
  Explicit Connection: close did NOT eliminate the failure. Main stays f8.
  Console cancellation observed15ms standalone/6ms paired, source bytes intact,
  no success/manifest, partial/replacement refused; one observation each.
  Watch68564 is obsolete/terminal; do not rerun old jobs. Prepared full-success
  /tmp/s3drive-check-e51-cohort.py and /tmp/s3drive-e51-reference-check.ps1 NOT RUN;
  new21/14 are not independently accepted. No promotion of failed e51.
  NEW uncommitted diagnostic: public request-body frame polling, independently
  controlled received bytes versus failed trust. No socket-written/commit
  inference. wire.rs + remote.rs focused authority tests passed at
  /tmp/s3drive-body-poll-focused1.log (Hyper2/lifetime2/HTTP10/TLS6).
  Full local gate PASSED exit0 /tmp/s3drive-body-poll-gate1.log:274 state,
  Hyper2/lifetime2/HTTP10/TLS6, snapshot CLI2, restart8, SDK17, managed8,
  lint/build/57-doc. Rust stayed frozen. Ready for local diagnostic checkpoint.
  Checkpoint7edc4f1 is now pushed under a14. The subsequent phase adds a common
  32-flush native HTTPS fixture + Linux/Windows ignored wrappers, failure-only
  control/cleanup/rethrow, early Windows step and no duplicate broad invocation.
  Exact binary bytes/fresh reads and one PUT per changed generation required;
  no injected outage, tolerated error, explicit retry or provider mutation.
  cargo check CLI tests and PyYAML workflow structure passed at
  /tmp/s3drive-flush-burst-check1.log. Combined full local gate PASSED exit0:
  /tmp/s3drive-flush-burst-gate1.log (274 state, Hyper2/lifetime2/HTTP10/TLS6,
  snapshot CLI2, restart8, SDK17, managed8, lint/build/57-doc). Rust stayed frozen.
  Native results NOT established: expected Linux9 and Windows earlyburst1 +
  lifecycle1 + broad7. Keep real provider/startup/root/hosting changes out of scope.
- Previous pushed topic d6d8bd915d7c04764013071f152076740959fd68:
  CI39102804077 is TERMINAL FAILURE: Linux117602351415/book117602351527
  succeeded; Windows117602351510 passed early standalone snapshot and private
  console Ctrl+C, then failed HTTP native lifecycle. Raw
  /tmp/s3drive-native-failure-probe-d6-windows.log. Cancel16ms is one observation:
  no success/manifest, source bytes preserved, partial/replacement refused,
  offline original-identity control passed. Later PendingRemoval request timeout
  caused native5 at lifecycle.rs364. Failure-only fresh CLI got OriginalVolume
  in28ms with both processes alive; not proof of earlier availability/outcome.
  Later Windows gates skipped. Main stays f8; no snapshot acceptance.
- Current pushed candidate declares Connection: close per metadata RPC.
  Only production change is that header in WireClient; no retry/deadline/TLS/
  dependency/pool-builder change. Two bounded socket tests passed:16 distinct
  transports versus baseline1, and client EOF after complete reply even when
  peer omits close. This proves policy, NOT a Windows reproducer/root cause.
  /tmp/s3drive-connection-lifetime-after2.log. Full local gate PASSED exit0:
  /tmp/s3drive-connection-lifetime-gate1.log (274 state, connection-lifetime2,
  Hyper2/HTTP10/TLS6, snapshot CLI2, restart/worker8, SDK17, managed8, lint/
  build/57-doc). Rust stayed frozen. Published as e51f737;
  new exact-source hosted acceptance is required, not a claimed timeout fix.
  Post-push HANDOFF/platform JSON and Linux packaging-baseline research are uncommitted; bundle
  with later substantive source rather than docs-only native CI. Fresh e51
  source-bound CLI SIGINT/busy checks passed; shared binary rebuilt from e51.
  /tmp/s3drive-e51-snapshot-{sigint,busy}.{json,stderr}; one1.995ms cancel and
  112.498ms busy refusal observation, source SHA preserved, no restore grant.
  Independent artifact validator now checks exact root fields as well as typed
  records. Known-good f8 passed; four controlled faults refused at
  /tmp/s3drive-artifact-envelope-controls1.log. Prepared (NOT RUN) full download/
  source/digest/oracle driver /tmp/s3drive-check-e51-cohort.py.

- Private local/remote main is now accepted diagnostic f8c4f5832e399c6c54567c122c875e2a438ce681.
  Topic CI39097275707 is terminal all-three SUCCESS, raw Linux/Windows native/
  startup logs and strict21/14 artifacts checked before guarded FF from a7994e6.
  Remote/main SHA and repository PRIVATE verified after push.
- Diagnostic worktree /home/marshall/src/s3drive-windows-transport remains clean
  at f8, branch codex/windows-transport-followup.
- Main repeat CI39099187117 is TERMINAL all-three SUCCESS, with raw native/
  startup and strict21/14 checked independently and by project PS oracles.
  Windows117591756871 standalone PASS05:28:48UTC, pair05:30:11, lifecycle and
  seven broad cases; Linux117591756859 all8 native and no-systemd startup.
  Raw /tmp/s3drive-main-f8-repeat-{linux,windows,book}.log, run metadata and
  /tmp/s3drive-main-f8-repeat-artifacts/{accepted-repeat,independent-check,
  download-proof}.json. Both downloaded ZIP digests match API metadata.
  Artifacts12126286440/12126586047. This pass does not explain the old timeout.
- Root codex/authority-snapshots was safely rebased onto f8. Current local commits
  f4c37bb/e218381/01af668 correspond to the old UNPUSHED defc8b9/43bfd46/e0a4353.
  Backup ref codex/authority-snapshots-before-transport retains old history.
  Config/plan/report private hashes stayed identical through rebase. Rust
  snapshot code and all f8 diagnostics were preserved; only docs conflicted.
- Snapshot candidate f9a52e2d4fcd575ea4ae73fd4e4a6e6ce75e9566 is now pushed to
  private origin/codex/authority-snapshots; exact remote SHA/privacy verified.
  CI39100010372 is TERMINAL FAILURE: Linux/book success; Windows117594176311
  snapshot CLI2/state269 and ordinary-account snapshot checks passed in both
  standalone05:44:01UTC and pair05:45:39UTC workflows. Later broad native tests
  failed: existing writes List/UnexpectedEof+native1117; mutation seed child
  Info/UnexpectedEof. TLS connections22/43 each had read0/write0/flush1 and
  dispatched=false before5sec header timeout. Not correlated to specific RPCs.
  Raw /tmp/s3drive-snapshot-f9-windows.log. Linux/book logs
  /tmp/s3drive-snapshot-f9-{linux,book}.log: snapshot CLI2/state274, all8
  native cases, paired PASS05:22:17UTC, ordinary no-systemd and HTTP/TLS
  authority startup passed. No snapshot main promotion.
  Combined-source gate PASSED exit0 at
  /tmp/s3drive-snapshot-integrated-gate1.log (274 state, snapshot CLI2,
  Hyper2/HTTP10/TLS6/SDK17/managed7 plus lint/build/57-doc).
- Both checkouts share /home/marshall/src/s3drive/target. Builds replace its CLI,
  so verify/rebuild the intended source before experiments. The integrated gate
  rebuilt the CLI; historical SIGINT/busy proofs remain bound to old defc8b9.

- Prior pushed topic33ddfe1366c901022cf516deb648dfeb31888dc0 had a normal FF
  push and exact remote SHA/privacy verified.
  CI39100806213 is TERMINAL FAILURE: Linux/book success, Windows117596502607
  failed the early lifecycle test at support/managed/lifecycle.rs404 cross-store
  replacement. Numeric-loopback HTTP (NOT TLS); Info/Request timeout, then
  PendingRemoval timeout and native5. No server-side commit/phase proof.
  C# console fixture and later Windows gates SKIPPED. Raw
  /tmp/s3drive-snapshot-console-33-windows.log. Later d6 signal component passed.

- New clean detached worktree /home/marshall/src/s3drive-snapshot-acceptance
  stays at failed33 for source inspection; no builds there. Prepared (NOT RUN)
  /tmp/s3drive-snapshot-console-33-reference-check.ps1 must not be treated as
  proof.33 has only replacement artifact; disposition step never ran.

Next: inspect CI39106656359 at exact sourcea14; raw controls and early burst
precede the full required cohort. Both new phases are pushed; no further push
is needed merely to record live observations. Require new
exact-source early Windows burst, all native/reference/startup checks before
main promotion. Previous e51 remains failed; do not restart its jobs. CI39103791947 is failed, not live. Require terminal Windows/native/reference/
startup evidence before any promotion. Require
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
Snapshot code is on private topic f9a52e2, not main or the diagnostic worktree.

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
were inspected; Windows ACL/reparse/handle component checks later passed at f9/d6;
full candidate native acceptance remains pending.

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
Actual Windows CLI/ACL/reparse/sharing checks passed in the scoped f9 component
workflows, but full candidate native gates failed as above. Windows console
cancellation subsequently passed the d6 component; full native acceptance remains. Remaining snapshot gaps include
broader permission/destination-busy faults and writer-version field validation (currently
nonempty/length only). Do not mark snapshot phase or overall goal complete.

## Live observations and prepared verification

- Main repeatCI39099187117 is complete/success as above. Source f8 has no
  snapshot code; its success must not substitute for the newer candidates.
  Standalone supervisor6904 retained while child2260→1268; pair supervisor3000
  retained while3100→2264. Independent AuthenticationIds, exact recovery and
  conflicts passed. Only expected partial-header HttpTimeout logged; no untyped
  or shutdown event in this repeat. Priority values still unmeasured.
- All three old runs (main f8, snapshot f9, console33) are terminal. Watch
  sessions63097/94801/7835 may lag; do not restart or wait as if jobs were live.
- Independent /tmp/s3drive-check-native-artifacts.py reads reference fixtures
  via exact git-show source; strict duplicate/nonfinite JSON, exact field/type/
  value checks,21/14 records, source/OS/driver and four legacy cases. Passed
  known-good f8 artifacts; rejected bool-as-int/missing/extra/duplicate faults.
  Use it on the NEW candidate artifacts, never substitute old success.
- NEW follow-up adds scripts/test-windows-snapshot-console.cs and
  Cancel phase/owned cleanup to the three Windows fixture PS files. It uses
  only a freshly allocated verified private console (no AttachConsole), checks
  helper/child membership, requires Cancelled/no stdout/no manifest, compares
  source bytes and refuses partial/replacement. Actual Windows signal component passed later at d6.
  The Rust authority_restart fixture adds SnapshotPad before managed dispatch:
  production owner lock plus separate SQLite connection,128MiB free pages,
  connections closed before hashing. Portable8MiB exact receipt-byte control
  passed /tmp/s3drive-snapshot-console-padding2.log. New C# compiles with mcs
  -warnaserror; fresh PS/embedded AST passed snapshot-console-ast1.log.
  Full local gate PASSED exit0 /tmp/s3drive-snapshot-console-gate1.log:
  274 state, eight restart/worker (new portable fixture), two snapshot CLI,
  Hyper2/HTTP10/TLS6/SDK17/managed7 plus lint/build/57-doc. Later PS-only
  original-identity control and numeric-type validation passed fresh AST at
  /tmp/s3drive-snapshot-console-final-ast2.log; C#5/.NET4.5 mcs compilation
  with warnings-as-errors passed. f9 CI tests the previous immutable source.
- Console follow-up33 was pushed after full local gate, final PS AST/C#5
  compilation and final57-doc links/build/tests. Rust stayed frozen during gate;
  the subsequent PS-only positive-control/type checks passed fresh AST.
- Pushed ind6d8bd9: failure_probe/report_native_failure helpers in
  support/managed.rs, Windows lifecycle catch/report/reap/resume-original-panic,
  and a managed_native.rs portable positive/negative probe. Fresh actual CLI
  Info uses a separate client/runtime, bounded pipes/waits and owned handles;
  no mutation retry or hidden PASS. Live/stopped control passed at
  /tmp/s3drive-native-failure-probe1.log (unchanged volume sequence, zero S3).
  Subsequent cleanup was factored/typechecked on Linux; full local gate PASSED
  exit0 at /tmp/s3drive-native-failure-probe-gate1.log:274 state, eight restart/
  worker, snapshot CLI2, Hyper2/HTTP10/TLS6/SDK17/managed8 plus lint/build/57-doc.
  Rust stayed frozen; later workflow/doc changes have separate checks.
- CI standalone Windows coordinator/snapshot/console step now runs early after
  real TLS/CLI compile, before replacement/lifecycle. It still runs exactly once;
  all existing native/pair/reference gates and deadlines retained. PyYAML parse/
  structure checks passed (actionlint not installed). No test is skipped to
  admit the candidate. No old CI restart.
- Rejected LOCAL experiment: Hyper capture_connection observer. Reqwest0.13.5
  converts extensions but execute_request discards them before Hyper; controlled
  received headers/body still yielded false. Fault tests failed at
  /tmp/s3drive-wire-selection-remote1.log. ALL authority code/Cargo/test edits
  restored exactly toHEAD33; no false flag shipped and no assertion weakened.
  Exact resolved source0.1.21/0.13.5 fingerprints in the dated platform record.
- Pushed ind6d8bd9: refreshed historical managed layout/publication/write/mutation/
  Windows-plan descriptions to distinguish old config phases from current main;
  links/build/test passed historical-status-book{,tests}1.log before latest
  platform records. New record authority-snapshot-platform-20261018.json and
  authority-startup/snapshot/implementation-plan explain partial component PASS,
  both overall failures and the failed capture experiment. Main-repeat record
  was committed with d6. Bundle with substantive probe source; no docs-only native CI.

## Packaging research observation

New docs/src/evidence/linux-runtime-baseline-20261018.json binds e51 debug binary
fca6f0e893bfe85365bd2121e05cd64cb73b2bcbb361880a41b7448c4e377e86 to observed
Debian12/glibc2.36, highest ELF requirementGLIBC_2.34, and actual --version smoke.
This is not release/MX/reboot acceptance. platform-integration.md proposes an
explicit compatible release baseline;57-doc links/build/test passed at
/tmp/s3drive-linux-baseline-book{,-tests}.log. Keep with next substantive source.

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
