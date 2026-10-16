# Development and verification

## Build

The workspace pins Rust 1.99.1 in `rust-toolchain.toml`; Cargo dependencies are
locked. Native builds need a C compiler for bundled SQLite. Use MSVC with the
Visual Studio C++ workload on Windows. Building and portable tests require no
filesystem drivers. The separate Linux mount test needs FUSE. Dev/test builds omit debug symbols to reduce the SDK's build footprint.

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
contents. Linux `mount` now serves the shared engine through FUSE; Windows
continues to return an explicit unsupported error.

The separate `list`, `stat` and `read` commands perform S3 reads with a real
configuration. See [provider contract](provider-contract.md) for their usage
and exact scope; the example configuration still contains placeholder buckets.

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
does not install WinFsp, access providers or use cloud credentials. A separate
explicit Linux native test uses `/dev/fuse` and a local fixture.

Behavioral tests cover namespace isolation/traversal, exact key bytes and byte
limits, endpoint/config rejection, alias collisions, SQLite persistence/schema
refusal, Linux filename projection/collisions, inode identity, and required CLI
mount arguments. Additional real TCP tests
exercise the S3 adapter, including signatures, pagination, malformed responses,
range/validator checks, cancellation and stream deadlines. A subprocess CLI
test verifies explicit configuration precedence using isolated dummy profiles.
The native acceptance command and its coverage are documented in
[the Linux mount contract](linux-mount.md); it is intentionally separate from
portable tests and must be invoked explicitly.

## Read the book locally

```bash
mdbook build docs
mdbook serve docs --hostname 127.0.0.1 --port 3000
```

The output is under `docs/book/` and ignored by Git. Serve on loopback and open
the local URL. No public GitHub Pages deployment is configured. Edit Markdown
under `docs/src/` and keep `SUMMARY.md` synchronized.

## Dependency policy

`fuser` and `nix` live under a Linux target dependency table. Add any Windows
binding under a Windows target table. Restrict unsafe FFI to an audited adapter module if needed; workspace
code currently forbids unsafe Rust. The AWS SDK now implements the read-only
transport, while its mutation APIs are not exposed by S3Drive.

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
