# S3Drive handoff

Updated 2026-10-18. Goal ACTIVE. Mira is the assistant; no voice handoff capability
was available. Read docs/src/implementation-plan.md before implementation.
Research, decisions, acceptance and plans belong in docs/src/.

## Checkouts and immediate next action

- Private local/remote main: a7994e6203ccb082c1b441d19806cfb6ed6d3929.
- Clean diagnostic worktree: /home/marshall/src/s3drive-windows-transport,
  branch codex/windows-transport-followup, privately pushed
  f8c4f5832e399c6c54567c122c875e2a438ce681.
- Exact-source CI39097275707 is IN_PROGRESS at the latest live check.
  Windows117586223172 remains active in early TLS focus; Linux117586223366
  and book117586223439 succeeded. Revalidate the same handles; never cancel/restart on observer
  timeout or a live job's log404.
- Root /home/marshall/src/s3drive is codex/authority-snapshots at local,
  UNPUSHED defc8b9. It adds the snapshot library and CLI. Current local changes
  add cancellation evidence/docs and this handoff refresh; preserve them.
- Both checkouts share /home/marshall/src/s3drive/target. Builds replace its CLI
  binary, so rebuild/verify the intended source before experiments. No local
  Cargo/gate jobs remain live. The most recent CLI build was root defc8b9 for
  the Linux SIGINT check.

First poll CI39097275707 and read completed native/startup job logs. Require
terminal exact-tip all-three success, raw native/startup proof and strict typed
21/14 source/platform artifacts before a guarded private main fast-forward from
a7994e6. Verify repository privacy before/after push; it was PRIVATE after f8.
No force push. Do not call diagnostics an attributed transport fix.

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

Current f8 adds the precise Hyper is_shutdown category, permitting ONLY that
phase with positive read/write/flush and dispatch counters AFTER all caller
reply/receipt assertions pass. Generic transport, truncation and timeout still
fail. The deterministic Hyper driver injected shutdown error only after a full
Content-Length reply was independently read. Do not retrospectively label the
old untyped event shutdown without classified runtime evidence.

f8 also observes current supervisor/child process priority on Windows, without
changing it. Getter/log failures cannot fail the launch. Task Scheduler default
priority/inheritance is only an unmeasured starvation hypothesis; primary
Microsoft links and limits are in docs/src/authority-startup.md. Root-only
upstream research in evidence/windows-tls-upstream-20261018.json records EOF
semantics and the already-present Hyper4143 flush guard; neither proves cause.

Local f8 Rust full gate PASSED exit0:
/tmp/s3drive-transport-shutdown-gate1.log (265 state, two I/O/Hyper cases, ten
HTTP, six TLS, seven CLI restart/worker,17 SDK, seven managed, lint/build/57-doc).
Two focused cases: /tmp/s3drive-transport-shutdown-tests1.log. The subsequent
PS-only observational logging passed fresh supervisor AST and all four actual
child policy cases at /tmp/s3drive-transport-priority-policy1.log. Final doc
links/build/tests passed /tmp/s3drive-transport-shutdown-book{,-tests}.log.
Native Windows/source-bound acceptance is still pending for f8.

Linux9b raw /tmp/s3drive-transport-linux1.log confirms all8 native cases, full
pair PASS04:05:50UTC, ordinary startup and both HTTP/TLS coordinator profiles.
Current f8 Linux also passed: raw /tmp/s3drive-transport-f8-linux.log confirms
all8 native cases, paired PASS04:37:25UTC, ordinary startup and both HTTP/TLS
coordinator profiles. Never reuse an older source's native proof for a new candidate.

## Unpublished offline snapshot prototype

Root local defc8b9 implements crates/s3drive-state/src/authority_snapshot{.rs,
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

Root full gate PASSED exit0 /tmp/s3drive-snapshot-gate1.log:274 state, two new
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
restoration, mount or provider acceptance. Latest rebuild log:
/tmp/s3drive-snapshot-cancel-build1.log. No local Rust source changed after the
full gate. New57-doc links/build/tests passed at snapshot-cancel-final-book{,-tests}.log.

Once f8 permits: carefully rebase/integrate the UNPUSHED snapshot checkpoint onto
accepted main, preserving the diagnostic changes and latest acceptance facts.
Then actual Windows/ordinary-account snapshot CLI, ACL/reparse/sharing and
native/reference/startup gates are required. Remaining snapshot gaps include
broader permission/busy faults and writer-version field validation (currently
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
