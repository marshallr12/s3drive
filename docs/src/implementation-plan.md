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
| P0 — research and scaffold | This book, workspace, local CLI, SQLite bootstrap, GitHub repository (intentionally public under D040) | Local gate passes; pushed source matches local commit; current public visibility verified read-only |
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

Latest diagnostic main checkpoint f8c4f58 passed terminal all-three
[CI39097275707](https://github.com/marshallr12/s3drive/actions/runs/39097275707):
265/261 state, two Hyper I/O cases, ten HTTP, six TLS,17 SDK, seven restart/worker,
Linux8 and Windows1+7 native cases, both paired recoveries and strict21/14
source/platform artifacts. See the [dated record](evidence/authority-transport-20261018.json).
The original timeout remains unproven; this acceptance does not attribute it.

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

#### P6m4d1 — private offline snapshots and inspection (prototype; acceptance pending)

The private topic branch `codex/authority-snapshots` is rebased onto accepted f8c4f58
and has an
experimental library and platform directory helpers. This is the first stage
of the full recovery contract, not writable restoration/failover. Experimental
`authority-snapshot create|inspect` commands exist on the topic; no restore is
implemented. Combined-source local/Windows acceptance is required before publication.

Local2026-10-18 evidence: state check/Clippy passed; nine filtered tests passed
(eight cases plus a guarded worker), covering actual WAL-only data, schema8
byte-identical operation/receipt blobs, real SQLITE_FULL, cancellation and eight
killed-process boundaries through copying/validation/data/manifest publication.
Partials refuse inspection; complete bytes remain non-bootable and inspection
reports neither history continuity nor restore authorization. Process crashes
are not power-loss proof. Two real local CLI round-trip/refusal cases passed,
with explicit identity, new output, private errors and no restore authorization.
The full local gate passed, including the combined f8c4f58-plus-snapshot source
(274 state plus CLI/lint/build/57-doc checks). Windows ACL/native acceptance
remains. The existing disposable ordinary-account startup workflows now
include prepared live-owner refusal and stopped create/inspect checks, exact
source bytes, duplicate-output preservation, widened-ACL/hardlink/junction/
case-alias refusal and concurrent read-handle sharing. These component checks
passed at f9a52e2; full candidate native/console acceptance remains pending; see the [fixture contract](authority-backup-recovery.md#windows-component-observations-full-acceptance-pending-2026-10-18). A separate real Linux CLI SIGINT
fixture passed at defc8b9: source bytes stayed identical, no success/manifest
was published, partial inspection refused and the original destination could
not be replaced. See the [dated cancellation proof](evidence/authority-snapshot-sigint-20261018.json);
no Windows-console or physical power-loss inference. A separate local
[locked-source CLI check](evidence/authority-snapshot-busy-20261018.json)
refused without destination creation or source-byte changes, then succeeded
after the independent writer released its lock. Destination contention and
actual Windows locking/ACL behavior still require acceptance.

The subsequent topic follow-up prepares an isolated Windows console Ctrl+C helper
and an ordinary-account padding mode. A portable fixture checks original
identity/sequence and byte-identical operation rows after temporary padding.
Local compilation/parsing is not Windows cancellation acceptance; require the
[isolated-console runtime contract](authority-backup-recovery.md#windows-console-cancellation-observation-2026-10-18)
and source-bound Windows acceptance. The follow-up local gate passed, along
with final PS AST and C#5/.NET4.5 compilation. Hosted d6d8bd9 passed the isolated
Ctrl+C component (one16ms cancellation observation), but failed later native
HTTP lifecycle operations. Full snapshot acceptance remains pending.
Never signal a shared runner console or substitute
forced termination for cooperative cleanup.

Hosted f9a52e2 subsequently passed269 Windows state cases, snapshot CLI2 and
ordinary-account Active/Stopped snapshot checks after both standalone and paired
recovery. Its overall CI failed in later native TLS operations. Follow-up33ddfe1
failed the earlier native lifecycle HTTP case before reaching console acceptance.
Both Linux/book jobs succeeded. No main promotion; see the
[dated platform record](evidence/authority-snapshot-platform-20261018.json).
A failure-only independent CLI metadata probe passed portable live/stopped
controls. At d6d8bd9 it received the original volume in28ms after the native
failure, with both owned processes alive. It preserves the original assertion;
post-failure availability does not establish the timed-out operation's outcome.
The next candidate declares finite metadata connection lifetime explicitly;
see the [transport contract and limits](remote-authority.md#explicit-metadata-connection-lifetime-candidate-2026-10-18).

The explicit-close candidate e51f737 passed Linux/book, Windows workspace,
lifecycle and paired recovery, but failed a later broad native flush with
Info/UnexpectedEof/native1117. Main remains f8c4f58. The next diagnostic observes
request-body polling with independently received-byte and failed-trust controls;
it does not establish a socket write or resolve an unknown operation. See the
[scoped diagnostic contract](remote-authority.md#client-body-poll-observation-candidate-2026-10-18).

The follow-up also prepares an early32-flush native HTTPS fixture, with distinct
binary bytes in a Unicode path, fresh reads, exact single-PUT counts, no injected
outage and no tolerated error. On failure the read-only control/owned cleanup
preserves the original panic. Windows runs it once before the long workspace;
Linux includes it in native acceptance. This adds a gate, retaining every earlier
case. Compilation and workflow structure checks passed; actual OS results remain
pending. The body diagnostic alone was checkpointed after its full local gate;
the combined source requires its own full gate and exact-source hosted cohort.

That combined source a14ffa3 passed Linux9 native cases, but Windows's fourth
flush failed Apply/UnexpectedEof/native1117 with body_frame_polled=false after
six Windows raw controls passed. The next candidate runs metadata requests in
caller-scoped Tokio tasks, with an absolute cooperative10-second queue/RPC
deadline and no identity/retry changes. Queue cancellation, foreign-thread exact
receipts and lost caller after actual commit passed local controls. Require the
[runtime execution contract](remote-authority.md#runtime-owned-request-execution-candidate-2026-10-18)
and new exact-source hosted acceptance before attributing a cause or promoting.

The runtime-owned candidate949073f subsequently passed Linux/book, Windows32
flushes, full lifecycle, workspace, standalone/paired recovery and reference
steps, but failed the broader write test opening an existing Unicode file:
List/UnexpectedEof/native1117 with body_frame_polled=false. Main remains f8c4f58.
The next bounded phase adds request-scoped connector call/success observations,
validated against independent concurrent TLS peers and wrong-scope completion.
Its criterion is correct attribution/privacy with unchanged recovery/no-replay
contracts, followed by the full local gate and the same hosted native gates.
It is not a substitute for correcting the transport failure. See
[connector observations](remote-authority.md#connector-observations-candidate-2026-10-18).

The next diagnostic phase uses the independently tested public connector
metadata API to record a bounded loopback candidate tag (address family and
client/server ports only). It must omit all other addresses and missing
metadata, mark multiple successful candidates as ambiguous, and retain the
original request scope across background completion. Server diagnostics may
include the same loopback-only tag. Acceptance requires independent TCP/TLS
comparison, missing/non-loopback/multiple-candidate and concurrency controls,
redaction/no-replay regressions and a new full gate plus hosted native cohort.
A candidate tag is not proof of selected socket, delivery, authorization or
commit. The earlier e988 hosted run is evidence for that exact failed commit.

The bounded loopback-tag phase is now accepted as checkpoint bf9509 at
[CI39113628548](https://github.com/marshallr12/s3drive/actions/runs/39113628548):
all three jobs, raw native/startup/snapshot review and independently strict21/14
artifacts passed. Private main was fast-forwarded to that exact source. This is
not a root-cause/reliability or full-goal completion claim. An
[unchanged-source main repeat](https://github.com/marshallr12/s3drive/actions/runs/39115395230)
also passed all three jobs, raw review and its own independently verified21/14
artifacts. The separate public Error::connect_info experiment led to the
unaccepted diagnostic candidate described below. Restore/failover,
real-provider/application and physical startup/desktop gates remain required.

The next bounded diagnostic phase adopts the independently verified public
Hyper-util error connection information without changing request execution.
It must distinguish an unobserved legacy error, observed legacy error without
connection info, omitted non-loopback/missing TCP metadata, one sanitized
loopback error connection, and ambiguous multiple legacy errors. A lost-reply
control must match the independently accepted peer; rejected TLS and truncated
response controls must validate the missing-information states. Preserve all
redaction/no-replay/outcome guards and require the full local gate plus a new
hosted cohort. Error-reported connection information is not delivery/commit proof.
The completed main repeat remains evidence only for the unchanged bf9509 source.

The error-connection candidate0bbc4db subsequently reproduced Windows's native
Info/UnexpectedEof failure at flush-burst step25. Its sanitized error connection
and connector candidate matched server connection185's empty-header deadline.
The fresh independent CLI also refused while both owned processes were alive.
Keep mainbf9509 and retain the failed source/raw evidence; later Windows gates
were skipped. This is useful correlation, not a root-cause or upload outcome.
Transport investigation takes priority before enabling additional upload paths.


Require exclusive existing-authority ownership and explicit volume/policy pin;
read supported legacy/current state without migrations or namespace changes.
Produce a new private artifact outside mounted trees, with a database filename
and manifest distinct from live authority state. Never overwrite an existing
artifact or source. Read inventory/sequence from the completed destination;
bind complete bytes to strict versioned metadata, then publish success only
after validation and required synchronization. Failures leave recognizable
partial output and preserve the original database and previous snapshots.

An offline inspector must verify ownership/links, format, digest/length,
application/schema, original identity/policy/root and standalone SQLite
integrity without modifying originals, starting listeners, reading credentials
or granting permission to serve. Snapshot integrity is not freshness proof.
Keep restoration/promotion unsupported until continuity and fencing pass.

Inspect the actual SQLite/rusqlite snapshot and completion contracts before
choosing the mechanism; use bounded work/cancellation and no raw FFI workaround.
Test WAL-only commits/receipts, schema8 and current state, competing/live owners,
corruption/foreign/client/future schemas, Unicode paths, retained exact operation
bytes and private ACL/link/path refusal. Force failure/crash at creation, copy,
validation, synchronization and completion publication; no false success or
replacement. Require real CLI round trips, the full local gate, actual Windows
file/ACL behavior and existing native/reference/startup regressions before
publication. Real deployment restore drills remain separate.

### P5b1 — bounded multipart geometry (candidate, 2026-10-18)

While the error-connection candidate0bbc4db runs its own hosted acceptance,
implement the [sizing-only phase](multipart-plan.md#first-implementation-phase-sizing-only-candidate-2026-10-18)
on a dependent topic. Require explicit validated limits, overflow-safe complete
coverage, bounded lazy part enumeration, boundary/exhaustive geometry tests and
the full local gate. No database migration, SDK/network operation, config change
or increase to the8MiB write bound is part of this phase. Persisted immutable
byte/binding plans and bounded filesystem range composition remain required
before multipart execution; do not substitute arithmetic tests for those gates.
The pure planner's six cases, complete local workspace/lint/build and book gate
passed. Its parent0bbc4db has a recorded native Windows failure; this local
sizing result is not acceptance of that parent or large-file execution.

### P6m4e — owned HTTP/1 exchange study (candidate, 2026-10-18)

Before replacing the production client, implement a test-only direct Hyper/
Rustls exchange that co-polls its driver and exactly one authenticated request.
Require independent ephemeral HTTP/TLS peers to observe exact JSON bytes,
Content-Length, Host, bearer and Connection: close; full replies, lost reply,
307 refusal, truncated/oversized response, wrong trust/name and stalled handshake
controls; cancellation after full peer receipt must close the owned socket.
Keep2-second TCP/TLS and10-second whole-exchange deadlines, bounded responses,
sanitized errors and no retry, proxy, pool or independently spawned driver.
Inspect resolved APIs before implementation and run the full local gate. This
study does not replace URL/config/platform-trust validation or production
transport, and cannot prove a Windows correction or a committed mutation.


### P6m4e closure prerequisite (candidate, 2026-10-18)

The pre-rewrite protocol review requires the [successful TLS closure control](remote-authority.md#successful-tls-closure-review-candidate-2026-10-18)
before production adoption. Retain a fully framed response while polling the
unfinished driver under the original absolute deadline; a cleanup error/expiry
must not discard that response. Incomplete replies still time out. Require a
controlled pending TLS shutdown that fails the old logic, clean TLS peer EOF,
and a delayed-response control that distinguishes remaining budget from a new
cleanup timeout. Reap both fixtures before assertions. Run the exact final
source's full local gate and hosted Linux/Windows controls. The older local
iteration's successful controls do not substitute for final result semantics.

Then replace the production client using the carryover audit, preserving public
constructors, URL/Host/SNI rules, private files, platform/explicit trust, request
bounds, every2xx with strict JSON, response limits, caller-owned task and absolute
queue/RPC deadline. Define truthful owned-socket diagnostics rather than inventing
legacy error metadata. Keep all exact-operation/receipt/cancellation controls,
add malformed framing and status201/204 cases, then require exact-source native/
startup/reference gates and a source-identical repeat. No proxy, replay, deadline
extension, new operation identity or unconditional overwrite fallback is permitted.
Transport adoption and native reliability remain work; multipart execution,
real managed Wasabi acceptance, independent hosts/partitions, history continuity,
actual desktop startup/reboot and packaging remain full-goal requirements.


### P6m4f — production owned client (candidate, 2026-10-18)

Implement the [owned production client](owned-authority-client.md) using the
pre-rewrite audit and corrected closure controls. Preserve validated constructors,
private bounded files, platform/explicit trust, original serialized requests,
all2xx bounded strict JSON, queue/RPC deadline and exact durable recovery. Reuse
the pinned DNS/TCP Happy Eyeballs connector; a sequential connect loop is a
rejected regression. One task owns TLS/HTTP driver/body/cleanup, with one private
result channel retaining a complete receipt through cleanup expiry. Error paths
attempt bounded stream shutdown without retry. Document the additive public
WireError diagnostic-field compatibility exception and actual owned TCP tags.

Require actual SQLite commit/lookup equality under clean/failed/stalled shutdown,
pre-dispatch expiry and invalid-input no-I/O, independent status/framing/trailer/
8MiB/DNS/SNI/IPv6 peers, all existing fault/recovery controls and the full local
gate. Then run exact-source Linux/Windows native/startup/reference acceptance and
a source-identical repeat, reviewing all warnings and independent strict21/14
artifacts before promotion. No root-cause or reliability claim follows from the
portable suite. Keep all larger-file/provider/deployment/recovery requirements.

### P6m4g — owned exchange progress (candidate, 2026-10-18)

P6m4f source47e9dd4 failed actual Windows flush step22 after its owned TCP tag
matched an empty-header server timeout; Linux/book passed. No main promotion or
same-source success repeat is eligible. Preserve both source47 and parentea9342a
terminal failures. Do not infer a Hyper, TLS, SQLite or scheduler cause.

Add bounded content-free task/HTTP/enqueue/driver timings and plaintext I/O
poll/Pending/byte observations to the owned exchange. Delegate scalar/vectored
I/O, capability reporting, flush and shutdown without changing polling order,
deadlines, retry, request identities, result retention or recovery. Keep the error
Copy and document the added public field. Require independent pre-write EOF vs
failed-write controls, partial/Pending/vectored/error accounting, existing real
TLS fault and receipt controls, and the full local gate. Then inspect exact-source
Windows native output to distinguish missing scheduling/dequeue/write work;
a portable pass alone cannot establish reliability. No main promotion is planned
from diagnostics alone, even if the instrumented native cohort passes.
Separately measure retained
journal ancestry cost without weakening integrity checks or dropping history.

### P6m4h — pending-scan proof reuse (candidate, 2026-10-18)

Measure retained settled-history scans before optimizing. The current pending
scan rechecks each file and repeatedly validates the entire remote receipt forest
through logical receipt verification. Preserve all records, byte/hash checks,
receipt equality, ancestry, pagination and corruption refusal, including covered
rows and rows outside the requested page. Any reusable proof must be scoped to
one synchronous read command on the exclusive state owner, with no awaits or
mutation while it exists; no proof may survive into another command.

Done criteria: equivalent results against the original scan algorithm, measured
reduction in SQLite VM progress callbacks for retained history, adversarial covered
record/receipt/ancestry corruption and changed-between-command checks, the full
local gate and own native acceptance. Timing is observational, not a flaky test
threshold. Reducing scan cost does not by itself prove the Windows transport
cause or permit dropping the unresolved transport acceptance requirements.
The [measured candidate](managed-edits.md#retained-history-scan-validation-candidate-2026-10-18)
passed its local gate:276 state,48 authority and workspace/book checks. Its own
Linux/Windows native/startup cohort at64aa073 and its own strict21/14 reference
reports also passed. Its unchanged-source repeat atCI39127440399 failed Windows
paired startup; Linux/book passed. The matched owned exchange had469 driver
polls but zero write polls/bytes before a server empty-header timeout. The failed
run is not eligible for the success artifact checker or repeat acceptance.
Transport reliability remains unresolved. The32-edit study is not a production latency or large-catalog
acceptance claim. Parent diagnostic90 passed its own cohort without explaining
the historical failure; neither cohort establishes a causal transport correction.


### P5b2 preparation — measured staging boundary (2026-10-18)

An isolated study of unchanged64aa073 production passed exact16/32/64MiB staged
range reads, full hashes, unchanged read SQL state and reopening. At a fixed16MiB,
increasing extents from16 to256 raised read-scan progress callbacks from11,610
to134,370. Source, logs and reproduction are retained in the
[staged-range investigation](multipart-plan.md#staged-range-cost-and-planning-boundary-investigated-2026-10-18).
This is diagnostic evidence, not multipart or native large-file acceptance.
Implement the concrete frozen-input/private-finalizer contract there, with
bounded owner work, durable quotas, legacy/role compatibility and restart/
corruption/disk-full controls. Preserve the original guard before hashing;
a ready plan issues no send permit. Existing8MiB upload/edit limits remain.


### P5b2 offline preparation — immutable plans (candidate, 2026-10-18)

The candidate now implements `Storage::multipart` on the existing owner, schema24
client-local plan/part tables, caller-retained idempotency IDs, original target/
credential/guard/ancestor-receipt bindings and explicit provider geometry.
Freeze reserves plan/part quota before hashing. Each async owner step verifies
and hashes at most1MiB; whole/part digests become Ready atomically through a
private finalizer that checks exact saved SQL effects before commit. Re-preparing
rehashes the same frozen input; it never adopts a new guard. Ready is an expected
byte commitment, not a send permit or atomic media-freshness proof.

Done criteria for this phase: exact original identity across restart/newer heads,
sparse/inherited/nonaligned bytes and16MiB queued planning, independent quota/
extent caps, bounded malformed-cell reads, actual process death, true SQLITE_FULL,
six ignored/deleted/corrupt SQL-effect controls, strict schema23 migration/hash
compatibility and both local-table authority exclusions. The final focused suite
passed16 tests. The full local gate passed292 state/48 authority tests and
workspace/build/lint/book checks on frozen final source. Require its own hosted
Windows/Linux schema/native/startup acceptance before considering integration. Record outcomes in the [dated evidence](evidence/multipart-plans-20261018.json).

No initiation/upload ID, shared single-PUT/multipart target admission, part send,
completion, abort, reclamation or large native edit path is enabled. Preserve
all8MiB bounds and unknown outcomes. Further phases in the
[multipart contract](multipart-plan.md) remain required, together with resolution
of the failed unchanged64 Windows paired-startup repeat. No new provider requests
or permissions arise from this offline work.


### P6m4i — dispatch-boundary observations (candidate, 2026-10-18)

The unchanged64 Windows repeat had469 active driver polls with no plaintext write
or body frame. The offline multipart candidatec7 retains that transport. Its Linux/book and
Windows planning tests passed, but its broad Windows startup fixture failed
because it still expected schema23 instead of24. Before selecting a scheduling correction, add only
bounded content-free observations: sender readiness immediately before the one
enqueue, whether Hyper queried body end-of-stream/size hints, and whether task
cooperative budget was exhausted immediately before/after a driver poll. These
are hints/observations, never request delivery, mutation or causal evidence.

Preserve exact public body hint/frame delegation, first-request buffering, biased
work/driver poll order, one send, all original absolute deadlines and complete
known-reply retention. Do not await readiness in this observational phase; the
pinned Hyper source explicitly supports buffering its first request. No extra
network operation, runtime, retry, dependency or schema change is allowed.

Done criteria: a deterministic production-exchange control reproduces active
polls plus no write/body inspection by draining budget inside a gated Pending
reader; releasing that gate must complete exactly one original request. An
independent EOF/failed-first-write control must distinguish body descriptor
access, and transparent existing I/O/body/deadline/TLS/SQLite/parity checks must
remain intact. Keep the sanitized error Copy and bounded, with no identifiers,
headers, bytes or URLs in new fields. Full local gate plus its own exact-source
native Windows/Linux evidence and later unchanged-source repeat remain required.
Read-only legacy review precedes implementation. This phase observes dispatch;
it cannot be reported as correcting the historical failure or enabling uploads.


The observational delta is implemented. Read-only legacy and final-delta reviews
found no blocker. Nine focused wire controls passed, including normal and gated
budget cases, body hint/frame parity and preserved exact TLS/SQLite replies.
The fixture bounds budget draining to4,096 iterations rather than assuming the
runtime budget size. It proves positive exhausted-after and negative exhausted-
before paths; positive exhausted-before is not separately covered. Frozen source,
actual results and the separate full/native acceptance status are in the
[dated evidence](evidence/dispatch-progress-20261018.json).


The c7 failure exposed a concrete fixture-version mismatch, not proof that the
private database failed to open. This candidate also corrects that one exact
assertion to24 and adds `scripts/check-schema-fixtures.py` to the local gate and
the beginning of the book CI job. The check binds both current-schema PowerShell
fixture assertions to the Rust constant; restoring either stale assertion makes
it fail. A separate PowerShell AST check parses the outer/embedded worker and
accepts24 while rejecting23/25. Preserve the native checks; no generic-version
match, retry or bypass replaces them. The first local gate passed unchanged
Rust sources under its old definition. The final extended gate2 then passed
schema preflight,50 authority/292 state and workspace/build/lint/book checks on
unchanged12 frozen source/contract files. Own native CI remains required.


### Large managed-file representation study (proposal, 2026-10-18)

The [source audit and representation comparison](large-managed-files.md) records
why multipart upload alone cannot remove the managed reader's whole-file
verification/buffer, native edit buffer, logical prehash and authority payload
limits. The existing bounded composition library should be reused where its
contract fits; local chunk staging does not currently imply chunked S3 objects.

The user selected immutable chunks and authenticated paged manifests on
2026-10-18 (D041), confirming the recommendation with explicit legacy
compatibility. No format, schema, authority hash,
8MiB bound, network behavior or provider policy has changed. The study includes
an architecture image and reproducible hypothetical storage arithmetic, not
performance or account pricing evidence. Single-object storage can also gain
verified random reads through authenticated range digests; the tradeoff is not
an assertion that range verification requires separate objects.

Done for this investigation: audit actual source2575374 boundaries, compare
recovery/reads/edits/object-count/retention implications, refresh primary AWS and
Wasabi sources, record the decision question, and specify the implementation
and native/provider acceptance sequence. A representation-specific codec or
publisher integration follows the selected chunked design. Source-bound CI review
and existing transport investigation can proceed independently.


### Dispatch candidate hosted closure (2026-10-18)

Source `25753746287aa52a36a9ba42d8bf5162f1de07d1`,
[run39133178423](https://github.com/marshallr12/s3drive/actions/runs/39133178423),
attempt1 passed Linux, Windows and book. Completed own raw logs were reviewed before
checking that run's ZIP digests, strict typed21 replacement/14 disposition
records and both project PowerShell oracles. The schema24 Windows session
fixture now passes natively; parentc7's failure remains recorded. See the
[dated dispatch evidence](evidence/dispatch-progress-20261018.json) for exact
jobs, hashes, scope and notices, including deferred disposable VM profile cleanup.
The exact-source attempt2 also passed Linux/Windows/book, completed own raw
reviews and fresh strict21/14 artifacts with both project oracles. Its separate
[repeat record](evidence/dispatch-repeat-20261018.json) binds new job/artifact IDs;
no primary artifacts were reused. The historical actively polled/no-write failure
did not recur and remains unexplained. No runtime promotion, new manifest-code
acceptance or new provider approval follows.


### Managed manifest codec foundation (offline candidate, 2026-10-18)

Use chunks/manifests within the user's selected managed layout. Implementation
began as a provisional offline default after the optional clarification window;
the user then explicitly selected that option (D041). This does not authorize
a live migration or expand provider-write approvals. Existing content formats
and all mounted/provider limits stay intact.

First coherent phase: a pure Rust core module for bounded canonical pages,
domain-separated SHA-256 page hashes, explicitly tagged holes/data/child pages,
context-bound root/child verification and allocation-free extent lookup. SHA-256
chunk bytes retain their ordinary whole-object meaning; a manifest root must
never replace a legacy payload digest. No SQLite, wire capability, native
callback, provider operation, deduplication or deletion is added.

Done criteria: independent Python-generated fixed binary/hash vectors; empty,
sparse and two-level manifests; every-byte tampering and all truncated prefixes
refused; exact object/length hashes, cross-file/offset/level substitution refused;
fanout/page/depth/signed-length/overflow/alignment and noncanonical encodings
refused; exact EOF and boundary lookup. Private validated page state and a
separate verified-page type prevent accidental data verification from merely
parsed metadata. Inspect resolved SHA2 APIs, keep existing hashes unchanged,
run the focused suite and full local gate, then bind any new native CI to its
own source. The concurrent257 repeat remains an independent earlier-source
observation and cannot certify this module.


Offline codec result: all10 focused tests pass, isolated regeneration matches
all8 golden files, core all-target Clippy passes with warnings denied, and the
full local gate passes on14 unchanged frozen source/fixture hashes:27core,
50authority,292state/1ignored,51S3 plus format/lint/build/book. This is not a
large-file native or provider acceptance claim. Exact results and limits are in
[the dated codec evidence](evidence/managed-manifest-20261018.json).


### Manifest object identities and compact roots (offline phase, 2026-10-18)

Next foundation on codex/manifest-object-identity, parent ec43132/runtime e48:
separate page/data key roles under a new v2 object namespace; bind keys to
volume, file-node, store, generation and ordinal using canonical lowercase hex.
Validate the store root structurally and retain the existing v1 key contract.
Define a fixed120byte explicitly versioned root descriptor carrying volume,
node, logical length and the page reference. This is an offline format only:
no authority capability, wire/schema change or network admission follows.

Done criteria: independent Python fixed vectors; exact encoding/decoding with
unknown/trailing/truncated forms rejected; empty/max-length roots; zero IDs and
invalid roots rejected; keys separate all identity dimensions and roles;
store/context mismatch refused; digest changes cannot silently create a new key
for the same identity. A descriptor is structurally valid, not authenticated
metadata. Verify its page binding separately and require future authority trust,
route admission and dependency receipts before publication. Run focused tests
and the local gate; old e48 native evidence cannot certify new Rust source.


Result: seven focused tests pass; both independent vectors reproduce in an
isolated directory; the full local gate passes on 19 unchanged source/fixture
hashes (34 core, 50 authority, 292 state/one ignored, 51 S3 plus formatting,
Clippy, build and book). No dependency, schema or wire change. This phase supplies
neither object publication nor authority trust. Next implement durable frozen
edit identities and a bounded dependency journal, preserving unknown operations,
original guards and every legacy payload/hash. See the
[object identity evidence](evidence/manifest-objects-20261018.json).


Next-phase source audit (2026-10-18): `journal::Target` and
`Staging::begin_operation` already freeze a bounded target and operation key
before a payload digest exists. Reuse those physical-object staging guarantees;
`logical::Input` separately requires the whole digest and caps bytes at 8 MiB,
so lifting that constant is not a chunked edit protocol. The new 16 byte object
generation identity also differs from the positive signed SQLite `GenerationId`:
retain an explicit mapping, never reinterpret either ID. Budget the frozen edit,
physical staged generations, object records and manifest edges independently.
`Owner::call` skips a cancelled queued command, but execution can commit before
a lost response; therefore persist/reuse the original operation key and inspect
its result before retrying. These are inspected existing contracts, not newly
implemented graph recovery or permission to send.


### Durable offline chunk plans (phase contract, 2026-10-18)

Branch codex/chunk-plan-journal from fa0a91d. Add schema25 local-only tables for
frozen edit inputs and immutable prepared objects. Freeze caller-retained edit
and object-generation IDs, original content guard, source/route, parent snapshot,
file identities and final size before hashing. Attach only the exact sealed
staging operation for that input. Resume deterministic 1 MiB chunk hashing and
at most128-child page construction after restart, using cursor compare/replay.
Explicit zero extents become holes; no missing-object fallback. Ready describes
an offline expected tree, never a remote receipt, authority root or write permit.

Initial preparation materializes a complete new generation from the local
snapshot. Cross-generation remote-object reuse remains explicitly unavailable
until authenticated base-root ancestry/retention is implemented; this is not
completion of the chosen incremental chunked filesystem. Keep all mounted
limits and live content formats unchanged. Use source-bound compatibility gates.

Done criteria: >8 MiB multilevel independent reads/edits/sparse and empty trees;
fixed page/root validation; exact identity/parent/route/guard refusal; cursor
replay and process death; missing/corrupt staged bytes and prepared rows refused;
metadata/payload quotas before work; SQL trigger/SQLITE_FULL rollback; schema24
migration preserves existing hashes/unknown operations; both new tables refused
by authority initialization/snapshots. Inspect resolved rusqlite0.40.2 APIs;
update strict native schema fixtures and old-schema test setup without weakening
assertions. Run focused state tests, the repository gate and own-source CI.


Offline chunk-plan result: 18 focused tests pass; all four independent vectors
reproduce; final full gate2 passes on 56 unchanged source/fixture hashes (310
state/one ignored, 34 core, 50 authority, 51 S3, format/lint/build and 56 book
chapters). Gate1 failed two stale downgrade/current-version fixtures; those were
corrected without weakening identity, hash or refusal assertions and all three
targeted migration/identity controls pass. The [chapter](chunk-plans.md) and
[dated evidence](evidence/chunk-plans-20261018.json) record scope and limits.
Logical 1 MiB slices do not cap physical staging I/O or checksum work under
fragmentation; metadata scans also need measurement/optimization before native
large-file admission. This phase provides no remote receipt or authority root
capability, and remote unchanged-reference reuse remains unfinished.

The recorded [Wasabi conditional-PUT evidence](evidence/wasabi-conditional-put-2026-10-16.json)
supports investigating bounded create-only chunk/page publication without a
multipart prerequisite. Its 4 KiB/empty disposable fixtures do not certify 1 MiB
chunk writes, a complete managed tree, current application policy or recovery.
No provider requests were made for the chunk-plan phase. New live acceptance
still requires a concrete bounded plan and new authorization.

## Chunk object upload bridge — implementation 2026-10-18

This phase connects a Ready offline chunk plan to existing create-only single-PUT
execution. It does not publish an authority root or acknowledge the logical file.
Use the same SQLite owner and frozen source binding throughout. Derive each
physical staging operation from the frozen plan, attached snapshot manifest and
exact object record; persist/recover it with `staged_operations`. Validate page
Merkle hashes separately from raw upload-body SHA-256. Pages require exact durable
flush receipts for their direct non-hole dependencies. Send data, leaves and
parents in that order. An uncertain attempt retains its original identity and
only observes on retry, including after evidence expiry or scope narrowing.

Done criteria: local state tests cover interrupted materialization, exact mapping,
corruption and dependency refusal; SDK/TCP tests cover binary/Unicode content,
holes/empty files, page hash domains, guarded collision, lost response and restart
without repeated PUT. The full local gate must pass. Existing generation/remote
quotas remain in force; no schema/dependency/mounted-format change, no provider
writes, no inference of native large-file or authority-CAS acceptance. Hosted
regressions and any future live-provider acceptance retain their own source IDs.

Focused result: 8 SDK/TCP and 5 state tests pass, including a sparse multi-level
file above 128 MiB with four physical objects and actual SQLite-full rollback.
Full local gate passed (315 state, 59 S3, 34 core and 50 authority tests;
format/Clippy/build, links and 57 book chapters). Own-source hosted regressions
remain separate. See the
[upload bridge contract](chunk-object-uploads.md).

## Manifest root authority contract — implementation 2026-10-18

Extend the existing authenticated coordinator with an explicit `ReserveManifest`
mutation and advertised `manifest-root-v1` feature. Embed optional versioned root
metadata in Reservation, omitted for legacy records. Keep physical page length
and raw SHA in existing fields, and expose logical length separately. Preserve
exact old serialized Apply/Reservation/Payload bytes and v1 CAS hashes; only the
new format selects a distinct content-reference domain. Reuse the existing atomic
Publish, content guard, detached and pending-removal checks. Bind volume, node,
store, object generation, root ordinal and derived v2 key; reject mismatches.
Schema26 marks the new persistent JSON format without rewriting old rows or
adding a side table. Older binaries must refuse the upgraded database.

This independently testable phase provides authority metadata publication, not
client logical settlement or mounted-format activation. Legacy managed inputs
and readers must refuse the new format explicitly; generic one-object stage/
flush must not bypass child receipts. The next phase needs an exact durable
chunk-plan publication/settlement journal and policy-bound publisher.

Done criteria: frozen independent legacy JSON/hash vectors first pass against
the preceding implementation; structural/key/hash-domain refusal tests; SQLite
reserve/publish/reopen, stale-content races, rename/detached/removal semantics,
ignored/deleted writes and rollback; real authenticated TCP protocol round trips
and explicit old reader/input refusal. Verify schema25 upgrade preserves legacy
bytes/operations and full local gate passes. No provider writes, credential or
visibility changes, native large-file claims or conversion of legacy content.

Focused results: 14 state tests and one authenticated TCP contract pass. Fault
injection exposed false saved-effect and substituted historical-result receipts;
checks now refuse both. Independent legacy vectors passed before the extension.
See [manifest root authority](manifest-root-authority.md) and its dated evidence
for archived logs. The full gate passed: 329 state, 59 S3, 34 core and 50
authority tests; all 18 managed publication contracts, formatting, Clippy, build,
62 doc-link files and 58 book chapters. Client settlement and mounted activation
remain unfinished.

### Next bounded slice: client chunk-root publication journal (proposal)

Keep this separate from native activation and from the legacy one-object journal.
Persist a new versioned record that binds the exact Ready plan input, attached
logical snapshot and manifest, authenticated store route, original content guard,
root descriptor/raw body checksum, reserve and publish IDs. Bind object-generation
UUID to reserve UUID. Do not derive a new ID or expected content after a crash.
Validate every saved field on load, with explicit retained rejection/conflict.

Before any authority request, persist its exact bytes. Recover a missing reply
using that same operation; never rebase an unknown request to the current volume
sequence. Before publication, require the root physical acknowledgment and the
plan's exact direct-child proofs through the existing owner. Persist the complete
Publish request before sending it, then the matching complete authority receipt.
Missing local acknowledgment, incomplete/uncertain/rival objects, a mismatched
store/plan/guard and SQLite-full must leave the source snapshot pending.

The first journal result should mean “this exact tree root was committed,” not
“the mounted file was flushed.” Current `logical::Record` requires a legacy
`managed::Input`, and `logical::Binding` rejects manifests. A root physical
GenerationId cannot be substituted into that chain. A later logical integration
phase must version this linkage, preserve the old serialized/hash contract and
atomically advance the exact snapshot's settlement marker without discarding a
newer dirty head. Handle/removal closing barriers must participate before native
callbacks can use the new path; no detached/owner shortcut may bypass them.

Done criteria for the journal slice: restart at each durable/network boundary,
real authenticated coordinator plus loopback S3 SDK upload, lost reserve/publish
responses, content race, corrupt/missing child/root acknowledgment, unchanged
uncertain operation identity, legacy refusal and actual SQLite-full rollback.
Require the local gate and its own source-bound Linux/Windows hosted regression;
no new provider mutation or mounted-file flush claim follows from those tests.


### Prerequisite: complete historical tree receipt — implementation 2026-10-18

The pre-journal legacy review found that `require_dependencies(root)` validates
only immediate children. A retained parent/root receipt can outlive deletion of
a grandchild's local flush/settlement records. Add one synchronous same-owner
read command that validates the Ready plan/attached snapshot/manifest/graph,
checks the retained receipt forest once, then matches every non-hole transfer's
exact physical generation, target, raw SHA, size, create-only guard, Verified
phase and single-generation acknowledgment. Return an opaque historical report
bound to that complete frozen context. Never retain CheckedFlushes across a
command, mutation or await. Have the S3 upload-tree wrapper finalize through this
report, preserving its existing getters and its non-file-flush meaning.

This report reads metadata, not every payload byte: explicit holes are recorded
as the validated plan's hole chunks; the existing uploader still freshly verifies
them during its bounded per-object walk. It does not certify current local media
or provider existence. No new persistent schema, metadata RPC, logical settlement,
write authorization, format migration or mounted limit follows. Full graph and
retained-history scans remain nonconstant work; no scalable latency claim.

Done criteria: empty/hole/data/multilevel reports retain the exact frozen source;
a deleted grandchild acknowledgment is refused even with immediate parent/root
receipts intact; missing/corrupt/redirected receipts and mappings fail without
rewriting evidence; a newer dirty head is preserved; reopening and read-only
operation at SQLite-full work; real SDK loopback tests verify report/object
counts, restart recovery and unchanged PUT identities. Run the local gate and
an independent source-bound hosted cohort. Publication journal and handle/removal
closing barriers remain the subsequent phase.


### Publication journal constraints from legacy review (2026-10-18)

The prerequisite report is a historical value; reacquire owner validation before
a publication transition. Keep CheckedFlushes within one immutable validation
section of the synchronous exclusive-owner command, after its final write and
with no further mutation or await before commit.
For later request recovery, resend the complete frozen Apply: bare operation
lookup alone does not validate request ownership. Preserve all unknown requests
and sequences. Initially classify only exact authority Conflict as a terminal
metadata rejection; a transport error is not rejection. Physical rejection needs
its exact recorded rejected operation and validated conditional 409/412 evidence.

A generic manifest publisher cannot reuse legacy authorize_publication because
it takes managed::Input, which must continue refusing ReserveManifest. Either
isolate its controller from native lifecycle APIs or add mutual persistent
exclusion: namespace/open/logical admission must see unresolved chunk-root rows,
and chunk publication must see unresolved namespace effects and removal owners.
A one-time absence check does not close this race. Start with attached normal
guards only; detached/removal modes remain explicitly unsupported until their
barriers and receipt links are implemented. Do not interpret OpenRecord.active()
as currently open or a close event as completion of all linked work.

Preserve all identity distinctions: source snapshot GenerationId, root physical
GenerationId, object-generation/reserve UUID, deterministic staging-operation
key and remote upload operation. Freeze the full plan/source manifest/store
route/guard plus reserve and distinct publish IDs; use raw page size/SHA for
physical proof, logical length/Merkle descriptor for the file tree. Legacy
logical acknowledgment requires its separate original-generation chain and
cannot be satisfied by the root physical flush. Newer dirty heads stay pending.


Prerequisite result: all 10 focused state transfer tests and 8 SDK tests pass.
The full local gate passed with 334 state/1 ignored, 59 S3, 34 core and 50
authority tests, all 18 managed publication contracts, formatting/Clippy/build,
63 doc-link files and 59 book chapters. All 244 frozen runtime/verification
hashes match. Independent final delta review found no blockers. See
[complete historical tree receipts](chunk-tree-receipts.md) and dated evidence.
No schema/RPC/native/provider or logical-settlement acceptance follows.

### Active bounded slice: isolated client root journal (2026-10-18)

Implement the persistence boundary first; the network controller is the next
independent slice. Schema27 adds a permanent client-role marker with a hash-bound
admission count and a bounded separate publication log. Freeze the Ready plan,
attached source manifest, authenticated base/store, original content guard and
exact reserve/publish identities. Retain complete requests and receipts. Never
modify the legacy journal serialization or acknowledge the original source.

Legacy managed/logical/namespace/open constructors must refuse this role even
after publication. Conversely, admission and every transition reject retained
legacy or authority state. No detached/removal guards are admitted. Each SQL
transaction reloads the exact record, role/count, plan and graph after its last
write; stages with a prepared Publish also require a freshly checked complete
historical tree receipt. The compact receipt proof exists only in the final
immutable validation section, with no writes or awaits before commit.

Done criteria: exact request recovery/reopen; historical success after a newer
authority publication; content-conflict retention; reciprocal lifecycle isolation;
cross-column operation-ID ownership; ignored/deleted/rewritten SQL effects and
post-write plan/receipt/isolation damage roll back; actual SQLITE_FULL preserves
the pending identity. Migrations retain old rows/bytes and reject damaged new
records. Run the full local gate and request this source's hosted regression.
No transport controller, mounted activation, source settlement or provider
mutation is included in this persistence slice.

Journal result: 15 focused tests pass; full local gate passes with 349 state/
1 ignored, 59 S3, 34 core and 50 authority tests, all 18 managed publication
contracts, formatting/Clippy/build, 64 doc-link files and 60 book chapters.
All 247 frozen hashes match, and independent final review found no blockers.
Own-source hosted regression passed at63164d0/run39153910031, all three jobs;
fresh21/14 native records and all35 raw fields/types matched. See the
[isolated journal contract](chunk-root-journal.md) and its archived evidence.

### Implemented slice: policy-bound chunk publisher (2026-10-18)

Build an explicit library controller over the isolated journal, existing chunk
uploader and authenticated authority client. Acquire base metadata directly from
that client; the legacy managed Reader intentionally rejects manifest bases.
Bind Store.route to the SDK reader's storage fingerprint, and independently bind
source/configuration fingerprint through the exact recovery target. Persist the
journal's full ReserveManifest before Apply; retain its complete matching reply.
Upload only through the current policy-bound tree executor, then freeze Publish
through fresh owner validation and send exactly that saved request.

Recovery must branch on saved stage. A prepared Publish is replayed directly,
with its historical tree proof, even if upload policy subsequently expires or
current source bytes are damaged. Do not rerun upload_tree, rebase sequence,
replace UUIDs or use bare operation lookup for that unknown CAS. A new upload
still needs fresh policy checks. Return an explicitly named root-commit result;
keep the original source snapshot pending and preserve exact authority Conflict.

Done criteria: real authenticated coordinator and real SDK loopback S3; lost
reserve/PUT/publish replies across controller reconstruction; exact request bytes
and operation IDs; no additional PUT after unknown metadata commit; recovery
with expired upload authorization/damaged current local bytes; rival publishers;
missing grandchild proof; wrong route/source/guard refusal; durable local failures
before a metadata send. Run full local gate and its own Linux/Windows cohort.
This phase still does not enable mounted chunk format, logical settlement,
detached/removal modes, provider writes or new native file-size limits.


Pre-implementation review adds a separate metadata-only Recovery factory with
explicit expected volume/store IDs and the same private Storage. It validates
retained journal state, needs no Executor/S3 credentials/policy, and can only
return a recorded result or replay an already-prepared exact Publish. An
unprepared record is a typed refusal with no RPC. No Features/Info/Node/Store
request is a prerequisite for that historical replay. The normal Publisher
retains authenticated metadata and exact Executor/store-route checks for new
work, and delegates its prepared branch to the same private recovery routine.

Implementation ownership: new s3drive-managed/src/chunks.rs and a separate CLI
integration-test binary/fixture. Keep old publisher, readers, JSON/hash, schemas,
wire protocol and native callbacks unchanged. Public API results mean root
commit only. Library admission remains bounded; no automatic retries or rebasing.
Test real child-process death after unknown root CAS, then recovery without S3
credentials/policy and with damaged source bytes; also expired policy, lost
reserve/PUT replies, independent content races, wrong binding, recursive proof
loss and local SQL write failure before send. Preserve pending original sources.


Publisher result: focused8 controls pass; final full gate50472 EXIT0,
349state/1ignored,59S3,34core,50authority,18existing publication tests and8new
controls,65Markdownfiles/61chapters. All249frozenruntime/fixturehashes unchanged;
independent review found no blockers.13member indexed local evidence retains
gate/source/review. See [publisher contract](chunk-root-publisher.md). Own-source
hosted regression remains pending; source settlement/native chunks/provider
acceptance are not established by this local gate.


### Following slice: chunk source settlement contract (proposal, 2026-10-18)

Do not pass the physical root generation to the legacy logical acknowledgment.
Current logical records bind `managed::Input` and require the physical receipt
size/SHA to equal the complete logical source; a manifest page has different
bytes, size and generation identity. The permanent chunk-client role deliberately
excludes these legacy APIs even after a root commit. Merely relaxing either
check would fabricate a file flush and reopen namespace/removal races.

First define a separate, versioned local linkage from the exact source snapshot
and manifest to the chunk plan, physical root, complete historical tree proof and
full authority receipt. Preserve all legacy JSON and hash vectors. Scope it to
the dedicated attached-only chunk client, retaining the existing reciprocal
legacy exclusion. Do not enable legacy namespace/open APIs or mounted callbacks
as part of this persistence slice. A distinct logical receipt/result must make
source settlement explicit instead of changing RootCommit's meaning.

The owner must freshly validate the exact frozen record, complete tree proof,
source lineage and authority receipt in one transaction before advancing a
versioned settlement marker. Repeated historical acknowledgment is idempotent;
a newer pending local head must remain pending, and a later remote root must
not rewrite the old receipt. Missing/corrupt proofs, disk-full, ignored or
rewritten SQL effects and owner death must not produce partial acknowledgment.
No payload reclamation, role release, migration of mounted users, detached or
removal-owner mode follows. Before implementing, review the exact legacy
logical/remote receipt invariants and choose the narrow linkage API; do not
silently extend generic remote flush semantics.

Acceptance: dedicated-state migration/isolation, legacy vectors unchanged,
newer heads/roots, restart at every local boundary, lost local replies,
concurrent repeated acknowledgment, conflict/unknown metadata refusal, recursive
receipt damage, SQLite-full and post-write trigger controls. Then real SDK plus
authenticated-authority recovery through the new controller, full local gate and
its own source-bound hosted cohort. Verified manifest range reads and native
lifecycle barriers remain later independently tested phases.


#### Concrete source-settlement contract (reviewed proposal, 2026-10-18)

The first slice settles **one exact frozen snapshot**. It does not suppress
ancestor generations or declare a whole file clean. This is a phase boundary,
not the final native flush contract: later lifecycle work must explicitly handle
superseded ancestors, unknown operations and retained conflicts before deciding
that any file has no outstanding work.

Use a separate owner-bound facade, provisionally
`Storage::chunk_publications().sources()`, with `settle_source(plan_id)` and
`source_status(plan_id)`. The caller supplies only the admitted plan ID. The
owner reloads the committed publication; a caller-supplied `RootCommit`,
`TreeReceipt` or authority receipt is never sufficient evidence. The new opaque
`SourceReceipt` has its own format/domain and binds the full committed record
hash, source target, source generation/manifest and physical root proof. It
preserves the complete historical authority receipt through that linkage.
Existing publication JSON and role hashes retain their current meanings.

Status must distinguish the requested snapshot's pending/settled state from its
publication phase (unprepared, prepared, committed or rejected), and report its
relationship to the current sealed local head. A settled older snapshot does
not settle that head. Matching the sealed head also says nothing about building
drafts; it is never a file-clean result. Unknown/rejected publication records remain inspectable;
no status API filters them out as a side effect of settling another snapshot.
An absent or corrupt plan is an error, not an empty clean result. There is no
new global file-clean or payload-deletion API in this slice.

Settlement is **historical metadata acknowledgment**. It does not rehash current
local payloads or query current S3 objects. Existing complete-tree receipts can
remain valid after local payload damage; that property supports recovery after
an upload and metadata commit. The new API must say this explicitly rather than
promise a fresh integrity check. It retains all payloads and does not authorize
eviction, reclamation or role release. Future bounded integrity checking and
verified range reads are separate requirements. Do not perform an unbounded
whole-file scan in one SQLite-owner command.

The transaction contract is:

1. Begin an immediate transaction on the same private SQLite owner. Reload and
   validate the complete frozen publication and exact source mapping. Require
   its durable complete successful authority receipt, attached guard and no
   rejection. Prepared/unknown publication cannot be settled.
2. Validate the source manifest and bounded same-target lineage to the current
   sealed head. Use `Input::local_target()`; generation order alone cannot prove
   ancestry. Preserve every newer sealed head and building generation. Reject
   generic physical remote intents, flushes or settlements on the exact source
   snapshot; a chunk-source receipt cannot mix those meanings.
3. Insert a separately versioned settlement and retained count/integrity state,
   or require byte-for-byte equality for an existing historical settlement.
   Keep the original `write_journal.status`, `remote_flushes` and
   `write_settlements` untouched. Consequently the generic staging pending API
   remains conservative; callers must use explicit chunk-source status.
4. After the final write, reload exact intended effects, the committed
   publication, source lineage, isolation and **every recursive tree receipt**.
   Refuse ignored, deleted or rewritten effects and damaged grandchildren. No
   write, callback or network await may intervene between this final proof and
   commit. Transaction failure leaves the previous settlement state intact.

This follows the existing
[publication validator](https://github.com/marshallr12/s3drive/blob/32d7cc7700797c0471e51c69904873d23b9c3889/crates/s3drive-state/src/chunk_publication.rs),
[source target/plan mapping](https://github.com/marshallr12/s3drive/blob/32d7cc7700797c0471e51c69904873d23b9c3889/crates/s3drive-state/src/chunked.rs)
and [staging invariants](https://github.com/marshallr12/s3drive/blob/32d7cc7700797c0471e51c69904873d23b9c3889/crates/s3drive-state/src/journal.rs).
The legacy
[logical receipt checks](https://github.com/marshallr12/s3drive/blob/32d7cc7700797c0471e51c69904873d23b9c3889/crates/s3drive-state/src/logical.rs)
and [physical ancestor acknowledgments](https://github.com/marshallr12/s3drive/blob/32d7cc7700797c0471e51c69904873d23b9c3889/crates/s3drive-state/src/remote/acknowledgments.rs)
remain unchanged. They are evidence for the design boundary, not APIs to relax.

Expected implementation footprint: schema 28 migration and schema validation;
new source-settlement child module under `chunk_publication`; reciprocal legacy
constructor guards; authority initialization/preflight; authority snapshot table
classification; migration/rewind fixtures; focused state controls. Validate
settlements through a nonrecursive helper: do not create a cycle between the
publication and settlement validators. Retained counts/hashes must detect lost
rows, including an orphan settlement with a missing role. Bound history by the
existing 256-publication limit; declare and test a separate finite lineage bound.
No wire, S3 object format, legacy hash vector or native callback change belongs
in that persistence slice. A later controller slice must invoke the facade
after durable root publication/recovery and prove restart/unknown-reply behavior
with the real SDK and authenticated loopback authority.

Required negative controls include missing root/child/grandchild receipts,
wrong source or manifest, mixed generic source intent, unknown/rejected Publish,
lost settlement/role rows, post-write trigger corruption and isolation injection,
actual SQLite-full rollback, and process death around commit/reply. Positive
controls include repeated/concurrent settlement, a newer dirty local head,
a newer remote root, metadata-only historical recovery after local payload
damage, generic pending unchanged and all legacy hashes unchanged. A lost local
reply must recover the identical receipt, not create a second settlement.
Read-only legacy review is complete. The corrected TLS fixture cohort has passed.
The state persistence slice is now implemented on `codex/chunk-source-settlement`;
31 focused controls passed, including 16 new controls. Full local verification
also passed: 365 state tests / 1 ignored, unchanged 252-file frozen map, existing
integration suites and 62 book chapters. This source's own hosted cohort remains
pending. See
[exact source acknowledgments](chunk-source-settlement.md). Controller integration
still follows as a separate phase; no mounted chunk activation follows here.

#### Next controller slice: explicit source acknowledgment (proposal, 2026-10-18)

Implement after the schema-28 state cohort is accepted. Add
`Publisher::resume_source(plan)` and `Recovery::resume_source(plan)`, returning
the state's opaque `SourceReceipt`. Keep `resume()` returning `RootCommit`
with its existing root-only semantics. Neither result declares every ancestor,
building draft or current file clean. This extends the existing
[chunk controller](https://github.com/marshallr12/s3drive/blob/41b759aeb09511f2e27956c8c6a0110bfe283dcb/crates/s3drive-managed/src/chunks.rs)
and uses the implemented [source-state facade](chunk-source-settlement.md).

Hold one controller-family operation permit across root recovery/publication
and local source acknowledgment. Refactor the existing publisher body into a
private helper called under that permit; calling public `resume()` from another
permit-holding method would fail its own Busy check. Cloned controllers share
the permit, while independent recovery controllers still rely on the SQLite
owner's transaction and idempotence checks. The acknowledgment must reload the
exact durable plan through the same owner's source facade. A returned
`RootCommit` alone is not sufficient proof.

`Recovery::resume_source` takes no S3 source, SDK executor, reader, credentials
or write policy. Scope the saved record to the expected volume/store first.
For an already committed root, perform zero metadata RPCs and zero S3 requests;
for a prepared unknown root, resend only the complete saved Apply, persist its
complete receipt, then acknowledge locally. Do not fetch fresh Features, Info,
Node or Store, replace operation IDs, rebase a sequence, or prepare new work.
The publisher's prepared/committed recovery branch must precede current S3
binding, policy admission, upload and fresh metadata reads as it does today.

Missing, unprepared, rejected, damaged or wrong-scope records must not yield a
source receipt. If root publication succeeds but local acknowledgment fails,
retain the root receipt and all source bytes so recovery can finish locally.
Retain unknown requests on transport or local receipt-save failure. No new
wire/schema/object format, payload reclamation, ancestor suppression, role
release or native callback activation belongs in this controller slice.

Use the existing real SDK/dummy loopback S3 and authenticated Rust authority
harness. Add end-to-end controls for:

- Binary/Unicode, sparse and empty sources; old root-only calls leave source
  status unsettled. A later source acknowledgment does not settle ancestors;
  first acknowledgment of an older root after a newer root/head stays historical.
- Lost Publish reply followed by actual killed-owner restart, unavailable dummy
  credentials/policy and damaged current payload. Recovery sends the exact saved
  Apply and performs no S3 requests, preserving a newer local head and remote root.
- Root receipt saved but source-row insertion refused, followed by killed-owner
  restart. Recovery of that known root performs no RPC or S3 request and returns
  the same historical receipt while preserving newer work.
- Expired/narrowed policy recovery, local publication-receipt-save failure,
  same-family Busy refusal while a request is held at an explicit barrier, and
  concurrent first source acknowledgment through independent recovery controllers.
  Compare complete requests/receipts and count unique object PUTs.

Reuse the existing subprocess fixture and add a small child test module rather
than copy its server and credential setup. Bound every observation and reap
owned children on failure; a killed child must have an unsuccessful exit status.
State-level SQLite-full, recursive grandchild damage and postcommit lost-reply
controls remain separate evidence, not claims of new controller fault coverage.
Accept the slice only after focused integration tests, the full local gate and
its own source-bound Linux/Windows/book cohort. Mounted chunk lifecycle and
verified manifest-range reads remain subsequent phases.

### Current acceptance repair: TLS budget fixture (2026-10-18)

Own publisher CI39156593000 failed Linux before the chunk tests, in the existing
pinned CLI restart test's assertion that all16 budgeted diagnostics are malformed
handshake failures. The log proves the category assertion failed after exactly17
events, but omits actual categories; the cause remains unproven. Windows and the
book job ultimately passed; the overall cohort failed. Do not rerun it or treat old artifacts
as publisher acceptance. Diagnose the fixture first. Earlier valid TLS exchanges
share its diagnostic budget; completed-response shutdown events are a hypothesis.
Retain strict16+1 budget, healthy authenticated service after overflow and private-
input refusal. If isolating the malformed flood, use a fresh listener and keep
restart/identity/receipt tests intact. Production diagnostics/retry/scheduling and
chunk format remain unchanged. Full local gate and own fresh hosted cohort are
required after any source correction. Settlement work follows this repair.


Repair result: final focused10/3ignored PASS; fullgate68896 EXIT0 with249frozen
hashes unchanged,349state/1ignored,59S3,34core,50authority,18existing managed
publication tests,8chunk controls,10TLS/restart controls,65docfiles/61chapters.
Independent review matched249/249 and closed the raw-cap truncation gap, whose
negative control failed before the guard. Sole test-file delta; production,
schema, wire and native paths unchanged. Corrected source `32d` hosted cohort
39158147595 passed Linux, Windows and book checks; all 3 raw logs were reviewed.
Its own 21/14 typed artifacts and all 35 raw field/type comparisons passed. The indexed 22-member
hosted archive retains its exact evidence. See
[fixture contract](owned-authority-client.md#tls-budget-fixture-isolation--2026-10-18)
and its dated evidence; the captured prior CI category remains unknown. The
next implementation work is the separately scoped source-settlement persistence
contract above, followed by controller integration. Existing native regression
does not activate mounted chunks or establish physical reboot/provider acceptance.
