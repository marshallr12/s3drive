# Owned authority client

Research and implementation date: **2026-10-18**. This is a production-path
candidate on `codex/owned-authority-client`, based on the reviewed test-only
checkpoint ea9342a. Exact-source47e9dd4 run39122239338 passed Linux/book but
failed Windows flush burst step22: Info/UnexpectedEof, no body consumption,
actual owned TCP tag matching server164's empty-header timeout5196ms. A fresh
CLI then returned the original volume in844ms while both processes remained
alive. This replacement did not remove the failure. No success artifact checker,
repeat or main promotion is eligible. Main's accepted checkpoint remains bf9509.
The [dated evidence](evidence/owned-authority-client-20261018.json) preserves the
completed raw logs and their hashes.

## Request ownership

`WireClient` now establishes one TCP/TLS connection for each metadata RPC and
polls its Hyper HTTP/1 driver alongside its response consumer in the same
caller-owned task. There is one `send_request`, no HTTP connection pool, no
redirect following and no HTTP retry. Dropping the caller aborts its owned task;
it does not roll back an operation already accepted by the authority.

DNS/TCP establishment reuses the pinned Hyper-util `HttpConnector`, including
its 300 ms alternate-family race, 2-second connection timeout, TCP_NODELAY and
the previous 15-second/15-second/3-probe keepalive settings. Only the winning
TCP stream enters one Rustls handshake. This retains address fallback before
HTTP without replaying TLS or an HTTP operation. The first local implementation
used Tokio's sequential address loop; review rejected the stalled-first-family
regression before publication. Localhost controls cover real name resolution,
immediate-refusal fallback, preserved DNS Host and TLS SNI. A network-level
blackholed-first-family experiment has not been performed; the fallback mechanism
is retained from the same resolved dependency rather than reimplemented.

The absolute cooperative ten-second deadline starts before the task is queued.
An expired task refuses before connection creation. TCP/TLS setup has its own
unchanged two-second cap, with the caller's deadline bounding all remaining work.
Foreign-thread/native callbacks still wait on the same owned task mechanism.

## Complete results and shutdown

Reading a complete response does not establish that the HTTP driver has finished
TLS shutdown. A successful exchange continues polling an unfinished driver under
the original deadline; it never polls a completed driver again. HTTP/TLS clients
send closure alerts before normal closure, while fully length/chunk-delimited
responses remain recognizable despite incomplete peer closure.
[RFC9112 §9.8](https://www.rfc-editor.org/rfc/rfc9112.html#section-9.8)

The response consumer publishes its parsed result once through a private
one-shot channel before attempting cleanup. The caller consumes that result
after task completion or its original deadline. This prevents the outer caller
timer from replacing an already received receipt with a cleanup timeout. The
channel holds at most one result; reply bytes are bounded before parsing, and
private data is not placed in diagnostic progress fields. No cleanup task is
detached and no second timeout budget is granted.

After an HTTP/status/framing/JSON error, the task stops HTTP processing, extracts
the owned transport through the pinned HTTP/1 connection API and attempts stream
shutdown within the remaining budget. TLS close_notify cannot be guaranteed on
an already broken transport, cancellation or expiry. A cleanup failure does not
discard a known result, create another request or authorize recovery under a
new operation identity. An incomplete response still fails or times out.

## Preserved boundaries

| Boundary | Candidate behavior and portable evidence |
| --- | --- |
| Endpoint | Existing constructors retain numeric-loopback HTTP or verified HTTPS, exact `/v1/namespace`, no URL credentials/query/fragment. Independent peers check origin-form target, DNS Host/SNI, IPv6 brackets and explicit port. |
| Trust/private files | Existing owner-private bounded token/CA reads remain. Explicit PEM roots replace platform trust; the default path constructs the same pinned platform verifier. Wrong root/name/expiry and invalid material controls remain. No insecure verifier or key logger is installed. |
| Request | Validate the original request and serialize its exact bytes before task/network work. Preserve the 4 KiB bound, Content-Length, content type, bearer and original operation IDs/guards. Invalid/oversized input opens no connection. |
| Reply | Every 2xx status proceeds to full bounded strict JSON decoding. 201/299 succeed with valid replies; 204/invalid/trailing JSON fail. Declared and streamed bodies are capped at 8 MiB. Chunk extensions/trailers, conflicting lengths and missing terminal chunks have independent peer controls. |
| Lifetime | One connection per RPC with `Connection: close`; peers omit their own close header and still observe client closure. Successful TLS peers require `read == Ok(0)`, distinguishing close_notify from missing-alert EOF/reset. |
| Recovery | Existing competing-client, foreign-thread, actual-commit/caller-cancellation and exact receipt lookup controls remain. Lost replies preserve uncertainty. No automatic resend, rebase, new ID or unconditional overwrite is added. |

## Diagnostics and API exception

`owned_connection` reports the TCP stream this RPC actually owns: `Unopened`,
`Omitted`, or canonical `Loopback` ports/family. It can be present when TLS fails.
No nonloopback IP or hostname is retained. `connector_started/completed` describe
connection setup; completion requires TLS success for HTTPS. The retained
`connector_candidate` is the completed-setup observation. Legacy
`error_connection` remains available but is always `Unobserved`: the direct
client does not fabricate a legacy HTTP-client error. Body polling still does
not establish socket write, authentication or commit.

The new `OwnedConnection` enum and added `owned_connection` field change the
public `WireError::Transport` shape. Existing constructors/request signatures and
variants remain, but downstream exhaustive field patterns or construction need
updating; this is **not full source compatibility** for that diagnostic enum.
All workspace consumers are updated in the same candidate. These crates remain
unpublished. The old task-local connector observer is removed; progress is passed
directly to the single owning task, preventing cross-request attribution.

## Verification and remaining acceptance

Production unit controls use a real TLS peer and SQLite authority. The peer
commits the original Unicode create operation and returns a 201 receipt, while
shutdown is held pending outside TLS. They require the caller to wait for cleanup,
then cover clean closure, injected shutdown failure and expiry after a delayed
six-second reply. The retained receipt must equal the authority's exact operation
lookup, with sequence advanced once and no second transport. An independent
fourteen-second watchdog rejects granting a new ten-second cleanup budget.

The focused suite also keeps all existing HTTP namespace/recovery and TLS fault
controls, plus the earlier nine-test study. New independent production peers
exercise all-success-status/strict-JSON behavior, framing failures, trailers,
exact streamed bounds, DNS/SNI and IPv6. The final full local gate passed:46 authority controls,17 core and274 state
cases, workspace/build/lint and58-document book checks. The later exact-source
hosted cohort failed Windows as recorded above; portable results did not predict
native acceptance. See the [dated evidence](evidence/owned-authority-client-20261018.json).
A source-identical hosted repeat is required before any reliability acceptance.
Passing portable tests does not explain the historical Windows failure.

Large-file/multipart execution, approved real managed Wasabi writes, independent
hosts/partitions, history continuity/fencing, actual desktop startup/reboot and
packaging remain full-goal work. This client change performs no real provider
requests or account/startup/hosting registration.

## Follow-up: locate the missing HTTP work

Investigation date: **2026-10-18**. The [instrumentation evidence](evidence/exchange-progress-20261018.json)
records checks and limits. The next instrumented candidate adds a fixed
`ExchangeObservation` to transport errors. It records request-relative task
start, HTTP-ready, locally queued and first/last driver-poll milliseconds; driver
poll count/result and largest poll gap; and plaintext read/write polls, Pending
polls, byte totals, flush polls and shutdown polls. Counts and milliseconds
saturate at u32::MAX; observations retain no contents, secrets, hostnames or
nonloopback addresses. The additional public diagnostic field is another source
compatibility exception for exhaustive `WireError::Transport` construction or
patterns. The error remains Copy and the summary has fixed size.

The wrapper delegates scalar/vectored writes, vectored capability, reads, flush
and shutdown exactly once. It sits above TLS, so written bytes mean the TLS I/O
interface accepted plaintext, not that TCP delivered it or that the authority
authenticated or committed it. Snapshots are best-effort concurrent observations
at error creation, before subsequent cleanup. A driver returning Ok can have
already delivered an error to the response consumer; that result is not an RPC
success. Poll gaps include legitimate network waits and cannot prove starvation.
The ten-second RPC/two-second connect budgets, one-send ownership, original
identities and uncertainty rules remain unchanged.

The pinned Hyper handshake constructs a sender/driver without doing I/O. Its
first request can be buffered before receiver readiness; the driver checks for
peer EOF before writing. Thus an existing EOF can yield no body consumption or
writes even though a request was locally queued. Independent controlled I/O
tests distinguish this case from a failed first write. They explain why the
old observations are insufficient, rather than identify the Windows cause.
[Hyper1.12 HTTP/1 source](https://docs.rs/hyper/1.12.0/src/hyper/client/conn/http1.rs.html),
[dispatch source](https://docs.rs/hyper/1.12.0/src/hyper/proto/h1/dispatch.rs.html),
[connection source](https://docs.rs/hyper/1.12.0/src/hyper/proto/h1/conn.rs.html).
These behaviors were verified against locally resolved source; the online source
URLs could not be fetched during this investigation. Exact local hashes are in
the instrumentation evidence.

Source inspection also found that `logical::Journal::pending` validates every
retained edit and repeatedly walks generation ancestry, including for settled
history. Both Linux and Windows burst timings grew across iterations. This is
a scaling concern to measure and address while preserving corruption/lineage
checks. SQLite runs on dedicated owner threads; no synchronous SQLite execution
on the Tokio pool was found in these paths. Neither observation establishes the
empty-header cause. Native source-bound diagnostics remain required.

The final instrumented local gate passed48 authority tests (including13 unit
controls),274 state and17 core cases, workspace formatting/lint/build and58-page
book checks. Its exact-source90a8357
[cohort39124027385](https://github.com/marshallr12/s3drive/actions/runs/39124027385)
then passed Linux/Windows/book, Windows32 flushes, full lifecycle, paired startup
and broad7 native cases. Its own21/14 reports passed ZIP/API digest, source,
strict field/type, observation-time and project-oracle checks. The completed logs
contain only deliberate TLS/authorization/conflict/stopped-authority faults and
documented toolchain/linker/action notices. This single cohort does not explain
the earlier failures or provide a source-identical repeat. No main promotion is
planned from diagnostics alone; see the instrumentation evidence for exact scope.

The later client-local [pending-scan candidate](managed-edits.md#retained-history-scan-validation-candidate-2026-10-18)
at64aa073 passed its own Linux/Windows native/startup and strict21/14 cohort.
Its [unchanged-source repeat](https://github.com/marshallr12/s3drive/actions/runs/39127440399)
failed Windows paired startup; Linux/book passed. The failed Info exchange had
469 driver polls,468 Pending reads and no write polls or plaintext bytes; its TCP
tag matched the server's empty-header timeout. Active polling alone did not
produce a request. Preserve that evidence without inferring a scheduler, TLS,
HTTP or SQLite cause. The failed run cannot supply repeat acceptance or a full
strict21/14 artifact cohort. The optimization reduces redundant journal checks;
it does not change HTTP polling/deadlines or establish a causal transport fix.

## Dependency-origin and upstream audit (2026-10-18)

For the fresh-connection Windows failure with469 driver polls and no plaintext
write polls, a read-only audit compared five inspected Hyper1.12.0 files and three
Tokio1.53.3 files with their cached crate members. Every byte matched, and both
crate archives matched Cargo.lock SHA-256 checksums. This rules out modified
local extractions for those inspected files. Exact hashes and scope are in the
[dependency-origin evidence](evidence/transport-dependency-origin-20261018.json).
No production or dependency source was changed by the audit.

The pinned release already includes the stale-want readiness correction4208.
That report concerned a busy connection being advertised as ready to a pool;
it is not evidence that the present fresh owned connection will send correctly.
[Hyper1.12.0 release](https://github.com/hyperium/hyper/releases/tag/v1.12.0),
[upstream readiness correction](https://github.com/hyperium/hyper/pull/4208).

A later master change4222 drains the closed dispatch receiver before shutdown
completes, retaining its original result while in-progress senders finish.
That is relevant to a response-retention audit, but it does not establish the
cause of our no-write timeout. In the owned path, synchronous `send_request`
returns before this same task first polls the connection driver; an overlapping
send-versus-close race has not been demonstrated. This is a source-based
inference, not native acceptance or a reason to upgrade blindly.
[Upstream shutdown correction](https://github.com/hyperium/hyper/pull/4222).

Before changing scheduling or transport policy, distinguish sender readiness,
first body-descriptor access and cooperative budget at driver polls. Preserve
one send, exact operation identity, original deadlines and the known-reply
retention contract. Any resulting correction still needs controlled negative
cases, the local gate, its own native Windows evidence and an unchanged-source
repeat. No retry, provider write or runtime promotion is authorized by this audit.

## Dispatch-boundary observations (candidate, 2026-10-18)

`ExchangeObservation` gains five compact public fields while retaining `Copy`
and the at-most128-byte sanitized error bound:

| Field | Observation and limit |
| --- | --- |
| `sender_ready_before_enqueue` | One `is_ready()` hint immediately before the existing synchronous send; `None` means unobserved, and `Some(false)` is expected for the fresh unpolled driver. It is not later readiness or an error |
| `body_end_stream_queried` / `body_size_hint_queried` | The existing HTTP body descriptor calls occurred; each inner call still delegates once with its return unchanged. Access is not body consumption, a write or delivery |
| `driver_budget_exhausted_before` / `driver_budget_exhausted_after` | Monotonic flags when a sampled boundary had no cooperative budget. They do not pair samples, locate the responsible operation, measure starvation duration or prove cause |

The implementation reads budget without consuming it and retains first-request
buffering, the biased work/driver order, one send, original deadlines and exact
known-result/cleanup behavior. It adds no readiness wait, yield, retry, network
operation or dependency change.
[Hyper readiness hint](https://docs.rs/hyper/1.12.0/hyper/client/conn/http1/struct.SendRequest.html#method.is_ready),
[Tokio budget observation](https://docs.rs/tokio/1.53.3/tokio/task/coop/fn.has_budget_remaining.html).
Both resolved sources were inspected before use. These online API pages were
unavailable through the research tool during this refresh; the claims were
checked against the resolved local files.

The controlled test drives the actual production exchange and completion path
against an ephemeral TCP peer and file-backed authority. A test-only reader
exhausts cooperative budget and returns Pending; several driver/read/flush polls
then occur with no body descriptor/frame or write. Releasing the gate completes
exactly the original request and reply. A baseline without drain also completes.
The fixture bounds its drain loop, uses a bounded observation predicate and
reaps workers before asserting. This demonstrates one possible mechanism;
it does not reproduce or establish the Windows failure's cause. Positive
exhausted-after and negative exhausted-before are covered; positive exhausted-
before is not separately exercised. Empty/Unicode bodies and pre-write EOF versus
failed-first-write controls preserve framing and distinguish descriptor access.

The parentc7 Windows run later failed a separate startup fixture's stale
schema23 assertion against a schema24 binary, after its paired recovery passed.
This candidate corrects that exact assertion and adds an early source-contract
check; it preserves strict version refusal and every native acceptance case.
That fixture correction does not identify or fix the earlier no-write timeout.

Nine focused wire tests passed. The final extended local gate then passed its
schema preflight,50 authority tests (15 unit),292 state tests (one diagnostic
study ignored), workspace/build/lint and book checks on unchanged frozen source.
Own source-bound native acceptance remains required and is recorded in the
[dated evidence](evidence/dispatch-progress-20261018.json). A future readiness
wait would need a separate contract: immediate peer EOF before enqueue can
change which error retains the concrete I/O cause. Do not turn this observation
phase into a retry, an inferred reliability fix or runtime promotion.

## Resolved implementation sources

The local resolved package sources were inspected before API use; exact hashes
are in the dated evidence. No dependency package version changed. The authority
adds a direct dependency on already resolved rustls-platform-verifier0.7.1;
reqwest remains only for its bounded body implementation, not a client/pool.

- [Hyper HTTP/1 handshake, connection parts and driver](https://docs.rs/hyper/1.12.0/hyper/client/conn/http1/index.html).
- [Hyper-util TCP connector settings](https://docs.rs/hyper-util/0.1.21/hyper_util/client/legacy/connect/struct.HttpConnector.html).
- [Rustls platform-verifier builder extension](https://docs.rs/rustls-platform-verifier/0.7.1/rustls_platform_verifier/trait.BuilderVerifierExt.html).
- [Tokio-rustls client stream](https://docs.rs/tokio-rustls/0.26.6/tokio_rustls/client/struct.TlsStream.html).
- [Tokio one-shot receive contract](https://docs.rs/tokio/1.53.3/tokio/sync/oneshot/struct.Receiver.html#method.try_recv).


### Exact instrumented repeat (2026-10-18)

Primary source2575374 passed run39133178423 **attempt1** with reviewed own raw
logs and strict21/14 reference artifacts. Before repeating, its complete
[cohort archive](evidence/dispatch-257-attempt1-20261018.tar.gz) preserved explicit
attempt metadata, job IDs, raw reviews/logs, original ZIP bytes and both checks.
SHA-256: `8d0e2c598aa091a54991c51e80b5c3b07912c4723b9a6451e039509f3b248d5b`.
A latest-run URL may now describe a later attempt; primary evidence is immutable
in that archive and the [primary record](evidence/dispatch-progress-20261018.json).

Attempt2 repeats the original commit to collect the new observations if the
historical failure recurs. It changes no scheduling, retry, deadline or provider
policy. Track its [separate evidence](evidence/dispatch-repeat-20261018.json).
All three attempt2 jobs completed successfully. Own raw logs were reviewed,
including expected fault controls and nonfatal cache-reservation warnings. Fresh
replacement12139721137/disposition12140285720 artifacts passed source/run/API
hashes, strict21/14 all-field/type checks and both project PowerShell oracles.
The checker closed EXIT0 and refused primary artifact IDs. The historical
actively-polled/no-write failure did not recur; its cause remains unresolved.
This is additional scoped evidence, not acceptance of the newer manifest codec,
Windows10/11, physical reboot or a live provider. Runtime main is unchanged.

GitHub documents that reruns retain the original commit/ref and exposes
attempt-specific run/job APIs. Its artifact client also documents that reruns
can produce artifacts with identical names. Therefore source/name alone cannot
identify a repeat artifact: require this attempt's own completed Windows upload
IDs, metadata/digests and strict field/type/oracle checks. Never reuse the prior
cohort simply because its JSON matches.
[GitHub rerun semantics](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/re-run-workflows-and-jobs),
[workflow-run attempt API](https://docs.github.com/en/rest/actions/workflow-runs#get-a-workflow-run-attempt),
[artifact client contract](https://github.com/actions/toolkit/blob/main/packages/artifact/src/internal/client.ts).


## TLS budget fixture isolation — 2026-10-18

Publisher source`cf9d972` [CI39156593000](https://github.com/marshallr12/s3drive/actions/runs/39156593000)
failed Linux in the existing pinned CLI restart test before the new chunk tests.
Exactly17 diagnostics were captured; the first16 were not all the expected fixed
handshake-rejection line. The original assertion printed no actual categories.
The historical event class and failure cause therefore remain **unknown**.
An instrumented local control passed32 bounded attempts; that is not causal proof.

The source audit found two fixture confounders: the listener's budget was already
shared with four completed authenticated RPCs, and TCP EOF did not prove that a
failed-handshake diagnostic had been written. The resolved dependency returns a
failed accept after dropping its IO; production reserves a diagnostic slot before
locking stderr. Audit paths, versions and hashes are retained in the
[dated evidence](evidence/tls-budget-fixture-20261018.json). No production
transport, logger, retry, timeout, schema or chunk-format behavior changes.

The repaired fixture finishes its restart/receipt assertions, then starts a fresh
listener on the same pinned database and volume. Readiness is stdout-only, with
no network probe. Its first17 malformed peers each wait for the exact expected
stderr line before the next peer; it still sends24 in total. The first16 must be
HandshakeRejected, followed by one suppression notice. Only after observing that
notice does an authenticated Info request verify the original volume and sequence.
Both listener logs retain the privacy checks, and assertion output uses a fixed
category allowlist. No line filtering, budget increase or arbitrary sleep hides
an unexpected event.

A bounded collector initially had a separate truncation flaw: CRLF stripping
could make its retained string shorter than its exhausted raw input allowance,
allowing later stderr to be omitted. Review identified this gap; a negative
control with17 diagnostics, CRLF padding and a private tail failed before the raw
exhaustion check. Exhausting that allowance now returns an error. This control
establishes the collector guard, not the cause of the captured CI failure.

Final focused controls passed10/3ignored. The full local gate passed with all249
frozen hashes unchanged:349state/1ignored,59S3,34core,50authority, all18 existing
managed-publication tests,8chunk-publication controls and10TLS/restart controls,
formatting/Clippy/build,65documentation files and61book chapters. Independent
review matched all249hashes, confirmed the sole test-file delta and found no
remaining blockers. State tests took195.62s; two old quota controls and the sparse
chunk test exceeded60s then passed. The
[11-member indexed local archive](evidence/tls-budget-fixture-local-20261018.tar.gz)
retains focused/red/full results, the frozen map, review and exact fixture source.

Corrected source`32d7cc7` is pushed on `codex/tls-budget-fixture`, with a clean
exact remote tip and all249 frozen hashes verified. Public visibility was checked
read-only before/after; main is unchanged. Its
[own hosted cohort](https://github.com/marshallr12/s3drive/actions/runs/39158147595)
completed successfully on Linux, Windows and the book job. All three raw logs
were reviewed. Both platforms passed the repaired CLI and CRLF controls and all 8
chunk-publication controls (Linux 24.51 s, Windows 85.06 s). Existing real FUSE/WinFsp,
32-HTTPS-flush and paired startup/recovery checks passed. Windows also verified 11
Unicode filenames and a Unicode directory. This is hosted regression evidence;
the new chunk layout is not yet mounted.

The independent checker downloaded only this cohort's finalized replacement
artifact 12147997314 and disposition artifact 12147329360, checked their API
digests/source/timestamps, and passed all 21/14 typed reference records plus the
project oracles. A separate comparison matched every raw-log record field and
type to the downloaded JSON. No earlier cohort's payload was reused. The
[22-member indexed hosted archive](evidence/tls-budget-fixture-32d-hosted-20261018.tar.gz)
retains raw logs, source/API metadata, ZIPs, reports, checkers and the exact fixture.

Windows retained the unused WinFsp delay-import linker warning, Node/cache
deprecations and three locked disposable profiles whose dummy credentials were
removed before VM destruction. Expected TLS refusals/timeouts, HEAD403, content
conflict and stopped-authority failure controls passed. Long sparse/quota tests
eventually passed. The failed older cohort stays a failure; its historical event
category remains unknown. This repair establishes no historical Windows NoWrite
cause, mounted chunk support, source settlement, physical reboot or real-provider
tree acceptance.
