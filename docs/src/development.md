# Development and verification

## Build

The workspace pins Rust 1.99.1 in `rust-toolchain.toml`; Cargo dependencies are
locked. Native builds need a C compiler for bundled SQLite. Use MSVC with the
Visual Studio C++ workload on Windows. Windows builds now require WinFsp 2.1 Developer files;
installing the runtime is separate from starting a mount. The separate Linux mount test needs FUSE. Dev/test builds omit debug symbols to reduce the SDK's build footprint.

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
has a WinFsp implementation with separate native read-only acceptance evidence.

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
native Linux and Windows Rust jobs and a separate Linux book job. CI installs a pinned WinFsp runtime/SDK and runs an explicit native Windows
mount test; it does not access providers or use cloud credentials. A separate
explicit Linux native test uses `/dev/fuse` and a local fixture.

Windows testing runs remotely in the private repository's GitHub Actions
`windows-latest` VM, not by emulation or cross-compilation on the MX Linux
workstation. The 2026-10-16 run logs identify Windows Server 2025
(`10.0.26100`, image `windows-2025-vs2026`). This is native Windows driver/API
evidence, not acceptance on a user's Windows 10/11 desktop or a rebooted PC.
The local Linux workstation separately runs its real FUSE and non-systemd
startup checks.

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

Linux startup has another explicit native gate:

```bash
python3 scripts/test-linux-session.py
```

It requires Python 3.9+, `tmux`, `gio` (Debian/MX package `libglib2.0-bin`),
FUSE and the built CLI. It uses isolated registration/profile files and a real
mount with a synthetic root; it neither changes the user's startup nor contacts
a provider. It does not require systemd. See [startup evidence](startup-and-sessions.md)
for the distinction between terminal loss and actual reboot/logout acceptance.

The Windows native mount gate also invokes `scripts/test-windows-session.ps1`
while its local HTTP fixture is running. Use an elevated disposable Windows
runner. The harness creates its own ordinary account, grants/removes that
account's batch-logon right, registers startup as that account and checks access
from independent logons. It enables Task Scheduler's operational event log on
the disposable runner for failure diagnostics. Passwords exist only in memory
and a redirected input pipe; the fixture S3 profile uses dummy credentials.
The harness removes its account, task, mount and private directory afterward.

## Read the book locally

```bash
mdbook build docs
mdbook serve docs --hostname 127.0.0.1 --port 3000
```

The output is under `docs/book/` and ignored by Git. Serve on loopback and open
the local URL. No public GitHub Pages deployment is configured. Edit Markdown
under `docs/src/` and keep `SUMMARY.md` synchronized.
Original architecture images are committed under `docs/src/images/`.
`python3 scripts/draw-architecture.py` regenerates them using Pillow and DejaVu
fonts; those optional authoring dependencies are unnecessary to build the book.

## Dependency policy

`fuser` and `nix` live under a Linux target dependency table. Add any Windows
binding under a Windows target table. Restrict unsafe FFI to an audited adapter module if needed; workspace
code forbids unsafe Rust except the reviewed Windows native API module. The AWS SDK now implements the read-only
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
