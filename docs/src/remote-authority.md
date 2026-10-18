# Shared authority over HTTPS

Design/research date: **2026-10-17**. Accepted at
`a87bd0bd3992bf0af838ac0f97c7a4e97687662e`, terminal all-green
[CI39075749765](https://github.com/marshallr12/s3drive/actions/runs/39075749765).
Both native OS writer suites used the real Rust HTTPS authority with a private
CA. All five TLS and ten HTTP cases passed, along with256/252 Linux/Windows
state tests,17 SDK cases, Linux7 and Windows1+7 native cases, both startup gates
and exact-source typed21/14 reference artifacts. No remote host was provisioned;
these are local/hosted-runner transport and native filesystem results, not real
multi-host or physical reboot acceptance. Metadata transactions, operation IDs
and the S3 layout are unchanged. See [operator setup](managed-bootstrap.md).

![Linux and Windows keep private local SQLite state and access one Rust metadata service over HTTPS; immutable file bytes travel directly between their S3 clients and Wasabi.](images/remote-authority.svg)

The current2026-10-18 [owned-client candidate](owned-authority-client.md) replaces
the metadata client's HTTP execution path while retaining this trust/recovery
contract. Its production-path code has portable controls; native and repeat
acceptance remain required. The dated sections below preserve earlier candidates
and their contradictory Windows evidence rather than establishing reliability.

## What runs where

The coordinator is a **Rust service** (`s3drive authority-serve`). It owns one
shared volume's namespace and authoritative database. SQLite is the implemented
backend; Turso remains an optional backend proposal requiring separate migration,
transaction and uncertainty acceptance. Hosting preference remains undecided.
Neither option requires putting file contents into the shared database.

Each computer runs its own Rust mount engine, S3 client, private SQLite recovery
journal and cache, with FUSE on Linux or WinFsp on Windows. Clients upload immutable
payloads directly to S3, then conditionally publish their references through the
coordinator. The coordinator has no S3 credentials. Local SQLite remains outside
the mounted tree. Only an online authoritative commit can establish a global
name/revision; TLS does not add offline ownership or turn S3 and SQL into one
transaction. Existing open snapshots and retained local dirty data keep their
documented behavior during a partition.

## Transport and trust contract

HTTP remains restricted to numeric loopback addresses. Nonloopback listeners
require both explicit `--tls-cert` and `--tls-key`; the service terminates TLS
itself. Rustls0.23.46/tokio-rustls0.26.6 provide TLS1.2/1.3, with HTTP/1.1 ALPN.
The server supplies a matching leaf-first PEM certificate chain and unencrypted
PEM private key. The files must pass the same owner-private local-file/link/ACL
checks as tokens, now bounded to64KiB each. Token limits remain66 bytes.
Keys read from disk are held in a zeroizing buffer during initialization; errors
never include PEM contents or source configuration. Files load once at startup;
certificate/token renewal requires an orderly restart in this phase.

The client accepts only `/v1/namespace`, with no URL credentials, query or
fragment. HTTPS verifies the server name and chain. By default it uses platform
trust; optional `managed.authority_ca_file` replaces that trust with the explicit
owner-private PEM roots. It cannot accompany HTTP. There is no insecure-certificate
switch, proxy inheritance, redirect following or automatic mutation retry.
TLS key logging is explicitly disabled. Resolved dependency APIs were inspected;
see [reqwest0.13.5 trust controls](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html#method.tls_certs_only)
and [tokio-rustls](https://docs.rs/tokio-rustls/0.26.6/tokio_rustls/struct.TlsAcceptor.html),
read2026-10-17. These are dependency contracts, not deployment acceptance.

A 256-bit bearer token grants all namespace operations on this service's one
volume, including mutations. It is intended for trusted devices of one owner;
there are no separate reader roles, device revocation lists or multi-tenant
accounts. Requests for another volume remain rejected by the authority. Share
the token privately with authorized machines; never place it in TOML, command-line
arguments, Git or chat. Each machine still supplies its own S3 credentials.

The TLS listener bounds accepted connections to 32, concurrent database requests
to 8, handshakes and headers to 5 seconds each, and an HTTP connection to 10 seconds.
It disables keepalive and caps header buffering at 16 KiB; JSON requests retain
the 4 KiB limit and clients cap replies at 8 MiB. Client connect/whole-request
deadlines remain 2/10 seconds. Long network delays can therefore produce an unknown
mutation outcome. Reconcile the original durable operation before any subsequent
effect; do not rebind, overwrite, or manufacture a new ID. On shutdown the listener
closes and connections drain; accepted database commands retain their worker
ownership even if the requesting socket disappears.

## Explicit metadata connection lifetime (candidate, 2026-10-18)

The snapshot-topic candidate sends `Connection: close` on every HTTP/1.1
metadata request, for both HTTPS and numeric-loopback HTTP. This makes the
client choose a finite connection lifetime, matching the TLS listener's
existing one-request policy. RFC9112 permits this choice; once the client
declares it, the connection must not carry another request, and the client
closes after the final response. Persistent clients are also allowed by the
standard; the earlier default was not itself a protocol violation.
[RFC9112 sections9.3 and9.6](https://www.rfc-editor.org/rfc/rfc9112.html#section-9.6)
(read2026-10-18).

The resolved reqwest0.13.5 public `RequestBuilder::header` and `http::header`
re-export were inspected locally before use. Dependency versions, timeouts,
TLS trust, request serialization, automatic retries and pool-builder options
remain unchanged.
The cost is a new connection per metadata exchange, including a new TLS
handshake for HTTPS. This is not a throughput improvement or an S3 setting.

Two bounded local wire tests observe actual sockets and complete replies. With
the prior client, sixteen sequential Info calls used one keep-alive socket;
with the candidate, all sixteen receive the expected volume through sixteen
connections declaring `close`. A separate raw peer omits its own close header
and leaves the socket open; the client still returns the complete reply and
closes its side. Fixture tasks are stopped/reaped before assertions. These
tests establish the lifetime policy, **not a reproducer or explanation of the
intermittent Windows failures**. Existing HTTP/TLS fault, native, startup and
strict reference gates remain required. The main branch remains f8c4f58 until
new exact-source acceptance; see the [dated record](evidence/authority-snapshot-platform-20261018.json).

At e51f737, [CI39103791947](https://github.com/marshallr12/s3drive/actions/runs/39103791947)
passed Linux/book and the Windows workspace, lifecycle and paired startup checks.
The later Windows broad suite failed an existing-file flush with
Info/UnexpectedEof and native1117. Six other broad cases passed. The lifetime
policy did **not** eliminate the failure, and this source was not promoted.

## Client body-poll observation (candidate, 2026-10-18)

The next diagnostic snapshots one `body_frame_polled` boolean when classifying a
client transport failure. It means the HTTP consumer polled a data frame by
that instant. It does not measure a socket write, identify a selected connection,
establish server receipt or prove authentication/commit. False is not a safe
unsent result: preserve the original unknown operation and its guards.

The implementation wraps reqwest's original `.json()` body through the public
`Body` interface, forwarding frames, length/end hints and errors. Resolved
reqwest0.13.5, Hyper1.12.0 and http-body1.1.0 sources were inspected; no arbitrary
request extension or private dependency field is used. The earlier rejected
connection-capture experiment remains separate. No dependency, deadline, retry,
trust or connection-policy changes are part of this diagnostic.
The wrapped body is not cloneable, unlike reqwest's original buffered variant;
redirects and retries remain disabled. This is instrumentation with a different
body representation, not proof that every scheduling detail is unchanged.

Local controls passed: a TLS peer received byte-identical serialized mutation
JSON with application/json, the original Content-Length and no chunked transfer;
dropped/truncated/stalled replies report a polled frame, while failed certificate
trust reports false before HTTP. Redirect refusal, absence of a second mutation
and private-error redaction remain required. These observations validate the
marker's meaning, not a correction of the Windows failure. Full local and new
exact-source native/reference/startup gates remain required before publication.

The accompanying native fixture adds32 consecutive positive flushes through a
real mounted Unicode path, each with a distinct32-byte binary payload, a fresh
read and exactly one additional SDK PUT. It has no injected outage or accepted
error and stops at the first unexpected failure. A failure runs the independent
read-only CLI control, reaps owned processes and resumes the original panic;
the fixture does not retry the failed step. Its control sends no mutations.
Blocking filesystem operations yield the test's
Tokio worker with `block_in_place`, so its S3 fixture can continue serving.

Windows CI runs this case explicitly after ordinary-account coordinator/snapshot
checks and before lifecycle/workspace checks. The raw TLS controls run first on
Windows too, so a native failure's marker is not supported only by Linux tests.
The broad Windows step excludes
only the two cases already run explicitly; all earlier checks remain. Linux's
native suite includes the same fixture. Local compilation/workflow parsing
cannot establish either native result; this workstation's FUSE restriction is
preserved, and new hosted acceptance is required.

## Runtime-owned request execution (candidate, 2026-10-18)

At a14ffa3, [CI39106656359](https://github.com/marshallr12/s3drive/actions/runs/39106656359)
passed Linux's nine native cases, including32 flushes, and the Windows six raw
TLS controls and standalone snapshot/console workflow. Windows's fourth flush
failed an Apply request with UnexpectedEof/native1117 and
`body_frame_polled=false`. A nearby empty TLS socket timed out; there is still
no exact socket/RPC correlation. The fresh CLI reached the original volume in
152ms afterward, with both processes alive. No candidate was promoted. The
false observation does not authorize discarding the original uncertain input.

Native adapters call asynchronous filesystem code from callback threads using
their runtime handle. The next candidate executes each metadata RPC in one
owned Tokio task on that current runtime. It clones the exact request once;
it creates no new operation identity, retries no request and changes no trust,
encoding or connection policy. This tests an execution-context hypothesis,
not an identified Tokio/reqwest/WinFsp defect or a proven Windows correction.

A caller-scoped JoinSet aborts its job when the caller disappears. An absolute,
cooperative10-second deadline covers both queuing and RPC work; the existing
two-second connect and ten-second reqwest deadlines remain. Outer timeout
classification uses whether response headers were observed, preserving request
versus response-body phases. Missing runtime/task failures return a fixed
Execution category without formatting dependency/panic details. Any accepted
remote mutation may still finish after cancellation: retain the same unknown
operation and reconcile its original receipt. Abort is not rollback.

The resolved Tokio1.53.3 JoinSet spawn/join/drop, runtime-handle and timeout
contracts were inspected locally. Local controls passed: cancel a queued job
before the single-thread runtime can run it, observe no surviving task or TCP
connection, then complete a real request with the same client; execute mutations
from a foreign thread with exact receipt lookup/replay and unchanged identities;
and cancel after an actual SQLite commit while response bytes are withheld,
recovering its original receipt without a second request or sequence advance.
Existing byte-framing, privacy, trust and timeout controls remain. These are
execution/uncertainty checks, not a native Windows failure reproducer. Windows
now runs all authority wire/execution controls before the unchanged32-flush
native gate, followed by every prior acceptance gate.

## Connector observations (candidate, 2026-10-18)

The runtime-owned candidate949073f failed
[CI39108821537](https://github.com/marshallr12/s3drive/actions/runs/39108821537).
Linux/book succeeded. Windows passed the unchanged32-flush gate, full lifecycle,
workspace, standalone/paired recovery and project reference checks, then failed
opening an existing Unicode file in the broader write case: List/Request/
UnexpectedEof, native1117, body_frame_polled=false. A nearby empty TLS socket
expired after5057ms, but it is not correlated to that exact RPC. Both console
checks exited5ms after cancellation (single observations, no timing guarantee).
Main stays at f8c4f58; runtime ownership did not eliminate the failure. See the
[dated raw-log record](evidence/authority-snapshot-platform-20261018.json).

The next diagnostic uses reqwest's public
[connector_layer](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html#method.connector_layer)
API, inspected in the resolved local source on2026-10-18. It reports whether a
connector was called under this RPC's task scope and whether such a future
successfully returned a connection. The layer captures that scope at call time,
so later completion cannot attach to a different concurrently polled request.
A single monotonic atomic snapshot distinguishes unobserved, started and at
least one success. Unscoped work is not attributed to any request.

Neither flag identifies the selected socket, proves HTTP bytes were sent or
resolves an unknown mutation. A false flag cannot authorize replay. Error output
contains only these fixed booleans and existing categories; destinations, tokens,
bodies, certificates and dependency error strings are never recorded. Trust,
framing, operation identities and existing deadlines/retry policy stay the same.
The public layer does select reqwest's boxed connector path, so scheduling
transparency is not claimed. This is diagnostic work, not an established fix.

Controlled concurrent TLS exchanges use one cloned client: one peer receives
HTTP then drops its reply; the other accepts TCP but stalls TLS. They must
produce different connector-completion/body-poll flags, with exactly two sockets
and no extra attempt in the bounded observation window. Existing trust, received
bytes, timeout, redaction and no-replay assertions remain. New native acceptance
and strict source-bound artifacts are still required before promotion. The full
local gate passed, including274 state tests, nine TLS cases, three authority
unit cases (two Hyper controls and one scope control), lint/build and57-document
checks. No native mount or live-provider acceptance follows from that gate.

### Upstream triage (researched 2026-10-18)

Hyper's [stale readiness fix](https://github.com/hyperium/hyper/pull/4208) is
already present in the resolved1.12.0 source (`Receiver::poll_recv` withdraws
`want`), with want0.3.2 in Cargo.lock. This is not a missing dependency update.
A separate [queued-request shutdown report](https://github.com/hyperium/hyper/issues/4202)
was closed by [asynchronous dispatcher draining](https://github.com/hyperium/hyper/pull/4222)
on October9, after [1.12.0's October6 release](https://github.com/hyperium/hyper/releases/tag/v1.12.0).
The resolved source lacks that later drain change; no patch has been applied.

That report concerns a queued request stranded after dispatcher closure until
its caller's timeout. Our recorded949 error returned EOF before the10-second
client deadline and lacks queue/socket correlation. A shared cause remains
unproven. Compare the new connector observations before changing dependencies;
upstream recovery of a request does not authorize replaying an uncertain logical
mutation. These findings are research, not a native reproducer or fix.

### Public socket-metadata control (Linux only, 2026-10-18)

An isolated [reproducer archive](evidence/public-connector-probe-20261018.tar.gz)
contains source, manifest, lockfile and instructions. It passed four local
controls: HTTP and HTTPS, each with a complete reply and with a deliberately
lost reply after receipt of request headers. Using only the public
[Connection](https://docs.rs/hyper-util/0.1.21/hyper_util/client/legacy/connect/trait.Connection.html),
[Connected::get_extras](https://docs.rs/hyper-util/0.1.21/hyper_util/client/legacy/connect/struct.Connected.html#method.get_extras)
and [HttpInfo](https://docs.rs/hyper-util/0.1.21/hyper_util/client/legacy/connect/struct.HttpInfo.html)
APIs, the connector observed the same TCP address pair as the independent peer.
No second connection occurred in the bounded observation window; no credentials
or provider requests were involved. Resolved source fingerprints and archive
hashes are in the [dated record](evidence/authority-snapshot-platform-20261018.json).

This establishes a possible diagnostic path even when a reply is lost; the
previous rejected request-extension capture is not required. The experiment is
not part of the production client and does not identify a socket selected by a
failed production RPC. Before adoption, limit output to bounded loopback-only
correlation, preserve request-scope attribution, and test missing metadata and
concurrent candidates. It provides no Windows mount or provider acceptance.

The e988 candidate subsequently failed
[CI39111567111](https://github.com/marshallr12/s3drive/actions/runs/39111567111).
Linux/book passed; Windows passed the controls, standalone snapshot/console,
32-flush gate, full lifecycle and workspace, then failed paired recovery. The
Info/Request/UnexpectedEof error recorded connector_started=true and
connector_completed=true but body_frame_polled=false. A connector returned
successfully before the error; the exact socket/dispatch stage is still unproven.
The nearby empty TLS connection133 expired after5083ms. The fixture's expected
single uncertain PUT was not established; do not claim an upload or commit.
Later14-reference/broad native steps were skipped. A locked disposable profile
also failed cleanup; the original failure was retained. Main remains f8c4f58.

Paired process-class logs showed both supervisors and children BelowNormal.
Actual thread priorities and a causal scheduling explanation remain unmeasured;
no priority change is part of this diagnostic phase. The standalone private-console
cancellation took16ms in one observation. See the
[dated record](evidence/authority-snapshot-platform-20261018.json) for scope and
raw-log hashes. None of this is acceptance for the later loopback-tag source.

## Bounded loopback candidate tags (implementation candidate, 2026-10-18)

The follow-up implements the independently tested public metadata path. A
transport error may now contain one connector candidate with only an IPv4/IPv6
flag and client/server TCP ports. Only the exact `127.0.0.1` or `::1` address pair,
matching families and nonzero ports qualify; other addresses and missing metadata
produce `Omitted`. No address, hostname, certificate, request field or dependency
error is retained. Before any successful connector handoff the tag is `Unobserved`; more than one
successful connector becomes `Multiple` and exposes no individual candidate.
The aggregate uses one atomic snapshot and never replaces the first observation
with a later single candidate.

The connector captures its request scope when called, then records sanitized
metadata before marking connector completion. Server TLS diagnostics record the
same loopback-only port/family tag, alongside their existing connection ordinal,
byte counters and dispatch flag. Existing rate limits and failure classes remain.
Ports can be reused; a matching candidate is not proof that a pooled RPC selected
that socket. It cannot establish delivery, authentication or commit, or permit
replay. Non-loopback deployment intentionally receives no address correlation.

Controlled lost-Apply-reply and concurrent TLS tests compare the client candidate
against the independent peer's actual TCP ports; a stalled handshake remains
unobserved. A real CLI listener's idle, partial-header and partial-body events
must match the caller's socket tag and their original byte/failure assertions.
Unit controls cover missing metadata, cross-scope completion, non-loopback and
mixed-family omission, duplicate candidates and concurrent successes. Existing
wire framing, privacy, trust, timeout and no-replay assertions remain. These
focused local checks and the full local gate passed (including274 state tests,
nine TLS cases and six authority unit cases). A new native cohort is required. The e988 hosted run covers its earlier source without these tags.

### Error connection metadata (research control, 2026-10-18)

A separate [standalone reproducer](evidence/error-connection-info-probe-20261018.tar.gz)
passed the four HTTP/TLS complete/lost-reply controls and two additional
comparisons. For each deliberately lost reply, Hyper-util's public
[Error::connect_info](https://docs.rs/hyper-util/0.1.21/hyper_util/client/legacy/struct.Error.html#method.connect_info)
was reachable through the bounded reqwest error source chain and matched the
independent peer's actual TCP connection. Production bf9509 does not collect it.

In resolved0.1.21, errors from the pooled send path attach connection information;
errors returning earlier from `connection_for` do not. This offers a stronger
future observation than a connector candidate alone. Dependency source chains
are not stable APIs across releases; validate the pinned versions and sanitize
non-loopback information before adoption. Missing metadata cannot prove an
unsent request or authorize replay. This was a Linux-only GET fixture without
bearer authentication, JSON body or real provider; it does not explain historical
native failures. See the [dated record](evidence/authority-snapshot-platform-20261018.json)
for exact source, archive and log hashes.

## Accepted hosted checkpoint bf9509 (2026-10-18)

[CI39113628548](https://github.com/marshallr12/s3drive/actions/runs/39113628548)
completed all three jobs successfully at exact source bf9509. Raw review confirmed
Linux's nine native cases, Windows's32-flush and complete lifecycle gates plus
seven broader native cases, ordinary-account standalone/paired supervisor and
snapshot checks, exact unknown-operation recovery, retained conflicts and
controlled removal. The only Windows metadata transport failure was the deliberate
authority-stop List/ConnectionRefused check. Typed shutdown diagnostics retained
positive counters and completed caller semantics; no unexpected EOF or generic
HTTP transport event was found.

The source-bound artifact download digests, independent strict21/14 field/type/
platform checks and project reference oracles passed. Private `main` was normally
fast-forwarded from f8c4f58 to bf9509, with remote SHA/privacy rechecked and dirty
local research preserved. See the [dated publication record](evidence/authority-snapshot-platform-20261018.json).

This accepts the offline snapshot/transport diagnostic checkpoint on hosted Linux
and Windows Server2025/WinFsp2.1.25156.ddca7bd. Snapshot restoration remains
unsupported. Console cancellation took4ms standalone and35ms paired, each one
observation. It does not prove the cause or elimination of earlier intermittent
EOFs, Windows10/11, physical reboot or live managed Wasabi writes. A new
[unchanged-source main repeat](https://github.com/marshallr12/s3drive/actions/runs/39115395230)
completed successfully. Its separately downloaded artifacts passed digest,
strict21/14 and project-oracle checks. Raw review confirmed Linux9 native cases
and Windows32-flush, lifecycle, paired recovery and broader7 native cases.
Windows's sole metadata failure was a Connect/TimedOut List call after the
deliberate authority stop; Linux's two Node/ConnectionRefused events were also
in that outage control. One typed Windows HttpShutdown event during the flush
burst had positive read/write/flush/dispatched counters and the complete32-flush
caller checks passed. No unexpected EOF or generic HttpTransport appeared.
Standalone/paired Windows console cancellation observations were3ms/5ms; these
are individual samples. Two passing cohorts do not establish the historical
failure's cause or a general reliability guarantee. The full goal remains active.

## Error-reported connection diagnostic (candidate, 2026-10-18)

The follow-up adds `error_connection` when classifying a transport failure.
It uses the pinned Hyper-util0.1.21 public
[Error::connect_info](https://docs.rs/hyper-util/0.1.21/hyper_util/client/legacy/struct.Error.html#method.connect_info)
getter after a bounded16-element error-source walk (resolved source inspected
2026-10-18). The existing connector candidate is retained separately. The new
field distinguishes these observations:

| Value | Observation |
| --- | --- |
| `Unobserved` | No matching legacy error in the bounded chain, or an outer deadline without a dependency error |
| `Unavailable` | Matching legacy error reports no connection information |
| `Omitted` | Information exists, but lacks TCP endpoints or fails the canonical loopback filter |
| `Loopback` | Sanitized family and client/server ports from error-reported connection information |
| `Multiple` | More than one matching legacy error; no individual connection is exposed |

Only the existing exact127.0.0.1 or::1 endpoint filter can produce a socket tag.
No host, other IP address, arbitrary connection extra, request, credential or
dependency error string is retained. This work runs on the error path and changes
no request execution, timeout, trust, retry, framing or operation-identity policy.
Source-chain availability is a pinned-version observation; dependency upgrades
must rerun these controls.

Local TLS controls match both candidate and error-reported tags to independently
accepted sockets for lost mutation replies and concurrent requests. Wrong trust
and a stalled handshake report `Unavailable`. A truncated-response control proves
full request receipt yet reports `Unobserved`, showing why missing information
cannot prove a request was unsent. Unit controls cover unknown extras and
ambiguous repeated observations; the shared endpoint filter retains its IPv4,
IPv6, mixed-family, noncanonical and non-loopback tests. Redaction and exact
no-replay assertions remain required. Error-reported connection information is
not authentication, delivery or commit proof and never authorizes replay.
The full local and new exact-source hosted native gates are required before
accepting this candidate; the bf9509 cohorts prove only their unchanged source.

At exact0bbc4db,
[CI39117045361](https://github.com/marshallr12/s3drive/actions/runs/39117045361)
passed the early Windows CLI/TLS and7-unit/10-HTTP/9-TLS controls plus standalone
ordinary-account startup and snapshot checks, then failed the native32-flush
burst at step25. The Info request returned UnexpectedEof/native1117 before its
body was polled. Both connector candidate and error-reported connection were
loopback client57583/server57240; that matches server connection185's header
timeout after5039ms, with read0/write0/flush1/dispatched=false. This is an actual
client-error/server socket match, beyond the earlier merely nearby empty sockets.

The failure-only fresh CLI also refused after5206ms while both owned processes
remained alive. A subsequent server connection186 expired its header deadline
with read0, but its exact association to that CLI was not independently logged.
Do not attribute the failure solely to the native callback client's runtime.
These observations narrow the failure boundary; they do not establish its
cause, whether any preceding upload committed or permission to retry. Later
Windows workspace/lifecycle/paired/reference/broad gates were skipped. This
candidate is not accepted and main remainsbf9509. Preserve the existing assertions
and deadlines while investigating; see the [dated raw evidence](evidence/authority-snapshot-platform-20261018.json).

### Next transport investigation boundary (proposal, 2026-10-18)

Resolved reqwest0.13.5 `ClientBuilder::connector_layer` requires the layer to
return its exact sealed `Conn` type. The observer can inspect that connection's
public metadata, but cannot substitute an I/O wrapper or attach custom metadata
through a changed return type. The connection's fields and construction are
private. Do not assume this hook supports client plaintext I/O counters, or
enable verbose HTTP logging that could disclose the bearer/request. The resolved
[connector-layer signature](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html#method.connector_layer)
and local source were inspected2026-10-18. This is an API constraint, not an
explanation of the native failure.

Evaluate a standalone per-RPC Hyper HTTP/1 connection with Rustls, where one
owned future polls both the connection driver and the single request/response,
without the legacy pool/independently spawned driver. This is a candidate design
to test, not a demonstrated correction. Inspect the resolved handshake,
request-channel, cancellation, TLS and platform-verifier contracts before code.
First prove independent HTTP/TLS peers, exact authenticated JSON/framing and
bounded response reads; expose only fixed stages and sanitized I/O counters.
Preserve numeric-loopback HTTP restrictions, hostname verification, explicit
private roots replacing platform trust, no key logging/proxy/redirect/retry,
2-second connection and10-second absolute caller deadline, and unknown outcomes
after cancellation/lost replies. No deadline extension or replay workaround.

Any replacement candidate must retain the behavioral fault/concurrency/receipt
controls, then pass real Linux and Windows native/startup/reference acceptance
and a source-identical repeat. A passing standalone client or a pure multipart
planner cannot establish that this transport resolves the correlated failure.

### Test-only owned exchange control (candidate, 2026-10-18)

`crates/s3drive-authority/tests/owned_exchange.rs` now implements that small
study, leaving `WireClient` and production routing unchanged. Hyper's client
feature is declared explicitly; package versions and the lockfile are unchanged.
The first request is queued before polling its HTTP/1 driver, using the pinned
Hyper1.12.0 one-buffered-request behavior. The request, driver, TCP/TLS stream
and response-body consumer remain inside the same exchange future. A completed
driver may already have queued a full response or body error; the consumer is
drained under the same deadline. No driver task or pooled socket survives the
exchange. This is an implementation study, not an established failure cause.

Seven local tests passed against independent ephemeral HTTP/TLS peers:32 exact
authenticated UTF-8 JSON requests on32 distinct transports per protocol;
lost replies,307 refusal and truncated/oversized replies without replay;
chunked replies exactly8MiB and one byte over; wrong roots/names; stalled TLS;
cancellation after full request receipt; and stalled partial responses. The
TCP/TLS handshake limit is2 seconds and the whole exchange limit10 seconds
after polling starts. Both protocols exercised the full10-second stalled-body
deadline. Declared body lengths, Host, bearer, content type, close policy and
all request bytes are checked independently. Replies need no peer close header.

The initial cancellation control expected graceful EOF. A measured TLS reset
showed that dropping an owned socket need not perform graceful shutdown. The
passing control accepts only EOF/UnexpectedEof or ConnectionReset as closure;
other errors, timeouts and extra bytes fail. The fixture joins or aborts/reaps
its server task before returning, and observes no extra transport during its
bounded post-exchange check. It commits no authority/S3 effect, so request receipt
and socket closure never establish a durable mutation or permission to replay.

The prototype uses generated explicit roots, a synthetic token and fixed local
endpoints. It does not implement platform trust, production URL/private-file
validation, JSON/volume/receipt checks or foreign-thread/caller-queue deadline
integration. Those contracts must be carried over before a production transport
candidate, followed by native and unchanged-source repeat gates. The full local
gate passed:7 owned-exchange controls, existing7 authority unit/10 HTTP/9 TLS,
17 core and274 state tests, other workspace suites, lint/build and57-document
book checks. No production transport or native correction is accepted from this
local result; see the [dated source and scope record](evidence/owned-http1-study-20261018.json).

### Successful TLS closure review (candidate, 2026-10-18)

The pre-rewrite review identified a gap in the original test-only study: reading
all response bytes could win the select and drop an unfinished connection driver.
A successful `read_u8()` peer check also conflated clean zero-byte EOF with TLS's
missing-alert `UnexpectedEof`. HTTP/TLS clients must send a closure alert before
normal closure; fully length/chunk-delimited replies should remain complete despite
incomplete peer closure. [RFC9112 §9.8](https://www.rfc-editor.org/rfc/rfc9112.html#section-9.8)
The pinned Hyper driver invokes transport shutdown, and tokio-rustls emits
`close_notify` there; dropping a pending driver cannot establish this completion.
See the [dated source/review record](evidence/owned-http1-close-20261018.json).

An independent control now holds shutdown pending outside the TLS stream after
an exact complete reply. Against the previous co-poll logic it failed with
“reply returned while TLS shutdown was held.” The revised logic retains the
completed result and polls the unfinished driver within the original absolute
budget. If the driver finishes first, only the pending consumer is drained; a
finished driver is never polled twice. Successful peers leave their write side
open and require `read(&mut [u8;1]) == Ok(0)`, rejecting reset, missing-alert EOF,
extra bytes and timeout. Cancellation/reset controls remain separate.

A second control spends six seconds before completing a reply, then never
releases shutdown. The already complete reply survives cleanup expiry at the
original ten-second deadline, and the owned socket closes. An independent
fourteen-second watchdog rejects granting a new ten-second cleanup budget;
pre-completion expiry still returns `Deadline`. All fixtures are reaped before
asserting their observations. This is a bounded cleanup attempt, not a claim that
an alert can always be sent after transport failure, cancellation or expiry.

An earlier local nine-test iteration discarded a complete reply on cleanup
expiry. Review rejected that result-semantics change before production adoption;
the retained-reply revision replaces it. The first full gate ran on that earlier
iteration and failed its documentation check before the new evidence file was
created; it is superseded. The final source subsequently passed the full local gate, including9 owned
controls,9 CLI restart/diagnostic cases,274 state tests, lint/build and57-document
book checks. Hosted controls remain required. Production `WireClient` is still unchanged;
this correction does not explain or establish a fix for native Windows failures.

The parent multipart source b793ea5 subsequently failed its broad Windows native
existing-file write test in [CI39117793980](https://github.com/marshallr12/s3drive/actions/runs/39117793980).
Info/Request/UnexpectedEof/native1117 with no body-frame poll matched empty-header
server connection24, client64582/server64551, timeout5044ms, read/write0,
flush1 and dispatched=false. Earlier32-flush, lifecycle, workspace and paired
recovery checks passed, but the broader suite finished6 passed/1 failed. Both
other jobs passed. Keep this contradictory evidence and mainbf9509; do not run
that candidate's success-only acceptance checker or promote it.

### Mixed fault/complete-reply diagnostic fixture (candidate, 2026-10-18)

The original study source e00b5e7 passed its seven owned-exchange controls twice
on hosted Windows, then failed an older CLI diagnostics assertion in
[CI39119010538](https://github.com/marshallr12/s3drive/actions/runs/39119010538).
Both complete `Info` replies had already returned sequence0. The server logged
the three induced idle/header/body failures plus `HttpShutdown` on connection5,
read243/write246/flush2 and dispatched=true. The blanket count expected3 and
saw4; later per-socket assertions were not executed in that cohort. This event
is distinct from the correlated empty-header failure, and does not explain it.

The corrected isolated fixture first requires both typed replies to carry the
original volume and sequence. It then consumes exactly one event for each of
the three injected sockets, with exact category, byte count and dispatch state.
Only zero to two remaining shutdown events may belong to distinct connection
ordinals4/5, with positive independently parsed read/write/flush counters,
dispatched=true, canonical loopback metadata, the actual authority server port,
nonzero client ports and no injected socket tag. The server assigns ordinals
on accept; three sequential TLS fixtures precede the two sequential `Info` calls.
This binds fixture order, not independently observed successful-client sockets.

Every `authority TLS` line is inspected, including suppression notices. Unknown
categories, timeouts, missing metadata, duplicate ordinals and all other leftover
events fail. Validator controls reject the historical timeout shape, generic I/O,
foreign ordinals, zero/duplicate counters, wrong port/family, false dispatch,
suppression and injected sockets. Existing private-input redaction and full
reply checks remain. The revised fixture and classifier passed the final full
local gate. Production transport behavior is unchanged; hosted acceptance must
establish this fixture revision's actual Windows result.

### Production carryover audit (2026-10-18; required before replacement)

Source review of `wire.rs` and `Request::validate` found these boundaries. This
table records remaining work, not implemented parity or a new service contract.

| Existing production behavior | Study coverage | Required carryover |
| --- | --- | --- |
| HTTPS or numeric-loopback HTTP, exact path, no URL credentials/query/fragment | Fixed generated local endpoint only | Reuse URL/config validation and test IPv6/Host/SNI and port handling |
| Owner-private token/optional CA files; default platform trust or explicit roots replacing it | In-memory synthetic token and explicit generated roots | Preserve private-file checks, bounded PEM parsing, platform verifier and redacted errors |
| Request validation and4KiB serialized JSON bound before I/O | One known valid payload, peer verifies exact bytes | Keep invalid-input refusal before task/connection and original operation bytes |
| Caller-owned task, foreign-thread support, absolute10-second queue plus RPC limit | Exchange limit starts when polled | Retain outer caller ownership/queue deadline and task failure classification |
| Any successful2xx HTTP status proceeds to bounded JSON parsing | Prototype accepts200 only | Preserve existing2xx behavior; do not silently tighten the accepted response set |
| Reply bound8MiB for declared and streamed length, then typed strict JSON | Raw bounded bytes, including chunked limit controls | Retain typed reply parsing and higher-layer volume/receipt checks |
| Connector/error-chain observations tied to legacy implementation | Dedicated connection owned by exchange | Define accurate direct-I/O diagnostics; no invented legacy metadata or absent-info replay permission |
| Exact unknown-operation recovery, SDK/native/startup/reference gates | Wire receipt/cancellation controls only | Preserve original identities, guards, receipts and every semantic acceptance case |

The resolved rustls-platform-verifier0.7.1 builder extension can construct its
platform verifier with the selected crypto provider; current reqwest uses that
same verifier's constructor for its default-trust path. This is API research from
the inspected local package sources, with hashes in the dated study record;
the prototype does not link or exercise a new verifier path. Actual Linux and
Windows trust behavior still needs its own production-candidate controls.

### Caller deadline and retained result (production proposal, 2026-10-18)

The production audit must include the caller's outer timer, not only the inner
exchange's timer. `WireClient::request` currently waits on an owned task with
an absolute queue/RPC deadline. Simply placing the revised prototype inside that
task would still let the caller timer discard a complete reply while cleanup
is pending. The production replacement must make the completed result available
to the caller at deadline without returning early and abandoning shutdown.

A proposed implementation uses one bounded one-shot result channel between
the response consumer and the caller, separate from sanitized progress fields.
Publish a fully parsed reply there once, keep the socket/driver inside the owned
task, and await bounded cleanup. On cleanup completion or the original caller
deadline, consume the retained result if present; otherwise preserve the existing
classified failure/unknown outcome. Caller cancellation still aborts and reaps
owned work. No second cleanup task, timeout reset, new request identity or replay
is allowed. This proposal is not implemented or accepted by the test-only study.
It needs controls where an exact committed receipt is retained while shutdown
stalls, plus expiry before task dispatch, response loss and cancellation.

Error paths need their own bounded closure policy: receiving an HTTP status or
invalid JSON is not evidence that TLS shutdown completed. Preserve prompt bounded
errors and no replay, while making a closure attempt where the connection is
still usable. Inspect the pinned connection/stream extraction APIs before
choosing that mechanism; do not infer all-path closure from successful peers.

## Bounded failure diagnostics (2026-10-17)

The [same-source Windows failure investigation](authority-startup.md#same-source-repeat-failure-2026-10-17)
adds plaintext byte/flush counts, elapsed HTTP milliseconds and a header-dispatch
flag to established-connection failures. These counters do not establish
authentication or commit, and retain no contents or peer/operation identities.
The existing sixteen-event/one-suppression budget remains; client transport
errors add only a fixed request category. No retry or timeout policy changes.

## Operator setup example

This illustrates a future operator-owned host. It is not an installed service
or a request to change firewall, root, accounts, credentials or startup entries.
Prepare a certificate valid for the hostname clients will use, its private key,
and an owner-private token outside the repository and mounted paths. Then run:

```sh
s3drive authority-serve \
  --directory /home/you/.local/share/s3drive-authority \
  --token-file /home/you/.local/share/s3drive-secrets/authority.token \
  --tls-cert /home/you/.local/share/s3drive-tls/server-chain.pem \
  --tls-key /home/you/.local/share/s3drive-tls/server-key.pem \
  --name-policy portable-collision-v1 \
  --listen 10.0.0.10:43128
```

The readiness endpoint reports the bound address. If it is a wildcard or differs
from the certificate name, clients must use the actual reachable DNS name whose
SAN the certificate covers. Keep the generated volume ID and existing store
bindings. A replacement authority with a different volume is not a continuation
of the old drive. Add these fields to an otherwise valid managed configuration:

```toml
[managed]
authority = "https://s3drive.example.net:43128/v1/namespace"
volume = "11111111111111111111111111111111" # actual existing volume ID
token_file = "/home/you/.local/share/s3drive-secrets/authority.token"
# Only for an explicit private CA/self-signed deployment; otherwise omit:
authority_ca_file = "/home/you/.local/share/s3drive-tls/authority-ca.pem"
# Existing stores/default_store/windows-disposition remain as appropriate.
```

Windows uses absolute local paths and owner/SYSTEM-only ACLs for these files.
The CA file must also remain outside the mount tree. This phase does not add an
installer, sign-in supervision of the authority, certificates, firewall rules,
backup/failover or a hosted account. Those remain deployment requirements.

## Acceptance

Require real local TCP/TLS with independent clients and SQLite, bad trust/name/
expiry/token/volume cases, plaintext refusal, slow handshake/header/body bounds,
no redirects/replay, exact operation recovery after an unread reply, and safe
shutdown. Exercise the real CLI and managed writer through HTTPS plus the SDK
fixture, then both native adapters. Full local and exact-source hosted
Linux/Windows/book gates, reference matrices and startup regressions are required
before publication. Tests on one loopback host do not prove WAN availability,
multi-host routing, remote credential handling, real reboot, Turso or production
deployment. Real Wasabi mutations still require their own selected policy and
authorized fixture scope.

## Local checkpoint

The full local gate passed exit0 (`/tmp/s3drive-remote-gate2.log`): 256 state,
ten HTTP and five TLS contract cases, seventeen SDK cases, CLI/shared-filesystem
HTTPS editing/recovery, formatting, Clippy, build and book checks. The diagram
was rendered and visually checked. An earlier gate process was terminated143
during compilation; its cause is unverified and it is not a passing gate.
Native HTTPS and exact-tip hosted acceptance still remain required. This local
checkpoint does not establish remote deployment or Turso support.
