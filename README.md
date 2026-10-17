# S3Drive

Research and Rust workspace for making one or more S3-compatible buckets appear
as a local filesystem on Linux and Windows.

**Status: read-only S3 transport, Linux FUSE and Windows WinFsp implementations.** Configuration
checking, bucket/key resolution, SQLite initialization, S3 listing, metadata
lookup and revision-pinned reads work. Linux `mount` exposes source aliases
through FUSE; see the [Linux contract](docs/src/linux-mount.md) and
[Windows contract](docs/src/windows-mount.md) for native verification scope.
Native read-only acceptance passed on both platforms. Optional [mounted caching](docs/src/storage-and-recovery.md)
uses bounded SQLite snapshots with checksums, eviction, pins and recovery tests.
Each new open still authorizes online. A [local staging/journal library](docs/src/staged-writes.md)
now preserves immutable write generations. Cache and staging commands share a
bounded async owner; writable mounts and remote commits remain planned.
Local edit plans support patch/truncate and revision-pinned hydration with
restart replay; the native mount adapters still expose read-only files.
An explicit [disposable-key write probe](docs/src/conditional-uploads.md)
can test a provider's conditional PUT behavior; mounted writes remain disabled.
A [local remote-attempt journal](docs/src/remote-journal.md) now preserves
single-use send claims and uncertain outcomes without executing network writes.
A [journal-driven SDK executor](docs/src/upload-executor.md) is tested against
local HTTP fixtures and has an explicit [source/evidence-bound library policy](docs/src/write-policy.md).
No ordinary upload command or writable mount is enabled.
[Linux sign-in startup](docs/src/startup-and-sessions.md) works without systemd,
including MX Linux, and has a native terminal-disconnect acceptance test.
Windows startup uses a per-user scheduled task and an NTFS folder shared across
logins; native cross-logon reads and removal passed on the hosted Windows VM.
Actual workstation reboot, sign-out policy and SSH authentication remain separate checks.
[Wasabi read acceptance](docs/src/wasabi.md) passed against isolated test
objects, including a real Linux mount. Broader provider conformance is pending.
The separately authorized [Wasabi single-PUT probe](docs/src/wasabi.md#conditional-put-acceptance--2026-10-16)
also passed ten checks; all six disposable fixtures were removed. CLI uploads
and mounted writes remain disabled.

The proposed architecture uses **FUSE on Linux**, **WinFsp on Windows**, a shared
Rust engine, and **SQLite on local disk** for metadata and eventual recovery state.
Start with read-only, explicitly named bucket roots; develop safe writes in later
phases. Windows Cloud Files is a separately evaluated sync-folder alternative.

## Read the research

The [mdBook](docs/src/README.md) contains the [technology comparison](docs/src/landscape.md),
[architecture](docs/src/architecture.md), [filesystem contract](docs/src/filesystem-contract.md),
[recovery design](docs/src/storage-and-recovery.md), and [primary sources](docs/src/sources.md).
It also includes architecture images and an evaluation of
[Turso for cross-machine coordination](docs/src/turso-coordination.md).
Findings were researched on **2026-10-16**. Recommendations and untested assumptions
are distinguished from implemented behavior.

```bash
cargo install mdbook --version 0.5.4 --locked  # only if not already installed
mdbook serve docs --hostname 127.0.0.1 --port 3000
```

Open `http://127.0.0.1:3000` locally. Building the book does not publish it.

## Try the scaffold

Rust 1.99.1 and a native C compiler are required; SQLite is bundled. Windows
builds use the MSVC toolchain, Visual Studio C++ build tools and installed
WinFsp 2.1 Developer files. The following local commands need no bucket or
credentials. Installing native filesystem drivers is a separate setup step.

```bash
cargo run --locked -- status
cargo run --locked -- config-check examples/s3drive.toml
cargo run --locked -- resolve examples/s3drive.toml documents/reports/example.txt
cargo run --locked -- init-state --directory .s3drive
bash scripts/gate.sh
```

The two example sources are placeholders. Configuration validation does not
establish that an endpoint or bucket exists. State initialization writes only a
local SQLite database and its ownership lock. The parent must already exist;
use private local disk outside any future mount. Existing directories must be
private; network/FUSE storage and Windows reparse paths are refused. See the
[state directory contract](docs/src/storage-and-recovery.md).

With your own configuration and credentials, the read-only network commands are:

```bash
cargo run --locked -- list s3drive.local.toml documents --max-keys 100
cargo run --locked -- stat s3drive.local.toml documents/report.pdf
cargo run --locked -- read s3drive.local.toml documents/report.pdf --offset 0 --length 4096
```

`read` writes raw bytes to stdout. These commands perform real S3 requests;
see the [provider contract](docs/src/provider-contract.md) for credentials,
limits, pagination and current verification scope. Ordinary CLI upload/delete commands
remain unavailable. For a drive, use
`cargo run --locked -- mount s3drive.local.toml /path/to/empty-owned-directory`
on Linux with `/dev/fuse` and `fusermount3`.
On Windows with WinFsp installed, choose an unused letter:
`cargo run --locked -- mount s3drive.local.toml S:`.

## Workspace

| Crate | Current responsibility |
| --- | --- |
| `s3drive-core` | Validate TOML and resolve virtual paths to bucket/key pairs |
| `s3drive-s3` | S3 reads, scoped probes and policy-bound single-PUT library executor |
| `s3drive-state` | Private SQLite state, bounded clean cache and local staged-generation journal |
| `s3drive-fs` | Shared namespace, revision-bound handles and optional cache orchestration |
| `s3drive-fuse` | Linux read-only FUSE callbacks and signal-driven unmount |
| `s3drive-winfsp` | Windows read-only WinFsp callbacks, native naming/security and teardown |
| `s3drive-cli` | Local commands, read-only S3 tools and foreground OS mounting |

See [development](docs/src/development.md) for verification scope and
[HANDOFF.md](HANDOFF.md) for the next phase. All crates have `publish = false`.
This is a private research repository; no project distribution license has been
selected. The Windows dependency licensing decision is recorded in
[decisions](docs/src/decisions.md).
