# Phased implementation plan

Each phase must leave a buildable tree, explicit behavior contract and durable
verification evidence. P0 is complete; P1 has a read-only transport and local
wire tests plus scoped live Wasabi read evidence. P2 implements a shared
read-only engine and Linux FUSE adapter with a separate native acceptance gate.
P3 implements Windows WinFsp, with native read-only acceptance verified.
The user's startup/session requirements add P3.5 before caching: Linux must
support MX without systemd, and both platforms must retain a mount after its
launching SSH terminal disconnects. See [startup/session design](startup-and-sessions.md).
Broader provider semantics, persistent cache identity/migration and writes
remain unverified or unimplemented.

| Phase | Deliverable | Completion evidence |
| --- | --- | --- |
| P0 — research and scaffold | This book, workspace, local CLI, SQLite bootstrap, private Git repository | Local gate passes; pushed source matches local commit; repository privacy verified |
| P1 — provider contract | Read-only S3 client, bounded list/head/range APIs, source identity, error model and provider evidence records | Deterministic HTTP contract tests plus an explicitly selected test bucket/provider; pagination, signed requests, exact ranges and revisions verified |
| P2 — Linux read-only drive | Shared namespace engine and FUSE callbacks | Real mount; directory traversal and random/sequential reads match remote bytes; no mutation calls; clean unmount and cancellation |
| P3 — Windows read-only drive | WinFsp binding decision and Windows adapter reusing P2 core | Native MSVC build and Windows runtime tests; Explorer/PowerShell access; stable names/handles and clean removal |
| P3.5a — Linux session startup | Detached per-user supervisor, XDG desktop entry and explicit shell login hook; no systemd dependency | Real FUSE mount, simultaneous starts, desktop-entry parsing, terminal loss, new-session access and safe removal |
| P3.5b — Windows session startup | Stable mount path shared by the account's desktop/SSH logons; backend independent of the launching session | Native folder mount, independent logon tokens, startup/removal, disconnect and reboot/sign-in checks |
| P4 — cache and local recovery | Revision-aware disk cache, eviction, pinning, single-process ownership, state migrations | Cache isolation, disk-full, stale revision, offline miss and restart tests; no dirty data exists yet |
| P5a — staged writes | In-memory storage test double plus immutable local generations and durable journal | Fault injection at every local boundary; acknowledged local state survives restart; network mutation remains disabled |
| P5b — conditional remote commits | Create/replace and multipart recovery; explicit flush contract | Provider preconditions verified; two-writer conflict tests; crash/timeout replay never overwrites unseen edits |
| P5c — desktop mutations | Safe delete, documented rename behavior, open-handle semantics, real application workflows | Delete races, save/replace, concurrent open, unmount failures and data-loss tests on both OSes |
| P6 — user application | Diagnostics, startup integration in installers, signed packaging/update process, optional tray UI | Clean-machine install/uninstall, missing-driver handling, upgrade/recovery, user-visible status |

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

### P3.5 progress

Linux implements a Python standard-library launch/supervision helper around the
Rust filesystem. Native tests passed on a host without an active systemd user
manager: real FUSE, `gio` desktop startup, racing duplicate starts, a login hook
in tmux, destruction of that terminal and subsequent process access. Tests also
cover private registration, preservation of existing profile content/mode and
refusal to remove edited startup entries. No real user login profile was changed.

Windows implements an owner-account, password-backed scheduled task with a
shared NTFS directory mount. Native acceptance at `029765e` in
[run 38945436975](https://github.com/marshallr12/s3drive/actions/runs/38945436975)
verifies two distinct authentication sessions reading the same daemon's S3 and
Unicode bytes, owner-only access, duplicate starts and graceful removal.
The disposable CI VM has an explicit exception for an OS-retained locked test
profile; see [verification details](development.md). Actual reboot, desktop
logout policy, SSH authentication and Windows 10/11 desktop checks remain
pending. These are not inferred from the scheduled-task definition.

### Optional coordination investigation

[Turso research](turso-coordination.md) evaluates currently documented remote
transactions, concurrent writes, embedded replicas and newer Sync separately.
Before P5b, decide whether to evaluate a managed object layout with a shared
metadata authority. Keep SQLite local and direct object mode unchanged while
that decision is open. Offline last-push-wins sync cannot decide exclusive
ownership; SQL metadata and S3 uploads need explicit reconciliation.

## Test matrix

| Layer | Required scenarios | Where it runs |
| --- | --- | --- |
| Pure Rust | Path containment, key identity, case collisions, handle lifetime, state machines | Linux and Windows CI |
| SQLite/cache | Migration, restart, full disk, process ownership, file/DB ordering | Native local disk tests |
| HTTP adapter | Malformed/error responses, pagination, partial reads, cancellation, uncertain completion | Deterministic local test server |
| Provider | AWS S3 general-purpose bucket plus the user's actual compatible provider | Isolated opted-in test buckets |
| Linux mount | Kernel callbacks, large trees, concurrent reads, signals and unmount | Linux host with `/dev/fuse` |
| Windows mount | WinFsp callbacks, sharing, names, Explorer, security and teardown | Windows host/VM with driver |
| Startup/session lifecycle | Desktop/SSH startup, duplicate instances, terminal loss, same-account access, uninstall and reboot | Native hosts, including Linux without systemd; separate Windows logon sessions |
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
