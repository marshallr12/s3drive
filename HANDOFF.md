# S3Drive handoff

Updated: 2026-10-16. Current phase: P1 read-only provider transport; live-provider
conformance is still pending. P0 research/scaffold is complete.

## Goal and scope

Enable S3-compatible storage to appear as local drives/filesystems on Linux and
Windows using Rust. This initial delivery researches the approach, creates a
Rust workspace and mdBook, and publishes a private `marshallr12/s3drive` GitHub
repository. Working mounts and network storage are later phases.

## Current implementation

- Six crates separate core/configuration, S3 transport/evidence types, SQLite state,
  Linux and Windows adapter boundaries, and the CLI.
- `config-check`, `resolve`, `init-state`, and `status` are operational local
  commands. `list`, `stat`, and `read` now perform real read-only S3 requests.
  `mount` still returns an explicit unimplemented error.
- The AWS SDK adapter implements paginated listings, HEAD and range reads with
  revision-bound handles, exact response checks, sanitized errors and deadlines.
  Explicit source endpoint/region/profile selection overrides conflicting
  ambient AWS settings. Opt-in HTTP is restricted to numeric loopback endpoints.
- Source configuration fingerprints are implemented, but do not identify the
  authenticated principal; do not populate the cache table without adding the
  persistent identity/migration boundary described in the book.
- SQLite schema 1 has an application ID, a reserved cache metadata table, WAL,
  FULL synchronous mode and foreign keys. No cache engine or upload journal yet.
- The book documents primary-source research, proposals, decisions, failure
  semantics, security/operations and an implementation plan through packaging.
- CI defines native Linux/Windows Rust jobs and a Linux mdBook job. Network tests
  use local TCP fixtures and dummy credentials only; no real buckets, drivers,
  mounts or public Pages deployment.

## Verification

Local full gate passed on 2026-10-16 with Rust 1.99.1, bundled SQLite 3.53.2 and
mdBook 0.5.4: formatting, Clippy with warnings denied, all 25 behavioral tests,
workspace build, local documentation links, book build and book tests. The CLI
status and two-source resolution example were also exercised in P0. P1 adds
12 SDK/TCP tests and a subprocess CLI test for configuration precedence. The
current gate log is `/tmp/s3drive-p1-gate.log` (ephemeral, not committed).

Check [GitHub Actions](https://github.com/marshallr12/s3drive/actions) for native
Linux/Windows CI on the pushed commit. Do not infer Windows mount or S3
compatibility from portable CI.

## Next phase

Read the [implemented provider contract](docs/src/provider-contract.md),
[plan](docs/src/implementation-plan.md), and [decisions](docs/src/decisions.md).
Provider selection was asked asynchronously and remains open. Do not read real
credential files or contact real buckets as a substitute for an isolated test.
Add no mutating probes to read-only mount startup.

Next independent work is the shared namespace and Linux read-only FUSE adapter,
using the existing S3 transport with a local fixture. This host has `/dev/fuse`
and `fusermount3` (observed 2026-10-16; refresh before mounting). Verify actual
kernel reads/unmount. Then implement and natively test the Windows adapter.

Key decisions: direct-object layout, alias roots, read-only first, local SQLite,
proposed FUSE/WinFsp adapters, implemented AWS SDK read transport. Binding/distribution
licensing and drive-versus-sync-folder preference remain open before P3/P6.

## First commands on resume

```bash
git status --short --branch
bash scripts/gate.sh
cargo run --locked -- status
mdbook serve docs --hostname 127.0.0.1 --port 3000
```

No real bucket credentials or user data are present; credential strings in tests
are dummy fixtures. Preserve the separation between implemented read tools and
the planned virtual filesystem. The overall goal is not complete.
