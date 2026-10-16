# Phased implementation plan

Each phase must leave a buildable tree, explicit behavior contract and durable
verification evidence. P0 is complete; P1 has a read-only transport and local
wire tests plus scoped live Wasabi read evidence. P2 implements a shared
read-only engine and Linux FUSE adapter with a separate native acceptance gate.
P3 implements Windows WinFsp, with native read-only acceptance verified.
Broader provider semantics, persistent cache identity/migration and writes
remain unverified or unimplemented.

| Phase | Deliverable | Completion evidence |
| --- | --- | --- |
| P0 — research and scaffold | This book, workspace, local CLI, SQLite bootstrap, private Git repository | Local gate passes; pushed source matches local commit; repository privacy verified |
| P1 — provider contract | Read-only S3 client, bounded list/head/range APIs, source identity, error model and provider evidence records | Deterministic HTTP contract tests plus an explicitly selected test bucket/provider; pagination, signed requests, exact ranges and revisions verified |
| P2 — Linux read-only drive | Shared namespace engine and FUSE callbacks | Real mount; directory traversal and random/sequential reads match remote bytes; no mutation calls; clean unmount and cancellation |
| P3 — Windows read-only drive | WinFsp binding decision and Windows adapter reusing P2 core | Native MSVC build and Windows runtime tests; Explorer/PowerShell access; stable names/handles and clean removal |
| P4 — cache and local recovery | Revision-aware disk cache, eviction, pinning, single-process ownership, state migrations | Cache isolation, disk-full, stale revision, offline miss and restart tests; no dirty data exists yet |
| P5a — staged writes | In-memory storage test double plus immutable local generations and durable journal | Fault injection at every local boundary; acknowledged local state survives restart; network mutation remains disabled |
| P5b — conditional remote commits | Create/replace and multipart recovery; explicit flush contract | Provider preconditions verified; two-writer conflict tests; crash/timeout replay never overwrites unseen edits |
| P5c — desktop mutations | Safe delete, documented rename behavior, open-handle semantics, real application workflows | Delete races, save/replace, concurrent open, unmount failures and data-loss tests on both OSes |
| P6 — user application | Diagnostics, per-user startup, signed packaging/update process, optional tray UI | Clean-machine install/uninstall, missing-driver handling, upgrade/recovery, user-visible status |

## P1 concrete tasks

1. Wasabi is the selected first provider; a dedicated test bucket/prefix is configured. Add AWS SDK client
   setup with explicit per-source endpoint/region/addressing/profile precedence.
   Pin and inspect resolved APIs. Keep credentials out of fixtures.
2. Establish a storage error taxonomy and streaming/cancellation contract.
   Build deterministic tests for transport errors, authorization versus missing,
   pagination tokens, range lengths and response validators.
3. Implement a bounded list/head/range spike. Include empty objects, Unicode,
   literal percent characters, folder markers, `a` versus `a/b`, and multiple
   pages. Preserve remote key bytes and reject ambiguous OS projections.
4. Design source IDs/config fingerprints and invalidate old cached metadata on
   reconfiguration. Migrate P0's reserved table before it stores live data.
5. Record provider facts as `unknown`, `unsupported`, or `verified`, with endpoint,
   bucket type, software version where known, test version, timestamp and scope.
   A successful GET cannot establish conditional-write support.

Mutating capability probes require a dedicated disposable prefix and explicit
test configuration; they must not run automatically during a read-only mount.
Never test collision or overwrite behavior on ordinary user objects.

### P1 progress

Implemented: per-source SDK configuration, paginated LIST, HEAD, bounded range
reads, revision-bound handles, sanitized errors, operation deadlines, a
configuration fingerprint, and `list`/`stat`/`read` CLI commands. The local TCP
contract tests exercise the real SDK HTTP path with dummy credentials. See
[the implemented contract](provider-contract.md).

Live Wasabi LIST/HEAD/GET, pagination, exact full/range/EOF bytes and a real
Linux mount passed with three explicitly approved disposable fixtures. All
were removed afterward. The test found a form-style listing URL dialect;
the explicit source option and regression are documented in [Wasabi evidence](wasabi.md).
Stale-validator and versioned reads, write capabilities and broader
real-service coverage remain unverified. The configuration hash
is not an authenticated account identity; persistent cache adoption must add
that identity and migrate the reserved P0 table before storing live cache data.

### P2 progress

Implemented and locally verified on 2026-10-16: shared bounded namespace,
Linux FUSE callbacks, real kernel reads from two fixture buckets, pagination,
Unicode/literal names, open-revision conflicts, rejected mutations, external
unmount and SIGTERM cancellation during a stalled GET. The native gate remounts
and verifies cleanup. See [Linux contract](linux-mount.md) for the exact scope.
The expanded native Unicode corpus preserves 11 filenames plus a Unicode
directory, including distinct composed/decomposed accents and supplementary
emoji. Large-scale performance, desktop applications and cache are not covered
by this isolated gate. Live Wasabi reads have separate evidence.

### P3 progress

Implemented: WinFsp callbacks using the shared engine, native Windows name
comparison and collision rejection, owner read-only security descriptors,
revision-pinned reads, bounded handles/directory snapshots, and cancellation.
Native CI passed file reads, sharing, rejected mutations, PowerShell,
owner ACLs and Explorer's Shell namespace. The full acceptance gate also passed
the upstream volume-information test, expanded Unicode corpus, normal removal
and cancellation during a stalled GET. See [Windows contract](windows-mount.md)
for the exact source/run and exclusions. P3 establishes the first read-only
milestone; caching, safe writes and desktop packaging remain separate phases.

## Test matrix

| Layer | Required scenarios | Where it runs |
| --- | --- | --- |
| Pure Rust | Path containment, key identity, case collisions, handle lifetime, state machines | Linux and Windows CI |
| SQLite/cache | Migration, restart, full disk, process ownership, file/DB ordering | Native local disk tests |
| HTTP adapter | Malformed/error responses, pagination, partial reads, cancellation, uncertain completion | Deterministic local test server |
| Provider | AWS S3 general-purpose bucket plus the user's actual compatible provider | Isolated opted-in test buckets |
| Linux mount | Kernel callbacks, large trees, concurrent reads, signals and unmount | Linux host with `/dev/fuse` |
| Windows mount | WinFsp callbacks, sharing, names, Explorer, security and teardown | Windows host/VM with driver |
| Recovery | Kill/restart at every durability boundary, external writer, credentials expired | Fault-injection runner with isolated objects |
| Desktop compatibility | Real open/edit/save/replace sequences in chosen applications | Both OSes in P5c |

Use an emulator for deterministic behavior, not as proof of all provider
semantics. An AWS-only pass does not certify third-party S3 support. Keep a
provider/OS support matrix containing actual versions and evidence locations.

## Stop conditions for mutations

Keep writable mounts disabled if conditional operations are unavailable or
unverified, if staging cannot guarantee the selected local durability, or if
the recovery journal cannot identify an uncertain outcome. Direct-mode rename
must remain unsupported unless its limitations and concurrency behavior are
deliberate and tested. Full distributed filesystem semantics require a revised
architecture, not a flag labeled “POSIX compatible.”

The first usable product milestone is **P3: read-only mounts on both platforms**.
A general editable desktop drive requires P5c, while a Google Drive-like
polished application also needs P6. Choose Cloud Files instead of the P3 WinFsp
surface only after resolving the drive-versus-sync-folder product preference.
