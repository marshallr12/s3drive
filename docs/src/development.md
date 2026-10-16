# Development and verification

## Build

The workspace pins Rust 1.99.1 in `rust-toolchain.toml`; Cargo dependencies are
locked. Native builds need a C compiler for bundled SQLite. Use MSVC with the
Visual Studio C++ workload on Windows. The P0 adapters require no drivers.

```bash
cargo build --workspace --locked
cargo run --locked -- status
cargo run --locked -- config-check examples/s3drive.toml
cargo run --locked -- resolve examples/s3drive.toml documents/reports/example.txt
cargo run --locked -- init-state --directory .s3drive
```

The initializer creates `metadata.sqlite3` in the given local directory. It is
safe to reopen the scaffold's current schema; future versions and databases
owned by other applications are rejected. It does not contact S3 or cache file
contents. `mount` is intentionally an error and must stay that way until P2/P3.

## Gate

```bash
bash scripts/gate.sh
```

This runs formatting, Clippy with warnings denied, workspace tests, a workspace
build, local documentation-link checks, mdBook build and mdBook tests. It
preserves failures with `set -euo pipefail`. Install mdBook 0.5.4 with
`cargo install mdbook --version 0.5.4 --locked` if needed. The link checker checks
local target files; it does not validate external websites or heading anchors.

On Windows, run the Rust commands directly or through Git Bash. CI defines
native Linux and Windows Rust jobs and a separate Linux book job. Initial CI
does not install WinFsp, mount FUSE, access providers or use cloud credentials.

Current behavioral tests cover namespace isolation/traversal, exact key bytes
and byte limits, unsafe endpoint/config rejection, alias collisions, SQLite
persistence/schema refusal, and CLI failure for unimplemented mounts. These
tests protect real scaffold invariants; they do not simulate a finished engine.

## Read the book locally

```bash
mdbook build docs
mdbook serve docs --hostname 127.0.0.1 --port 3000
```

The output is under `docs/book/` and ignored by Git. Serve on loopback and open
the local URL. No public GitHub Pages deployment is configured. Edit Markdown
under `docs/src/` and keep `SUMMARY.md` synchronized.

## Dependency policy

Only platform-neutral dependencies are compiled today. Add `fuser` under a
Linux target dependency table and any Windows binding under a Windows target
table. Restrict unsafe FFI to an audited adapter module if needed; workspace
code currently forbids unsafe Rust. The AWS SDK remains a P1 candidate rather
than an unused dependency that inflates every build.

Inspect local resolved source before using new dependency APIs. Keep provider
compatibility evidence separate from library version compatibility. Review
licenses before adding/distributing native runtime dependencies. There is no
declared older MSRV: the pinned compiler is the supported development toolchain.

## Evidence boundaries

A green portable test suite means the scaffold works at its implemented scope.
It does not mean the drive mounts, Windows callbacks run, the cache survives
power loss, uploads are crash safe, or arbitrary S3 providers are compatible.
Record the exact source commit, command, host/platform and result when each
future acceptance gate is executed. Refresh the root handoff after a verified
phase so the next session starts from evidence rather than inference.
