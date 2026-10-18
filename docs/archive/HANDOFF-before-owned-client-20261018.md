# S3Drive handoff

Updated 2026-10-18. Goal ACTIVE. Mira is the assistant; no voice-context handoff
capability was available. Read this and docs/src/implementation-plan.md before
implementation. Keep research/decisions/status/plans in docs/src/. Run the local
gate with `bash scripts/gate.sh`; native/provider/recovery gates are separate.

## Accepted checkpoint and first action

Current branch: codex/owned-http1-close at
ea9342a78d11d97877a20584622227921e6b52f7, normally pushed privately with
source/ref/privacy-before-and-after guards. Proof /tmp/s3drive-owned-close-publication.json.
Post-push HANDOFF/closure JSON observations are intentionally dirty; preserve
for the next substantive phase without a docs-only native CI cycle.
Exact-source CI39120702390 is in_progress. See
/tmp/s3drive-owned-close-hosted-observed.json. Live watch session97697 writes
/tmp/s3drive-owned-close-hosted-watch.log. Main remains
bf9509. Goal remains ACTIVE. A required pre-rewrite review
found an untested TLS close_notify boundary in the test-only study. New gated
shutdown control failed old logic EXIT101 (/tmp/s3drive-owned-close-before.log),
then revised final focused9 controls passed EXIT0 (/tmp/s3drive-owned-close-
focused2.log). Clean successful TLS peer read==Ok(0) now rejects missing-alert
EOF/reset. Complete framed replies survive cleanup failure/expiry; unfinished
replies still time out. One absolute10sec budget starts before TCP/TLS and
covers driver cleanup, with no detached driver/retry. The delayed6sec-response
control holds shutdown and rejects a fresh10sec cleanup budget via14sec watchdog.
Read-only reviewer found no remaining load-bearing issue in this scoped revision.
No production WireClient/routing/config/dependency/lock change or provider call.

First full gate /tmp/s3drive-owned-close-gate1.log is terminal EXIT1: doc link
checked before the new evidence JSON was created. Its earlier Rust iteration
also discarded a complete reply at cleanup expiry and is superseded. Final combined full gate3 completed EXIT0 session11113,
/tmp/s3drive-owned-close-gate3.log: owned9, authority7unit/10HTTP/9TLS,
CLI restart9, core17/state274, workspace/build/lint and57-doc book. Frozen
source fingerprints /tmp/s3drive-owned-close-frozen-source.json match.
Hosted closure/native acceptance remains. See remote-authority.md,
implementation-plan.md and evidence/owned-http1-close-20261018.json. Preserve all
existing dirty docs, including multipart terminal failure and e00 observations.

b793 CI39117793980 is now terminal FAILURE: Windows117647008136 broad native
existing-file write failed Info/Request/UnexpectedEof/native1117, bodyfalse;
candidate/error loopback client64582/server64551 matches empty server24,
HttpTimeout5044/read0/write0/flush1/dispatchedfalse. Earlier32-flush, lifecycle,
workspace and paired recovery passed; final broader6PASS/1FAIL. Linux/book passed.
Completed raw /tmp/s3drive-multipart-b793-windows.log reviewed. DO NOT RUN the
success-only b793 checker or promote; main remainsbf9509. Dated sizing JSON has
terminal evidence. e00 CI39119010538 is terminal FAILURE: Windows workspace diagnostic fixture
expected3 events but saw the3 controlled faults plus positive/dispatched
HttpShutdown connection5/read243/write246/flush2 after both typed Info replies.
Owned7 passed twice,32 native flushes/lifecycle/startup passed; later gates
skipped. Linux/book terminal SUCCESS. Required narrow fixture correction must
retain exact controlled socket counts/categories/privacy and reject all other
I/O; it is not the original EOF cause. Completed raw /tmp/s3drive-owned-e00-{linux,book}.log retained;
Linux owned7 and native9/32flush/startup passed. Still unchanged production client.

The diagnosed e00 mixed-fault fixture is now corrected locally: two full Info
replies must match original volume+seq0; exactly3 injected socket/category/byte/
dispatch events remain; only optional distinct complete Info shutdown IDs4/5
with positive counters and correct non-injected loopback metadata are allowed.
Every authority TLS line is checked; suppression/unknown/duplicate events fail.
Focused validator negatives passed EXIT0 /tmp/s3drive-owned-close-classifier.log.
No production API/transport change. Final combined gate3 includes this revision and passed EXIT0. Earlier gate2
completed EXIT0 on final closure logic but preceded this fixture correction,
so it is superseded for combined acceptance.

Next: poll exact-source CI39120702390 and retain completed raw logs. Do not
restart solely on observation timeout. Require the new9 closure controls and
corrected mixed-fault fixture on Windows before any portability claim.
Do not promote from test-only study results or infer a production correction. Then implement the production carryover audit,
including complete-reply retention under caller deadline and truthful direct-I/O
diagnostics, followed by native/reference/startup gates and same-source repeat.
Do not substitute the prototype or passing arithmetic for production reliability.

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
