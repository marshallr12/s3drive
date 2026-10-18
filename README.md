# S3Drive

Research and Rust workspace for making one or more S3-compatible buckets appear
as a local filesystem on Linux and Windows.

**Target selected: [managed layout with stronger coordination](docs/src/managed-layout.md).**
A shared Rust metadata service owns the authoritative SQLite namespace; each
client keeps its own private SQLite cache/journal and transfers immutable file
generations directly to S3. The [publication library](docs/src/managed-publication.md)
verifies blobs before metadata CAS and local acknowledgment.

Managed native reads, policy-bound writes, directory/file creation and
[version6 rename/replacement and Linux removal](docs/src/managed-native-mutations.md)
have accepted Linux/Windows checkpoints. Windows deletion uses the explicit
[version7/POSIX lifecycle profile](docs/src/native-windows-lifecycle.md), with
measured WinFsp limitations. [HTTPS](docs/src/remote-authority.md) and
[store/configuration bootstrap](docs/src/managed-bootstrap.md) are accepted
through main `f8c4f58`. Existing direct mounts remain available without
implicit migration. Multipart/large files, reclamation, full application
compatibility and real remote deployment remain work.

**Earlier hosted checkpoint:** [coordinator supervision and TLS diagnostics](docs/src/authority-startup.md)
passed hosted Linux/Windows native, paired recovery and strict reference gates
at `f8c4f58`, [CI39097275707](https://github.com/marshallr12/s3drive/actions/runs/39097275707).
Exact uncertain identities, dirty conflicts, Unicode/binary/empty reads and
same-supervisor child replacement passed. Earlier timeout causes remain open.

**Current work:** the user selected immutable chunks and manifests for large
managed files. The [offline planner](docs/src/chunk-plans.md) passed its own
Linux/Windows hosted cohort. The [chunk object upload bridge](docs/src/chunk-object-uploads.md)
connects prepared trees to exact create-only operations and verified child
receipts. The [manifest authority contract](docs/src/manifest-root-authority.md)
adds versioned root metadata and content CAS; client publication/settlement and
mounted large-file activation remain unfinished. A
[complete-tree receipt check](docs/src/chunk-tree-receipts.md) supplies the next
prerequisite by checking every object, including grandchildren. No real user startup, remote hosting or Turso backend is installed.

The shared engine preserves uncertain/conflicting bytes and exact operation
identities. Direct and managed writes require explicit source/prefix policy and
provider evidence; the [scoped probe](docs/src/conditional-uploads.md) never enables
writes automatically. [Wasabi read acceptance](docs/src/wasabi.md), including
Unicode fixtures, and the separately authorized
[single-PUT probe](docs/src/wasabi.md#conditional-put-acceptance--2026-10-16)
passed. Its disposable objects were removed; live managed-provider acceptance
requires a separate scope. No real writable user configuration is installed.

The adapters use **Linux FUSE** and **Windows WinFsp**. Linux sign-in startup
supports MX without systemd. Windows uses an account-owned scheduled supervisor
and NTFS folder path shared across logons. Physical workstation reboot/PAM/SSH,
Windows10/11, multi-host partitions and backup/failover remain separate checks.

## Read the research

The [mdBook](docs/src/README.md) contains the [technology comparison](docs/src/landscape.md),
[architecture](docs/src/architecture.md), [filesystem contract](docs/src/filesystem-contract.md),
[recovery design](docs/src/storage-and-recovery.md), and [primary sources](docs/src/sources.md).
It also includes architecture images and an evaluation of
[Turso for cross-machine coordination](docs/src/turso-coordination.md).
Findings were researched on **2026-10-16–17**. Recommendations and untested assumptions
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
| `s3drive-state` | Private SQLite authority/client state, cache, generations, namespace/publication/open-session journals and exact recovery |
| `s3drive-authority` | Shared Rust metadata service, authenticated HTTP/HTTPS and serialized SQLite transactions |
| `s3drive-managed` | Immutable payload publication, namespace control and durable client reconciliation |
| `s3drive-fs` | Shared namespace, journal-backed file I/O and optional clean cache |
| `s3drive-fuse` | Linux FUSE callbacks and signal-driven unmount |
| `s3drive-winfsp` | Windows WinFsp callbacks, native naming/security and teardown |
| `s3drive-cli` | Local/S3 tools, explicit managed setup, foreground coordinator and OS mounting |

See [development](docs/src/development.md) for verification scope and
[HANDOFF.md](HANDOFF.md) for the next phase. All crates have `publish = false`.
This is a public research repository; no project distribution license has been
selected. The Windows dependency licensing decision is recorded in
[decisions](docs/src/decisions.md).
