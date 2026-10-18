# S3Drive handoff

Updated 2026-10-18. Goal ACTIVE. Mira is the assistant; no voice-context handoff
capability was available. Read this and docs/src/implementation-plan.md before
implementation. Research, decisions and acceptance records belong in docs/src/.

## Current source and first action

- Private topic codex/authority-snapshots is e9887dfc8a73cae80b0f30a01f8ed29184911c1b,
  normal FF pushed from949; exact remote SHA/privacy and main f8 rechecked.
- CI39111567111 at e988 is TERMINAL FAILURE. Linux117628636755/book117628636627
  SUCCESS; Windows117628636806 failed paired managed startup/recovery after raw
  controls, standalone snapshot/console,32flush, full lifecycle and workspace
  (269state) passed. Raw /tmp/s3drive-connector-e988-{linux,windows,book}.log.
  Paired Info/Request/UnexpectedEof at08:52:23Z had body_frame_polled=false,
  connector_started=true AND connector_completed=true. Thus a scoped connector
  returned successfully before the failure; no exact socket/dispatch correlation.
  Nearby emptyTLSsocket133 HttpTimeout5083ms/read0/write0/flush1/dispatched=false.
  Semantic failure: lost upload response did not retain exactly one uncertain PUT;
  actual provider upload/commit NOT proven. Secondary locked disposable profile
  cleanup failed; preserve original failure. Later14-reference/broad steps skipped.
  Process-class logs show both supervisors/children BelowNormal in this paired
  run; actual thread priorities and any causal connection remain unmeasured.
  Standalone cancellation16ms is a single observation. No artifact acceptance or
  main promotion. Prepared e988 cohort/oracle scripts MUST NOT run for failure.
- NEW loopback-tag source (parent e988) remains UNCOMMITTED. Full local gate1 EXIT0,
  /tmp/s3drive-loopback-tag-gate1.log; Rust/Cargo fingerprints unchanged.
  Counts: authority6unit, lifetime2/execution1/HTTP10/TLS9, state274, remaining
  workspace suites/build/57-doc links/mdBook build/test. Session43574 is closed.
  Authority focused2 and CLI2 controls exit0. Final docs/source/privacy review,
  then normal topic push to obtain NEW hosted proof are next.
  e988 results never establish acceptance for the new working tree.
- Local new source records one canonical loopback candidate with only family/
  client+server ports, omitted for other/missing metadata and ambiguous after
  multiple successes. Server adds the same sanitized tag before final dispatched=;
  original failure classes/assertions stay. Scope/concurrent/missing/multiple,
  independent lost mutation TCP tags and actual CLI failure-event tags pass.
  No native fixture or workflow changes. All new source/research/probe archive
  should be bundled in the next substantive checkpoint; no docs-only native CI.
- Main remains f8c4f5832e399c6c54567c122c875e2a438ce681. No new candidate accepted.
- CI39108821537 at949 is TERMINAL FAILURE: Linux117620351955 and book117620351830
  SUCCESS; Windows117620351957 failed broad native existing-file writes. Raw logs
  /tmp/s3drive-rpc-dispatch-949-{linux,windows,book}.log reviewed. Windows controls,
  unchanged32flush, complete lifecycle, workspace269state, standalone/paired
  recovery/snapshot/console and project21/14 reference steps passed. Broad6passed/
  1failed: managed.rs1758 existing nonempty Unicode file open, List/Request/
  UnexpectedEof/native1117/body_frame_polled=false. Nearby empty TLS socket19
  HttpTimeout5057ms/read0/write0/flush1/dispatched=false is NOT exactly correlated.
  Deliberate authority-stop List/Connect timeout in the immutable-read case is
  expected by that fixture; no other broad transport warning found. Do not treat
  the older e51 passing-mutation List/EOF as classified. No main promotion.
- The earlier e988 checkpoint adds request-scoped connector call/success
  flags through public reqwest connector_layer. Layer captures task-local Arc at
  call; one monotonic atomic snapshot; no destinations/headers/bodies/errors.
  Public layering changes connector boxing/scheduling; no causal fix claim.
  Direct tower-layer/service0.3.3 dependencies already resolved, no version churn.
  Local focused controls passed; gate1 caught Clippy async-yields-async in test,
  corrected with inspected sync_scope. Full gate2 EXIT0, log
  /tmp/s3drive-connector-observation-gate2.log:274state, authority3unit including
  connector1, lifetime2/execution1/HTTP10/TLS9, other workspace suites/build/57docs.
  Rust/Cargo fingerprint check unchanged. First action after publication: inspect
  exact new CI using gh run list/view; no restart of the failed949 source.
- A14 CI39106656359 is TERMINAL FAILURE: Linux/book success, nine Linux native
  including32flush passed; Windows six raw controls + standalone snapshot/console
  passed, then fourth flush failed Apply/UnexpectedEof/native1117 with
  body_frame_polled=false. Nearby empty TLS socket32 timed out5105ms, not directly
  correlated. Fresh CLI OriginalVolume152ms after failure, both processes alive.
  Later Windows steps skipped. Raw /tmp/s3drive-body-burst-a14-{linux,windows,book}.log.
  Watch72681 is closed/obsolete. No artifact acceptance or main promotion.
- The new source runs all authority wire/execution controls early on Windows,
  then the unchanged native32-flush case and every prior gate. At the connector
  candidate expect nine TLS controls, authority unit6, execution1, lifetime2/HTTP10;
  Linux9 native, Windows burst1+
  lifecycle1+broad7. Validate actual counts in raw logs, not a step label alone.
- Prepared NOT RUN /tmp/s3drive-check-949-cohort.py and
  /tmp/s3drive-949-reference-check.ps1 require terminal all-three SUCCESS, then
  exact-source ZIP digests, strict21/14 and project oracles in
  /tmp/s3drive-rpc-dispatch-949-artifacts. Do not use a14/e51/f8 artifacts.
  Raw logs are /tmp/s3drive-rpc-dispatch-949-{linux,windows,book}.log; failed cohort
  checker MUST NOT run. Prepare a new source-bound checker for the next cohort.
- The connector checkpoint bundles prior post-push949 observations and the
  offline Wasabi option; no provider requests or new mutation approval. Archive
  and runtime code/tests/docs/evidence were committed/pushed in949.
- Other clean worktrees: /home/marshall/src/s3drive-windows-transport at f8 on
  codex/windows-transport-followup; /home/marshall/src/s3drive-snapshot-acceptance
  detached at failed33ddfe1 for inspection only. All share
  /home/marshall/src/s3drive/target; rebuild/verify the intended source before
  using that executable, because another checkout's build replaces it.

## Parallel read-only transport investigation (2026-10-18)

The following investigation was completed before further production changes.
A new bounded loopback-tag diagnostic phase is now being implemented aftere988;
its acceptance criteria are in implementation-plan. Existing CI39111567111
remains evidence for exacte988; do not claim its results for the new working tree. Upstream4208 stale want fix is already
in resolvedHyper1.12.0/want0.3.2. Separate4222 shutdown/drain fix mergedOct9 AFTER
latest release1.12.0 Oct6 and is absent locally; do not attribute our EOFs to it
without proof (different queued-request hang mechanism).

A standalone public connector metadata control passed HTTP/HTTPS complete+lost
reply cases on local Linux, comparing public Connection::connected/get_extras
HttpInfo local/remote addresses with independently accepted TCP metadata. Four
PASS lines, exit0 /tmp/s3drive-public-connector-probe2.log. Scratch directory
/tmp/s3drive-public-connector-20261018-jdfysrlm; pointer
/tmp/s3drive-public-connector-probe-location.txt. Reproducible source/manifest/lock
archive docs/src/evidence/public-connector-probe-20261018.tar.gz; hashes and limits
in platform JSON. No credentials/provider requests and no Windows/native proof.
This avoids the earlier unsupported request-extension observer, but a connector
candidate is STILL not proven to be the selected failed RPC socket. Adoption
needs bounded loopback-only correlation, missing-metadata and concurrency tests;
do not expose addresses or infer safe replay. API source contracts inspected.
Local links/build/test passed for both upstream discussion and the new probe link:
/tmp/s3drive-e988-public-info-book{,-tests}.log; no production source change.

## Runtime-owned RPC candidate (locally green; pushed949, hosted FAILED)

WireClient now clones the exact request once and runs the RPC in a caller-scoped
JoinSet job on the current runtime, with10-second absolute cooperative queue+
RPC deadline; connect2/reqwest10 remain. Drop aborts owned jobs, not already
accepted remote effects. New fixed Execution errors preserve unknown outcomes.
No operation IDs, retries, trust, framing, connection policy or native32-flush
acceptance changed. This is an execution-context candidate, not root-cause proof.

New tests: queued cancellation leaves no task/TCP connection and same client
works afterward; foreign-thread real TLS6 commits/receipt lookups/exact replays;
caller canceled after actual SQLite commit still recovers original receipt with
one sequence advance and no second request. Early Windows authority controls
now include all test binaries (execution plus eight TLS cases); all other CI
steps are byte-identical to a14. Structural comparison passed.

Focused /tmp/s3drive-rpc-dispatch-focused2.log exit0 (Hyper2/lifetime2/execution1/
HTTP10/TLS8). Full /tmp/s3drive-rpc-dispatch-gate1.log exit0:274 state, those
controls, snapshot CLI2, restart8, SDK17, managed8, lint/build/57-doc. Rust stayed
frozen. Final docs and source/archive-hash checks passed, then949 was pushed
privately without force. Next: inspect CI39108821537 at exact949, including
Windows controls and unchanged early burst. This run now failed later broad writes;
see current state above. Keep main at f8 pending new full proof.

## What a14 changes and what is verified

7edc4f1 adds a per-request body_frame_polled diagnostic through reqwest's public
Body wrapper. It snapshots HTTP data-frame polling at transport-error
classification. True is NOT proof of a socket write, selected connection,
server receipt, authentication or commit. False is NOT a safe unsent result and
cannot authorize replay, new IDs, rebasing or overwriting.

It preserves the original JSON body, framing hints and errors. Real TLS controls
independently received byte-identical Apply JSON, Content-Length/application/json
and no chunked framing. Lost/truncated/stalled replies observe true; wrong
certificate trust observes false before HTTP. No second mutation/redirect and
redaction assertions remain. The wrapper is non-cloneable, unlike the original
buffered variant: do not claim all scheduling details unchanged. No dependency,
timeout, retry, trust or connection-policy option changed in this phase.

Actual resolved APIs were inspected: reqwest0.13.5, Hyper1.12.0, http-body1.1.0;
source fingerprints are in docs/src/evidence/authority-snapshot-platform-20261018.json.
The earlier Hyper capture_connection experiment was REJECTED: reqwest drops
arbitrary request extensions in execute_request. Controlled received bytes still
gave false. That observer is removed; do not resurrect its unsupported flag.

The existing e51 Connection: close policy remains: one exchange per HTTP/1.1
transport, matching the TLS listener's existing finite lifetime. Sixteen sequential
calls used16 sockets versus baseline1, and a raw peer omitting close saw client
EOF after a complete reply. This did NOT eliminate the Windows failure.

A14 adds a real native32-flush fixture to the common managed test harness, with
Linux/Windows ignored wrappers. It uses the existing Unicode empty-file path,
32 distinct32-byte binary generations, fresh reads, and exactly one additional
SDK PUT per successful flush. Close cannot repeat a settled PUT. There is no
injected outage or tolerated failure. At first unexpected error, the read-only
CLI control checks owned process liveness/original volume, reaps owned children
and resumes the original panic. The fixture does not retry a failed step.
Native filesystem work uses block_in_place so its S3 fixture can progress.

CI now runs Windows raw TLS body-poll controls before standalone startup and
before the early native burst. Check those controls before interpreting a
Windows marker. The burst runs once before replacement/lifecycle/workspace;
the later broad step excludes only this and the lifecycle already run explicitly.
Every previous gate remains. Expected native counts are Linux9, Windows early
burst1 + lifecycle1 + broad7. Actual a14 Windows burst failed as recorded above.

Local verification (Rust frozen during each gate):
- Body phase: /tmp/s3drive-body-poll-focused1.log and
  /tmp/s3drive-body-poll-gate1.log, exit0.
- Combined fixture: /tmp/s3drive-flush-burst-check1.log and
  /tmp/s3drive-flush-burst-gate1.log, exit0:274 state, Hyper2/lifetime2/HTTP10/
  TLS6, snapshot CLI2, restart/worker8, SDK17, managed8, lint/build/57-doc.
- Windows-only early raw-controls workflow step was added after the combined
  Rust gate. Rust remained identical; fresh PyYAML structure checks passed,
  proving two added early steps, retained old steps and one burst invocation.
  Final links/build/tests and later links/build passed; book CI is SUCCESS.

## Exact-source acceptance and publication

Require all three jobs terminal SUCCESS, then inspect raw native/startup logs,
including the new early Windows controls/burst and all nine Linux cases. Verify
ordinary-account snapshot/console checks after standalone AND paired recovery,
independent logons, original volume/receipts, exact unknown IDs/no repeated PUT,
Unicode/binary/empty data, preserved dirty conflict and independent removal.
Unit/portable tests cannot substitute for native/provider/recovery acceptance.

Fetch only completed jobs with:
`gh api --allow-escape-sequences repos/marshallr12/s3drive/actions/jobs/JOB/logs`.
Save a14 logs under /tmp/s3drive-body-burst-a14-{linux,windows,book}.log.

Prepared, NOT RUN:
- /tmp/s3drive-check-a14-cohort.py: refuses incomplete/failed/wrong-source CI;
  checks repository privacy, exact two artifacts/source, download ZIP digest,
  bounded exact archive members, then independent strict JSON and project oracles.
- /tmp/s3drive-a14-reference-check.ps1: project21/14 oracles on exact a14.
- /tmp/s3drive-check-native-artifacts.py: duplicate/nonfinite JSON rejection,
  exact root/record fields and Python types (bool is not int), every21/14 record,
  source/OS/driver, and four pinned upstream legacy cases. Known-good f8 passes;
  extra/missing/duplicate fields and bool-for-int faults refuse at
  /tmp/s3drive-artifact-envelope-controls1.log.

New artifacts must be in /tmp/s3drive-body-burst-a14-artifacts. Do not substitute
any f8/e51 artifact. Before main promotion, check exact source/remote main/private
state, all raw proof and strict artifacts; use guarded normal FF and recheck
privacy/remote SHA afterward. Main is not checked out in a worktree; preserve
all dirty files. A green run does not itself explain older intermittent errors.

## Prior accepted and contradictory evidence

Main f8 primary CI39097275707 and unchanged-source repeat CI39099187117 were
terminal all-three SUCCESS, raw native/startup and strict21/14 independently
checked before/after guarded private publication. Durable record:
docs/src/evidence/authority-transport-20261018.json. Repeat artifacts and proof:
/tmp/s3drive-main-f8-repeat-artifacts/{accepted-repeat,independent-check,
download-proof}.json; primary proof /tmp/s3drive-f8-publication.json.

F8 categorizes ONLY Hyper is_shutdown as HttpShutdown. It is permitted only after
complete caller receipts/sequence proof and positive read/write/flush/dispatch
counters. Deterministic full Content-Length reply before injected shutdown is
proved. Generic transport, truncation and timeout still fail. This does not
attribute older untyped events, scheduler behavior or the original timeout.

Subsequent snapshot candidates all FAILED overall; preserve partial component
passes and failures in docs/src/evidence/authority-snapshot-platform-20261018.json:
- f9a52e2 CI39100010372: Windows snapshot269/CLI2/ordinary-account ACL/link/handle
  checks passed, later existing writes List/EOF and seed Info/EOF failed.
- 33ddfe1 CI39100806213: early lifecycle numeric-loopback HTTP Info timeout then
  PendingRemoval timeout/native5; console workflow never reached.
- d6d8bd9 CI39102804077: snapshot/isolated Ctrl+C component passed (16ms one
  observation); later HTTP lifecycle failed. Fresh read-only CLI got original
  volume in28ms after failure with both processes alive, not proof of earlier
  availability or any prior mutation outcome.
- e51f737 CI39103791947: Linux/book, Windows lifecycle/workspace/standalone/pair/
  snapshot/console passed, broad native6passed/1failed. At managed.rs1708,
  file.sync_all failed native1117, Info/Request/UnexpectedEof (not client timeout).
  Nearby TLS socket24:5015ms/read0/write0/flush1/dispatched=false. No RPC/socket
  correlation. List/EOF near empty socket228 occurred inside a passing mutation
  case; intent/cause not established, do not silently call it expected.
  Raw /tmp/s3drive-connection-lifetime-e51-{linux,windows,book}.log.
  Console15ms standalone/6ms paired are single observations. No main promotion.

Older a799 same-source repeat CI39092631904 failed store-apply seed before mount
with Request/UnexpectedEof/server HttpTimeout; no commit/outcome attribution.
A server's empty TLS socket can be a background connection: actual Hyper-util
0.1.21 races pool checkout/connect. No proof binds those sockets to failed RPCs.

## Snapshot contract and remaining acceptance

Private offline create|inspect prototype only; no restore/failover. Library:
crates/s3drive-state/src/authority_snapshot{.rs,/native.rs}; platform directory
helpers; CLI snapshot.rs and tests/authority_snapshots.rs. Requires exclusive
existing authority and explicit volume/policy, no migration, credentials or S3.
New private output outside source/mount trees; snapshot.lock/sqlite3/json never
live filenames. Defaults512MiB/30sec,32-page copy steps, cooperative cancellation,
SQLite integrity/identity, close/reopen validation, synchronization and hash,
manifest LAST. Post-marker errors are CompletionUnknown: preserve and inspect
that exact output, no automatic deletion/retry/new name.

Never raw-open/close a DB inode while a SQLite connection in that process is
active: POSIX locks can be released. Hash/recheck after connections close.
Validation is sqlite-integrity-authority-identity-v1, not a full history/freshness
or semantic audit. Inspection always denies continuity/restore. Same-ID stale
history forgetting an acknowledged operation was measured at7962058; identity
and integrity are insufficient. Broader destination-busy/permission faults and
writer_version validation (currently nonempty/length only) remain work.

E51 source-bound Linux SIGINT/busy CLI checks passed after rebuilding the shared
binary, hash fca6f0e893bfe85365bd2121e05cd64cb73b2bcbb361880a41b7448c4e377e86.
/tmp/s3drive-e51-snapshot-{sigint,busy}.{json,stderr}: cancellation1.995ms,
source-lock refusal112.498ms, source bytes unchanged, no false completion or
replacement, create/inspect positive after lock release. Single observations,
not guarantees or power-loss/restore/native/provider acceptance. Original older
SIGINT/busy reproducers and dated records remain in docs/src/evidence/.

## Requirements and authorization

User chose managed layout/stronger coordination. Rust authority owns shared
SQLite namespace; machines keep private SQLite cache/journals outside mount and
transfer immutable payloads directly to S3. Hosting undecided: retain SQLite-host
and optional Turso choices. Turso is researched, not provisioned/enabled; offline
replica/last-push-wins state cannot decide shared ownership.

Schema23/config7. Preserve dirty bytes, exact unknown physical/logical IDs,
guards, receipts and explicit conflicts. No unconditional overwrite/rebase/new-ID
fallback. Only online authoritative commit establishes ownership; S3 PUT alone
does not acknowledge flush. Keep unsupported paths and explicit retention ack.

Sign-in startup must support MX WITHOUT systemd and same-account incoming SSH/
tmux after disconnect, plus Windows. No real user startup entry was installed.
NoNewPrivs=1 blocks local native FUSE: NEVER bypass through root/SSH/other launcher.
Use hosted native Linux. Windows proof is Server2025/WinFsp2.1.25156.ddca7bd,
not this Linux workstation or actual Windows10/11/reboot. Disposable CI account/
rights/hooks/fixtures are distinct from real user provisioning. Locked fixture
profiles may defer only under the documented successful-acceptance/ephemeral-VM
policy after owned-process and dummy-credential cleanup; never hide a test failure.

Real Wasabi remains read-only: alias wasabi, s3drivetest, us-east-2,
https://s3.us-east-2.wasabisys.com, profile wasabi-s3drive. Preserve ignored local
config/plan/report. Never print/commit keys, configuration source on error or the
private credential-generation digest. No new live mutations are authorized.
Earlier three-object Unicode/read scope was cleaned. Later six-key prefix
s3drive-conformance/d147830ba5257d762e14c3962cde60f9/ ran ONCE Oct16T23:36:18Z at
8eeda5b,10 checks passed, six keys deleted/prefix empty; all14 PUT attempts consumed.
Plan digest98914e5210466c6c1e074ea36184ea580d20c93749363e18650e5ddf14906de6;
evidence docs/src/evidence/wasabi-conditional-put-2026-10-16.json. Do not reuse that
approval for probes/multipart/managed writes. New mutations need a prepared exact
scope and explicit approval. No root/user credentials/hosting/Turso/startup changes.

## Prepared provider option (NOT authorized or executed)

An offline fresh six-key conditional probe plan was generated from the real
wasabi source at949, without credentials resolution or provider requests:
/tmp/s3drive-wasabi-next-20261018-wz8t4v1w/probe-plan.toml (mode0600).
Location pointer /tmp/s3drive-next-wasabi-plan-location.txt. Prefix
s3drivetest/s3drive-conformance/69329649f34f798791386c4d3c62ac1e/;
canonical confirmation03febed0e9df68b61994d830830d73abdb972b096b0d74d8344ad0ec728ee177,
file SHA649c8edbb7f51a27c953b19959ea23f00311af9409551b8f5bed5924068b1720.
At most14 PUT attempts, six keys,4KiB/object,<1MiB transfer. No approval requested
or granted for this new scope. DO NOT RUN without explicit fresh approval.
It is an optional prepared renewal, not proof renewal is necessary: first inspect
original report age/binding and intended explicit policy max age. Do not extend
an existing expired policy to bypass it. Application writes cannot use reserved
s3drive-conformance/ keys; permits_full_key rejects them independently of report.
Managed Wasabi acceptance needs its own separate approved application prefix,
concrete runner/plan and bounded effects. Old probe approval grants none of that.
Sanitized preparation record docs/src/evidence/wasabi-next-plan-20261018.json.

## Full-goal work still required

- Explain/correct classified Windows transport failures with direct evidence.
- Snapshot/platform acceptance, continuity/fencing/restore drills, independent
  hosts/partitions and secure actual deployment.
- Physical reboot/PAM/real SSH, target Windows10/11, approved managed Wasabi,
  large/multipart files, reference-aware reclamation and application saves.
- Product packaging/distribution and recorded WinFsp licensing decisions.

Local debug ELF/--version observation at e51 reports Debian12/glibc2.36 and highest
GLIBC_2.34 requirement. docs/src/evidence/linux-runtime-baseline-20261018.json
is not release/MX/reboot proof. platform-integration.md proposes release-baseline
acceptance; no packaged artifact was made. Use bash scripts/gate.sh per source;
native/provider/recovery gates remain separate. Inspect resolved dependency
sources before new APIs; unused installed versions are not the lockfile graph.

## Archive

The full previous handoff (411 lines) was copied byte-identically before refresh:
[before a14 refresh](docs/archive/HANDOFF-before-a14-refresh-20261018.md).
Archive SHA256: 7c9f8fd4bbecb6cf827f55a3dae193b640194f8e6367a473599bb32e7dd9f46a.
Earlier complete histories remain in docs/archive/HANDOFF-before-snapshot-cancel-20261018.md
(SHA2568d6eec5434162ea07035bb829ceec5b04936cc352f4939a574b6fe43c5580060) and
HANDOFF-before-flush-receipts.md. No previous evidence, decisions or divergences
were removed from repository-owned history.
