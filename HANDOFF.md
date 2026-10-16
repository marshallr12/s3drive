# S3Drive handoff

Updated: 2026-10-16. Current phase: P0 research and scaffold.

## Goal and scope

Enable S3-compatible storage to appear as local drives/filesystems on Linux and
Windows using Rust. This initial delivery researches the approach, creates a
Rust workspace and mdBook, and publishes a private `marshallr12/s3drive` GitHub
repository. Working mounts and network storage are later phases.

## Current implementation

- Six crates separate core/configuration, S3 evidence types, SQLite state,
  Linux and Windows adapter boundaries, and the CLI.
- `config-check`, `resolve`, `init-state`, and `status` are operational local
  commands. `mount` returns an explicit unimplemented error.
- SQLite schema 1 has an application ID, a reserved cache metadata table, WAL,
  FULL synchronous mode and foreign keys. No cache engine or upload journal yet.
- The book documents primary-source research, proposals, decisions, failure
  semantics, security/operations and an implementation plan through packaging.
- CI defines native Linux/Windows Rust jobs and a Linux mdBook job; no cloud
  credentials, bucket operations, drivers, mounts or public Pages deployment.

## Verification

Local full gate passed on 2026-10-16 with Rust 1.99.1, bundled SQLite 3.53.2 and
mdBook 0.5.4: formatting, Clippy with warnings denied, all 11 behavioral tests,
workspace build, local documentation links, book build and book tests. The CLI
status and two-source resolution example were also exercised. The gate log for
this session is `/tmp/s3drive-p0-gate.log` (ephemeral, not committed).

Check [GitHub Actions](https://github.com/marshallr12/s3drive/actions) for native
Linux/Windows CI on the pushed commit. Do not infer Windows mount or S3
compatibility from portable CI.

## Next phase

Read [P1](docs/src/implementation-plan.md) and [decisions](docs/src/decisions.md).
Select a provider and isolated test prefix; build the read-only SDK adapter
with deterministic transport tests and explicit source identity. Add no
mutating probes to read-only mount startup. Then implement the Linux mount,
followed by the native Windows adapter spike and read-only drive.

Key decisions: direct-object layout, alias roots, read-only first, local SQLite,
proposed FUSE/WinFsp adapters, proposed AWS SDK transport. Binding/distribution
licensing and drive-versus-sync-folder preference remain open before P3/P6.

## First commands on resume

```bash
git status --short --branch
bash scripts/gate.sh
cargo run --locked -- status
mdbook serve docs --hostname 127.0.0.1 --port 3000
```

No real bucket credentials or user data are present. Preserve the separation
between implemented P0 behavior and the planned virtual filesystem.
