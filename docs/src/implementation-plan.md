# Phased implementation plan

Each phase must leave a buildable tree, explicit behavior contract and durable
verification evidence. P0 is complete; P1 has a read-only transport and local
wire tests plus scoped live Wasabi read evidence. P2 implements a shared
read-only engine and Linux FUSE adapter with a separate native acceptance gate.
P3 implements Windows WinFsp, with native read-only acceptance verified.
The user's startup/session requirements add P3.5 before caching: Linux must
support MX without systemd, and both platforms must retain a mount after its
launching SSH terminal disconnects. See [startup/session design](startup-and-sessions.md).
P4a now adds credential-bound read handles and a SQLite binding registry with
migration. Optional mounted clean caching is implemented and natively tested.
P5a1 adds local staged-generation storage, and P5a2a shares the async cache/journal
owner. P5a2b implements local file-edit plans and pinned-base hydration; remote commit
and writable OS callbacks remain subsequent phases.

| Phase | Deliverable | Completion evidence |
| --- | --- | --- |
| P0 — research and scaffold | This book, workspace, local CLI, SQLite bootstrap, private Git repository | Local gate passes; pushed source matches local commit; repository privacy verified |
| P1 — provider contract | Read-only S3 client, bounded list/head/range APIs, source identity, error model and provider evidence records | Deterministic HTTP contract tests plus an explicitly selected test bucket/provider; pagination, signed requests, exact ranges and revisions verified |
| P2 — Linux read-only drive | Shared namespace engine and FUSE callbacks | Real mount; directory traversal and random/sequential reads match remote bytes; no mutation calls; clean unmount and cancellation |
| P3 — Windows read-only drive | WinFsp binding decision and Windows adapter reusing P2 core | Native MSVC build and Windows runtime tests; Explorer/PowerShell access; stable names/handles and clean removal |
| P3.5a — Linux session startup | Detached per-user supervisor, XDG desktop entry and explicit shell login hook; no systemd dependency | Real FUSE mount, simultaneous starts, desktop-entry parsing, terminal loss, new-session access and safe removal |
| P3.5b — Windows session startup | Stable mount path shared by the account's desktop/SSH logons; backend independent of the launching session | Native folder mount, independent logon tokens, startup/removal, disconnect and reboot/sign-in checks |
| P4a — cache prerequisites | Credential-bound read handles and schema 2 source registry; preserve unbound legacy rows | Rotation/recreation/authorization tests, migration rollback and binding isolation |
| P4b1 — local ownership | Private local directory and exclusive lock retained for SQLite's lifetime | Competing processes, forced owner termination, link/ACL refusal and ordinary Windows account access |
| P4b2a — clean-cache storage | Bounded SQLite BLOB worker, integrity checks, LRU and pins | Native storage tests, forced process death, SQL disk-full rollback and migration |
| P4 — cache and local recovery | Revision-aware disk cache, eviction, pinning, single-process ownership, state migrations | Cache isolation, disk-full, stale revision, offline miss and restart tests; no dirty data exists yet |
| P5a1 — local generation store | Immutable chunked payloads, manifests, local heads and pending intent | Native state tests, nine process-kill boundaries, lost acknowledgments, corruption and SQLITE_FULL rollback |
| P5a2a — shared async owner | Cache/journal commands on one connection and lock; bounded queue, uncertain outcomes and immutable snapshot handles | Native storage concurrency, queue pressure, cancellation/response loss, worker failure, draining/close and disk-full tests |
| P5a2b — file-edit plans | Patch/truncate, sparse parent copying, pinned S3 hydration, durable intent-key recovery and immutable read-your-writes snapshots | Deterministic 120-step model, process-kill/response-loss/restart recovery, conflicting/concurrent plans, quota and changed/short/stalled base tests; real SDK HTTP hydration |
| P5b1 — conditional transport probe | Private single-attempt PUT/observation and an explicit six-key, source-bound probe plan/run/cleanup CLI | Real SDK/TCP tests for lost responses, enforced/ignored guards, races, corruption and guarded fixture disposal; live provider evidence requires separately authorized execution |
| P5b2a — durable remote-attempt records | Schema 6, single-use send claims, uncertain-result recovery and verified-parent guards on the shared async owner | Process-kill boundaries, lost owner reply, no duplicate permission, exact observation matching, newer dirty-head preservation, quota/disk-full/corruption/migration tests; no network executor |
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
is not an authenticated account identity. P4a adds a per-credential-generation
binding after a successful HEAD and migrates the reserved P0 table; persistent
cache adoption must use that binding plus key/revision/size, not the config hash
alone. This does not establish a provider-global account ID.

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

### P4 progress and remaining acceptance

P4a implements an opaque digest of the source configuration and exact credential
generation used for HEAD. The same generation signs the handle's GET requests;
new opens resolve credentials again and must complete an online HEAD. Tests
cover rotation, stable binding across reader recreation, denied opens, discovery
timeouts and private error/Debug output. SQLite schema 2 registers bindings,
rejects conflicting registrations and preserves schema 1 rows in a separate
legacy table. Migration rollback and restart are tested. See
[storage and recovery](storage-and-recovery.md) for the implemented boundary.

P4b1 adds private state directories on Linux and Windows, an exclusive process
lock, and a managed SQLite entry point used by `init-state`. It refuses unknown
entries and unsafe paths before SQLite use. Tests cover contention, process
death and committed-state recovery, links and permissions/ACLs. The native
Windows fixture also checks state access across logons. See the
[ownership contract](storage-and-recovery.md) for supported local filesystems.

P4b2a implements bounded small-file BLOB storage with checksum/revision keys,
eviction/pinning and a bounded database worker, using the owned directory.
Storage tests cover quota pressure, corruption, restart, forced termination and
SQLITE_FULL rollback. P4b2b now serves these snapshots through optional mounted caching.

P4b2b connects the worker to the shared engine and optional `[cache]` configuration.
Each new open authorizes online before cache access. Native Linux/Windows gates
include cache hits, immutable old snapshots/fresh revisions, static credential
rotation, pinned quota pressure, corrupt/missing data, restart/process death and
SQL failure propagation. State tests separately induce SQLITE_FULL and both
adapters map it to no-space. Session fixtures run with cache enabled and preserve
its data on removal. See the storage chapter for defaults and bounds.

Broader workload/performance testing, bulk/sparse caching and live Wasabi cached
reads remain separate acceptance work. New offline opens remain denied. P4's
clean-cache boundary is the prerequisite for P5a staged writes; the upload
journal's application upload execution and mounted mutations remain disabled.
The explicit P5b1 provider probe is a separate disposable-key tool.

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

## P5a local-write foundation

P5a1 implements the [local generation/journal store](staged-writes.md) with
non-evictable SQLite chunks, sparse extents, complete manifests, local-parent
compare-and-swap and idempotent append/seal receipts. Dirty bytes are preserved
on quota, corruption, conflict and process death. There is no sealed-data
garbage collection, upload executor or writable mount yet.

P5a2a now adds the bounded async owner on the same connection as cache work.
It validates request bounds before admission, distinguishes rejected requests
from unknown accepted outcomes, and exposes immutable generation snapshots.
P5a2b now implements patch/truncate/read-your-writes plans, hydration from a pinned
base and a deterministic in-memory storage failure model. Schema 5 maps complete
edit intent keys to generations; lost begin replies recover the same builder.
The SDK hydration fixture checks real conditional GETs with an exact Unicode key.
P5b must verify provider
preconditions and reconcile uncertain remote results before P5c exposes writes.
P5b1 prepares the [scoped conditional-write probe](conditional-uploads.md).
P5b2a adds the [local attempt journal](remote-journal.md), preserving single-use
claims, uncertain outcomes and verified-parent ETag guards. It provides no
application upload executor, remote flush or dirty-byte reclamation yet.
Credential-generation rebinding and unresolved-attempt/supersession resolution
still need explicit policies; a pending local record is not permission to upload.
