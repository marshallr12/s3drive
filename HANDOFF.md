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
include main's policy commit; the c7fca789 feature lineage includes that rule.

## Current W1c2 combined baseline (2026-10-18)

Canonical /home/marshall/src/s3drive is now clean on main at
35abcc909f42d9aac1dc12847cfaa569e1aaef8a, exactremote and PUBLIC. It contains
acceptedbf2e6ae runtime only:261 hashes/all268 non-document paths equal2e7b6ca,
with final docs checks; history/reopen code is NOT on main. Proof
/tmp/s3drive-primary-native-integration-publication.json. This draft branch
currently diverges only in intended new authority code plus docs from that main;
include the current main ancestry before eventual promotion, preserve user edits.
No live CI/Cargo process remains; implement W1c2 next rather than repeat accepted
cohorts. New reader code still needs its own full/hosted/native acceptance.


Local W1c1 e18d6f5 is now combined with accepted W1b2e7b6ca. Runtime merge is a
new draft, baseline all-target check9230 CLOSED0 and focused reader77295 CLOSED0
(9pass/1nativeignored10.69s). All264 inputs unchanged in
/tmp/s3drive-history-reader-baseline-frozen.json; result
/tmp/s3drive-history-reader-baseline-result.json. No own combined full/hosted
acceptance. Reader changes have not begun. Implement the W1c2 proposal in the
implementation plan, preserve all existing namespace/native behavior and run a
fresh full/own hosted gate before W2 or activation. Keep chunk writes unsupported.
No pending CI remains for the W1b prerequisite; watch7139/checker12398 CLOSED0.

## Local prerequisite W1c1 historical manifest metadata (2026-10-18)

Isolated /home/marshall/src/worktrees/s3drive-payload-history-authority,
codex/payload-history-authority from accepted da84d3a. Page-proof sourcebf2e6ae
is accepted in its own checkout/run39187701991 at2e7b6ca; that does not certify
this combined history draft. Scope: authenticated, exact-reference historical manifest
payload lookup from existing namespace_uploads, state/authority controls, book.
No schema migration, S3 authorization/write, arbitrary File constructor or
native activation. New additive manifest-history-v1 request/reply/capability.
Use one read transaction, validate reservation operation and publication against
requested volume/file/reservation/reference; retain original CAS, never rebase.
Require focused corruption/restart/auth/old-client controls and full local gate;
then reader base-token/reopen integration and own combined hosted gate before use.
Read implementation plan. Full sparse views/mixed publication/settlement/native
chunk writes remain required. No new provider mutations authorized.
Initial all-target check44456 CLOSED0; state3 passed0.68s. HTTP1 failed to find
integration-test module; explicit namespace/history.rs path fixed. Size negative
77836 CLOSED101 atfault12 proved valid padded JSON bypassed proposed16KiB bound.
SQL CASE now rejects all five oversized fields before Rust Vec copying; same
read transaction prechecks records reused by replay validation. Statefocused2
93999 CLOSED0:3 controls/17 corruption scenarios1.70s. HTTPfocused2 68127 CLOSED0:
production WireClient/auth HTTP history/restart/no-rebase/auth/bad bindings0.18s.
Full local gate60956 CLOSED EXIT0, /tmp/s3drive-payload-history-gate.log, frozen262
/tmp/s3drive-payload-history-frozen.json, all unchanged.368state/1ignored,11authority
namespace,8reader/format1nativeignored,12publication,62S3,fmt/clippy/build/book passed.
Indexed20-member local archive71917bytes SHA
d7e7aedb493448a7794ddb15f66503d52e188caced3284fe36724ada15942668. No own
hosted endpoint acceptance yet. Logs and book retain both initial failures.
Next W1c2 proposal in implementation plan: separate metadata-free content version
from named File, 88-byte untrusted base descriptor, authority-bound old-root
reopen/current S3 auth, original own-payload CAS (not its predecessor guard).


## Current W1b page-proof foundation (2026-10-18)

Accepted runtime bf2e6aed16d32c62e094082a50769141f8f90ef6, own run39187701991
attempt1 ALL3 SUCCESS. Watch7139 CLOSED0, checker12398 CLOSED0. All three raw
logs reviewed and source-bound: Linux FUSE1.13s, Windows WinFsp3.04s;9reader/
format1nativeignored,62S3, Linux365state1ignored,Windows360state1ignored.
Own artifact ZIP API digests, typed21+14/project oracles/all35 raw fields/types
passed, no older artifacts reused. All261 frozen working/committed inputs match.
Indexed31-member hosted archive172177bytes SHA
7ea07240c7ab082534754536d2809b90168b0f53342cd955c3a0ddc0d8ea3096.
Windows Server2025 image20261007.264.1; three disposable profiles locked, dummy
credentials removed, removal deferred to VM destruction. Unused delay-import,
long-test and Node action notices retained. Linux no-systemd HTTP/privateCA23.71s;
Windows paired recovery80.16s. No performance, physical reboot or provider claim.
Fresh full local gate68162 CLOSED0, all261 unchanged; indexed19-member local
archive100784bytes SHA333f2722f4672361a9bcf2304fb92061df42813c001bcebfe4066facd8b7cb8e.
No native chunk writes or historical-base reopening follows from W1b acceptance.

Next implementation /home/marshall/src/worktrees/s3drive-payload-history-authority,
codex/payload-history-authority: local clean W1c1 checkpoint
e18d6f5b619f3c1b17cdb734af093ce0b1d4df84, not pushed. Full gate60956 CLOSED0,
frozen262 unchanged,368state1ignored/11authoritynamespace plus all workspace/
book checks.20-member local archive71917bytes SHA
d7e7aedb493448a7794ddb15f66503d52e188caced3284fe36724ada15942668.
Authenticated exact history metadata,16KiB SQL guards and no-rebase HTTP controls
passed; no reader/native activation. Read that HANDOFF/plan. Combine accepted W1b
then implement W1c2 BaseFile/version split and authority-bound old-root reopening;
require own combined full/hosted gates. Sparse views, mixed publication, exact-view
settlement/native chunk writes remain required. Goal ACTIVE; PUBLIC intentional.

Isolated /home/marshall/src/worktrees/s3drive-manifest-page-proofs,
branch codex/manifest-page-proofs from local-green W1a ed2e93b (not hosted-
accepted separately). Combined candidate8c3448f is accepted in its own checkout at da84d3a;
run39184629344 ALL3/raw/artifacts passed; this does not automatically accept W1b.
Scope: opaque PageProof metadata witness, reader exports, real SDK/authority
fixture controls and book. No native activation, schema/wire changes, writes or
receipt conversions. Data and page witnesses share a base identity but distinct
encodings. Prove root/canonical and noncanonical subtrees, published-empty root,
base/kind/shape/corruption/trailing refusal, shared capacity/deadline/cancel and
actual reuse of a prior root as a child with zero data reads during proof export.
Focused7/7 passed5.54s, log /tmp/s3drive-page-proofs-focused2.log. Initial build
needed explicit authority-error conversion; initial positive reuse fixture used
a noncanonical level-one/two-chunk root and correctly failed. Corrected fixture
uses canonical128-to129-chunk height growth and retains wrong-child-span refusal.
Full local gate session95188 CLOSED EXIT0, /tmp/s3drive-page-proofs-gate.log,
frozen260 /tmp/s3drive-page-proofs-frozen.json. Clippy,7reader and12publication
controls passed with365state/1ignored,62S3,fmt/clippy/build/book. Runtime unchanged.
Indexed16-member local archive61969bytes SHA
cc54ea160c8c9bf36ae4c8daf8c6ca24663a5efaf5845a391dc1ea35d962f902.
Combined8c3448f now ALL3/raw/typed21+14/raw35 accepted at da84d3a;
watch73843 CLOSED0, checker68867 CLOSED0. Own W1b hosted gate now accepted above.
Independent and combined local gates plus own hosted gate passed. Full sparse edits/publication/
settlement/native chunk writes remain later work. Read implementation plan.

## Accepted prerequisite snapshot-format repair (2026-10-18)

Accepted runtime8c3448f478f9ab62fe1a23ce965907546be5d10c: own run39184629344
attempt1 ALL3 SUCCESS; watch73843 CLOSED0, checker68867 CLOSED0. All three raw
logs reviewed/bound to exact jobs; Linux FUSE1.63s, Windows WinFsp2.53s. Own
artifact ZIP API digests, typed21+14/project oracles and all35 raw fields/types
passed. No c633/earlier artifacts reused. Frozen259 working/committed unchanged.
Indexed29member hosted archive169525bytes SHA
318b4981938bf217e29b681bcef31ca7edccaaebe53c417ca03d8e93c6b6147a.
Windows Server2025 image20261007.264.1; three disposable profiles locked,
dummy credentials removed, removal deferred to VM destruction; unused delay
import warnings retained. Linux no-systemd HTTP/privateCA24.26s and paired
FUSE recovery passed; Windows paired recovery84.85s. These are loopback runner
controls, not live Wasabi, physical reboot or native chunk-write acceptance.

Next worktree /home/marshall/src/worktrees/s3drive-manifest-page-proofs,
codex/manifest-page-proofs from ed2e93b: W1b metadata-only PageProof, focused7
passed5.54s; independent full local gate95188 CLOSED0, frozen260 unchanged.
W1b3890584 combined accepted da84d3a into bf2e6ae; its full and own hosted
gates are now accepted above. Historical base reopen/sparse edits/mixed-tree publication/
exact-view settlement/native chunk writes remain required; goal ACTIVE.


Worktree /home/marshall/src/worktrees/s3drive-manifest-reference-proofs,
branch codex/manifest-reference-proofs. Local merge9c49fe4 combines accepted
native52f591c with local-green W1a ed2e93b. Combined source is now accepted by its own all3/raw/artifact cohort.
Negative control55850 EXIT101: old metadata-empty inode reported4 instead of0
following manifest publication; /tmp/s3drive-formats-negative.log. Production
was unchanged for this control. Repair now snapshots all file content versions
in read-only chunk-capable backends (empty/legacy/manifest), keeps directory
identity stable, and guards both manifest and legacy factory results against
stale payload/identity/kind. Direct, writable and capability-absent paths stay
scoped unchanged. No schema, wire or provider mutation changes.

Focused91719 EXIT0:8 passed/1 native ignored6.49s. Includes format reversal,
legacy-to-legacy race, metadata-only attributes with zero namespace/S3 requests,
old immutable bytes and early stale-open refusal. Initial expanded fixture hit
unchanged legacy4-snapshot cap; completed scenarios now release handles before
new independent scenarios. No production quota raised or assertions relaxed.
Two unused-mut warnings were removed; Clippy30783 EXIT0. Native control extends
same-name publication and retained fd sizes/bytes to empty/legacy/root formats,
republication and rename, with existing teardown deadlines and legacy cap.
New real OS assertions passed in this exact-source cohort.
Combined gate1 session38083 CLOSED101 on existing readonly engine fixture
reopening a stale inode. Updated only that engine case to refresh path lookup,
assert stale-open Conflict and distinct snapshot inode, preserve global ID,
old bytes, rename and deletion assertions. Writable cases unchanged. Its isolated
positive26588 passed1.83s. Failure/map retained in /tmp/s3drive-formats-combined-gate1-failure.json
and /tmp/s3drive-formats-combined-gate1-frozen.json. Full combined gate2
session81306 CLOSED EXIT0 -> /tmp/s3drive-formats-combined-gate2.log; all259
inputs unchanged in /tmp/s3drive-formats-combined-frozen.json. Eight reader/format
controls/1nativeignored,12publication,62S3,365state/1ignored,fmt/clippy/build/book
passed. Indexed25member local archive116562bytes SHA
d7ad206a5579ef5dcaaedb4ee3e7c82374c262093098f375208506bdf4076e5b. Own fresh native/hosted cohort accepted. Previous
W1a/native maps and archives retain their own source; do not reuse as acceptance.

## Historical W1a foundation: authenticated chunk-reference proofs (2026-10-18)

Isolated /home/marshall/src/worktrees/s3drive-manifest-reference-proofs,
branch codex/manifest-reference-proofs from accepted reader7c2b454 (all3 own
reader gates accepted). Independent library foundation while native repair
c633 run39178517491 ALL3/strict21+14/raw35 accepted at52f591c; watch30345 CLOSED0.
Preserve that accepted candidate; its artifacts do not accept this W1a source.
Local archive 16members/53064bytes SHAb1af7908f2646d1f33436949897ae04ce22b093baade991cb137ff52530eee8a.
W1a is accepted in combined source8c3448f, with native prerequisites and its
own fresh exact-source hosted cohort; the standalone ed2e93b did not run CI.
Next audit before primary promotion: add empty/legacy opened-handle metadata
checks across payload-format publication; current native control begins with
a manifest base. Preserve original pinned size/bytes assertions.
Scope: reader child provenance module/export, existing SDK/authority fixture
controls and book. No filesystem activation, writes, schema, wire or codec changes.
W1a exports only opaque data/hole chunk proofs; not the full W1 page-reuse or
historical-base-reopen API. Prove before acceptance: bounded path-only reads,
no data fetch, pure snapshot-bound roundtrip validation, rename-stable base,
new-root/foreign-file/corrupt/trailing/truncated refusal, aggregate holes, EOF,
64GiB sparse bound and shared operation budget. Full local gate required;
independent hosted gate needed before integration. Initial cargo check passed;
six focused controls passed4.97s, including shared-budget and proof scenarios.
Full gate1 session93249 exited143/SIGTERM during the existing remote TLS
suite, no test failure reported, cause unknown; no Cargo/rustc remained.
Log /tmp/s3drive-reference-proofs-gate.log; interrupted evidence in
/tmp/s3drive-reference-proofs-gate1-interrupted.json. All258 frozen inputs
unchanged. Pending TLS diagnostic test passed alone10.58s in session77216
(package-only command, not whole-workspace acceptance). Fresh full gate2
session98776 CLOSED EXIT0 (tty), same production code and RUST_TEST_THREADS2:
/tmp/s3drive-reference-proofs-gate2.log. All258 frozen hashes unchanged; full fmt/clippy/test/build/book passed.
No whole-workspace test failure reproduced; original signal cause stays unknown. Frozen258 /tmp/s3drive-reference-proofs-frozen.json.
Preserve separate source
acknowledgment and inherited-reference publication proof work for later phases.

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

## Accepted read-only manifest native integration (2026-10-18)

Runtime c633d02d47ccf2da20cfadd50e0b8e11f6e06811, all257 working/committed
hashes remain frozen. Own run39178517491 attempt1 ALL3 SUCCESS and raw-reviewed:
Linux117826123544, Windows117826123721, book117826123692. Watch30345 CLOSED0.
Both explicit real native manifest controls passed: Linux1.22s, Windows1.79s.
Strict independent typed21replacement/14disposition, project oracles and all35
raw-field/type comparisons passed; checker95100 CLOSED0. Own finalized artifacts
12155790591 and12155504654, API ZIP digests checked; no older artifacts reused.
Indexed26member hosted archive143913bytes SHA
800057c4b01f08120681438acc67ce0099fe083ffdc476af37e0e6ce5ee912fd, every member
verified. See docs/src/evidence/manifest-snapshot-inodes-20261018.json and
manifest-snapshot-inodes-c633-hosted-20261018.tar.gz. One-use checker/rawcompare/
archive already completed; do not rerun their mkdir/assert-not-exists paths.

Linux9existing native63.27s,365state/1ignored106.66s, no-systemd coordinator
HTTP/TLS23.97s. Windows7existing native95.32s,360state/1ignored533.18s,
paired startup81.64s. Timings identify executions, not benchmarks. Raw Windows
notices retained: three locked disposable profiles, dummy credentials removed,
profile disposal deferred to VM destruction; unused WinFsp delay import in11
unique targets plus standalone manifest invocation; Node notices; five long
controls ultimately passed. No physical reboot or new Wasabi mutation claimed.
Local full gate68406 passed before publication: all257 unchanged,12publication,
6reader/1nativeignored,62S3,365state/1ignored,fmt/clippy/build/book. Indexed13member
local archive133937bytes SHAe243742e802539035a8efa36478ce339a6f1ce43646460592bade4feb887a2aa.
Old d98 cohort39176574738 is terminal FAILURE and retained separately: Linux
old130MiB fd stat returned4 after new publication; Windows passed2.39s. The repair
keeps that assertion and adds same-name replacement before rename plus guards
against stale/racing opens. Do not reuse the failed cohort as acceptance.

Next: W1a independent data/hole reference-proof foundation is uncommitted in
/home/marshall/src/worktrees/s3drive-manifest-reference-proofs,
codex/manifest-reference-proofs from accepted reader7c2b454. Focused6 passed4.97s;
first full gate93249 interrupted143 without test failure, cause unknown; pending
TLS test passed alone10.58s. Fresh full gate2 session98776 LIVE ->
/tmp/s3drive-reference-proofs-gate2.log,258 inputs frozen. Read its handoff before
work. Do not edit mapped source during gate. It does not activate native writes.
W1-W6 plan here records authenticated bases, sparse edits, mixed-tree proofs,
publication, exact-view settlement and actual native writes. Source inspection
corrected the prior-root claim: root_key uses Role::Page; geometrically valid
old roots may be inherited as children with explicit provenance. No invented
upload receipts. Goal ACTIVE; preserve PUBLIC and main policy. Primary runtime
promotion is a separate integration action, not inferred from evidence publication.

## Failed native-read candidate / historical hosted gate (2026-10-18)

Source d98ea2ff2ec9cc72b55458190d657bb53642d227 pushed on
codex/native-manifest-reads from local feature9ed5765 + accepted reader7c2b454.
Clean exactremote/all257 committed/runtime hashes verified; PUBLIC retained and
main1a93b5f unchanged. Own run39176574738 attempt1 was launched; Linux
subsequently FAILED as detailed above, so this cohort cannot be accepted:
Windows117820371735, Linux117820371839, book117820371587.
Watch24055 CLOSED EXIT1 -> /tmp/s3drive-native-manifest-d98-watch.log;
do not resume or run this failed cohort's success checker.
Both rust jobs explicitly include Verify native manifest ranges and remount.
Do not run this failed cohort's success checker or borrow its artifacts for the
repair. The repair requires all3 terminal source-bound jobs, rawlog review, own
finalized Windows reference artifacts and both native controls. Preserve cohort and
diagnose before fresh source. No own Linux/Windows raw review or combined hosted acceptance yet.
Runtime257 remains frozen in/tmp/s3drive-native-manifest-v3-frozen.json.

Book117820371587 SUCCESS; own raw log reviewed/hash-bound in
/tmp/s3drive-native-manifest-d98-book-reviewed.json (schema28/69files/65chapters).
Linux/Windows still live; current observed Linux at old native mount gate,
Windows at disposition lifecycle before fullworkspace/newnative step.
Prepared (not executed) /tmp/s3drive-check-native-manifest-d98.py,
/tmp/s3drive-native-manifest-d98-reference-check.ps1, raw-compare.py andarchive.py.
Checker binds source/run/jobs/257map and BOTH explicit native step successes plus
named engine/native tests; requires6reader/1ignored+12publication+62S3 controls.
Combined /tmp/s3drive-native-manifest-d98-raw-reviewed.json and artifacts directory
must remain absent until all3terminalSUCCESS and all ownrawlogs reviewed.
Archive also retains exact FS/native-test sources, not just the managed reader.


Local fullgate3 session58981 CLOSED EXIT0, RUST_TEST_THREADS2, all257 unchanged:
12publication+6reader/1nativeignored+62S3+365state/1ignored+69docfiles/65chapters,
fmt/clippy/build. 27member local archive114301bytes SHA
138761972263bdf707c249fccf8954a4766a1e4497153ff2b796fd4e71f2a2a6;
all indexed members verified. Gate1TLSsetupcause stillunproven; gate2serial-libtest
marker framing proven withnegative/positivecontrols and repaired fixture newlines,
without relaxed deadlines/assertions or productiontransport/writer changes.
Explicit local native attempt failed FUSEreadiness OperationNotPermitted under
NoNewPrivs1; ownedchildreaped/noFUSEleft. No root bypass or provider mutation.

After native acceptance, chunk writes remain: implementation-plan.md records
legacylogical8MiB/rejectedmanifestbase and full-snapshot planner boundaries.
Do not pretend raising8MiB enables bounded/reusing writes. No such API implemented.
Primary253controller and acceptedreader256 remain unchanged. GoalACTIVE/PUBLIC.
This current section supersedes historical pending/publication notes below.

## Native local gate passed; publication next (2026-10-18)

Worktree /home/marshall/src/worktrees/s3drive-native-manifest-reads,
branch codex/native-manifest-reads fromb3e30f0, local feature commit9ed5765. Read plan and
native-manifest-reads.md. Current257 frozen map is
/tmp/s3drive-native-manifest-v3-frozen.json; old canonical map records gates1/2.
Gate3 CLOSED EXIT0,257 hashes unchanged; do not repeat without new change/failure. Shared CARGO_TARGET_DIR=/home/marshall/src/s3drive/target,
RUST_TEST_THREADS=2, same scripts/gate.sh, inner concurrency/deadlines unchanged.
Gate3 session58981 EXIT0, log /tmp/s3drive-native-manifest-gate3.log. Preserve prior failed cohorts.

Gate1 22825 EXIT101 at existing setup managed store-plan InfoTLSConnecttimeout
beforeHTTP/body/newFS; causeunproven, observedhighCPUpressure. Isolated unchanged
startup39166 EXIT0,2.69s. Gate2 46929 EXIT101 at pairedfixture30s marker timeout.
Diagnostic33451 defaultthreads passed4.01s; full workspace serial83006 failed at
ENGINE_READY after file-open-write. Negative20443 caught exact libtest prefix
in2.54s (EXIT101): serial runner merged test-name prefix with first marker.
Fixed only fixture framing: leading newline before ENGINE_READY andPRODUCER_READY;
kept exact marker matching/30s bound and added static stage/marker diagnostics.
Positive55315 fullworkspace serial fixture EXIT0,4.60s inclrecovery/conflict;
producer62449 fullworkspace serial startup EXIT0,2.16s. No production retry,
transport deadline, schema/wire or writer change. Native test now explicitly
checks native open makes zero data-chunk GETs. No real native success yet.

Native read-only implementation: version3 mixedlegacy/manifest namespace,
logical64GiB sizes, bounded pinned range OpenFile, two stores/Unicode/sparse/
oldroot/fault controls, real FUSE/WinFsp test + explicit CIstep. Before latest
fixture refinements,6focused+1ignored PASS5.74s andClippy3 PASS. Explicitlocal
native attempt EXIT101 atFUSEreadiness OperationNotPermitted/NoNewPrivs1,
ownedchildreaped/noFUSEleft. No bypass/root/provider mutations.

Reader88d ACCEPTED all3 ownCI39171113280 andstrict21/14+raw35; checkpoint
7c2b45435cd4aa3999e63ee16ccb6b03f5985c76 pushed, cleanexactremote.
Next: commitdraft, merge accepted reader doccheckpoint
preserving257map, push ownnative source for real FUSE/WinFspCI. Local archive builder executed successfully:27members/114301bytes SHA
138761972263bdf707c249fccf8954a4766a1e4497153ff2b796fd4e71f2a2a6;
all indexed members verified. Result /tmp/s3drive-native-manifest-gate-result.json.
LogSHA0a6fc4dd936983e8268d298f871e179f02a79acd92e3916e9d193d4ef40c3466;
mapSHA6f192f26f798fc4c411135b73aef3e9d620fcf3a4051b882f40e133653686924.
12publication+6reader/1nativeignored+62S3+365state/1ignored+65chapters PASS.
Native FUSE/WinFsp acceptance still pending. Primary253/reader256 unchanged.
PUBLIC remains current; goalACTIVE. This section supersedes historical pending state.

## Accepted reader; native draft continuation (2026-10-18)

Source88d02f7a6605bf13bfd212d67c6609ae16b0ebfd own run39171113280 attempt1
all3SUCCESS:Windows117804178631,Linux117804178819,book117804178853.
Watch42929 CLOSED EXIT0; all own raw logs reviewed/hash-bound. One-use checker
3479 CLOSED EXIT0, own finalized replacement12152741226/disposition12153275892,
independent strict21/14 + project oracles + raw35 field/type comparison PASS.
22-member hosted archive manifest-reader-88d-hosted-20261018.tar.gz,118459bytes,
SHA994942870840d5efd3082d61dd3533e89dbf0bd154844f5b3d2d93b7b37428ac;
all indexed members verified. Do not reexecute one-use checker/archive builders.
Windows5reader4.61s/12publication32.89s/62S3/360state1ignored; Linux5reader4.12s/
12publication51.31s/62S3/365state1ignored. Existing native regression only:
Windows7native82.70s + pairedstartup71.97s; Linux9native76.31s + no-systemd/SSH.
Windows3 locked disposable profiles retained after dummy credentials removed;
unused WinFsp delay-import +11target reports and Node/cache notices preserved.
This accepts library reader, not mounted manifests/provider/reboot behavior.
Runtime256 hashes remain frozen in the isolated reader checkout; publication proof in/tmp.

Current draft is /home/marshall/src/worktrees/s3drive-native-manifest-reads,
branch codex/native-manifest-reads fromb3e30f0. Read its HANDOFF/plan/contract.
Six focused portable controls/one native ignored passed; explicit local native
attempt failed at FUSE readiness OperationNotPermitted under NoNewPrivs1,
child reaped/noFUSEmount remains. Full localgate22825 live with257 frozen files.
No new native source publication yet; do not confuse its pending gate with this
accepted reader. Primarycontroller253 stays unchanged. PUBLIC/goalACTIVE.

## Active reader implementation (2026-10-18)

Checkout /home/marshall/src/worktrees/s3drive-manifest-range-reader,
branch codex/manifest-range-reader. Accepted controller474 and helperfef sources
are now combined here; primary and helper checkouts retain accepted sources.
New reader and owned read fixture passed5 focused controls (78806 EXIT0,4.68s)
and Clippy75607 EXIT0 before combination. Full combined gate84960 CLOSED EXIT0; all256 frozen source hashes unchanged.
No legacy Reader, schema/wire/codec/native changes. Fixed4active/256snapshots,
8MiB output/1MiB data/10432byte page/27visits/9data, max30s overall deadline.
ReadOptions accepts shorter positive deadlines. Sparse64GiB deepest tree,
older chunk reuse, holes, Unicode, corruption/denial, cancellation and capacity
controls passed locally. No mounted chunks or new provider acceptance yet.
Historical pending/live sections below are superseded by current acceptance.

Full combined gate:12publication/source,5reader(4.94s),62S3,365state/1ignored,
68docfiles/64chapters,fmt/clippy/build. Existing retained-open quota long notice
then passed; no performance claim. Log /tmp/s3drive-manifest-reader-gate.log SHA
1766197333b078b036f5f728b0996e89efe845a6ac0c584b490ca83a68360093.
Frozen map /tmp/s3drive-manifest-reader-frozen.json SHA
2aff69552178c0881e75f09bfb1fa94c84a8a1443941e5a850cfa1f7bd1dc49b.
14-member local archive manifest-range-reader-local-20261018.tar.gz,72706bytes,
SHAa1737729817e21d78c356c699987e1d4129f128172c71363007b04d9bb4a2ffd;
all indexed members verified. Self-review, no independent agent review.
SVG XML/render/visual review passed after fixing Rsvg.Rectangle initialization.
Do not repeat local gate without new change/failure. Next own Linux/Windows/book
cohort with own finalized native artifacts/raw checks; old helper/controller
artifacts cannot certify new reader source. No new provider mutation/main push.

## Accepted helper; next manifest reader (2026-10-18)

Source fef944268c89d2c0b1d859a6a86d0f6761c24361, own run39167583915 attempt1:
Windows117793655137, Linux117793655088, book117793654818 all SUCCESS.
Watch43951 CLOSED EXIT0; checker60633 CLOSED EXIT0. All three raw logs reviewed
and hash-bound; own finalized replacement12151153703/disposition12151479241
passed independent strict21/14, project oracles and raw35 field/type comparison.
22-member archive pinned-related-fef-hosted-20261018.tar.gz,118377bytes, SHA
15d38ecf5047cd5f67c70c37347a19b527e2476d77c5580654edfbf44c629d26;
all indexed members verified. Do not reexecute one-use checker/archive builders.
Both prerequisites now accepted; combine with controller474 before reader gate.
Reader draft: /home/marshall/src/worktrees/s3drive-manifest-range-reader,
codex/manifest-range-reader. Focused reader78806 EXIT0,5 controls,4.68s;
Clippy75607 EXIT0. No mounted chunk acceptance or main promotion.
Existing native regression only; Windows3 locked disposable profiles retain
no dummy credentials, delay-import/Node notices retained. No physical reboot or
new Wasabi mutations. PUBLIC policy remains current. Goal ACTIVE.
Helper full local gate57146 CLOSED EXIT0,252 frozen files unchanged; archive
pinned-related-reads-local-20261018.tar.gz retains source/audit/self-review.
Only production delta is ReadHandle.open_related with fresh scoped HEAD under
captured credentials. Independent new opens rotate; legacy reader unchanged.
Three new S3 controls prove chained credential pinning, exact scope/fresh denial
and bounded inline future layout. No profile hot-reload promise. See book
pinned-related-reads.md and dated evidence for detailed checks and failure history.


## Current work: controller accepted; verified reader preparation (2026-10-18)

Previous goal turn was verified wait on39163298774/Windows117780950165. This
turn completed own41b acceptance:all3SUCCESS, watch28386 EXIT0, ownrawreview,
strict21/14+projectoracles andraw35PASS,22memberarchive verified; docscheckpoint
20706cd pushed oncodex/chunk-source-settlement. PUBLIC retained. No rerun.

Branch codex/chunk-source-controller from20706cd. Productiondelta only
crates/s3drive-managed/src/chunks.rs: explicitPublisher/Recovery.resume_source
returnsSourceReceipt afterdurableroot then sameownerfacadesettle; onefamilypermit
throughboth; privateunlockedpublisherhelper preservesoldresume.RootCommit.
Knownroot noRPC; Preparedunknown fullsavedApplyonly, scopefirst. No schema,
S3format/wire/native change. Tests parentchunk_publication.rs + newchildmodule
chunk_publication/source_settlement.rs reuseexistingownedSDK/authauthorityfixture.
Fournewcases:Unicode/binary/sparse/empty/history; killedunknownroot withnewerroot/
localhead; killedknownroot afterrefusedsourceinsert; expired/narrowpolicy plus
rootreceiptsavefailure, barrierBusy andconcurrentfirstack. Parent assertskilled
exitunsuccessful, removesonlyfixturetrigger, disablesdummycredentials/policy,
corruptssourcebytes, countsallS3+uniquePUTs. Rivalconflictalsoexplicitlyrefuses
newAPI. Source state unchanged. One compile error (double-reference to manifest
getter) fixed; focused2 session75700 CLOSED EXIT0,12pass60.30s; log
/tmp/s3drive-chunk-source-controller-focus2.log. Additive corrupt-record newAPI refusal passed29993 EXIT0,1pass0.90s.
Fullgate80848 CLOSED EXIT0, runtime frozen253files in
/tmp/s3drive-chunk-source-controller-frozen.json. Log
/tmp/s3drive-chunk-source-controller-gate.log. Full local gate PASS:12chunk59.79s,
365state/1ignored172.37s,59S3,34core,50authority,18existingpublication,10TLS/3ignored,
66docfiles/62chapters;fmt/clippy/build. Oneoldretainedsessionquota longnotice then
PASS; no benchmark/regressioncause claim. 253hashes unchanged. ResultJSON
/tmp/s3drive-chunk-source-controller-gate-result.json. GateLogSHA
0c441d6b18a101269bbca02e2dd4b2fb059f74fa9936c0f7795fccad7135484a.
12memberlocalarchive chunk-source-controller-local-20261018.tar.gz,53154bytes,
SHA9289080295e870ccf68f1915aba328e5e69c60353190da7e0516604aa2589228,
allindexedmembersverified. Implementation self-review only, noindependentreviewer.
Published testedsource474601edd5582ebed9316d04cf4524db6d72694b,8files, clean
exactremote/all253hashes/PUBLIC/main1a93b5funchanged; normalnewbranchpush. Proof
/tmp/s3drive-chunk-source-controller-publication.json. Publication21576 EXIT0.
Own CI39165918104 attempt1 all3SUCCESS:Windows117788707767,Linux117788707913,
book117788707918. Watch79476 CLOSED EXIT0. All3ownrawlogsreviewed andhashbound.
Oneusechecker25752 EXIT0; own4newcases passedbothOSlogs,253sourcehashes+4oracles
match, ownfinalizedIDs12150891787/12151260949 withAPIdigest/timechecks; strict
21/14+projectoracles PASS and all35rawfields/types equal. Ownoutput
/tmp/s3drive-chunk-source-controller-474-artifacts; no oldartifactpayloadsreused.
22memberhostedarchive chunk-source-controller-474-hosted-20261018.tar.gz,
116497bytes,SHAdc56024d08b133c10b51e9db8609814df2922e8b91b01ee0ed4d53c6cc8dde41;
allindexedmembersverified. Builder /tmp/s3drive-chunk-source-controller-474-archive.py
executedEXIT0; notprepared-onlyanymore. Do notreexecuteoneusechecker/archive.
Windows12chunk32.39s,360state/1ignored,7native71.81s,pairedstartup70.87s,
32HTTPSflushes/disposition/snapshotconsole PASS. 3lockeddisposableprofileswith
removeddummycredentials,unuseddelayimport+9dups,Node/cachedeprecations andexpected
HEAD403/contentconflict/stoppedauthorityListtimeout retained. Linux12chunk44.40s,
365state/1ignored,9native62.68s,no-systemd/SSH/HTTP/privateCAHTTPS recovery PASS.
Book66files/62chapters PASS. Existingmountedformat only; notmountedchunks,
newWasabi orphysicalreboot acceptance. Source474controllerlibraryisaccepted.
RelatedS3helper isisolatedbelow andhasownliveCI39167583915/watch43951. FullgoalACTIVE.
 Nativechunklifecycle/rangereader remainlater. Concrete subsequent verified-range
reader contract now inimplementation-plan.md; preserveslegacyrefusal, pinnedroot,
separatephysical/pagehashes, boundedauthenticatedpath/chunkfetches andcurrentS3
authorization. Plan only, no reader implementation yet.
Book117788707918 SUCCESS/rawreviewed own474/schema28/66files/62chapters; marker
/tmp/s3drive-chunk-source-controller-474-book-reviewed.json. Linux117788707913
alsoSUCCESS/rawreviewed:12chunk44.40s(all4new),365state/1ignored,9native62.68s,
no-systemd desktop/SSH andpinnedHTTP/HTTPS/privateCA recovery24.15s. Marker
/tmp/s3drive-chunk-source-controller-474-linux-reviewed.json, logSHA
c5273e9fe0732986a166f42a71c5193a744bb3a78aff5286f6594ea7fc86b0a3. Expected
HEAD403/ConnectionRefused/removedstartup.json refusal andNode/cachedeprecations
retained. Existingmountedformat only; notmountedchunks/Wasabi/physicalreboot.
Windows117788707767 subsequentlySUCCESS; owncontroller acceptance above. Watch79476closed.

Readerdesignaudit foundmissing pinned-credential related-object open API. New
concretecontract inimplementation-plan: samecredentialgeneration forroot/children,
rootactuallevel == descriptorordinallevel (authoritymaxlevel2), metadata-only
EmptyFile vs publishedemptyroot,4activeoperations/256pinnedsnapshots/8MiBoutput/
1MiBdatabuffer/10432pagebytes/27pagevisits/9dataobjects/30sdeadline, perreaderfamily.
Budgetsproposed, notmeasuredRSS; no readerimplementation. Lockedaws-config1.12.0
andcredential-types1.3.0 sources inspected+hashed; docs.rsprimaryconfirmsprofile
filesparsedonce/cached, so testrotation viaexistingRotatingCredentials fixture.
Datedevidence manifest-range-reader-contract-20261018.json. IndependentS3helper
preparation allowedinnewisolatedworktreefromaccepted20706cd; fullmanagedreader
stillfollowscontrolleracceptance. Keeporiginal253sourcehashes frozen for474checker.


## Accepted prerequisite: exact chunk source settlement state (2026-10-18)

Accepted prerequisite: corrected32d CI all3SUCCESS, own strict21/14+raw35 PASS,
22member hosted archive verified; state contract/docs pushed at96b6441. Schema28 source41b's own cohort is now accepted below; no CI wait remains.

Branch codex/chunk-source-settlement from96b6441. Implemented schema28 separate
chunk_source_role/chunk_source_settlements, opaque exact SourceReceipt/SourceStatus,
same-owner source facade, recursive final postwrite proof, source lineage and
mixed-physical-intent refusal. Generic pending/physical flushes unchanged. No
ancestor suppression, source payload reclamation, role release, native activation,
new S3/wire format, detached/removal mode or new controller integration. Historical
metadata-only semantics explicit; same-length damaged payload is not freshly hashed.
Added lineage bounds4096generations/1mextents; not total validator latency bounds.
Journal inherited allocation gap fixed: phase byte shape before String; digest
length+substr32 before Vec, valid digest/NULL-hole hash bytes unchanged.

Authority preflight/init/snapshot allowlists and reciprocal guards cover newtables.
21 schema-rewind fixture files only drop newtables first; two Windows fixture
expectations now28. No legacy serialized vectors/domains intentionally changed.
Resolvedrusqlite0.40.2 transaction/commit/query/Optional APIs andTokio1.53.3join
inspected. Full local gate now PASS; source41b759a pushed, own CI39163298774
completed SUCCESS. Own41b raw/artifact acceptance passed;32d artifacts not reused.

Focused4 session17100 CLOSED EXIT0,31pass (15existing+16new),54.76s; log
/tmp/s3drive-chunk-source-focus4.log. Includes unknown/conflict refusal, first old
settlement after newer root/head+draft, newer settlement leaves ancestor pending,
concurrent first admission/replay/lost reply, actual SQLite-full rollback, all
trigger effects, missing grandchild, 2MiB corrupt cells, orphan exclusions,
migration collision, actual killedowner Role/Row/Committed (unsuccessful exit
asserted),4096lineage boundary. Earlier compile issues corrected; no runtime
assertion failure in focused2(26PASS31.81s) or4. Do not repeat by default.

Existing legacy reviewer reused gpt-6-astra/xhigh, local rubric8(spec1 blast2
depth2 verify1 cost2), persistence/security override. No production false-ack
blocker; transitive metadata bounds fixed; first-settlement/concurrency evidence
refined. Last two fixture suggestions (non-success killed exit and narrow oversize
testname) applied before focused4. No reviewer tests/network/credential calls.

Frozen252 runtime/fixture map /tmp/s3drive-chunk-source-frozen.json,
SHA babdf6487875da1630e35b98fab9d2d0697f640e8d9f8cb15e76189068afea42.
Production sources.rs988c53cce8ba073bc2f61e57856c10920b2bcd33abc3cdc7d09719d07b63acd8;
journal.rs9afe7a1363af8d2e796d679033ed4354af10a0ad4aef07f696dc8c2d1172e5e1;
newtests eb09b12eb14688f147639d72ed28c6ed803a55fb492b206157f0582052677999.
Do not edit runtime during full gate. New chapter/SVG distinguish source/root and
newer pending head. SVG XML/text extents/render and visual review passed after
shortening overflowing labels; librsvg render_cairo deprecation retained. Final
doc links/build/test/HTTP chapter/SVG/archive checks PASS. Durable14member local
archive chunk-source-settlement-local-20261018.tar.gz,90271bytes,SHA
a1193c2f2a2876cf0a78436fccd1a6a236c09426574a0bd241514f02094d7180;
all indexed members verified. Builder /tmp/s3drive-archive-chunk-source-local.py
executed EXIT0; gate-result /tmp/s3drive-chunk-source-gate-result.json.

Fullgate93252 CLOSED EXIT0:365state/1ignored,16new source controls,59S3,34core,
50authority,18existing managedpublication,8chunk,10TLSrestart/3ignored,
66docfiles/62chapters;252frozen hashes unchanged. State562.46s;12long-test
notices eventually passed, retained in resultJSON/log; not a benchmark or
established regression cause. Log /tmp/s3drive-chunk-source-gate.log,
SHA487be3ec765e156c2c7b053d4fe85e02ec50dfa7b604256c370d8f39f5f013b3.
Final reviewer verified both last test refinements/hash, no remaining scoped
blocker and no reviewer execution. Do not repeat the local gate without a new
change/failure. Inspect its own new Linux/Windows/book cohort;32d artifacts
cannot substitute. Controller integration
follows that accepted baseline, with realSDK/authenticatedloopback controls.

Published source41b759aeb09511f2e27956c8c6a0110bfe283dcb on
codex/chunk-source-settlement,39files; clean exactremote/all252frozenhashes/PUBLIC
before-after and main1a93b5f unchanged. Proof /tmp/s3drive-chunk-source-publication.json.
Own CI39163298774 attempt1 all3SUCCESS:Windows117780950165,book117780950315,
Linux117780950360. Watch28386 CLOSED EXIT0. All3ownraw logs reviewed/sha-bound;
combinedmarker /tmp/s3drive-chunk-source-41b-raw-reviewed.json. One-usechecker
/tmp/s3drive-check-chunk-source-41b.py executed71610 EXIT0, strict21/14+project
oracles PASS. OwnfinalizedIDs12149243457/12149814008, API digests/times checked;
all35rawrecords match everyfield/type;252frozenhashes and4unchangedoracles match.
Output /tmp/s3drive-chunk-source-41b-artifacts. No older artifacts reused.
22memberarchive docs/src/evidence/chunk-source-settlement-41b-hosted-20261018.tar.gz
116143bytes SHA7254c8692b7b205b390d4fdd2d35f49f529ee7c5889cc358e2070344a4543214;
all indexedmembersverified, exactsource/logs/checkers/artifacts/map retained.
Archivebuilderowncheckerpath correctedbeforeexecution; not older32d checker.

Linux365state/1ignored/16sourcecontrols, separate9FUSE63.33s, no-systemd desktop/
SSH/terminal-loss and pinnedHTTP/private-CA HTTPS coordinator recovery PASS.
Windows360state/1ignored/16sourcecontrols/8chunk32.49s; separate7native89.61s,
32HTTPSflushes/disposition/pairedindependentlogonstartup/snapshotconsole PASS.
ExpectedHEAD403/contentconflict/stoppedauthorityListtimeout; unusedWinFspdelay
import+9duplicatewarnings,Node/cachedeprecations,3lockeddisposableprofiles with
removeddummycredentials retained. Existingmountedformat, notmountedchunks,
physicalreboot ornewWasabi acceptance. Book66files/62chapters PASS. Repository
PUBLIC; no visibility/credentialmutation. Current controller work is above.

Controller contract recorded at this state checkpoint in implementation-plan.md under
"Next controller slice: explicit source acknowledgment" (2026-10-18 proposal).
Read complete current chunks.rs and existing realSDK/authenticatedauthority
subprocess fixture. AddPublisher/Recovery.resume_source returningopaqueSourceReceipt,
onefamilypermit throughroot+ack via privateunlockedpublisherhelper; preserveold
resume.RootCommit. Knownroot recoveryzeroRPC/S3; preparedunknown onlyexactApply,
thenlocalack; scopefirst,nofreshmetadata/policy/rebase/IDs. Sameprivateownerreloads
proof. Proposedfourcontrols:Unicode/binary/sparse/empty historicalflow; killed
unknownreply withnewerremote/localhead; killedknownroot afterfailedsourceinsert;
expired/narrowpolicy/localreceiptsavefailure/explicitbarrierBusy/concurrentfirstack.
Reuseparentharness+smallchildmodule; actualkilledstatusunsuccessful/reap, countall
S3+uniquePUTs+completeApplys. This checkpoint had no controller implementation; current active section above
records its implementation and tests. Accepted41b prerequisite satisfied. Native
chunklifecycle/verifiedrange reader remainlater. StateFULL/grandchild/postcommit
faults remainstateevidence, not newcontrollerclaims. No newagentsdispatched.

## Preceding checkpoint: TLS fixture accepted (2026-10-18)

Previous goal turn was PROGRESS: publisher runtime cf9d972 and status b70717a
pushed with full local gate. Its own CI39156593000 attempt1 failed Linux job
117760980367 at authority_restart.rs719 before reaching new chunk tests:
8 pass/1 fail/3 ignored; exactly17 diagnostics, first16 not all the fixed
HandshakeRejected line. Actual categories were not printed and remain UNKNOWN.
Book117760980164 and Windows117760980265 SUCCESS; overall cohort FAILURE.
Watch5878 CLOSED EXIT1. All3raw logs reviewed;10member failure-only archive
chunk-root-publisher-cf9-failed-20261018.tar.gz,90889bytes,SHA
1ce2df69defa2f42d2aef2d896088e150d8259d9cb629ee8d663d4756a9f62fa;
all indexed members verified, original cf9 test/frozen249map bound throughgitshow.
Windows8chunk controls PASS66.40s;344state/1ignored,59S3,34core,50authority,
18publication; existing7WinFsp/32HTTPS/paired/disposition/snapshot controls PASS.
Warnings retained: unusedWinFspdelayimport+10duplicates,Node/cachedeprecations,
3lockeddisposableprofiles(dummycredsremoved),expectedHEAD403/contentconflict/
stoppedauthorityListtimeout. No cf9artifact payload download/independentstrict
check/successchecker. Fresh cf9successoutput/marker remain absent. Never rerun
or certify that failed cohort using older artifacts. Raw API metadata retains
cf9finalizedIDs12147331157/12147209844 only, without payload acceptance.

Branch codex/tls-budget-fixture fromb70717a. Published tested source
32d7cc7700797c0471e51c69904873d23b9c3889. Sole test-file delta is
crates/s3drive-cli/tests/authority_restart.rs; all production, schema27, wire,
policy, native callbacks and chunk-library code remain byte-identical tocf9.
The old fixture shared a listener budget with four completed TLS RPCs and used
socket EOF as a log-completion barrier. Source proves both can confound strict
budget accounting; neither is established as the captured failure's cause.
32 instrumented local reproductions passed without a failure; not causal proof.

Repair finishes restart/replay checks, then starts a fresh listener on the same
pinned database/volume with stdout-only readiness. First17 malformed peers each
await the exact expected stderr line before the next peer;24 total remain.
Only after observed suppression does authenticated Info check volume and seq2.
Exactly16 rejections+one suppression and both logs' private-input checks remain.
Collector uses bounded channel32 and64KiB raw/retained capture; exhausted raw
allowance fails closed even when CRLF stripping shortens retained text. A
negative control failed before that guard (14171 EXIT101) and passes now.
Assertion output uses only fixed categories/static errors, never arbitrary lines.

model-select: existing legacy reviewer -> gpt-6-astra/xhigh (score7: spec1,
blast1,depth2,verify1,cost2; override security verification; assessment local).
No new agents spawned. Final static review found no blockers and matched all249
frozen hashes; onlyauthority_restart differs from the prior publisher map.
Test SHA60ef013f0aa501849fdf89f50302015b774178252ad4e287557af720741d7549.
Focused97165 CLOSED0:10pass/3ignored,10.81s. Fullgate68896 CLOSED EXIT0:
349state/1ignored,59S3,34core,50authority,18existing publication,8chunk and10TLS
controls,65docfiles/61chapters;state195.62s. All249 frozen hashes unchanged at
/tmp/s3drive-tls-budget-frozen.json, SHA169ddf6e8fc9dc41516dc849b36e3016a086d890c79efb0f0d9af9330499cd0c.
Log /tmp/s3drive-tls-budget-gate.log. Durable11member local archive
 tls-budget-fixture-local-20261018.tar.gz,47322bytes,SHA
98608b40264e05a6b0cc37eee8d10c37a24b2d8391cfed9c516730e67ed8c0b0;
all indexed members verified. No reviewer execution/provider calls.

Publication clean/exact remote/all249hashes/PUBLIC before-after, main1a93b5f
unchanged; proof /tmp/s3drive-tls-budget-publication.json. Own corrected CI
39158147595 attempt1 COMPLETE SUCCESS, exact32d7cc7 source: Windows117765563031,
book117765563156,Linux117765562837. Watch39440 CLOSED EXIT0; do not rerun.
All3raw logs reviewed and hashed in
/tmp/s3drive-tls-budget-32d-raw-reviewed.json. Book:Ubuntu24.04.5,65docfiles/
61chapters,pinnedRust override/defaultbranch/Node20forced24 notices retained.
Checker /tmp/s3drive-check-tls-budget-32d.py executed ONCE,74397 CLOSED EXIT0.
Own finalized artifact IDs replacement12147997314/disposition12147329360;
API digests/source/timestamps verified, strict21/14+project oracles PASS.
/tmp/s3drive-tls-budget-32d-raw-compare.py EXIT0:all35 raw record fields/types
match downloadedJSON; no borrowed payloads. Output
/tmp/s3drive-tls-budget-32d-artifacts. Previous cf9 IDs remain rejected.
Archive builder91040 CLOSED EXIT0; durable22member
 tls-budget-fixture-32d-hosted-20261018.tar.gz,125072bytes,SHA
 af77e96c872616afc688fca14742035054b45035c7e6ac35680ef809245a6097.
All indexed members verified, plus all249source hashes throughgitshow32d.

Chapter owned-authority-client.md and both dated fixture/publisher JSON records
now distinguish accepted32d from failedcf9. This is a docs-only status child of
32d7cc7; runtime/native/schema hashes unchanged. First revalidate git status.
Corrected baseline is accepted; next implement the reviewed source-settlement
persistence slice, then its controller integration. No historical Windows
NoWrite fix, logical flush, mounted chunks, real-provider tree or physical
startup/reboot acceptance is claimed. Full goal ACTIVE and unbounded; PUBLIC.

Read-only source-settlement design review is complete against32d7cc7. The
implementation-plan now contains a concrete reviewed PROPOSAL, not runtime
acceptance: exact-snapshot settlement only, explicit publication/status and
newer-head relationship, separate schema28 rows/domain/retained integrity,
same-owner final post-write recursive proof, generic pending unchanged. Choose
historical metadata acknowledgment (no fresh payload/provider-health claim),
retaining all payloads. Do not infer ancestor coverage, clean-file state or
reclamation permission; these remain native lifecycle work. Reject mixed generic
physical intent on the exact source snapshot. Existing legacy reviewer performed
static review only; no tests/network/provider calls. No runtime files changed.
The next implementation phase is the separate state persistence slice; the
corrected-source hosted prerequisite now passed. A separately gated controller
slice follows. Contract includes SQLite-full, process-death, lost-reply, post-write trigger,
recursive damage, newer-head/root and legacy-vector controls.

Corrected Linux117765562837 terminalSUCCESS/raw-reviewed:8chunk controls
PASS24.51s,349state/1ignored,59S3,34core,50authority,18existing publication,
9realFUSE/32HTTPSflushes and paired no-systemd drive/coordinator startup.
Windows117765563031 terminalSUCCESS/raw-reviewed:8chunk controls85.06s,
344state/1ignored262.24s,7existingWinFsp99.24s,32HTTPSflushes,complete disposition,
paired non-admin coordinator/drive logons/recovery and snapshot cancellation.
Fixed pinned CLI/CRLF guard passed both platforms;11Unicode names+directory on
Windows. Expected faults:TLSHandshakeRejected/partial-headerHttpTimeout/Lifetime,
HEAD403,dirtycontentconflict,stoppedauthorityListConnecttimeout. Warnings retained:
unusedWinFspdelayimport LNK4199+10CLItest warnings(9duplicates),Node/cache notices,
3lockeddisposableprofiles(dummycredentialsremoved,VMdestructioncleansprofile).
Sparse chunk and two old quota controls exceeded60s then passed. Own raw markers
/tmp/s3drive-tls-budget-32d-{linux,windows}-reviewed.json. No new provider calls.

## Preceding policy-bound chunk-root publisher (2026-10-18)

Branch codex/chunk-root-publisher from00a8829. Implemented separate
s3drive-managed::chunks Publisher, S3-independent Recovery, opaque historical
RootCommit and separate CLI integration binary. Same private owner; exact SDK
route + separate source/config binding; authenticated Features/Info/Node/Store
for new work. Persist full reserve before Apply; policy-bound child-first uploads;
fresh tree proof then persist full Publish before send. Saved Prepared bypasses
all S3 binding/policy/metadata refresh. Recovery needs Storage/WireClient/expected
volume+store only, and never invents IDs/reserves/uploads/reads source bytes.
RootCommit awaits durable receipt; original source stays pending. Old publisher,
reader, schema27, wire/hashes/native callbacks unchanged. No provider mutations.

Focused latest8tests PASS65.90s /tmp/s3drive-chunk-publisher-focus4.log. Controls
retain unknown PUT across controller reconstruction, lost reserve/root replies,
real child kill then recovery without S3 credentials/policy and with damaged
source bytes, newer remote root/dirty local head, expired/narrowed policies,
receipt-save failure after Applied, synchronized same-owner same-plan recovery,
independent writer conflict, wrong bindings/corrupt admission/ignored SQL, missing
grandchild proof, empty/sparse/Unicode and EXACT-policy names/rename. All expected
PUT targets unique. Initial fixture/append_at errors fixed without weakening.
Clippy first found redundantunit(); removed and fmt awaited. Clippy2 EXIT0.

Required pre-implementation and final delta review reused existing legacy reviewer
gpt-6-astra/xhigh, local rubric8/spec1 blast2 depth2 verify1 cost2 with persistence/
security override. No new agents spawned. No blockers. Latest production SHA
9f4abdc9d1ff185e8b9d51b561e7744f3eec4b9718b50dd745573533e10fccd2;
test SHA6038916bcacd63f9505b4286337a2f0a6502d6c82a2cae58a3e835d2b6e9d7bf.
Review verified preceding c7ec5c production differs solely by redundantunit removal.
No reviewer tests/provider calls. Fullgate50472 CLOSED EXIT0 with249runtime/fixture files
frozen /tmp/s3drive-chunk-publisher-frozen.json. Log
/tmp/s3drive-chunk-publisher-gate.log. All249hashes match;349state/1ignored,59S3,34core,50authority,18existing
publication+8newcontrols,65Markdownfiles/61chapters; state202.17s.
New chapter/image/evidence chunk-root-publisher.md and matching SVG/dated JSON.
Durable13member localarchive chunk-root-publisher-local-20261018.tar.gz,
46976bytes,SHA5d9638bf5e489aced912113346aba42a9d92dd6914a6a25d1a3817bc1b98d591;
allindexedmembersverified. SVG XML/librsvg/visual and HTTP33767 passed. Final
review matched249/249; corrected sparsefixture description to128MiB+64bytes.
Published tested runtime cf9d972173b0b0709c08f3c65ec102fa44fa5a6e, clean/exact
remote tip/all249hashes/PUBLICbefore-after; no visibilitymutations/mainunchanged.
Proof /tmp/s3drive-chunk-publisher-publication.json. OwnCI39156593000 attempt1
Linux117760980367 FAILED before chunk tests;book117760980164 and
Windows117760980265 SUCCESS. This cohort is NOTaccepted. Watch5878 CLOSED1,
/tmp/s3drive-chunk-publisher-cf9-watch.log. Resume the same handle/run; do not
rerun on observation timeout. Checker /tmp/s3drive-check-chunk-publisher-cf9.py
and rawcompare /tmp/s3drive-chunk-publisher-cf9-raw-compare.py prepared/compiled
only, NOTexecuted. Fresh output /tmp/s3drive-chunk-publisher-cf9-artifacts absent;
Linuxfailed, so never execute its successchecker or make a successreview marker.
The original checker required all3 terminal success and own rawreview marker
/tmp/s3drive-chunk-publisher-cf9-raw-reviewed.json before checker. Bind source/run/
attempt/jobs/finalizedartifactIDs/APIdigest/time; stricttyped21/14+projectoracles,
then all35rawfields/types. RejectpriorIDs including parent12146631312/12146506974.
Download each own completed job's rawlog, review controls/warnings/expectedfaults
before making marker. On failure do not run successchecker or borrow oldcohort.
Failure-only archive is now closed/indexed above; cf9checker remains unexecuted.
Its all-success prerequisites cannot be met by this failedattempt. Corrected32d
has its own independent cohort and source binding. Main remains1a93b5f; no native
chunk activation follows. Logical
settlement, range reader, native chunk activation, provider/physical startup stay
unfinished. Goal ACTIVE; repository must stay PUBLIC.

## Preceding isolated chunk-root journal (2026-10-18)

Branch codex/chunk-root-publication fromceef8e1. Published runtime
63164d012bc150fd0f5258afd5466e9efb41bd2e. Implemented schema27 isolated
client journal with exact full reserve/publish requests, complete historical
receipts, retained Conflict, permanent role/count, reciprocal legacy lifecycle
exclusion, cross-column operation ownership and bounded restart enumeration.
New chunk_publication.rs uses freshly reloaded post-write plan/source/graph,
exact saved row, role/isolation and stage-dependent complete tree proof. Its
CheckedFlushes lives only in the final immutable validation section. Separate
role/log tables preserve all legacy JSON/hashes; downgrade fixtures drop them,
both Windows native schema fixtures expect27. No detached/removal mode or role
release; generic staging/remote/chunk APIs remain usable. Authority restart and
snapshot preflight reject populated new tables, including old-schema paths.

15focused state tests PASS; full gate42218 CLOSED EXIT0:
349state/1ignored,59S3,34core,50authority,18managed-publication; format/Clippy/build,
64doclinks and60book chapters. All247 frozen hashes match
/tmp/s3drive-chunk-publication-frozen.json. Log
/tmp/s3drive-chunk-publication-gate.log. State tests229.12s; existing retirement/
obsolete-full-open/migration-spawn tests exceeded60s then passed. No cause claim.
Independent final review: no blockers,247/247 hashes match; all37changed runtime/
fixture files covered. No tests/provider calls by reviewer. Durable9member local
archive docs/src/evidence/chunk-root-journal-local-20261018.tar.gz,
SHA25656e38f7a54e2001b55c55a058171cf3e7f238fb862c50f0e91c5c245388af8c0
(32185bytes), all indexed members verified. Evidence JSON and reviewed SVG in
new chunk-root-journal.md; XML/librsvg/visual and HTTP preview33767 passed.

Review corrected separate route/configuration fingerprints and general exact-
name validation; fault controls now insert valid two-column identity rows.
Initial fixture/FromSql/Clippy failures fixed without suppressions. Controls
cover lost replies/restart, newer roots/dirty heads, manifest bases, exact names,
Unicode, content races, missing recursive receipts, post-write trigger damage,
actual SQLITE_FULL and schema26 collision rollback. Historical prepared recovery
works after current payload corruption while source remains pending.

Publication clean/exact remote/all247hashes/PUBLIC before-after, main1a93b5f
unchanged; proof /tmp/s3drive-chunk-publication-publication.json. Own CI39153910031 attempt1 at63164d0 all3 SUCCESS; watch20504 CLOSED0.
Windows117753033763,book117753033479,Linux117753033780 raw logs reviewed;
checker33526 CLOSED0, fresh own artifacts12146631312/12146506974 stricttyped21/14
+projectoracles +all35rawfields/typesPASS. Windows344state/1ignored,Ubuntu349/1;
all15journalcontrols PASS both. Existing9FUSE/7WinFsp/32HTTPS/paired/nonsystemd/
cross-logon/snapshotcontrolsPASS; WindowsServer2025/WinFsp2.1.25156,Ubuntu24.04.5.
No physicalWin10/11/reboot/provider/chunkedmount or historicalNoWritecause claim.
Warnings retained: unusedWinFspdelayimport+9duplicates,Node/cachedeprecations,
three disposablelockedprofileswithdummycredentialsremoved,recoveredbooknetwork
retries,expectedTLS/HEAD403/stoppedauthorityerrors. Durable20memberarchive
chunk-root-journal-631-hosted-20261018.tar.gz,100013bytes,SHA
b43ac8f51c97aa58f7d5dba25307963200def440c6485ab8c1fa98d3697bfeea;
allindexedmembersverified. Exactsourcebound parent evidence; cannot substitute
for current publisher cohort. Logical settlement, verified range reader, native
chunk activation and approved provider/physical acceptance remain. Goal ACTIVE.

## Preceding complete historical chunk-tree receipt (2026-10-18)

Branch codex/chunk-tree-receipts from259e0f8 (status child of91db06b).
Published runtime3e55fe78f6112d817a2ea131bcb4602aa7212410.
Implemented opaque historical TreeReceipt, one synchronous metadata-only owner
command validating every non-hole exact receipt, Planner wrapper and S3 uploader
finalization through full-context comparison. Five new state tests (10including
existing transfer tests) PASS; eight SDK tests exploratory PASS. Full gate74791 CLOSED EXIT0:334state/1ignored,59S3,34core,50authority,18managed
publication; format/Clippy/build,63doclinks and59book chapters PASS. Log
/tmp/s3drive-tree-receipts-gate.log; all244runtime/fixture hashes unchanged at
/tmp/s3drive-tree-receipts-frozen.json. Final delta review: no blockers; reviewer independently
matched all244frozen hashes. No schema, wire, metadata RPC, logical settlement or mounted
activation change. Historical planned holes are not current-media claims; the
uploader keeps its fresh per-object hole checks. Report may survive current
payload corruption but must never acknowledge source or settle a file.

Decisive control deletes both grandchild flush and settlement; remaining forest
and root direct dependencies still pass, new report fails and exact local re-ack
restores it without PUT. Also no writes, actual SQLITE_FULL read-only success,
missing/corrupt/redirected records, reopen, source manifest and newer dirty head
retention. Initial head() fixture type mistake and unused mut fixed; no assertion
or stack-size changes. Schema stays26. New docs/src/chunk-tree-receipts.md and
image XML/librsvg/visual checks PASS; full book/links passed. Durable6member
local log/hash archive docs/src/evidence/chunk-tree-receipts-local-20261018.tar.gz
SHA256cfde2422ab636fefc6efa777fe4a19d674cb841e0b64cf996f885c3bf50e3ef2
(30938bytes), indexed members all verified. Normal publication clean/exact remote,
all244frozen hashes/PUBLIC before-after, main1a93b5f unchanged; proof
/tmp/s3drive-tree-receipts-publication.json. Preview33767/chunk-tree-receipts.html
and exactSVG HTTP, XML/librsvg/visual review all passed.

Own CI39150996811 attempt1 at3e55fe7 CLOSED SUCCESS all3: Windows117744444907,
book117744444680, Linux117744444873. Watch79516 and checker38202 CLOSED EXIT0.
All3 raw logs reviewed; own fresh replacement12145416969/disposition12145194101
artifacts pass strict typed21/14 and project oracles; all35 raw records match
all fields/types with duplicate/nonfinite JSON refusal. Durable indexed21member
archive docs/src/evidence/chunk-tree-receipts-3e5-hosted-20261018.tar.gz
SHA2563e1627fa1629bf04d00750901d71024eb12efe164017ad416a8398ae74de8566 (96029bytes), all members verified.
Hosted Ubuntu24.04.5/WindowsServer2025/WinFsp2.1.25156. Existing9FUSE/7WinFsp,
32HTTPS, paired/snapshot/cross-logon/no-systemd regressions passed. Windows329state/
1ignored; all5new receipt controls PASS. Node/cache/unused delay-import notices,
3locked disposable profiles(dummy credentials removed) and passing long Windows
supervision/snapshot/retirement tests retained. This does not accept journal
source, chunked native drives, provider behavior or physical reboot; historical
NoWrite cause remains unresolved.

Required legacy review agreed metadata-only proof scope and future journal
barriers. model-select: reused gpt-6-astra/xhigh (local8:spec1 blast2 depth2
verify1 cost2; persistence/security override), no additional agents spawned.
Read final implementation-plan prerequisites/constraints before next work.
Preceding91d cohort is now closed SUCCESS with own fresh strict artifacts;
own3e5 cohort also closed. Next work is the client root publication
journal with persistent lifecycle exclusion, then logical settlement/range reader.
No real provider mutations are authorized by these local/hosted phases.

## Preceding manifest root authority (2026-10-18)

Branch codex/manifest-root-authority from2097d44; published runtime source
91db06bb4c98ecc9e251817444672e1ab44764ac. Schema26 is a
comments-only persistent JSON-format marker, no data/hash rewriting. Optional
strict ManifestRoot is omitted from old reservations, keeping independent frozen
legacy bytes/hash vectors. ReserveManifest binds root identity/store/key/geometry
and mandatory original ContentGuard; existing Publish CAS handles conflicts,
rename, detached nodes and pending-removal fences. New references have a distinct
hash domain; old inputs/bindings/readers explicitly refuse manifests. No mounted
activation, provider calls, credential changes or visibility mutations.

Full local gate2 session9009 CLOSED EXIT0:329state/1ignored,59S3,34core,
50authority,18managed-publication; format/Clippy/build,62doclinks and58book
chapters PASS. All243 frozen runtime/fixture hashes unchanged. Log
/tmp/s3drive-manifest-authority-gate2.log. Evidence JSON and durable16member
local log/hash archive are in docs/src/evidence/manifest-root-authority-*.
Independent7fixture regeneration matches. XML/librsvg/visual review and both
HTTP preview requests passed at33767/manifest-root-authority.html.

Required legacy review found two receipt gaps; both reproduced/fixed. Fresh
commit verifies exact saved effects; historical replay binds immutable request/
reservation without comparing to a newer node. Seven SQL fault injections,
four historical publication corruptions and three reservation corruptions covered.
Initial full gate10914 EXIT101: owner_move_start child stack overflow (17PASS/
1FAIL). Metadata/new-branch boxing alone did not fix it. Split the four original
owner-move scenarios into boxed futures, preserving all statements/checkpoints;
local debug frame audit:841992bytes before,63304dispatcher+359592max scenario
after. No stack-size or assertion changes. All18contracts pass in the full gate.
Clippy constant-chunk/collapsible-match/large-enum findings fixed without
suppressions; only the new mutation boxes metadata, JSON unchanged.

Published normally: clean/exact remote/all243frozen hashes/PUBLIC before-after;
main1a93b5f unchanged. Proof /tmp/s3drive-manifest-authority-publication.json.
Own CI39149314656 attempt1 at91db06b CLOSED SUCCESS all3: Windows117739515199,
book117739515201, Linux117739515267. Watch21911 CLOSED EXIT0; checker33071 CLOSED
EXIT0. Own fresh replacement12144981486 and disposition12144507996 artifacts
pass strict typed21/14 plus project oracles; all35raw records exactly match every
field/type, duplicate/nonfinite JSON rejected. Proof dir
/tmp/s3drive-manifest-authority-91d-artifacts and durable20member archive
 docs/src/evidence/manifest-authority-91d-hosted-20261018.tar.gz
SHA2567164f27c6a0035cd7ed787532e10c9f3acf41424c4316568803422385a741db1
(95143bytes), all indexed members verified. Root evidence JSON records ZIP/JSON
hashes/API IDs/source. Raw logs reviewed for all3; old representation native9FUSE/
7WinFsp,32HTTPS,paired/snapshot/cross-logon/no-systemd passed; root metadata TCP
and14state contracts passed both platforms (329Linux/324Windows state totals).
Hosted Ubuntu24.04.5 / Windows Server2025 / WinFsp2.1.25156 only. Retain Node/cache/
unused delay-import notices and3locked disposable profiles (dummy creds removed,
VM disposal). TLS handshake/partial29byte timeout/lifetime, HEAD403/rival and
stopped-authority List connectTimedOut are expected controls. Long stress and
supervision/native cases passed. No new chunked mount/provider/physical startup
acceptance, current3e5 source acceptance or historical NoWrite-cause resolution.

Next phase requires an exact durable chunk-publication/settlement journal binding
original snapshot/guard/parent/frozen plan/route, reserve+publish UUIDs, physical
root receipt and immutable authority request/result. Do not settle a logical edit
using only root physical generation acknowledgment. Preserve newer dirty heads,
old references and unknown identities. Then verified range reader/native paths.

## Preceding chunk object upload bridge (2026-10-18)

Published branch codex/chunk-object-uploads frome2f0709. Runtime source
2524573c2bd476e609daae529e6c3d1fcb32cb80. State Transfer/materialize
bind exact physical operations via existing staged_operations; no schema change.
Planner inherits Storage's exact staging quotas. New policy-bound
s3drive-s3::uploads::chunks::Uploader uses that same owner, checks binding/policy,
sends data then pages, and requires exact durable direct-child receipts. Unknown
attempts only observe, including after expiry/narrowed policy; collisions retained.
Page Merkle vs raw-body checksums remain distinct. UploadedTree is not a logical
file flush: original snapshot stays pending; authority root CAS is future work.

Focused8 SDK/TCP tests PASS /tmp/s3drive-chunk-upload-s3-2.log, including >128MiB
sparse multi-level tree with exactly4object PUTs, Unicode/binary/empty/hole,
missing local acknowledgment, lost response/restart, claimed recovery after
expiry/narrowed policy, rejected rival and uncertain-absent child. State5 PASS
/tmp/s3drive-chunk-upload-state-final.log: begin/append/seal reopen, exact mappings,
corrupt media/dependencies, quotas and actual SQLITE_FULL. Initial state run had
2fixture failures (UNIQUE/FK prevented deliberate damage); corrected to redirect
to unmapped foreign generation / explicitly disable FKs for the damage fixture.
No assertions weakened. Initial compile/API test errors corrected using inspected
Target/Reference getters and UTF8 strings; full local gate2990 closed EXIT0. Log /tmp/s3drive-chunk-upload-gate.log;
315state/1ignored,59S3,34core,50authority, format/Clippy/build,61doclinks and
57book chapters PASS. All230 frozen runtime/fixture hashes unchanged; no warnings
or errors. Existing full-open quota stress test exceeded60s then passed. Do not
restart the completed gate. Evidence docs/src/evidence/chunk-object-uploads-20261018.json.

New docs/src/chunk-object-uploads.md and images/chunk-object-uploads.svg: raster
and visual review passed. Sequential uploads; repeated graph/retained-history
scans can amplify work; no scalable-throughput/native-limit claim. No S3 provider
requests, authority wire changes, credential installs or repository visibility
mutations. Existing mounted representation and main remain unchanged.

Published normally: clean/exact remote/all230 frozen hashes/PUBLIC before-after;
main1a93b5f unchanged. Proof /tmp/s3drive-chunk-upload-publication.json. Final
book/link checks and preview33767/chunk-object-uploads.html passed. Own CI
39145482478 attempt1 at2524573 completed SUCCESS for all3 jobs: Windows117728329566,
book117728329702, Linux117728329705. Watch59922 CLOSED EXIT0. All3 raw logs reviewed;
checker36340 CLOSED EXIT0. Own fresh replacement12143780290 and disposition12143656516
artifacts passed strict typed21/14/project oracles; all35records match raw logs
exactly including field types. Proofs /tmp/s3drive-chunk-upload-252-artifacts/.
Durable20member archive docs/src/evidence/chunk-upload-252-hosted-20261018.tar.gz
SHA256f76be66c3d4b94d38c110836b467747ac449a62df4caf710f5430df550a5f3d6
(93775bytes), every indexed member verified.
Windows is hosted Server2025/WinFsp2.1.25156. Expected negative controls and
Node/cache/unused delay-import notices retained; three disposable dummy-credential
profiles left to VM disposal. No historical NoWrite cause, physical workstation,
provider, chunked native large-file or newer schema26 acceptance inferred.
Next phase: authenticate/reserve chunk root identities and original content CAS,
publish UploadedTree through authority and settle the exact logical edit; then
verified range reads/native integration and authenticated remote-reference reuse.
Read namespace/publication.rs (Reservation/Payload currently one-object <=8MiB),
managed.rs and managed/lib.rs. Preserve legacy serialized bytes/content hashes,
unknown operations and rollback; use a separately versioned format/capability.

## Preceding implementation: durable offline chunk plans (2026-10-18)

Branch codex/chunk-plan-journal from fa0a91d; published runtime source
bdb220c69b187d3b3d764f4b3832d95f01d11e07.
Schema25 adds local chunk_plans/chunk_objects, frozen input before data hashing,
exact staging-operation attachment, durable cursor/hash/page preparation and an
offline Ready root. New worker handle uses the existing private owner. No upload
permit, live root capability, file-content conversion or mounted-limit increase.
Initial full local snapshot preparation; remote unchanged-object reuse still
needs authenticated base-root ancestry/retention. Original guards, parent/route
and edit/object IDs stay bound. Keep dirty and unknown results. Ordinals are8byte
BLOBs; checked i64 conversions handle SQLite numbers (fallible_uint is disabled).
Resolved rusqlite0.40.2 signatures/features inspected; no dependency changes.

Phase contract/done criteria in docs/src/implementation-plan.md. New chapter
chunk-plans.md and SVG diagram explain workflow and limits. Independent Python
fixtures reproduce all3 pages plus input hash for a sparse >128MiB file.
Focused final18PASS/293filtered EXIT0:
/tmp/s3drive-chunk-plan-focused-final.log. Covers child edits/retained old roots,
missing dependencies, actual SQLITE_FULL, process death after a committed step,
malformed cells, ignored/deleted SQL effects, authority roles and migration.
Ready is expected metadata, never current-media freshness or a flush receipt.

Gate1 session22553 closed EXIT101:308state PASS/2FAIL/1ignored. Exact failures:
old22 authority fixture left new tables present; old23 migration test expected24.
Fixed both simulated22 setups to drop new client tables and the migration target
assertion to exact crate::SCHEMA_VERSION. All3 targeted identity/migration checks
pass; original byte/hash/refusal assertions remain. No source edits during gate1.
Old55 hashes preserved /tmp/s3drive-chunk-plan-frozen-gate1.json.
Gate2 session42132 closed EXIT0 on unchanged frozen56 source/fixture hashes:
/tmp/s3drive-chunk-plan-frozen-gate2.json, /tmp/s3drive-chunk-plan-gate2.log.
310state PASS/1ignored,34core,50authority,51S3,format/Clippy/build,60doclinks and
56book chapters passed; no warnings/errors. Do not restart completed gate handles.
Both corrected migration/identity fixtures pass within the full suite. Evidence docs/src/evidence/chunk-plans-20261018.json.

Initial tests caught a real post-UPDATE object deletion hole. Final written-row
and expected-count checks now force rollback. Parser/API/test-fixture errors
were corrected (reserved gen, disabled u64 SQL traits, append_at, deliberate FK
corruption and an external crash marker). Clippy2 passed before final tests;
full gate validates final Clippy. Failed evidence remains recorded separately.
Diagram XML/raster reviewed. No provider requests, installs or visibility changes.

Source audit: journal::read_data verifies whole stored chunks per intersecting
extent. A logical1MiB slice does NOT cap total physical I/O/checksum work on a
fragmented snapshot. Metadata and final graph validation also scan admitted
records. Limits are documented; no constant-latency or native large-file claim.
Published normally, clean/exact remote/all56 frozen hashes/PUBLIC before-after,
main1a93b5f unchanged. Proof /tmp/s3drive-chunk-plan-publication.json. Final status
doc links/build/56chapter checks passed; preview33767 serves new chapter/image.
Own CI39142749429 attempt1 at bdb220c is ALL3 terminal SUCCESS:
book117720340366, Linux117720340601, Windows117720340529. Watch33001 closed
EXIT0. Own completed raw reviews closed in /tmp/s3drive-chunk-plan-bdb-raw-reviewed.json.
Checker24959 closed EXIT0, /tmp/s3drive-chunk-plan-bdb-artifact-check.log; fresh
replacement12142621547 and disposition12142503573 from own finalized uploads,
API source/run/digests/times, strict21/14 all-field/type and both project oracles
PASS. All35 raw trace records equal own fresh artifact fields/types. New output
/tmp/s3drive-chunk-plan-bdb-artifacts is final: do not rerun into it or borrow it
for later sources. WindowsServer2025 10.0.26100/WinFsp2.1.25156;305state/1ignored,
34core/50authority/51S3,32HTTPS/disposition/paired/snapshot/cross-logon/7native.
Linux310state/1ignored plus9FUSE/32HTTPS/paired/no-systemd startup. Reviewed notices:
LNK4199 unused imports on9CLI test targets, action/cache Node deprecations/forced24,
3disposable locked profiles with dummy credentials removed for VM disposal.
Expected injected TLS/partial-header/HEAD403/rival/stopped-authority failures were
reviewed against fixture requirements. No new provider or chunked native large-file
acceptance, no exact-source repeat or main promotion. Historical failures remain.
Next integration needs physical publication/unknown recovery (in progress above),
authority root capability/CAS, verified range readers, native adapters, remote
reference reuse and separately authorized provider acceptance.

## Preceding object identity checkpoint (2026-10-18)

Branch codex/manifest-object-identity from ec43132. Pure core module implemented:
role-separated v2 object keys and120byte root descriptor, independent vectors,
context/store/length/version/encoding refusal tests. Exact phase contract and
done criteria in implementation-plan.md. No SQLite/schema/wire/network changes,
no implicit object adoption, no mount limit increase or provider request.
Old source e48 CI and its own strict21/14 artifacts are now closed PASS, below.
That earlier-source acceptance cannot certify this new phase.
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
Own CI39138395876 attempt1, source52cd213 is terminal SUCCESS at all3 jobs:
book117707557507, Linux117707557713, Windows117707557677. Watch65541 closed EXIT0.
Own completed raw reviews closed; marker
/tmp/s3drive-manifest-objects-52-raw-reviewed.json. Fresh replacement12141120690
and disposition12141530185 selected from own Windows uploads; source/run/API
ZIP hashes, strict21/14 typed fields and both project oracles passed. Checker
/tmp/s3drive-check-manifest-objects-52.py closed EXIT0 session95911, log
/tmp/s3drive-manifest-objects-52-artifact-check.log. Directory
/tmp/s3drive-manifest-objects-52-artifacts is final; do not rerun one-use checker
or borrow its artifacts for schema25. Linux34core/17manifest,292state/9FUSE,
32HTTPSflushes/11Unicode and both no-systemd coordinator protocols passed.
Windows34core/17manifest,287state/1ignored,32HTTPSflushes,lifecycle,paired/snapshots,
7broad native,11Unicode/cross-logon startup passed on Server2025/WinFsp2.1.25156.
Reviewed expected TLS faults, HEAD403, rival conflict and stopped-authority List
timeout; LNK4199 on9 CLI targets, Node deprecations/forced24, three disposable
locked profiles after dummy credentials removed and long ultimately-passing
state test. Both cache saves normal. No schema25 builder, native large files,
physical MX reboot, Win10/11 or Wasabi acceptance; no historical cause fixed,
no identical52 repeat, no main promotion. Prior failures retained.

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
Own CI39137161051 attempt1 at e48 is terminal SUCCESS: book117703908126,
Linux117703908277, Windows117703908294. Watch42188 closed EXIT0. All completed
raw reviews closed under /tmp/s3drive-managed-manifest-e48-raw-reviewed.json.
Own checker /tmp/s3drive-check-managed-manifest-e48.py closed EXIT0 session56389,
/tmp/s3drive-managed-manifest-e48-artifact-check.log. Own replacement12139939008
and disposition12140403755 were selected from the Windows finalized uploads;
API source/run/digests, strict21/14 all-fields/types and both project PowerShell
oracles passed. Directory /tmp/s3drive-managed-manifest-e48-artifacts is final;
do not rerun the one-use checker into it or borrow its artifacts for52.
Linux27core/10manifest,292state,9FUSE,32HTTPS flushes,11Unicode names and both
no-systemd coordinator protocols passed. Windows27core/10manifest,287state,
32HTTPS flushes,lifecycle,paired recovery/snapshots,7broad native,Unicode and
cross-logon startup passed on Server2025/WinFsp2.1.25156. Own Windows raw SHA
a688a7523da4f35a9e1370b2d36a59b3c55398329ad4ddafa7b0848a0fd32a34.
Expected TLS faults, HEAD403, rival conflict and stopped-authority List timeout
were reviewed (fixture intentionally refuses new opens while retaining pinned
snapshot reads). LNK4199 on9 CLI targets, Node action deprecations/forced24,
three locked disposable profiles (dummy credentials removed) and long state
tests were reviewed. Cache saves completed normally. No Win10/11, physical
MX reboot, Wasabi, new52 module or native large-file acceptance follows. One
cohort, no e48 exact-source repeat or historical transport cause established.
No main promotion. All earlier failures remain recorded.
The object identity phase is implemented above. Next implement an offline builder/
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

## Independent helper checkout running (2026-10-18)

Related-read prep is now implemented only in
/home/marshall/src/worktrees/s3drive-pinned-related-reads, branch
codex/pinned-related-reads fromaccepted20706cd. Productionreader.rs adds8lines
ReadHandle.open_related (samecapturedprovider,freshHEAD,boxedfuture); testsadd
3controls: chainedroot/child/grandchildcredentialgeneration vs newgeneration,
exactscope/GETpreconditions, freshHEADdenial/missing/invalidkey, inlinefuturebound.
Focused1 95205 EXIT101:2pass/1fixturetargetassertfail (SDKx-id=GetObject query);
fixedexactparsedpath, productionunchanged. Focus2 64173 EXIT0,62S3pass21.04s.
Fullgate57146 CLOSED EXIT0, /tmp/s3drive-pinned-related-gate.log. Its own252filemap
/tmp/s3drive-pinned-related-frozen.json (twoS3deltasvsacceptedbase). Root253
controllerhashesstillunchanged. Newworktree source/docs published atfef944268c89d2c0b1d859a6a86d0f6761c24361; readitsown
HANDOFF andpinned-related-reads.md beforecontinuing. No newmanagedreader/native
activation. Fullgatesourcefrozen. Helperownhostedacceptancepending; fullgate/localarchivepassed.
LocalbuildusesCARGO_TARGET_DIR=/home/marshall/src/s3drive/target to reuseexisting
buildcache; do notstartanotherlocalS3Drivecargo job while57146is live. Controller
CI runsremotely andisindependent. OwnhelperCI39167583915 attempt1 LIVE:book117793654818,Linux117793655088,
Windows117793655137. Watch43951 /tmp/s3drive-pinned-related-fef-watch.log. Its
252map/ROOT areisolatedworktree, notprimary253. 474controllerisaccepted above.
Next accepthelperowncohort thenmergecodewithcontroller andimplementfullreader;
fullcontract/budgets alreadydocumented. Primaryruntime253stillunchanged.
