# S3Drive

Research and Rust workspace for making one or more S3-compatible buckets appear
as a local filesystem on Linux and Windows.

**Status: research scaffold (P0), not a working drive.** Configuration checking,
bucket/key resolution, and SQLite initialization work. There is no S3 client,
file cache, synchronization engine, or working filesystem mount yet. `mount`
deliberately returns an error. See the [implementation plan](docs/src/implementation-plan.md).

The proposed architecture uses **FUSE on Linux**, **WinFsp on Windows**, a shared
Rust engine, and **SQLite on local disk** for metadata and eventual recovery state.
Start with read-only, explicitly named bucket roots; develop safe writes in later
phases. Windows Cloud Files is a separately evaluated sync-folder alternative.

## Read the research

The [mdBook](docs/src/README.md) contains the [technology comparison](docs/src/landscape.md),
[architecture](docs/src/architecture.md), [filesystem contract](docs/src/filesystem-contract.md),
[recovery design](docs/src/storage-and-recovery.md), and [primary sources](docs/src/sources.md).
Findings were researched on **2026-10-16**. Recommendations and untested assumptions
are distinguished from implemented behavior.

```bash
cargo install mdbook --version 0.5.4 --locked  # only if not already installed
mdbook serve docs --hostname 127.0.0.1 --port 3000
```

Open `http://127.0.0.1:3000` locally. Building the book does not publish it.

## Try the scaffold

Rust 1.99.1 and a native C compiler are required; SQLite is bundled. Windows
builds use the MSVC toolchain and Visual Studio C++ build tools. No driver,
credentials, bucket, or elevated privileges are needed for these commands.

```bash
cargo run --locked -- status
cargo run --locked -- config-check examples/s3drive.toml
cargo run --locked -- resolve examples/s3drive.toml documents/reports/example.txt
cargo run --locked -- init-state --directory .s3drive
bash scripts/gate.sh
```

The two example sources are placeholders. Configuration validation does not
establish that an endpoint or bucket exists. State initialization writes only a
local SQLite database; use a private local directory outside any future mount.

## Workspace

| Crate | Current responsibility |
| --- | --- |
| `s3drive-core` | Validate TOML and resolve virtual paths to bucket/key pairs |
| `s3drive-s3` | Provider capability/revision types; network implementation deferred |
| `s3drive-state` | Versioned local SQLite initialization and ownership checks |
| `s3drive-fuse` | Explicitly unimplemented Linux adapter boundary |
| `s3drive-winfsp` | Explicitly unimplemented Windows adapter boundary |
| `s3drive-cli` | Local developer commands and truthful implementation status |

See [development](docs/src/development.md) for verification scope and
[HANDOFF.md](HANDOFF.md) for the next phase. All crates have `publish = false`.
This is a private research repository; no project distribution license has been
selected. The Windows dependency licensing decision is recorded in
[decisions](docs/src/decisions.md).
