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
and opt-in writable OS callbacks have subsequently passed native acceptance.
The user selected managed layout on 2026-10-16; P6m phases now define the target.

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
| P5b2b — journal-driven SDK executor | Single-PUT permit consumption, pinned binding, full-byte observation, four execution slots; production construction gated | Real SDK/TCP lost-response/timeout/cancellation tests, independent-journal create/replace races, process death after server commit and recovery without a second PUT; live provider acceptance/write policy still required |
| P5b2c — explicit library write policy | Source/report-bound factory, exact prefix and byte limits, credential-generation matching and evidence expiry | Strict local evidence parsing, denied targets before send claims, full SDK/TCP tests through the public factory, observation-only recovery after expiry; no live mutations or mount configuration changes |
| P5b2d — generation-specific remote flush | Verified remote receipt plus atomic local acknowledgment/supersession records; retain immutable snapshots and newer pending heads | Lost local replies, restart/process-kill and disk-full rollback, corruption/migration refusal, exact ancestor coverage and SDK/TCP flush without replay; no payload reclamation or writable callbacks |
| P5c1a — native existing-file writes | Explicit version-2 policy-bound mounts, shared SQLite/edit/upload owner, truncate/append/read-your-writes and remote flush for existing files | Real FUSE and WinFsp SDK/TCP workflows, restart recovery, conflicts/uncertainty, exact Unicode bytes and read-only regression gates; no real bucket policy is installed implicitly |
| P5c1b — native file creation | Guarded empty-file creation in existing directories, writable-parent projection and durable namespace merge | FUSE/WinFsp create/exclusive/reopen/restart and competing-create tests; no overwrite fallback |
| P5c1c — direct directories | Archived experiment on `codex/persistent-directories`; superseded by managed choice | Local engine passed; native run 38995625850 failed, helper corrected without claiming acceptance; not merged |
| P6m1 — managed namespace authority | Private SQLite authority, stable IDs, sequence CAS, atomic create/move/remove, operation-result recovery and loopback HTTP | Independent TCP clients, concurrent cycles/creates, lost reply/restart, process death, full disk, migration and limits |
| P6m2 — immutable payload publication | Implemented library: store IDs, reservations, create-only verified blobs, metadata CAS and separate logical acknowledgment | Two-writer SDK/authority races, unknown upload/commit outcomes, retained conflicts; separate approved provider acceptance |
| P6m3a — managed native reads | Explicit version-3 read-only mounts, shared namespace, multi-store pinned payloads and private-token foreground authority CLI | SDK/HTTP/SQLite and real FUSE/WinFsp acceptance passed at `653c804`, run 39002521132; see [contract](managed-native.md) |
| P6m3b — managed native writes | Shared engine and FUSE/WinFsp use authoritative namespace and pinned blobs; portable names, save/replace, open-unlinked handles | Both real native suites plus cross-machine invalidation, failure/recovery and approved provider checks |
| P6m4 — authority deployment/Turso | Authenticated TLS service, migrations, backup/failover; optional pinned Turso backend | Real multi-host/partition/commit-uncertainty/authorization tests; no offline replica ownership decisions |
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
journal now has a policy-bound library executor; mounted mutations remain disabled.
The explicit P5b1 provider probe is a separate disposable-key tool.

### Optional coordination investigation

[Turso research](turso-coordination.md) evaluates currently documented remote
transactions, concurrent writes, embedded replicas and newer Sync separately.
The user selected managed layout with a shared
metadata authority on 2026-10-16. Keep local SQLite and existing direct
configurations; implement P6m without implicit conversion. Offline last-push-wins sync cannot decide exclusive
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
claims, uncertain outcomes and verified-parent ETag guards. P5b2b adds the
[journal-driven executor](upload-executor.md). P5b2c adds its explicit
[source/evidence-bound library policy and factory](write-policy.md). P5b2d adds
[generation-specific remote flush and atomic acknowledgment](flush-receipts.md),
preserving newer pending heads and all snapshots. P5c1a now connects [opt-in existing-file writes](native-writes.md) to both adapters,
with native acceptance on both OSes. No ordinary upload command or dirty-byte reclamation
is enabled. The existing real configuration remains read-only.
The scoped Wasabi single-PUT probe passed with cleanup; no real application
write policy has been installed. Multipart is a separate capability;
the [current research and protocol plan](multipart-plan.md) records provider
limit differences and the remaining durability boundaries.
Credential-generation rebinding and unresolved-attempt/supersession resolution
still need explicit policies; a pending local record is not permission to upload.


## P5c1b native acceptance

[Native regular-file creation](native-creation.md) is implemented for existing
policy-writable directories. Local SDK/SQLite integration, the bound head
catalog test and actual FUSE/WinFsp workflows pass. Exact-tip CI is required
for final publication. No
real Wasabi write configuration or new cloud probe is installed or executed.


## Managed target after the layout decision

[Managed layout](managed-layout.md) is the accepted target. P6m1 implements the
first persistent authority protocol, with local/TCP acceptance recorded in the
handoff. The [P6m2 publisher library](managed-publication.md) now separates logical IDs
from physical create-only keys and joins SDK verification, metadata CAS and
local logical acknowledgment. Native managed reads are implemented; managed writes and a
Turso backend remain subsequent phases. P6m3b must integrate logical heads/edit plans and portable names
rather than reusing direct-key namespace logic. No new cloud
mutation is authorized by the layout selection.


## P6m3a managed native reader checkpoint

Implemented 2026-10-17: the shared filesystem calls the managed Reader for live
namespace lookups and complete, verified immutable payload snapshots. It maps
global stores to local source profiles across buckets. Version 3 requires
read-only mode; writes/cache and direct-key resolution are refused. The local
authority CLI creates private tokens and serves its separate SQLite database.
See [the contract and acceptance commands](managed-native.md).

Next, P6m3b must combine durable logical edit heads with native mutations and
flush/recovery; do not reuse a mutable S3 key as a logical file's identity. Add
portable naming, concurrent save/replace conflicts and retained open-unlinked
snapshots. P6m4 separately deploys/supervises the authority and tests real
multi-host/Turso coordination. Sign-in supervision must include authority
availability and retain MX/no-systemd and Windows cross-logon requirements.


## P6m3b managed writable steps (updated 2026-10-17)

P6m3a now has real native read acceptance. Complete these independently green
steps before claiming full managed filesystem/save support:

1. **P6m3b1: durable logical edit heads — library implemented.** See the
   [contract and recovery acceptance](managed-edits.md). Persist volume/node/store identity,
   the base authority file revision and payload reference separately from each
   physical blob key. Store bounded immutable local edits and intent-key replay
   on the existing SQLite owner. Link an exact sealed logical generation to
   its reservation/publication operation IDs before network effects. Only a
   verified S3 upload, metadata CAS and committed local logical settlement may
   acknowledge it; settlement of an older generation must retain a newer head.
   Acceptance: process death/lost replies at each durable boundary, full-disk
   rollback, corruption/refusal, concurrent local edits and retained conflicts.
2. **P6m3b2: existing-file native editing — native checkpoint accepted.** See [the phase contract](managed-write-plan.md). Add explicit policy-bound managed
   write configuration and a shared local file view, patch/truncate/append,
   local read-your-writes and generation-specific remote fsync. Coalesce local
   writes before publication rather than uploading each patch. An external
   change must not silently replace a dirty base. Add an explicit verified
   refresh path for a clean externally changed file without losing old snapshots.
   Serialize publication per logical file; independent-file writes must avoid
   spurious conflicts from unrelated global sequence changes while hierarchy
   mutations retain their invariant guards. Reconcile
   the same uncertain operation IDs; retain and report conflicts. Acceptance:
   actual FUSE/WinFsp editing, simultaneous readers/writers, external competing
   publication, restart and failed/uncertain flush without a repeated PUT.
3. **P6m3b3: managed namespace and application saves.** Follow [the creation/replacement/save phases](managed-namespace-plan.md); creation/directories passed native Linux/Windows acceptance. Guarded authority entry transactions and the [local acknowledgment/detached publication bridge](managed-unlinked.md) are implemented; [version-6 native integration](managed-native-mutations.md) is the current candidate for rename/replacement and Linux retained-handle removal. The [Windows disposition phases](windows-delete-plan.md) now define the node-wide authority reservation, client lifecycle and native acceptance work. Windows delete-intent/cleanup and full application saves remain required. Add create/mkdir,
   rename-overwrite/replace-save and unlink callbacks with stable open-handle
   identity and a versioned portable naming/collision policy. Keep metadata
   transactions independent of uploads and reference-aware reclamation disabled.
   Acceptance: real application save patterns on both platforms, concurrent
   folder/name edits, open-unlinked files, collisions and recovery. A metadata
   unlink alone must never become an unconditional S3 object deletion.

P6m4 separately supplies authority supervision, authenticated remote deployment,
backup/failover and optional Turso acceptance. Include authority availability in
managed sign-in startup and test MX without systemd, Windows cross-logon access,
SSH/tmux disconnect and actual workstation reboot. These remain requirements,
not behaviors proved by the current local-loopback tests.


### P6m3b3b4b1 — durable client disposition ownership (2026-10-17)

The accepted authority checkpoint is `4bca4ad`, terminal Linux/Windows/book
[run 39026528993](https://github.com/marshallr12/s3drive/actions/runs/39026528993),
including seven native cases per OS and pending-directory cancellation/open.
Accepted at `188e03c`, [run 39028253459](https://github.com/marshallr12/s3drive/actions/runs/39028253459): schema-14 client identity, exact
arm/cancel/commit journaling and atomic logical observation. It derives owned
fences from immutable receipt history, blocks ordinary local admission against
owned reservations, and preserves closure capacity and dirty bytes. See the
[client ownership contract](windows-delete-plan.md#client-ownership-checkpoint-p6m3b3b4b1).

Acceptance requires migration/refusal, identity/ownership corruption, ignored
SQLite updates, real full disk, replay after re-arm, closing quota and killed
client/authority recovery through actual HTTP/SDK. Run the repository gate and
terminal exact-tip Linux/Windows/book CI, preserving all seven existing native
cases and startup acceptance. Windows session checks strictly require schema 14.
Next b4b2 adds pending-owner SDK publication; b4b3 adds durable per-open/node-wide
lifecycle. Native Windows deletion remains refused until b4c acceptance. No live
provider writes, installed startup changes or Turso deployment are part of this
local checkpoint.


### P6m3b3b4b2 — pending-owner publication and retained conflicts (2026-10-17)

Follow [the implementation contract](pending-owner-publication.md). Schema 15
accepts new pending-owner SDK work only with the acknowledged local fence,
frozen bindings and logical operation ownership. Complete publication uses the
existing one-shot physical verification, exact metadata CAS and logical receipt.
Separate retained rejection/conflict records allow explicit resolution of
confirmed failures before cancellation/commit without discarding dirty bytes.
Unknown or temporary failures continue to block closing. Old hashes stay valid.

Acceptance: state admission, integrity, ignored writes, full disk and schema-14
migration/refusal; actual SDK/HTTP killed-client and authority recovery after
lost reserve/upload/metadata/local results, exact PUT counts, binary/empty and
Unicode data, physical and metadata conflicts, retained orphan/rival bytes,
explicit closing admission and later fresh generations. Run the full repository
gate and terminal exact-tip Linux/Windows/book CI, preserving seven native cases
per OS and startup acceptance. Windows startup strictly requires schema 15.
Native Windows deletion waits for b4b3 lifecycle + b4c acceptance. No live Wasabi
mutation, workstation startup/credential change or Turso deployment is included.


### P6m3b3b4b3r — Windows reference prerequisite (2026-10-17)

The owner-publication/conflict checkpoint is accepted at `fd52692`, terminal
[run 39030411026](https://github.com/marshallr12/s3drive/actions/runs/39030411026).
Before implementing deferred authority and per-open lifecycle, follow the
[reference contract](windows-disposition-reference.md): execute eight Rust NTFS
probes and pinned upstream WinFsp legacy create_test on the disposable Windows
VM. Verify open sharing, nonempty deferred directory cleanup, final nonarming
handle and retain observed cross-context cancellation/child-creation results.
These are native reference experiments, not S3Drive deletion acceptance.

Require the repository gate, terminal exact-tip Linux/Windows/book CI, the new
reference step and unchanged seven native cases per OS/startup gates. Review
structured JSON before fixing protocol semantics. Next add deferred authority
state with immutable promotion evidence, then durable client lifecycle and full
native deletion acceptance. Keep schema 15, version 6 and unsupported Windows
delete callbacks unchanged throughout this reference phase.

The first reference run `6e5fef7` / CI 39032499844 passed. Its eight measured
NTFS outcomes are in the book's JSON evidence fixture. The finalized script
compares all fields and rejects missing/duplicate cases; its exact-tip native
run is required. NTFS cross-context observations do not establish WinFsp parity.


### P6m3b3b4b3a — deferred authority and persistent client modes (2026-10-17)

The Windows reference prerequisite is accepted at `e091a3d`, terminal
[run 39033496306](https://github.com/marshallr12/s3drive/actions/runs/39033496306).
Implement the [deferred-removal contract](deferred-removal.md) as an independently
verified protocol/client-journal phase. Schema 16 retains immutable mode events;
PrepareRemoval permits nonempty directories and child operations. TransitionRemoval
keeps the same fence and guarded identity; pending promotion requires emptiness.
Both modes admit only the owner's fully guarded verified content publications.
Local history preserves the original binding and atomic dirty-file observations.
Reserve promotion/closure headroom; reject corruption and migration ambiguity.

Require actual SQLite full/rollback, killed authority owner at each mode boundary,
TCP independent-client race/dropped result/restart and actual SDK client-death
recovery with Unicode/binary/empty files. Local gate plus terminal exact-tip
Linux/Windows/book and unchanged native/reference/startup suites are mandatory.
Native Windows deletion stays unsupported, version 6 unchanged. Next implement
atomic deferred create/owner-fenced move and durable per-open/node-wide lifecycle;
then full Windows native deletion acceptance. Real provider mutation and
workstation startup/credential changes remain outside this phase.


The implementation passed at `43dbdea`, terminal all-green
[run 39036011774](https://github.com/marshallr12/s3drive/actions/runs/39036011774):
167/163 Linux/Windows state tests, seven TCP tests, nine managed-publication cases,
seven actual native cases per OS and unchanged startup/reference gates. The final
documentation tip still requires CI before publication. Next prioritize explicit
same-fence cancel recovery for unresolved mode changes at local quota: prove both
authority orderings and late receipt observation without discarding history or
dirty bytes. Then add atomic deferred create/move and the per-open lifecycle.


### P6m3b3b4b3a2 — unknown mode cancellation recovery (2026-10-17)

Baseline `2d9d479` passed terminal CI 39037304416 and guarded publication.
Implement the [recovery contract](mode-cancellation-recovery.md). Explicit
same-fence cancellation may pass an unknown transition; exact cancellation
history proves epoch closure without inventing the original outcome. Keep
unknown audit rows, dirty bytes and original bindings. Test real authority
quota rejection without a retained result, both authority orderings, late
results and a successor fence, atomic/full-disk failure, barriers, corruption
and actual SDK killed clients. Require local gate and terminal exact-tip
native/reference/startup CI before publication. Then proceed to atomic deferred
create/owner-fenced moves and the full Windows per-open lifecycle.


Local gate passed with 173 state tests, seven authority HTTP tests and 11 managed
SDK cases, including killed owners in both authority orderings. Typed epoch
closure proof is separate from the original unknown result; the audit row is
retained. Publisher refuses a closed unknown transition before HTTP. Schema 16
and configuration 6 remain unchanged. Require terminal exact-commit hosted CI
before publication, then measure deferred rename/multiple DoC references and
implement atomic deferred create/move before the native Windows lifecycle.


### P6m3b3b4b3r2 — deferred create/rename/handle reference (2026-10-17)

Cancellation recovery passed at `0c8f6c7`, terminal CI 39039582208, and was
published privately. Follow the [additional reference plan](windows-deferred-reference.md):
measure six new NTFS cases, preserve eight existing goldens, run four selected
pinned WinFsp legacy disk/network tests and retain source-bound observations.
Then freeze all fourteen records and require exact field comparison plus
terminal native/reference/startup CI before publication. Keep native S3Drive
delete callbacks unsupported. Next atomic deferred create and owner-fenced
rename/move must reflect the measured behavior before per-open integration.


The reference step at `53f09b1` / CI 39041403340 measured all fourteen NTFS
cases and passed four pinned upstream legacy cases. See the cited
[results and implications](windows-deferred-reference.md#measured-ntfs-results-2026-10-17).
Deferred file/directory rename succeeds, create+DoC initially shares, multiple
DoC cleanups can reassert pending and duplicates share one cleanup lifetime.
NTFS extended cancellation is distinct from unsupported WinFsp ON_CLOSE.
Freeze all fourteen scalar observations and actual OS profile; reject malformed
reference data. Require final terminal exact-tip native/reference/startup CI
before publishing. Next atomic create/reserve and owner-fenced move; deferred
target replacement needs its own proof that old cleanup cannot remove the
new name before the full Windows lifecycle is enabled.


### P6m3b3b4b3b1 — atomic deferred create (2026-10-17)

Reference contract accepted at `cc23aa4`, terminal CI 39042534931. Implement the
[atomic deferred create contract](atomic-deferred-create.md): guarded creation,
reservation, immutable origin event and exact result in one authority transaction;
unknown predicted-node barriers and exact acknowledged local ownership. Preserve
closing headroom and old bytes/hashes. Require corruption/full disk/quota/kill,
TCP races and actual SDK binary/empty/Unicode recovery plus all existing native
gates before publication. Then same-fence owner move and the native lifecycle.

Local implementation and full gate passed: 183 Linux state tests, eight HTTP
tests and twelve SDK cases. Creation and deferred ownership are atomic, client
recovery uses the original frozen input, and unknown predicted IDs block further
effects. Native callbacks remain unsupported; require terminal exact-tip hosted
mount/reference/startup CI before publication. The next implementation phase is
same-fence owner rename/move, followed by the persistent per-open lifecycle.


### P6m3b3b4b3b2 — deferred owner rename/move (2026-10-17)

Atomic create accepted at `317a047`, terminal CI 39045723902, and published
privately. Implement the [owner move contract](deferred-owner-move.md), retaining
the same fence and original frozen binding with immutable identity lineage.
Require corruption/atomic rollback/quota/kill, TCP and actual SDK recovery plus
all existing native/reference/startup gates. Then implement the persistent full
Windows per-open/node-wide lifecycle, including explicit deferred destination
replacement proof before enabling delete callbacks.

Local full gate passed: 197 state, nine authority HTTP and thirteen SDK cases.
Same-fence move and ordinary replacement preserve identity lineage, original
bindings, dirty generations staged during unknown results, both closing slots
and exact replay. Four SDK stages made six PUTs, retained every immutable blob
and recovered metadata without replay. Schema16/config6 remain unchanged.
Require terminal exact-tip hosted native/reference/startup CI before private
publication. Next deferred destination replacement reference/native proof and
persistent per-open/node-wide Windows integration, including duplicated handles,
cleanup followed by I/O, multiple DoC opens, cancellation and recovery.


### P6m3b3b4b3b3a — durable per-open callback journal (2026-10-17)

Owner moves accepted at `dda0464`, terminal CI 39048160996 and private main.
Implement the [per-open journal contract](windows-open-journal.md) on the existing
SQLite worker with schema17/config6. Persist exact admission and callback/effect
identities before RPC; verify settled namespace results, never infer deletion
from local flags. Require corruption/full disk/quota/migration/kill and actual
SDK recovery plus terminal native/reference/startup CI. Then integrate full
node-wide/per-open policy and WinFsp callbacks, including deferred destination
replacement proof and final closing-work/retention policy before enabling them.

Local full gate passed: 212 state, nine HTTP and fourteen SDK cases. The new
callback journal retains exact deferred activation proofs and linked namespace
outcomes, detects missing/corrupt histories, protects callback/known-effect
acknowledgment slots, and migrates schema16 to17 without changing existing
namespace/content hashes. Five SDK stages recover killed owners and three PUTs
without replay. Require terminal exact-tip hosted native/reference/startup CI
before publication. Next integrate the full lifecycle controller and WinFsp;
callback facts alone neither implement disposition policy nor prove deletion.


### P6m3b3b4b3b3b — executable disposition controller (2026-10-17)

Per-open journal accepted at `b3267a4`, terminal CI39051731184, schema17/config6.
Implement the [controller contract](windows-disposition-controller.md): durable
callback response/completion, same-fence rejected-response recovery, node-wide
mode changes, deleting cleanup, ordinary final release and protected closing
work. Exercise the real SDK and existing worker, not a disconnected state model.
Require local and exact-tip hosted native/reference/startup gates. Then finish
retention/session recovery and integrate WinFsp with explicit new config and full
native DoC/cancellation/cleanup/late-I/O/replacement acceptance before enablement.

Local full gate passed: 220 state, nine HTTP and fifteen SDK cases, including
five killed controller stages and six immutable PUTs without replay. Schema18
retains old schema17 hashes; config6 and native unsupported paths stay unchanged.
Response-proof, compensation, full-disk/quota and exact open-ID checks passed.
Require terminal exact-tip hosted native/reference/startup CI before publication;
next resolve retention and abandoned-session policy before native integration.


### Completed-open retirement (2026-10-17)

Controller accepted at `28c527e`, terminal CI39055610947, schema18/config6.
Implement [lossless retirement](windows-open-retirement.md) on the same SQLite
worker, including controller Close/recovery. Only completed controlled records
leave live budgets; full proof rows remain immutable and old IDs never reopen.
Require >256 lifetimes with retained live readers, SQL-full/ignored writes,
corruption/migration, killed SDK retirement and exact-tip hosted acceptance.
Then resolve abandoned-session/response acknowledgment and deferred replacement
before complete native WinFsp integration. Do not infer cleanup from a new ID.

Local full gate passed: 228 state, nine HTTP and fifteen SDK cases. Eight new
retirement tests and six SDK process stages preserve complete proof/data history,
reuse live admission and recover failed retirement without PUT replay. Schema19
and the strict Windows startup expectation agree; config6/native deletion paths
stay unchanged. Require terminal exact-tip hosted native/reference/startup CI
before private publication; then implement the remaining mount-session policy.


### Exclusive mount-session recovery (2026-10-17)

Retirement accepted at `50e7709`, terminal CI39056932966. Implement the
[session contract](windows-session-recovery.md): exclusive runtime bindings,
current-session callback admission, exact old work replay, retained-file
cancellation and separate abandonment proof without invented native callbacks.
Preserve raw hashes, dirty/conflict data, successor fences and original unknown
requests. Require same-owner refusal, full disk/quota/corruption/migration and
actual SDK killed-owner recovery, then local and exact-tip hosted acceptance.
Next reconcile native reply uncertainty, prove deferred target replacement and
integrate the complete WinFsp lifecycle with new config/native acceptance.

Local final gate passed: 240 state, nine HTTP, sixteen SDK, lint/build/book.
Twelve new state cases and five SDK processes cover original cancellation replay,
separate abandonment, full open quota, identity loss, SQL-full/ignored writes,
historical proofs, successor refusal, migration and retained Unicode/empty/binary
objects with four PUTs and no replay. Exact old link replay no longer deadlocks
recovery of a later uncertain commit. Schema20/config6 stay explicit; require
terminal exact-tip hosted native/reference/startup CI before publication.


### Deferred replacement and native lifecycle integration (2026-10-17)

Session recovery accepted at `27697fd`, terminal CI39059948714. Measure
[replacement behavior](windows-replacement-reference.md) on NTFS and selected
WinFsp legacy before implementing deferred destination policy. Record actual
forwarding, pending flags and old-context I/O/cleanup; freeze observed results
and require strict typed/malformed-data checks, not guessed parity. Keep all
existing acceptance gates. Then integrate the complete native lifecycle with
explicit config and native DoC/SetDelete/cancel/cleanup/close/late-I/O/rename/
replacement/recovery tests; unsupported config6 remains until that gate passes.


Replacement measurements now include the POSIX-capable profile. Frozen21 records
preserve the original14 exactly and expose material NTFS/WinFsp differences;
strict matrix/type checks and12 malformed-data cases precede final publication.
Schema21 bridge preparation records void facts before RPC, preserves missing
cleanup on abandonment, and releases only an owned intent after observed
non-deleting cleanup. SDK simulations cover queued deleting cleanup and HTTP
failure without extra PUTs. The old five-second test shutdown wrapper was hit
on Windows; joined thirty-second bounds retain immediate reopen assertions.
Require the full final gate/hosted run; this is not native bridge acceptance.


Final local gate passed241 state/nine HTTP/sixteen SDK with schema21/config6.
The SDK case now also proves late writes after intent release and five intended
PUTs without replay. The semantic migration preserves schema20 hashes and raw
missing-cleanup Close facts. Required Windows CI strictly checks all21 measured
replacement records and12 malformed variants alongside the existing14/four/nine
reference checks, seven native cases per OS and startup. Require exact-tip
terminal all-green before publication; profile/native enablement is still gated.


### Atomic owned-target replacement (2026-10-17)

Reference/schema21 accepted at `cc01752`, terminal CI39064052369. Implement the
[owned destination contract](owned-target-replacement.md) with an exact optional
claim on existing move requests, preserving old serialized hashes. The authority
closes the target epoch and moves the source in one transaction. The client
must prove ownership of both affected reservations, retain detached target I/O,
block conflicting unknown work and replay original outcomes across process death.
Require explicit stale/foreign/corrupt/full-disk/quota/migration tests, actual SDK
late writes and killed-owner recovery, full gate and exact-tip hosted acceptance.
Next connect the complete native lifecycle/profile with its native acceptance;
this metadata phase does not enable native deletion or imply NTFS parity.


Local full gate passed253 state/ten HTTP/seventeen SDK with schema22/config6.
Twelve new state cases retain both ownership chains and dirty target data across
quota/full disk/corruption/restart; HTTP racing replacement has one winner.
Three SDK processes prove four source/target combinations, target late I/O and
8/12/12 PUT counts without replay or DELETE. Require final exact-tip terminal
native/reference/startup acceptance before publication. Full native callbacks,
explicit profile/config selection and early POSIX cleanup remain the next work.


### Complete native Windows lifecycle bridge (2026-10-17)

Owned-target replacement accepted at `971c6fb`, terminal CI39066548525, schema22.
Implement the [native bridge contract](native-windows-lifecycle.md) on the shared
filesystem write path, with explicit config7/POSIX profile, durable per-file-object
callbacks, paired store controls, early cleanup, stable-ID rename/replacement and
retained I/O. Preserve raw callbacks before network and exact uncertain work;
recover previous exclusive sessions without inventing kernel receipt. Require
local gates and complete new native acceptance before publication; versions1–6
keep unsupported deletion. This phase must connect the native callbacks, not stop
at another disconnected metadata API.

Accepted at0f6104c, terminal all-green
[CI39072688497](https://github.com/marshallr12/s3drive/actions/runs/39072688497).
255/251 state, ten HTTP, seventeen SDK, seven Linux and1+7 Windows native cases;
both startup gates and typed21/14 reference artifacts passed. Explicit extended
ON_CLOSE refusal reflects the pinned WinFsp kernel limit. Main is private/clean.

### P6m4a — shared HTTPS authority transport (2026-10-17)

Implement the [transport contract](remote-authority.md): explicit native TLS
listener, authenticated HTTPS client, private certificate/key/optional CA inputs,
unchanged stable IDs and exact uncertain-operation recovery. Keep numeric
loopback HTTP for existing local configurations. Require real CLI/shared-FS
HTTPS and SDK tests plus native Linux/Windows acceptance. No host or Turso
provisioning; the user chose to keep both hosting options open. Real multi-host
deployment, startup supervision, backup/failover and optional Turso remain later
requirements, not implied by successful local TLS tests.

HTTPS accepted at `a87bd0b`, terminal CI39075749765, with native Linux/Windows
HTTPS writers, five TLS cases, both startup gates and strict21/14 artifacts.

### P6m4b — operator-managed volume bootstrap (2026-10-17)

Implement the [setup contract](managed-bootstrap.md): prepare a flushed private
immutable store-registration plan, apply/observe its exact operation, and
generate per-machine read-only or explicit policy-bound writable configuration.
No S3 credential resolution, provider mutation or account provisioning during
setup. Preserve uncertain IDs, sequence guards and original files. Require real
CLI/TCP/SQLite/TLS concurrency/lost-reply/killed-client tests, generated config
use in shared/native filesystems, local gate and exact-tip hosted OS/reference/
startup acceptance before publication. Deployment supervision, real multi-host
partitions, backup/failover, provider acceptance and large files remain required.

Local bootstrap gate passed:257 state/ten HTTP/five TLS/17 SDK/six CLI/six
shared-engine cases, builds/lint/book. The existing portable lifecycle now uses
CLI-generated config7, and save/mutation recovery uses CLI-generated config6
and HTTPS. Explicit refusal checks preserve existing state and reject an
out-of-policy default root. Require terminal exact-tip hosted native/reference/
startup gates before guarded private publication. No real provider mutation.

Bootstrap accepted at `712882e`, terminal all-green CI39079281204:257/253
state, six setup CLI/six shared-engine cases, Linux7/Windows1+7 native cases,
both startup gates and strict source-bound21/14 artifacts. Private main is clean.

### P6m4c — pinned coordinator startup (accepted2026-10-17)

Accepted at private main `a7994e6203ccb082c1b441d19806cfb6ed6d3929`, terminal
all-green [CI39090568625](https://github.com/marshallr12/s3drive/actions/runs/39090568625).
See the [startup contract and scoped evidence](authority-startup.md) and
[acceptance record](evidence/authority-startup-20261017.json).

Existing-only Rust restart/check paths, Linux no-systemd and Windows account
supervision, and both paired native recovery workflows passed. The Windows
logs show a replacement child under the same supervisor, independent logon
AuthenticationIds, original volume/receipts, exact uncertain recovery without
another PUT, Unicode/binary/empty data, dirty rival conflict and independent
removal. Linux passed all eight native cases and both HTTP/TLS supervisor
profiles. Windows passed its lifecycle plus seven broad native cases.

Local/CI scope includes265/261 Linux/Windows state, ten HTTP, six TLS,17 SDK,
five restart/worker and seven managed cases, lint/build/57-doc checks and strict
source/platform-bound21/14 reference artifacts12123120389/12123325993. Private
main matched the accepted source after fast-forward. Windows is hosted
Server2025/WinFsp2.1.25156.ddca7bd, not this workstation or Windows10/11.

The earlier unexplained metadata-read/rename transport failures remain open;
the accepted run does not attribute or claim to fix their causes. Physical
reboot/PAM/SSH, Windows10/11, real independent hosts/partitions, backup/failover,
managed Wasabi write acceptance, large files and product packaging remain work.
No real user startup registration, cloud account or provider mutation occurred.

### P6m4d — coordinator snapshots and recovery (proposal, 2026-10-17)

After pinned-startup acceptance, follow the [backup/recovery research contract](authority-backup-recovery.md).
The isolated Linux CLI experiment at7962058 measured a same-ID sequence0
snapshot accepting a pinned restart after a later sequence1 operation had been
acknowledged; lookup then forgot that operation. Identity/schema/integrity do
not establish restored-history freshness. The dated result/reproducer are in
the chapter; no real state or S3 provider was modified.

Implement private complete snapshots and quarantined inspection first, followed
by bounded online operation and separately proved continuity/fencing before
any writable restoration or failover. Preserve originals, exact unknown input
and dirty journals. This staged plan does not replace the full history-loss,
independent-host/partition and actual platform recovery requirements. No
backup/restore feature or cloud guarantee is enabled by the research.


Historical Windows job117562340718 at7962058 failed the TLS fault matrix
before startup: TruncatedBody received a request timeout and a fixture task
reported unlabeled header EOF. The follow-up prebuilt synchronous setup off
runtime workers, retaining six concurrent exchanges and every assertion. Its
per-case progress/timing and full startup pairing later passed at a7994e6 in
CI39090568625. Neither that pass nor the revised fixture explains the older
production failure; the same-source repeat below supplies fresh failure evidence.


### P6m4c follow-up — classified Windows TLS regression (2026-10-17)

Main a7994e6 passed primary CI39090568625, then same-source repeat
CI39092631904 failed Windows paired setup during store-apply before mounting.
Request/UnexpectedEof and server HttpTimeout do not establish commit outcome
or root cause. Linux/book succeeded. Keep the successful historical result and
this contradictory reliability evidence; no retry/deadline/TLS weakening.

The diagnostic candidate adds bounded failed-connection plaintext read/write/
flush counts and a header-dispatch flag, without contents, identities or raw
dependency errors. Fixed client request categories and early real CLI/TLS
connection churn check exact operation receipts and volume sequence. Focused
Linux tests passed, including no/partial headers, partial body, privacy and
16 registrations/64 exact-plan observations/128 concurrent Info requests. The
full local gate passed:265 state, one I/O pass-through, ten HTTP, six TLS,
seven restart/worker,17 SDK and seven managed cases plus lint/build/57-doc
checks. Exact-source hosted native/reference/startup gates remain required. Do not claim a diagnostic
checkpoint fixes the production issue; use measured failure phases to choose
the next correction. Snapshot prototype remains unpublished in the root
worktree, outside this clean investigation.


The first diagnostic candidate9b403bb failed Windows workspace only at a new
blanket zero-log-error assertion after every client reply/receipt check passed.
A2ms dispatched read321/write481/five-flush generic event is not the older
request/header-timeout failure. The follow-up adds the explicit Hyper shutdown
category and permits only that phase after independent complete replies, with
positive exchange counters. A deterministic Hyper-driver fault test covers the
boundary; no deadlines, retries, native assertions or TLS policy change. Require
new exact-source Windows evidence before attributing the old event or promoting.
