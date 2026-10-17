# Shared authority over HTTPS

Design/research date: **2026-10-17**. Under implementation on
`codex/remote-authority-tls`; no remote service has been provisioned. Accepted
baseline0f6104c has native Linux/Windows managed mutations and the explicit
Windows lifecycle profile. This phase connects that filesystem to authenticated
HTTPS without changing metadata transactions, operation IDs or the S3 layout.

![Linux and Windows keep private local SQLite state and access one Rust metadata service over HTTPS; immutable file bytes travel directly between their S3 clients and Wasabi.](images/remote-authority.svg)

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
