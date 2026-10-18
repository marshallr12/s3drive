# Owned authority client

Research and implementation date: **2026-10-18**. This is a production-path
candidate on `codex/owned-authority-client`, based on the reviewed test-only
checkpoint ea9342a. Native Linux/Windows and unchanged-source repeat acceptance
remain required. Main's accepted checkpoint remains bf9509. The historical
Windows empty-header failure is not explained or declared fixed by this design.

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
cases, workspace/build/lint and58-document book checks. Hosted native/startup/
reference and repeat acceptance remain pending in the [dated evidence](evidence/owned-authority-client-20261018.json).
A source-identical hosted repeat is required before any reliability acceptance.
Passing portable tests does not explain the historical Windows failure.

Large-file/multipart execution, approved real managed Wasabi writes, independent
hosts/partitions, history continuity/fencing, actual desktop startup/reboot and
packaging remain full-goal work. This client change performs no real provider
requests or account/startup/hosting registration.

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
