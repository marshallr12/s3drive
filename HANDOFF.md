# S3Drive handoff

Updated 2026-10-18. Goal ACTIVE. Mira is the assistant; no voice-context handoff
capability was available. Read this and docs/src/implementation-plan.md before
implementation. Keep research/decisions/status/plans in docs/src/. Run the local
gate with `bash scripts/gate.sh`; native/provider/recovery gates are separate.

## Current repository visibility: PUBLIC (user decision, 2026-10-18)

The user intentionally made marshallr12/s3drive public. Preserve that setting.
This supersedes the initial private-repository request and every historical
private-publication guard below. Never PATCH visibility or use repo-edit to
restore privacy unless the user directly asks for that specific change.
Read-only identity/visibility checks may report state; they must not repair it.
The assistant's earlier commands DID set private=true on2026-10-17 at17:52:11Z
and20:35:41Z; both returned success/private=true. Those were mistaken attempts
to enforce the original request, not an identified autonomous GitHub rule.
Current GitHub GET reports public. The rule was published to main as
1a93b5f7248892fec96dc8da4438ee7967692e77 (four policy/docs files only), normal
fast-forward frombf9509; docs links/build passed and public was read-only
verified before/after. Proof /tmp/s3drive-public-visibility-policy-publication.json.
No visibility or credential mutation was performed during this investigation.
Do not reuse stale private-only publication guards as current policy. The active
repeat checker now verifies identity/source/artifacts without requiring private.
See AGENTS.md and docs/src/decisions.md D040. Future feature integration must
include main's policy commit; current HEADc7fca789 already includes that rule.

## Large-file representation study (published, 2026-10-18)

New docs/src/large-managed-files.md, images/large-managed-files.svg and
scripts/study-managed-layout.py plus dated JSON compare one-object generations
with immutable chunks/authenticated paged manifests. Actual2575374 source audit:
managed Reader downloads/hashes whole file; native writes buffer whole payload;
logical Input requires whole SHA before begin; authority reservation caps8MiB.
Existing fs/edits.rs already has bounded range composition; do not reinvent it.
Multipart alone does not remove these boundaries. Recommendation is chunked
managed storage. User now explicitly selected chunks/manifests on2026-10-18
(D041), confirming the provisional offline implementation default. No implicit
data migration, mounted-format activation or new provider writes follow.
Keep legacy references/hashes/unknown operations exactly compatible. Alternatives
include one object plus authenticated range hashes and future multipart copy.
Study's1GiB/100edits arithmetic is hypothetical, excludes manifests/overhead,
not a benchmark/bill/limit. Primary AWS range and Wasabi retention/accounting/
multipart docs refreshed; no account policy or provider objects changed.
Full local study gate2 EXIT0 session67227, log
/tmp/s3drive-large-managed-study-gate2.log and result JSON beside it:
292state/1ignored,50authority,format/lint/build,59doclinks/55chapters. All12
frozen runtime/contract hashes unchanged. Study gate1 terminated143 during
managed-publication tests without an assertion failure; cause not established,
no pass claimed. Gate2 completed normally. Arithmetic JSON reproduces exactly.
Diagram XML/text extents checked. ImageMagick renderer unavailable and browser
screenshot timed out; system librsvg/Cairo rendered successfully and local PNG
was visually reviewed. Authority pointer was rerouted to avoid the S3 heading.
Study and reviewed evidence were pushed at e08f607b9f0e27537f022752a40123be52361079
on codex/large-file-representation; proof /tmp/s3drive-large-file-research-publication.json,
clean, exact remote tip, all12runtime hashes match, PUBLIC before/after. This is
a documentation-only child of2575374. A [skip ci] commit avoids another identical
hosted native cycle: full local gate plus final docs checks passed; runtime/native
evidence stays bound to2575374. No main/runtime promotion. Verify publication
with git status and ls-remote; repository visibility must remain PUBLIC.
P0 table now reflects D040 public policy instead of stale privacy requirement.

## Current phase: offline object identities/root descriptors (2026-10-18)

Branch codex/manifest-object-identity from ec43132. Pure core module implemented:
role-separated v2 object keys and120byte root descriptor, independent vectors,
context/store/length/version/encoding refusal tests. Exact phase contract and
done criteria in implementation-plan.md. No SQLite/schema/wire/network changes,
no implicit object adoption, no mount limit increase or provider request.
Old source e48 CI remains independently live; review it using the same run and
watch handle below. Its pass cannot certify this new phase.
Focused7 PASS/27filtered, final source log
/tmp/s3drive-manifest-objects-focused-final.log. Independent vector regeneration
matches both files. Frozen19 source/fixture hashes
/tmp/s3drive-manifest-objects-frozen.json. Full local gate30377 closed EXIT0,
/tmp/s3drive-manifest-objects-gate.log:34core,50authority,292state/1ignored,51S3,
format/Clippy/build/book; no warnings/errors and all19 hashes unchanged. Do not
restart the terminal gate. One long state test ultimately passed. No dependency
or schema/wire version changed. Published source52cd2139e148d2e5055493f8e1c50880c6a57e98 normally on
codex/manifest-object-identity; clean/exact remote/all19 hashes/PUBLIC before-after,
main1a93b5f unchanged. Proof /tmp/s3drive-manifest-objects-publication.json.
Evidence docs/src/evidence/manifest-objects-20261018.json. Own future CI must be
tied to new source, not e48. Next phase needs durable SQLite frozen edit IDs and
object dependencies before publication; never cast unsigned ordinals to signed
SQL integers. New code supplies no adoption, write or root-publication permit.
Own CI39138395876 attempt1, source52cd213: book117707557507 SUCCESS with
own completed review; Linux117707557713 and Windows117707557677 LIVE. Watch65541
redirects /tmp/s3drive-manifest-objects-52-watch.log; poll same run/handle, never
restart on observation timeout. Prepared checker
/tmp/s3drive-check-manifest-objects-52.py compiled only, NOT executed. Require all3
terminal successes and own completed raw reviews in
/tmp/s3drive-manifest-objects-52-raw-reviewed.json before success checker.
New directory /tmp/s3drive-manifest-objects-52-artifacts; select own Windows
upload IDs, validate source/run/API hashes and typed21/14/project oracles. Old
e48 and257 artifact IDs refused. Do not borrow old source acceptance.

## Preceding codec checkpoint: source e48 (2026-10-18)

Branch codex/managed-manifest, parent e08f607. New core manifest module and tests,
eight independent fixed Python vectors plus generator, one existing SHA2
workspace dependency edge/lockfile entry. No version upgrade. Correct resolved
sha2 0.11.1/digest0.11.3 APIs inspected before use (initial0.10cache was not the
resolved dependency and was corrected). No legacy wire/schema/hash changes.
Implementation began with chunks/manifests as the surfaced provisional offline
default after the optional clarification window; the user explicitly selected
that option during this phase (D041). No migration/activation/provider approval
is inferred. Existing hashes, journals and content references stay compatible.

64byte canonical header, <=128 entries/10432bytes/page, levels0..7, signed64
length arithmetic,1MiB data objects. Explicit hole/data/child roles; root and
child identity/span/level binding, raw data SHA vs domain-separated page SHA.
Page is parsed/validated only; VerifiedPage gates child/data verification.
Lookup is bounded/no allocation. Role-specific keys, adoption, authority root
capability, SQLite journal, native large files and live writes remain absent.
Contract docs/src/large-managed-files.md; evidence managed-manifest-20261018.json.
Focused10PASS/17filtered and core all-targetClippy-Dwarnings EXIT0. Independent
vectors fix bytes/hashes; all byte tampers/truncated prefixes and context,
alignment, overflow/limits and sparse EOF paths covered. Frozen14 source/fixture
hashes /tmp/s3drive-managed-manifest-frozen.json. Full local gate EXIT0
session91280 /tmp/s3drive-managed-manifest-gate.log:27core,50authority,292state/
1ignored,51S3,format/lint/build/book. No warnings/errors, all14 frozen hashes
match. Isolated Python regeneration matches all8 vectors. Final docs checks
follow the D041 updates; do not cite the concurrent257 repeat as native
acceptance of this new module.
Final D041/status documentation links, build and55chapter tests passed.
Published e48f36fdaf05ea73fc69282e5b616630545a13a5 on codex/managed-manifest,
ordinary push, clean/exact remote/all14 frozen hashes/PUBLIC before-after;
proof /tmp/s3drive-managed-manifest-publication.json. Main remains1a93b5f.
Own CI39137161051 attempt1 at e48: book117703908126 SUCCESS with completed raw
review; Linux117703908277 SUCCESS and its completed raw review closed;
Windows117703908294 LIVE at last observation. Linux27core/10manifest,292state,
9FUSE,32HTTPS flushes,11Unicode names and both no-systemd coordinator protocols
passed. Linux review /tmp/s3drive-managed-manifest-e48-linux-reviewed.json;
SHA87d3c513dd953d8f96a83d3c880e370c9efb9cd6c0d4346881028166ae0f9630.
Expected TLS/HEAD403/stopped-authority refusal and post-uninstall missing config
were reviewed; cache saved normally. One long state test ultimately passed.
At the latest explicit API check, e48 Windows completed its paired and broad
native acceptance steps successfully and remains LIVE saving the build cache.
Do not claim terminal cohort/artifact acceptance before the job is completed
and its own full raw log is reviewed. Existing watch42188 remains live; no
rerun requested. Own artifact checker is still ineligible/unexecuted.
Watch42188 redirects /tmp/s3drive-managed-manifest-e48-watch.log; poll this same
handle/run, do not restart on timeout. Do not cite257's separate repeat as proof.
Prepared /tmp/s3drive-check-managed-manifest-e48.py (Python compiled only), own
PowerShell oracle script and new output /tmp/s3drive-managed-manifest-e48-artifacts.
Checker MUST wait for all3 terminal-success jobs and own completed raw reviews
with source/run/attempt/job IDs and log hashes in
/tmp/s3drive-managed-manifest-e48-raw-reviewed.json. Extract fresh artifact IDs
from this Windows completed upload log, verify API digests/source/timestamps,
strict21/14 typed fields and both project oracles. All previous artifact IDs
are explicitly refused. On any failure preserve it; no success checker or
success claim. After own closure, update dated evidence and current book status.
Next implementation is an independently testable offline chunk identity/builder/
dependency-journal phase; read the large-file chapter's sequence and inspect
resolved dependency APIs first. Legacy logical Input currently needs a whole SHA
before begin: do not simply lift its8MiB limit or reuse a root as that SHA.

## Completed exact257 repeat attempt2 (2026-10-18)

OriginalCI39133178423 attempt2 at source25753746287aa52a36a9ba42d8bf5162f1de07d1
passed all3 jobs: Windows117697477386, Linux117697477621, book117697477604.
Watch67342 closed EXIT0. Own completed raw reviews closed; checker70361 EXIT0,
/tmp/s3drive-dispatch-257-attempt2-artifact-check.log. Fresh replacement12139721137
and disposition12140285720 IDs came from this attempt's Windows upload log;
API source/run/digests and strict21/14 all-field/type checks plus both project
PowerShell oracles passed. Separate output
/tmp/s3drive-dispatch-257-attempt2-artifacts. Primary IDs were explicitly refused.
No artifact borrowing. Evidence docs/src/evidence/dispatch-repeat-20261018.json.
This observational repeat did not reproduce the historical actively-polled/
no-write failure and establishes no cause or causal correction. No main promotion,
new manifest-code acceptance, workstation reboot or provider acceptance follows.
Reviewed nonfatal same-key cache-reservation warnings; tests all completed.

Before rerun, attempt1 explicit API run/jobs/artifacts and all reviewed raw logs,
ZIPs, typed/project checks were frozen in repository archive
`docs/src/evidence/dispatch-257-attempt1-20261018.tar.gz` (17members/88090bytes),
SHA2568d0e2c598aa091a54991c51e80b5c3b07912c4723b9a6451e039509f3b248d5b.
All bytes re-extracted and verified in memory. Primary job/artifact IDs retained.
Do not rerun the old primary checker against the latest-run endpoint now.
Initial rerun request EXIT0; immediate GET still showed attempt1/queued, causing
an assertion failure; subsequent GET confirmed attempt2/in_progress. No duplicate
rerun was requested. All watch/checker handles are closed; do not restart them.
The new manifest codec requires its own source-bound CI, independent of257.

## Current candidate: dispatch observations (P6m4i)

Runtime candidate is codex/dispatch-progress at25753746287aa52a36a9ba42d8bf5162f1de07d1,
normally pushed with all17 scoped files. Parentc7fca789's Windows CI is terminal
FAILURE (stale schema23 fixture), below. Publication
/tmp/s3drive-dispatch-progress-publication.json: clean after push, all12 frozen
source hashes unchanged, PUBLIC before/after and0visibility mutations. Main stays
1a93b5f. Post-push HANDOFF/dispatch observations are included with the new representation
study on its documentation branch; no new hosted native run is requested for
that docs-only commit. Runtime candidate and CI remain2575374. Previous
candidate branch remains pushed atc7; main is unchanged/public.
Plan appended to implementation-plan.md. The observational delta is implemented
in wire.rs, wire/owned.rs, wire/progress.rs and wire/tests.rs; no schema/dependency
or scheduling/policy change. Five compact public fields track initial readiness,
body descriptor access and aggregate before/after driver budget exhaustion.
Legacy dispatch_contract_review is read-only before touching first-send behavior;
preliminary finding: first buffered send is explicitly supported, ready() wait is
optional and must be co-polled with the driver. Thus this phase adds observation
flags only; no readiness wait, poll-order/retry/deadline/schema/dependency change.
model-select: dispatch reviewer → gpt-6-astra/xhigh (score7:spec0,blast2,depth2,
verify1,cost2; overrides:none; assessment:local). No external assessment used.
Read-only preimplementation/final-delta review found no blocker. Focused2 EXIT0
session41120 /tmp/s3drive-dispatch-progress-focused2.log:9wire controls pass.
Initial focused1 compile errors were test-only try_accept/extra unwrap; fixed with
inspected poll_accept after workers are reaped and correct Result depth.
Baseline and actual production-exchange bounded budget-gated Pending-reader
control, empty/Unicode body hints, EOF/failed-write and existing exact TLS/SQLite
reply controls passed. Positive exhausted-before is not separately tested; do
not claim it. Error staysCopy<=128bytes. New hints cannot prove delivery/cause.
Frozen7 source/manifest hashes /tmp/s3drive-dispatch-progress-frozen-source.json.
Evidence docs/src/evidence/dispatch-progress-20261018.json.
Gate1 EXIT0 session71299 /tmp/s3drive-dispatch-progress-gate1.log on unchanged
Rust sources, but SUPERSEDED after adding the schema fixture/check/gate/workflow
correction below. Final12 hashes frozen at
/tmp/s3drive-dispatch-progress-frozen-final-source.json.
Final extended gate2 EXIT0 session50715:
/tmp/s3drive-dispatch-progress-gate2.log. Schema fixture preflight,50authority
(15unit),292state/1ignored,17core,51S3 and workspace/build/lint/book passed;
all12 frozen hashes unchanged, no warnings/errors. Proof
/tmp/s3drive-dispatch-progress-gate2-result.json.
Final result docs links/build/test also EXIT0; commit/push EXIT0 session33314.
CI39133178423 is terminal SUCCESS at exact2575374: Windows117692328597,
Linux117692328400, book117692328244. Watch36321 closed EXIT0. All own completed
raw reviews are closed; marker /tmp/s3drive-dispatch-progress-257-raw-reviewed.json.
Windows raw SHA25625377375a16cf8cd9b73db4c40b1823fc725a5c6a4f829f60e1ce3bdc68773c6;
Linux SHA1bbcf0753b4296cc065a95a0bf170e6ea247e7a2793ac5abdbfbf2982ba11452.
Own checker /tmp/s3drive-check-dispatch-progress-257.py EXIT0 session75387:
/tmp/s3drive-dispatch-progress-257-artifact-check.log. Own source/run ZIP API
hashes and strict all-field/type21replacement/14disposition records plus both
PowerShell project oracles match. No prior artifacts borrowed. New directory
/tmp/s3drive-dispatch-progress-257-artifacts. Do not rerun this one-use checker
into the same directory. Book58links/54chapters; Linux9real FUSE tests,292state,
50authority,32HTTPS flushes,11Unicode names and paired no-systemd recovery.
Windows Server2025/WinFsp2.1.25156:287state/1ignored,50authority,16multipart,
32HTTPS flushes,lifecycle,paired startup/recovery,snapshots and7broad native pass.
Own corrected schema24 session fixture now passed; parentc7 remains failed.
Raw notices reviewed: LNK4199 on9non-WinFsp CLI tests, three locked disposable
profiles left to VM destruction after dummy credential removal, expected TLS/
HEAD403/dirty-conflict faults and deliberate stopped-service List Connect timeout.
Node/action notices and ultimately-passing long tests retained. Ubuntu-latest
migration to26 starts2026-10-19; this run actually used24.04.5. Future CI runner
pinning is worth a separate change, not silently claiming a new OS was tested.
This is ONE passing cohort; no causal fix or identical-source repeat acceptance.
No runtime promotion, physical workstation reboot, Win10/11, provider requests,
remote deployment or snapshot restoration is established. Preserve all earlier
failures and the unresolved transport requirement.

## Current work: offline immutable multipart plans (2026-10-18)

Branch codex/multipart-plans, HEADc7fca789a394a1bde63f90154bb261396fb68c8b,
normally pushed with all40 scoped files after final gate3. Parent9c14c996 merges
public-policy main1a93b5f into candidate64aa073. Main/origin-main remain1a93b5f
(runtimebf9509 plus policy only). No owned-client runtime promotion. Publication
proof /tmp/s3drive-multipart-plan-publication.json: clean immediately after push,
all29 frozen sources unchanged, repository PUBLIC before/after;0visibility changes.
Post-push HANDOFF/evidence observations are intentionally dirty; preserve them
for the next substantive phase rather than triggering docs-only native CI.

New multipart.rs/tests and migration024 implement OFFLINE Frozen→Ready plans.
Caller-supplied ID freezes exact sealed manifest, target/config/credential binding,
original guard and ancestor state-hash receipt, explicit geometry and reserved
plan/part quotas. Async prepare verifies/hashes <=1MiB per owner command and
yields between commands; no metadata proof crosses commands. Private finalizer
atomically saves all part/whole hashes and checks exact stored SQL effects before
commit. Ready commits expected bytes, not current-media freshness or network
permission. Re-prepare rehashes the same input. Current sealed retention keeps
bytes; future GC/executor must respect references and verify send-range bytes.

Schema24 role/migration/snapshot checks move together; Windows snapshot helper
expects24. Shared read-only legacy remote guard extraction preserves old error
order and intent/state hashes. Existing8MiB single-PUT/managed/logical/native
bounds stay unchanged. No upload IDs, target admission, send/completion/abort/
flush permits, network calls or reclamation were added.

Final focused5 EXIT0 /tmp/s3drive-multipart-plan-focused5.log:16PASS, including
real process kills at5 boundaries, SQLITE_FULL, sparse/inherited/nonaligned
parts,16MiB async owner interleaving, independent quota/extent caps, bounded
malformed cells, exact schema23 legacy hashes/collision rollback, both new-table
authority exclusions and6 ignored/deleted/corrupted SQL trigger controls.
Read-only multipart_contract_review found atomicity gap closed and no new blocker.
First state regression284PASS/2FAIL/1ignored exposed synthetic lowered-schema
fixtures retaining new tables; corrected fixtures, no weakened production checks.
Gate1 failed only Clippy drop_non_drop; fixed test lexical scope. Gate2 EXIT0,
291state/1ignored but started before final atomicity fix/test; SUPERSEDED, not
final-source acceptance. Frozen29 source hashes:
/tmp/s3drive-multipart-plan-frozen-source.json and dated evidence below.

Final full gate3 EXIT0 session70024 /tmp/s3drive-multipart-plan-gate3.log:
292state/1ignored,48authority,17core and workspace/build/lint/book checks passed;
no warnings/errors and all29 frozen source hashes still match. Log SHA256
13e970aaaafccefc03b300d20b8b9dfbef29c653f045270e7aa3332d83289634.
Result /tmp/s3drive-multipart-plan-gate3-result.json. This supersedes gate2.

Final result docs links/build/test also EXIT0, normal commit/push EXIT0 session93646.
CI39130994866 is terminal FAILURE at exactc7fca78. Windows117685945192,
Linux117685945294, book117685945066. Watch25319 redirects to
/tmp/s3drive-multipart-plans-hosted-watch.log. Latest complete metadata
/tmp/s3drive-multipart-plans-hosted-observed.json. Watch25319 closed EXIT1; do not poll/restart that terminal handle. Require completed own raw logs and
review all transport/native warnings before acceptance. Only after all3 terminal
success and actual raw review, write
/tmp/s3drive-multipart-plans-c7-raw-reviewed.json then run prepared
/tmp/s3drive-check-multipart-plans-c7.py and its PowerShell oracle. Their new
output directory must be /tmp/s3drive-multipart-plans-c7-artifacts. Checker
prepared/compiled only, NOT executed; INELIGIBLE because the whole run failed. On any
failure, do not run it or borrow64/90 artifacts; inspect exact failed source.
No runtime main promotion is planned from this offline candidate alone.
Book117685945066 is terminal SUCCESS; completed raw log via jobs REST endpoint
reviewed at /tmp/s3drive-multipart-plans-c7-book.log, marker
/tmp/s3drive-multipart-plans-c7-book-reviewed.json.58 documentation files links,
mdBook build/54 chapter tests; only Git initial-branch advice, pinned Rust override
and pinned-checkout Node20→24 notice. Windows failed as described below. `gh run view --log`
refuses logs until whole run completes; `gh api --allow-escape-sequences` can
retrieve an already completed job's log (save raw; strip ANSI only for display). Do not
promote runtime to main while transport reliability remains unresolved.
Linux117685945294 is terminal SUCCESS; own completed raw log reviewed and hashed
/tmp/s3drive-multipart-plans-c7-linux.log, marker
/tmp/s3drive-multipart-plans-c7-linux-reviewed.json.292state/1ignored,48authority,
9 real native/32 HTTPS flushes, paired recovery, schema24 and both no-systemd
coordinator protocols passed. Only deliberate TLS/HEAD403/stopped Node Connect
refusals, missing startup.json after uninstall and standard Git/Node action
notices. No physical MX reboot/provider or Windows acceptance inferred.
Book: docs/src/multipart-plan.md and implementation-plan.md.
Evidence: docs/src/evidence/multipart-plans-20261018.json.

## c7 Windows failure and concrete schema fixture correction

Windows117685945192 FAILED broad actual_windows_drive_names_reads_security_and_teardown
in disposable-user Install: generic private-local-state initialization/reopen error.
Raw /tmp/s3drive-multipart-plans-c7-windows.log failure section/result summaries
reviewed.287state/1ignored,all16 new planning tests,32HTTPS flushes,standalone
snapshots/startup/lifecycle, paired recovery passed; broad native6PASS/1FAIL.
This is NOT a fully accepted cohort, no success-marker/artifact checker or main
promotion. The two uploaded reference artifacts cannot turn the failed run green.

Source shows scripts/test-windows-session.ps1:242 still matched schema=23 while
Rust SCHEMA_VERSION=24. It deterministically rejects successful24 output; raw
logs do not independently expose init-state's exit status. Corrected exact24,
not a generic match or bypass. New scripts/check-schema-fixtures.py ties the
session and snapshot worker expectations to the Rust constant, in local gate
and early book CI. Isolated stale-session and stale-snapshot controls EXIT1;
fixed EXIT0, proof /tmp/s3drive-schema-fixture-negative-controls.json. PowerShell
outer+embedded AST/current-pattern check oldc7 EXIT1/fixed EXIT0,23/25 refused.
No Windows execution is claimed by these portable checks. New source must pass
its own native acceptance. Historical empty-header transport failure remains
unresolved; this correction is separate. Full final extended gate2 passed after the superseded
gate1 definition; the corrected fixture still requires its own hosted acceptance.

## Previous64 acceptance: primary passed, exact repeat FAILED

Primary64aa073 CI39125456457 passed Linux/Windows/book and its OWN strict21
replacement/14NTFSdisposition artifacts. Completed raw logs/API ZIP digests/
field/type/project oracles/timestamps reviewed and closed. Marker
/tmp/s3drive-pending-scan-64-raw-reviewed.json; checker EXIT0
/tmp/s3drive-pending-scan-64-artifact-check.log. All scoped details retained in
pending-scan-20261018.json; never infer a transport fix from this primary pass.

Source-identical repeat CI39127440399 is terminal FAILURE at exact64aa073.
Windows117675434721 failed paired startup; Linux117675434731 and book117675434589
succeeded. Current GitHub state reverified. Raw completed logs:
/tmp/s3drive-pending-scan-repeat-64-{windows,linux,book}.log. Linux/book reviewed;
Windows failure section reviewed. Earlier Windows workspace271state,32 HTTPS
flushes, standalone startup/snapshot and lifecycle passed, but later14disposition
reference and broad native were skipped. SUCCESS checker is INELIGIBLE and must
not run. Do not create a success raw-review marker or use partial artifacts as a
complete cohort. Prepared /tmp/s3drive-check-pending-scan-repeat-64.py remains
unexecuted. No restart/repeat is needed merely to erase the failure.

Matched failing Info exchange: client53632/server53628/server connection4;
469 driver polls, maxgap57ms,468 Pending reads,0readbytes,0writepolls/bytes,
468flushpolls; driver_succeeded=true. Server HttpTimeout5098ms/read0/write0/
dispatchedfalse. This disproves a claim that the driver simply never polled;
HTTP/TLS/scheduler/root cause remains unresolved. Fresh child install identity
refusal/worker panic are observations, not established root cause.
Historical source47/parentea failures remain retained; source90 had one passing
cohort without same-source repeat. Runtime mainbf9509 retains its accepted
primary39113628548/repeat39115395230 scoped checkpoint. Never reuse another
source/run's acceptance artifacts.

Pending-scan optimization reuses proofs only within one synchronous owner read;
32-row study callbacks2,069,199→162,294, no fixed latency guarantee. Source64
isolated staged-range study passed16/32/64MiB exact hashes/reopen/unchanged read
SQL state.16MiB at16/64/256extents took11,610/36,162/134,370 progress callbacks.
Its final test source/log/Clippy archive is
 docs/src/evidence/staged-range-study-20261018.tar.gz,
SHA2564d19af5c5c5da8389a315ef54eb6c07f7966172d49d54f2575e40d6de3797b88.
Payload step bounds do not remove repeated manifest or global catalog scans.

## Read-only transport audit while c7 CI runs

Five inspected Hyper1.12.0 and three Tokio1.53.3 files match their cached crate
members; both archives match Cargo.lock SHA256. No local extraction modifications
were found in this scope. New dirty evidence
 docs/src/evidence/transport-dependency-origin-20261018.json and discussion in
owned-authority-client.md. No dependency or production changes/provider calls.
Hyper1.12 already contains readiness fix4208; master post-release4222 drains
in-progress sends on dispatcher shutdown. Neither is established as this fresh
owned no-write failure's cause. Synchronous send_request enqueues before this
same task polls its driver. Next investigate sender readiness/body descriptor/
cooperative-budget states before claiming a behavioral fix. Keep exact identity,
one send/original deadlines/known reply retention; require native+repeat proof.
Carry these observations with the next substantive phase, not a docs-only CI push.

## Remaining full-goal requirements

Resolve Windows transport reliability; implement multipart executor uncertainty,
streamed native large-file edits and later reference-aware GC; obtain fresh
concrete Wasabi approval (all14 earlier PUT attempts exhausted). Preserve exact
unknown identities/conflicts/dirty bytes. No real provider calls in this phase.
Managed live Wasabi, independent hosts/partitions, restore/continuity/fencing,
physical Win10/11/MX reboot/real SSH+tmux, application workflows and packaging/
licensing remain incomplete. Local FUSE is blocked by workstation privilege
constraints; hosted mounts are separate evidence. No user startup/hosting/Turso
service installed. Goal remains ACTIVE.

Previous current-state handoff is archived at
`docs/archive/HANDOFF-before-multipart-plans-20261018.md`; historical live/privacy
statements there and below are superseded, not instructions to mutate visibility.

### Historical e00 study checkpoint (superseded closure control)

Historical branch: codex/owned-http1-probe at
e00b5e7d3c6b84aaedc93bd73b213af969f6d26f, normally pushed privately with
source/ref/privacy-before-and-after guards. Proof /tmp/s3drive-owned-http1-publication.json.
Post-push HANDOFF/study JSON updates are intentionally dirty; preserve for the
next substantive phase. No docs-only native CI cycle.
Exact-source CI39119010538 is live: Linux117650584393,
Windows117650584540, book117650584582. Watch67313,
/tmp/s3drive-owned-http1-hosted-watch.log. Poll this exact handle/run, never
restart solely on observation timeout. First inspect the independent7-test owned
exchange results on Windows before treating the prototype as portable. Any
production native results still exercise unchanged WireClient, not the prototype.
No production WireClient/routing or dependency version/lockfile change. Public
Hyper1.12/Rustls/tokio-rustls/rcgen/body APIs inspected in resolved sources.
Seven independent local HTTP/TLS controls passed at
/tmp/s3drive-owned-http1-focused3.log. Initial strict cancellation EOF failed;
measured TLS ConnectionReset, then accepted only EOF or Reset, never arbitrary
errors/extra bytes. Plain HTTP cancellation observed EOF, TLS Reset. Both10sec
stalled-response controls and2sec stalled TLS passed. Full gate1 failed Clippy
err_expect only; fixed expect_err. Full gate2 completed EXIT0 session86164,
/tmp/s3drive-owned-http1-gate2.log: owned7, authority7unit/10HTTP/9TLS, core17,
state274 and workspace/build/lint/57-doc book. No local gate process remains.
Code fingerprints in new study JSON are frozen. Explicit roots/fixed ephemeral
endpoints/synthetic bearer only; no provider/native/Windows correction claim.
Production URL/private files/platform roots/JSON receipts/caller-queue parity
and native/repeat acceptance remain required before adopting this transport.

Prior multipart checkpoint b793ea58e662f7884981f9ce89ce94482e3fb489 was
normally pushed privately; proof /tmp/s3drive-multipart-sizing-publication.json.
It depends on diagnostic0bbc4db, whose Windows failure is retained below.
Its full local gate passed
EXIT0 (session75443, /tmp/s3drive-multipart-sizing-gate1.log): core17 including
multipart6, state274, all workspace/build/lint/book checks. No payload/network,
schema/config or8MiB write-bound change. New evidence: docs/src/evidence/
multipart-sizing-20261018.json. Done criteria in multipart-plan.md and plan.
Historical in-progress observation (now failed above): CI39117793980:  Windows117647008136, Linux117647008237,
book117647008193. Watch87567, /tmp/s3drive-multipart-sizing-hosted-watch.log.
Linux and book are terminal SUCCESS. Completed raw /tmp/s3drive-multipart-b793-
{linux,book}.log reviewed: Linux9 native/32-flush/paired and both coordinator
startup protocols passed; only controlled faults and stopped-authority refusal.
After terminal Windows raw review and all-three success only, prepared
/tmp/s3drive-check-multipart-b793.py and its reference checker may validate the
own strict21/14 artifacts at new /tmp/s3drive-multipart-b793-artifacts. They have
NOT RUN and MUST NOT RUN after the recorded Windows failure. Never run them on a failed source or reuse another cohort's artifacts.
Poll this exact handle/run, never restart solely on observation timeout. This
planner does not change transport behavior; even a passing run cannot erase the
parent failure or establish a transport correction. No main promotion is planned
from geometry alone. Post-push status/research docs may be dirty; preserve them
for the next substantive phase, without docs-only native CI cycles.

Diagnostic0bbc4dbdf5915cd6252e2d718bdf370c15407b7d was normally pushed privately
to codex/error-connection-metadata with frozen source/privacy/ref guards, proof
/tmp/s3drive-error-connection-publication.json. Main staysbf9509. Exact-source
CI39117045361 has terminal Windows117644838870 FAILURE (native flush burst),
book117644838943 and Linux117644839047 SUCCESS. Whole run is terminal FAILURE.
Completed logs /tmp/s3drive-error-connection-0bbc-{linux,windows,book}.log reviewed;
run metadata /tmp/s3drive-error-connection-0bbc-run.json. No active0bbc job remains.
Linux9 native,32-flush, paired no-systemd recovery and both coordinator startup
protocols passed; only deliberate stopped-authority refusals and TLS fault cases.
DO NOT RUN prepared
/tmp/s3drive-check-error-connection-0bbc.py or its reference checker: this failed
source is not eligible. Later Windows native/reference/workspace gates skipped.
Completed Windows raw /tmp/s3drive-error-connection-0bbc-windows.log reviewed.
At burst step25, Info/Request/UnexpectedEof/native1117, body_frame_polled=false;
candidate AND error connection client57583/server57240 exactly match server185
HttpTimeout5039ms/read0/write0/flush1/dispatched=false. Fresh CLI also Refused
after5206ms with both owned processes alive. Subsequent empty server186 was not
independently bound to that CLI. Standalone startup/snapshot and authority7unit/
10HTTP/9TLS passed; console Ctrl+C5ms single observation only. No upload/outcome
or root-cause inference. Current multipart changes do not belong to hosted0bbc.

First action next: poll both exact hosted runs and retain completed raw logs;
require scoped raw/strict artifacts before any cohort acceptance. Then prioritize
the correlated transport
failure before enabling new upload paths. Do not promote0bbc despite portable
success or replace its original IDs/assertions/deadlines. Fresh CLI refusal means
native-callback-only explanations are not established. A different per-RPC
transport ownership design is only a proposal, not a demonstrated correction.
Resolved reqwest connector_layer requires the exact sealed Conn response type;
it cannot return a new I/O wrapper/attach custom metadata through a substituted
response. Do not implement that invalid hook or turn on verbose bearer/body logs.
The next proposed study is one owned future co-polling direct Hyper HTTP/1 driver
and request, with unchanged Rustls trust/deadlines/no-replay and full behavior
controls, before any native claim. See remote-authority's next-boundary proposal.

Local/remote main AND codex/authority-snapshots are now
bf9509eb5dceb99101ddf5ac37ac432bd6b8ff05. Exact-source primary CI39113628548 is
terminal all-three SUCCESS: Linux117634799264, Windows117634799237,
book117634798973. Completed raw logs reviewed:
/tmp/s3drive-loopback-bf9509-{linux,windows,book}.log.

Independent strict21/14 field/type/source/platform validation and project oracles
passed after exact artifact ZIP SHA256 verification. Proof directory:
/tmp/s3drive-loopback-bf9509-artifacts/{run,download-proof,independent-check}.json;
project-oracles.log and both ZIP/JSON reports are there. Guarded normal FF from
f8c4f5832e399c6c54567c122c875e2a438ce681 to bf9509 verified remote SHA/privacy,
main not checked out anywhere, unchanged branch/worktree and preserved dirty files.
Publication proof: /tmp/s3drive-bf9509-publication.json.

This accepts the offline snapshot/diagnostic checkpoint on hosted Linux and
Windows Server2025/WinFsp2.1.25156.ddca7bd. It does NOT prove an explanation or
elimination of earlier intermittent EOFs, Windows10/11, physical reboot, remote
hosting, Turso, live managed Wasabi writes, failover or restoration. Keep goal active.

## Accepted unchanged-source main repeat

Main-push CI39115395230 at exact bf9509 is terminal all-three SUCCESS:
Linux117640049798, Windows117640049853, book117640049659. Completed raw logs
reviewed at /tmp/s3drive-main-bf9509-repeat-{linux,windows,book}.log.
The following scripts completed exit0 against the repeat's own artifacts:
- /tmp/s3drive-check-main-bf9509-repeat.py
- /tmp/s3drive-main-bf9509-repeat-reference-check.ps1 (called by checker)
Proof directory: /tmp/s3drive-main-bf9509-repeat-artifacts, including run,
download-proof, independent-check JSON, both ZIP/JSON artifacts and project log.
Checker log: /tmp/s3drive-main-bf9509-repeat-check.log. Strict21/14 source/platform/
types/fields and ZIP digests passed. Do not rerun mkdir-only scripts into this
completed directory or substitute primary artifacts.

Repeat Linux9 native, Windows32-flush/lifecycle/broader7, both startup/recovery
and snapshots passed. Windows sole native RPC failure List/Connect/TimedOut
followed the intentional authority stop; Linux two Node/ConnectionRefused likewise.
One typed HttpShutdown in Windows flush burst had read243/write239/flush5 and
dispatched=true; all32 caller byte/single-PUT checks completed. No UnexpectedEof
or generic HttpTransport. Console Ctrl+C3ms/5ms single observations only.
Two passing cohorts do not establish historical EOF root cause or reliability.

The strict independent checker is /tmp/s3drive-check-native-artifacts.py. It
rejects duplicate/nonfinite JSON, extra/missing fields, wrong exact Python types
(bool is not int), wrong source/platform/driver and any21/14 mismatch. Known-good
f8 plus malformed envelope controls passed earlier; logs retained in /tmp.

## Verified scope at bf9509

Full local /tmp/s3drive-loopback-tag-gate1.log EXIT0: authority6unit (Hyper2,
scope1, tag3), lifetime2, execution1, HTTP10, TLS9, state274, other workspace
suites, build, lint and57-doc links/mdBook build/test. Rust/Cargo fingerprints
remained unchanged. Final /tmp/s3drive-loopback-tag-final-book{,-tests}.log passed.
No native mount was run locally; NoNewPrivs=1 blocks FUSE here. NEVER bypass via
root, SSH, another launcher or a VM. Hosted Linux supplies native evidence.

Linux raw: all9 native, native32 exact binary/Unicode HTTPS flushes with one PUT
per generation, paired no-systemd HTTPS/FUSE terminal loss/coordinator crashes,
exact unknown IDs/no repeated PUT, retained rival conflict, independent removal,
and coordinator HTTP+TLS supervision passed. Only intentional stop/refusal and
controlled wire faults occurred; no unclassified native RPC failure found.

Windows raw: early wire controls6unit/9TLS plus real CLI socket-tag controls;
ordinary-account standalone startup/snapshot; native32 flushes; complete native
DoC/deferred lifecycle and killed-owner/lost-callback recovery; workspace269state;
paired coordinator+drive separate logons, exact unknown recovery/no repeated PUT,
retained conflict, independent removal and snapshots; broader native7 cases passed.
Standalone/paired snapshot Ctrl+C exit observations4ms/35ms are single samples,
not guarantees. Typed HttpShutdown events had positive read/write/flush/dispatched
and completed caller semantics. Sole native metadata failure List/ConnectionRefused
followed the deliberate authority stop; no UnexpectedEof or generic HttpTransport.
Reference21 replacement and14 disposition plus four pinned WinFsp legacy cases
passed, with exact-source independently checked artifacts. Scope is Server2025,
not actual Windows10/11, reboot or real SSH authentication.

## Source behavior and safety invariants

bf9509 adds bounded loopback connector-candidate tags through the public
Connection::connected/get_extras/HttpInfo APIs. Exact127.0.0.1 or::1 matching
families/nonzero client+server ports only; other/missing info Omitted, more than
one successful connector Multiple. One atomic snapshot never replaces a prior
candidate with a misleading later single candidate. Request scope captured at
Service::call; unscoped/background completion cannot attach to another request.
Server diagnostics include the same sanitized tag before final dispatched=.
Candidate is NOT proof of selected socket, send, authentication or commit.
No flag/tag, including false/unobserved, authorizes replay or a new identity.

The earlier e988 layer observes connector start/success and selects reqwest's
boxed connector path; scheduling equivalence is not claimed. Earlier runtime
ownership runs each exact cloned request once in caller-scoped JoinSet on the
current runtime, with absolute cooperative10-second queue/RPC deadline; original
connect2/reqwest10 remain. Drop aborts owned work, not accepted remote effects.
Execution errors keep outcomes unknown. Body-frame polling is not socket-write
proof. Existing Connection: close means one exchange per HTTP/1 transport.
No transport retry/deadline/trust/framing/operation-identity or priority change
was made by the bounded-tag phase; native fixtures/workflow stayed unchanged.

Preserve dirty bytes, original unknown logical/physical IDs, guards and receipts.
No automatic transport replay, new-ID/rebase or unconditional overwrite fallback.
Only authoritative online commit establishes ownership; S3 PUT alone is not flush.
Schema23/config7. Local private SQLite cache/journal stays outside mounted trees.
Managed layout uses immutable S3 payloads and one Rust authority's shared SQLite
namespace. Authority has no S3 credentials. User chose strong managed coordination;
hosting undecided, keep SQLite-host and optional Turso choices. Turso is not enabled.

## Error-connection diagnostic candidate and next action

Post-push observations/research are intentionally uncommitted; bundle with the
next substantive source, not a docs-only native CI cycle. Preserve:
- HANDOFF.md and its new before-refresh archive
- docs/src/evidence/authority-snapshot-platform-20261018.json (primary acceptance,
  main publication, repeat status and research)
- docs/src/remote-authority.md and implementation-plan.md (same findings/limits)
- docs/src/evidence/error-connection-info-probe-20261018.tar.gz
The error-connection metadata phase was implemented after bf9509 on
codex/error-connection-metadata. Main and its completed repeat remain exactbf9509.
New source adds ErrorConnection to WireError, collected only on the error path
through bounded16 legacy sources. Unobserved/Unavailable/Omitted/Loopback/Multiple
are distinct. Shared canonical loopback filter exposes family/ports only, never
raw extras or dependency text. No execution/timeout/trust/retry/framing/ID changes.
Lost TLS reply and concurrent peer controls match independent sockets; failed
trust/stalled handshake is Unavailable, truncated response is Unobserved despite
full request receipt. Unknown extras and ambiguous observations are covered.

Focused authority tests EXIT0 (/tmp/s3drive-error-connection-focused1.log), full
local gate EXIT0 (/tmp/s3drive-error-connection-gate1.log, recovered session77211):
authority7unit, lifetime2, execution1, HTTP10/TLS9, state274, workspace/build/lint
and57-doc book checks. Final docs build/test and links passed. Topic is now
pushed as0bbc4db with its failed Windows gate above. Main staysbf9509;0bbc is not
accepted. No native mount or provider call was
performed locally. See implementation-plan and dated JSON.

A separate standalone Linux control proved public Hyper-util Error::connect_info
through the bounded reqwest error source chain: HTTP and HTTPS lost replies
matched independently accepted TCP sockets, plus all four original connector/
complete/lost controls. Six PASS lines, EXIT0 at
/tmp/s3drive-error-connection-info-probe1.log; scratch
/tmp/s3drive-error-connection-info-20261018-1p10x0dp and location pointer
/tmp/s3drive-error-connection-info-probe-location.txt. Source/manifest/lock/README
archive is committed in the evidence tar.gz above; hashes/limits in platform JSON.
Pooled send errors attach conn_info; earlier connection_for errors do not. This
distinguishes error-reported connection info from candidates. The candidate now
collects it; accepted mainbf9509 does not. Hosted acceptance is pending. Omission,
missing-state and exact-version regressions remain required. Source chains are not
stable across upgrades. Missing info never proves unsent or permits replay.
No real provider/credentials/native/Windows proof. Book links/build/test passed
at /tmp/s3drive-bf9509-error-info-book{,-tests}.log.

Earlier public connector API control (HTTP/TLS complete+lost replies) is committed
at docs/src/evidence/public-connector-probe-20261018.tar.gz. Original scratch
/tmp/s3drive-public-connector-20261018-jdfysrlm, log
/tmp/s3drive-public-connector-probe2.log. Public metadata comparison succeeds even
without a response; the rejected request-extension capture is NOT needed.

## Retained failure history and rejected explanations

All prior candidate failures remain in platform JSON, remote-authority.md and
archives. Main previously accepted f8 with two passing cohorts, not a universal
Windows correction. bf9509's passing primary and repeat likewise do not explain them.

Earlier connector candidate e988 CI39111567111: Linux/book success, Windows raw
controls/standalone snapshot-console/native32/lifecycle/workspace passed, then
paired Info/Request/UnexpectedEof at08:52:23Z, body_frame_polled=false with
connector_started AND connector_completed true. Nearby empty TLS socket133
expired5083ms/read0/write0/flush1/dispatched=false; exact socket correlation absent.
Expected single uncertain PUT was NOT established; no upload/commit claim.
Secondary locked disposable profile cleanup failed, original failure preserved.
Later14-reference/broad checks skipped. Actual process-class logs showed both
supervisors/children BelowNormal; thread priorities/causality unmeasured.
Raw /tmp/s3drive-connector-e988-{linux,windows,book}.log. Failed e988/949/a14
acceptance scripts MUST NOT be run or their artifacts reused for bf9509.

Older source failures:949 broad List EOF; a14 fourth burst Apply EOF; e51 broad
Info EOF (and an unclassified List/EOF inside a passing negative mutation case);
d6 HTTP lifecycle timeout;33 earlier HTTP lifecycle timeout;f9 broad native EOF;
a799 repeat seed EOF. No attribution to snapshot code, scheduler, S3 or a dependency
bug is proved. Hyper's4208 stale-want fix is already in1.12.0/want0.3.2; separate
4222 shutdown drain merged AFTER latest release and is absent. Its queued-request
hang differs from recorded EOF-before-client-deadline; do not patch on assumption.
Rejected capture_connection experiment: reqwest drops arbitrary request
extensions in execute_request; even a fully received request gave false. Removed,
not weakened. Empty server TLS sockets may be unused background connections;
Hyper-util0.1.21 races checkout/connect. A matching candidate is not selection.

## Snapshot contract and remaining recovery work

Offline create|inspect only, no restore/failover. Library state/authority_snapshot
and CLI snapshot.rs; known schemas8..23, exclusive existing stopped authority,
explicit volume/policy, no migration, credential resolution, provider or listener.
New private output outside source/mount tree; distinct snapshot.lock/sqlite3/json.
Default512MiB/30sec,32-page cooperative backup, close/reopen validate/close/sync/hash,
manifest LAST. Post-marker failures CompletionUnknown preserve exact output and
require inspection, not deletion/retry/new name. Never raw-open/close a DB inode
while a same-process SQLite connection is active (POSIX lock-release hazard).
Validation sqlite-integrity-authority-identity-v1 is NOT semantic/freshness audit.
Inspection always denies restore/continuity; same-ID stale history losing a prior
ack was measured. Broader destination-busy/permission faults, writer_version
validation and independent-host continuity/fencing/restoration remain work.

Linux CLI SIGINT/source-busy checks at e51, binary SHA
fca6f0e893bfe85365bd2121e05cd64cb73b2bcbb361880a41b7448c4e377e86:
/tmp/s3drive-e51-snapshot-{sigint,busy}.{json,stderr}. Source bytes preserved,
no false manifest/replacement, partial output retained and retry refused,
positive create/inspect after lock released. Durations1.995ms/112.498ms are single
observations. Current accepted Windows ordinary-account snapshot controls add
ACL/link/junction/case/read-handle and isolated console checks, not restore proof.

## Startup, provider approval and remaining full-goal work

At sign-in with same-account incoming SSH/tmux persistence after disconnect,
Linux MUST support MX without systemd, plus Windows. No real user startup entry,
root, credentials, hosting or Turso account changes were made. Actual reboot,
PAM/SSH and Windows10/11 desktop acceptance remain. Local debug build baseline
was Debian12/glibc2.36/maxGLIBC_2.34, not release packaging or MX/reboot proof.

Real Wasabi remains read-only: aliaswasabi, bucket s3drivetest, regionus-east-2,
endpoint https://s3.us-east-2.wasabisys.com, profilewasabi-s3drive. Preserve ignored
local config/plan/report; never print keys/config source/private credential digest.
Earlier three-object Unicode/read scope cleaned. Later six-key prefix
s3drive-conformance/d147830ba5257d762e14c3962cde60f9/ ran ONCE Oct16T23:36:18Z at
8eeda5b,10 checks passed and six keys deleted/prefix empty. All14 PUT attempts
consumed; plan digest98914e5210466c6c1e074ea36184ea580d20c93749363e18650e5ddf14906de6.
No new live mutations authorized. Provider capability evidence is not permission.

Prepared ONLY, NOT approved/executed: fresh six-key conditional plan (at949),
/tmp/s3drive-wasabi-next-20261018-wz8t4v1w/probe-plan.toml, mode0600; pointer
/tmp/s3drive-next-wasabi-plan-location.txt. Prefix
s3drive-conformance/69329649f34f798791386c4d3c62ac1e/; confirmation
03febed0e9df68b61994d830830d73abdb972b096b0d74d8344ad0ec728ee177;
file SHA649c8edbb7f51a27c953b19959ea23f00311af9409551b8f5bed5924068b1720.
At most14 PUTs,6 keys,4KiB/object,<1MiB planned script transfer. No credentials
resolved/provider calls in preparation. Renewal is optional: first inspect original
report timestamp/binding and explicit max age; never extend expiry to bypass it.
Application writes reject reserved s3drive-conformance/ namespace. Managed live
acceptance needs a separate concrete runner/approved application prefix and bounds.

Full goal still requires reliability investigation/repeat evidence; approved real
managed Wasabi and large/multipart files; reference-aware reclamation/application
workflows; independent-host partitions/deployment; continuity/fencing/restore;
physical startup/target desktops and packaging/distribution/licensing decisions.
Do not redefine completion around this passing subset.

Other clean worktrees: /home/marshall/src/s3drive-windows-transport at f8 on
codex/windows-transport-followup; /home/marshall/src/s3drive-snapshot-acceptance
detached at33ddfe1 for inspection. All share root target; rebuilding elsewhere
replaces binaries. Rebuild/verify intended source before CLI experiments.

## Archive

The full previous handoff was archived byte-identically before refresh:
[before bf9509 refresh](docs/archive/HANDOFF-before-bf9509-refresh-20261018.md).
Archive SHA256: 7e6ea7afa13c681ba9f46f166b0cbcf4a75b1091722fab54f77f820d7877769c. Before: 405 lines; after: 242 lines.
Previous complete archives before-a14-refresh, before-snapshot-cancel and
before-flush-receipts remain in docs/archive; no history was discarded.
