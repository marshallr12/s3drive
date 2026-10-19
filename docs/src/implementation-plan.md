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
integration suites and 62 book chapters. This source's own hosted cohort passed all three jobs; raw logs and its own
21/14 Windows artifact records were reviewed and strictly compared. See
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

Current status: schema-28 source `41b759a` is accepted through its own three-job
cohort, raw review and independent artifact comparison. Controller methods and
four additional real-SDK integration scenarios are implemented on
`codex/chunk-source-controller`. Focused tests and the full local gate passed:
12 integration controls, 365 state tests / 1 ignored and 253 frozen hashes
unchanged. Its own hosted cohort passed all three jobs; raw review and
independent 21/14 artifact plus 35 raw-record comparisons passed. No mounted
chunk activation follows from either library slice.

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

#### Subsequent slice: verified manifest range reads (proposal, 2026-10-18)

After controller acceptance, add a separate managed manifest reader that opens
an authenticated authority node and retains its exact root. The current
[managed Reader](https://github.com/marshallr12/s3drive/blob/41b759aeb09511f2e27956c8c6a0110bfe283dcb/crates/s3drive-managed/src/reader.rs)
intentionally refuses manifests and retains whole-file buffers. Preserve that
explicit refusal and the old format while building the bounded reader; do not
raise native limits or replace its snapshot type with an unverified lazy view.
The existing [codec verification](https://github.com/marshallr12/s3drive/blob/41b759aeb09511f2e27956c8c6a0110bfe283dcb/crates/s3drive-core/src/manifest.rs)
and [role-specific object mapping](https://github.com/marshallr12/s3drive/blob/41b759aeb09511f2e27956c8c6a0110bfe283dcb/crates/s3drive-core/src/manifest/objects.rs)
provide the structural/hash primitives; they do not authenticate a caller's
descriptor or confer S3 authorization.

Bind the expected volume/name policy, feature and store route through the
authenticated Rust metadata client. Obtain the manifest descriptor from that
client's validated live Node response, never an arbitrary caller-supplied root.
Validate payload/store/node/volume/generation context and derive the root key
through `Namespace`. HEAD the root using the mapped S3 reader, require the exact
published size/ETag/version, read its bounded complete bytes and verify both its
ordinary physical SHA and the descriptor's domain-separated page hash. They
are different hash meanings. Keep that root immutable for the open handle;
rename, replacement or a later root must not silently retarget it.

For each bounded range, traverse only intersecting authenticated child entries.
Verify each complete bounded page against its parent's hash, identity, start,
length and decreasing level before following any reference. Derive data/page
keys with the pinned file namespace and require the mapped store. Use the
existing S3 pinned HEAD/range-GET handle for each fetched object, require its
expected size, and verify the entire data chunk before exposing its subrange.
Only authenticated Hole entries produce zeros; missing, denied, truncated or
changed objects are errors. No missing-object fallback or whole-file scan.

Declare finite output, concurrent-read, traversal and temporary-buffer budgets
before implementation. Keep read lengths within the current S3 reader's 8 MiB
bound and data verification within the codec's 1 MiB chunk bound; check EOF and
arithmetic before allocation. An initial reader without shared persistent cache
can establish integrity and online authorization first. Any later cache must
preserve credential/configuration isolation and require current authorization;
cache presence never grants access. No new database is needed for this slice.

Real-SDK/authenticated-authority acceptance must cover empty/sparse/binary/Unicode,
unaligned ranges, cross-chunk and cross-page reads, EOF and limits; request counts
must show that opening/reading a small range does not fetch the whole tree/file.
Corruption outside the returned portion of a fetched chunk must still fail.
Test wrong hash/domain/context/store/role, malformed pages and depth, changed
ETag/version, missing/denied/truncated/oversized responses, cancellation and
concurrent read capacity. Prove pinned old-root reads after newer publication
and rename while new opens see the new root. Existing native callbacks, legacy
whole-file readers and mounted-format activation remain separately gated.

##### Concrete reader contract and footprint (proposal, 2026-10-18)

Read-only source inspection found an additional dependency boundary: the root
`ReadHandle` already captures one credential generation, while the original
`S3Reader` may use a provider that supplies a different generation on a later
open. The new file handle must open child pages/data through the root handle's
captured provider. Add a narrow `ReadHandle::open_related(key)`-style method
that performs a fresh scoped HEAD with those captured credentials; do not
expose credentials or let a caller substitute metadata/configuration. Keep the
root handle alive for the file snapshot. Expired captured credentials fail
explicitly; a new file open can obtain newly resolved credentials. This follows
the existing [open implementation and rotating-provider control](https://github.com/marshallr12/s3drive/blob/474601edd5582ebed9316d04cf4524db6d72694b/crates/s3drive-s3/src/reader/tests.rs).

Do not assume that editing an INI file hot-reloads an existing SDK provider.
The resolved `aws-config` 1.12.0 profile provider caches parsed profile files
after its first load. The planned rotation control should extend the existing
injected `RotatingCredentials` fixture and inspect signed dummy requests, rather
than depend on profile-file rereading. This is a dependency-source finding,
verified 2026-10-18, not a new S3Drive reload feature.
[AWS SDK profile-provider documentation](https://docs.rs/aws-config/1.12.0/aws_config/profile/struct.ProfileFileCredentialsProvider.html).

Proposed public surface: a reader under `s3drive_managed::chunks`, configured
from an authenticated metadata client, expected volume and at most 16 mapped
stores; `open_file(node_id)` returns an opaque cloneable pinned file, whose
`read_range(offset, length)` returns a complete verified bounded buffer or an
error. Expose its pinned Node/logical length for callers. The legacy managed
Reader and `FileSnapshot::bytes()` retain their current meanings. No arbitrary
caller-supplied descriptor constructor or mounted callback dispatch is added.

Distinguish two empty-file states. An authenticated live `Kind::EmptyFile` Node
without a payload has no S3 object to authorize; validate its optional placement
and return a pinned zero-length view using metadata authorization alone. A
published empty manifest is a `Kind::File` with a real root page and must still
perform the root HEAD/GET/hash checks at open. Missing payload on `Kind::File`
is an integrity error. Legacy one-object payloads remain explicitly unsupported
by this new reader rather than being mistaken for empty files. Deleted nodes
fail new opens; an already pinned empty/manifest view retains its identity.

Initial finite budgets, per shared reader family:

- Four active open/read operations, acquired without an unbounded queue; a
  clone shares the same permits. One permit covers the complete operation.
- 256 retained pinned file snapshots; clones of one snapshot share its retained
  permit. Acquire permits before metadata/provider work; errors and cancellation
  release them. These are library ownership bounds, not total SDK/RSS bounds.
- At most 8 MiB output and one complete 1 MiB data buffer per active range read.
  Read pages within the 10,432-byte codec bound and retain only the bounded
  authenticated path/current page needed for traversal. No whole-tree cache.
- One overall 30-second open/read deadline, including all metadata/provider
  awaits, in addition to existing per-request deadlines. Return no partial buffer
  after integrity failure or timeout. Zero-length/EOF reads make no network call.

Require the verified root page's level to equal the level encoded in the
validated descriptor ordinal. The authority's manifest length/ordinal policy
admits at most level 2 (64 GiB logical length), even though the general codec
admits level 7. A root with a different actual level must fail. Child verification
then strictly decreases level while checking exact covered ranges. An 8 MiB
unaligned read intersects at most nine chunk intervals; conservatively budget
27 visited pages and nine complete data objects per read. Count visits and
fetched bytes explicitly, skip authenticated holes by their recorded lengths,
and fail on exceeded bounds. These are proposed algorithm budgets, not measured
request counts or new file-size support. See the current
[authority descriptor policy](https://github.com/marshallr12/s3drive/blob/474601edd5582ebed9316d04cf4524db6d72694b/crates/s3drive-state/src/namespace/publication/manifest.rs).
Do not require a reused child/data reference's generation or ordinal to equal
the current root generation or logical chunk index: the authenticated reference
names its own immutable object. Keep role/store/file namespace checks intact.

Expected write footprint: additive S3 ReadHandle method and its existing
credential/scope/HEAD-denial/future-size controls; new managed chunk-reader child
module and an additive module export; a dedicated real-SDK/authenticated-authority
integration test with owned dummy S3 fault/gate fixture; this book/evidence.
Prefer a new focused read fixture over changing the widely shared write fixture
merely to inject arbitrary page faults. No state migration, wire/codec vector,
native callback, write-policy or credential-file change is needed. Before coding,
inspect the resolved dependency APIs used by deadline/semaphore/test gates and
verify that the chosen traversal satisfies the stated visit/buffer budgets.

The S3 helper's meaningful controls must show related HEAD and GET retain the
root's access key/session token after an injected provider rotation, independent
new opens use the new generation, and denied/missing related objects never reuse
cached authorization. Keep the future's inline size bounded as the existing
S3 open control does; size checks cannot substitute for native stack acceptance.
The managed tests must additionally prove both physical and page hash checks,
actual root-level refusal, all-or-error cross-chunk corruption, explicit request
and byte counts, capacity at controlled barriers, cancellation/released permits,
and preserved old-root reads after newer publication/rename. No new API is
implemented by this contract. The full managed reader follows acceptance of
the controller cohort. Its additive pinned-credential S3 helper can be developed
as independent preparation in an isolated checkout from the accepted S3 baseline:
it does not depend on source settlement or alter the controller checkout's frozen
files. It still needs focused tests, its full local gate and source-bound hosted
acceptance before integration.
The [dated source-audit evidence](evidence/manifest-range-reader-contract-20261018.json)
records the proposed budgets and resolved dependency hashes.

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

### Independent preparation: pinned related-object reads (2026-10-18)

Source settlement is accepted at checkpoint `20706cd`; controller work proceeds
separately on `codex/chunk-source-controller`. Read-only design inspection for
the future manifest reader identified an independent S3 API gap. Add
`ReadHandle::open_related(key)` on the captured credential generation, with fresh
online HEAD, existing scope/deadline/revision checks and no exposed credentials.
Keep the primary controller checkout frozen while this preparation is tested in
an isolated checkout from the accepted S3 baseline. No managed reader or native
activation follows from this helper. See [contract and controls](pinned-related-reads.md).

Done criteria: injected rotation through chained root/child/data operations,
independent new generation, exact scope and GET preconditions, fresh denial and
absence refusal, invalid input before requests and bounded future layout; then
the full local gate and this source's own hosted cohort before integration.
The focused S3 suite passed 62 controls. The full local gate passed with
252 frozen hashes unchanged, 62 S3 / 365 state tests and 63 book chapters.
Its own source-bound cohort 39167583915 has passed all three jobs and strict
21/14 artifact plus raw35 comparisons. Both prerequisites are accepted.

### Verified manifest range reader (2026-10-18)

Implement the [reader contract](manifest-range-reader.md) in an isolated checkout.
Controller and helper acceptance are complete; combine their accepted sources
before the full reader gate. Scope: new managed chunk-reader child module/export,
dedicated owned real-SDK/authenticated-authority read fixture and book/evidence.
Keep legacy Reader/FileSnapshot, schemas, wire/codec vectors and native callbacks
unchanged. Done: declared range/integrity/authorization/budget/cancellation
controls, full local gate and its own hosted cohort before native activation.
Focused reader controls passed 5/5 in 4.68s, including sparse 64 GiB metadata.


Next proposed slice after this reader's own acceptance: expose authenticated
manifest metadata and bounded read-only snapshots through the filesystem layer.
Current managed namespace validation explicitly refuses manifests, file handles
hold the whole-byte FileSnapshot, and writes use that legacy snapshot. Therefore
native integration must cover lookup/list/getattr logical sizes as well as open
and range-read dispatch; merely switching the byte-read call is insufficient.
Keep legacy empty/one-object behavior compatible, retain pinned handles across
rename/republication, map Busy/integrity/authorization errors explicitly and
refuse chunk writes until their independent acceptance. Require dedicated real
FUSE and WinFsp chunk-read controls with Unicode, two stores, sparse/cross-page
ranges, late corruption, denied objects and no whole-file download. This is a
source-inspection plan, not mounted acceptance or a finalized public API.

Reader result: combined full local gate84960 EXIT0, all256 frozen hashes
unchanged, 12 publication/source +5 reader +62 S3 +365state/1ignored,64chapters.
The 14-member local archive is indexed and verified. Own hosted cohort
39171113280 passed all3 jobs, reviewed raw logs and strict21/14 + raw35 checks.
The indexed22-member hosted archive is verified. Native integration follows
in codex/native-manifest-reads; portable controls do not activate native chunks.

### Draft native manifest reads (2026-10-18)

Worktree /home/marshall/src/worktrees/s3drive-native-manifest-reads, branch
codex/native-manifest-reads, from b3e30f0. The reader256 runtime remains frozen
in its own worktree; CI39171113280 is accepted. The native local gate passed
with257 hashes unchanged, and actual FUSE/WinFsp acceptance follows on its own source. See
[native read integration contract](native-manifest-reads.md) for footprint,
compatibility, failure behavior and engine/FUSE/WinFsp done criteria.

### Next write integration: load-bearing source findings (2026-10-18)

Proposal, pending native read acceptance. The existing logical writer explicitly
rejects manifest bases and caps a file at8MiB (`logical::Binding::validate` and
`MAX_BYTES`). Raising that constant would retain whole-file buffering and hashing.
The offline chunk planner accepts a64GiB logical size but currently prepares a
complete local staging snapshot under configured object/scan quotas; it has no
authenticated remote-reference overlay.
These are distinct boundaries. See the accepted source's
[logical binding](https://github.com/marshallr12/s3drive/blob/88d02f7a6605bf13bfd212d67c6609ae16b0ebfd/crates/s3drive-state/src/logical.rs),
[full-snapshot planner](https://github.com/marshallr12/s3drive/blob/88d02f7a6605bf13bfd212d67c6609ae16b0ebfd/crates/s3drive-state/src/chunked.rs)
and existing bounded range composer in crates/s3drive-fs/src/edits.rs.

The write design therefore needs a durable sparse edit view based on an
authenticated pinned manifest, bounded changed-chunk staging and reuse of
unchanged authenticated references. Persist the exact base content CAS guard,
local dirty identity and eventual publication request before network mutations.
Keep root commit and source acknowledgment distinct; retain conflicting or unknown
work after restart. Do not reinterpret a manifest root hash as a whole-file SHA
or silently route chunks through the legacy logical writer.

Boundary edits/truncation must verify complete old chunks before using their
bytes. A shortened final data object requires new bytes/hash/length; a prefix of
an old object is not a valid unchanged reference. Growth can change tree height.
A previous root may be reused as a child page when its authenticated identity,
start, length and level match the new parent: roots already use the page-key
namespace. Reuse still needs inherited-base provenance, not a fabricated fresh
upload receipt. Existing reader controls allow older generations/ordinals while
enforcing same store/file/role scope. Preserve those checks during reuse.

Plan independently green storage/edit/publication/controller phases before native
write callbacks. Keep one-object writes compatible and make new format activation
explicit. Required eventual controls include sparse growth to64GiB without whole-
file allocation, small edits with measured unchanged-chunk reuse, truncate/append,
rename during dirty edits, unknown root replies, killed owners, SQLite-full rollback,
local/remote conflict retention and real FUSE/WinFsp flush/read/restart behavior.
This section records source findings and proposals, not an implemented write API.

### Incremental writes: proof and settlement boundaries (2026-10-18)

Source inspection at `c633d02`, proposal only. Reusing old references needs a
separate mixed-tree proof, not just an edit overlay. The current
[`require_dependencies` and `inspect_tree_receipt`](https://github.com/marshallr12/s3drive/blob/c633d02d47ccf2da20cfadd50e0b8e11f6e06811/crates/s3drive-state/src/chunked/transfers.rs)
require a mapped local generation and exact historical flush receipt for every
non-hole object. `inspect_transfer` derives that mapping from this complete plan's
snapshot and object identity. A reference from another machine's authenticated
manifest has no such local upload history. Do not invent a local receipt, adopt
an arbitrary bucket object, or weaken the existing full-snapshot verifier.

The current [source settlement validator](https://github.com/marshallr12/s3drive/blob/c633d02d47ccf2da20cfadd50e0b8e11f6e06811/crates/s3drive-state/src/chunk_publication/sources.rs)
also requires a sealed local journal generation with the complete source size and
manifest. A sparse view with remote-backed unchanged ranges is not that snapshot.
It needs its own exact-view acknowledgment, preserving newer dirty heads and
unknown/conflicting work. A remote root commit alone must not mark a local edit
view clean. The [reader](https://github.com/marshallr12/s3drive/blob/c633d02d47ccf2da20cfadd50e0b8e11f6e06811/crates/s3drive-managed/src/chunks/reader.rs)
currently exposes metadata and verified bytes, not an opaque, persistable
provenance export for inherited references.

Implement after repaired native-read acceptance, in independently green phases:

| Phase | Concrete contract | Required evidence |
| --- | --- | --- |
| W1 authenticated base provenance | Export bounded manifest paths from an authorized pinned File; bind inherited data/page references to volume, file, store, role, exact base descriptor and content CAS guard; no arbitrary-reference constructor | Wrong file/store/role, forged/short paths, older valid generations, valid/invalid old-root child shapes, bounded traversal/cancellation; no data download for unchanged-reference discovery |
| W2 durable sparse edit views | SQLite stores exact base binding plus immutable local patches/zero ranges, size and view lineage; persist operation keys before changes; retain dirty bytes across restart | 64 GiB sparse growth with bounded memory/disk growth, competing/lost-reply edits, process kills, corrupt rows/media and SQLite-full rollback; bounded read-your-writes composition |
| W3 incremental tree preparation | New changed objects plus tagged inherited references; persist bounded preparation cursor and new/mixed proof format; retain old full-snapshot planner/verifier | Measured one-byte edit reuses unchanged data, authenticated path coverage, final partial-chunk truncate/growth, 127/128 chunk boundary, tree-height growth and holes; no manufactured flush receipts |
| W4 mixed-tree publication | Upload new dependencies before pages/root; inherited dependencies require validated base provenance; persist exact original CAS and operation identities; recover unknown results without unconditional PUT | Two-machine base conflict, lost reserve/PUT/root replies, renamed/deleted base, restart and quota failure; unchanged chunks produce zero PUTs and no full-file download |
| W5 exact edit-view settlement | Separate root commitment from acknowledgment of one immutable sparse view; preserve ancestors/newer drafts according to explicit coverage rules; no generic full-snapshot acknowledgment substitution | Kill/response-loss around both acknowledgments, newer dirty head, corrupt proof, disk-full rollback; repeat acknowledgment recovers the same result and cannot clear another view |
| W6 native chunk writes | Explicitly activate shared engine and FUSE/WinFsp write/truncate/append/flush through the accepted mixed-tree controller; keep one-object compatibility | Actual mounted Unicode, large/sparse/random edits, application save/replace, open-unlinked/rename, restart/conflict/uncertainty, measured reuse; separately authorized Wasabi conformance and sign-in/reboot/session checks |

For W3/W4, inherited proof establishes reachability from the authenticated frozen
base, not current object availability or permission to reclaim it. Missing/denied
objects remain explicit errors; reference retention and future garbage collection
must honor pinned bases and dirty views. Earlier planning text incorrectly
assigned a distinct physical root role. In the actual
[`Namespace::root_key`](https://github.com/marshallr12/s3drive/blob/c633d02d47ccf2da20cfadd50e0b8e11f6e06811/crates/s3drive-core/src/manifest/objects.rs)
implementation, roots use `Role::Page`. Reusing an old root as a child is eligible
when the new parent's expected hash, file identity, start, length and level pass
[`VerifiedPage::verify_child`](https://github.com/marshallr12/s3drive/blob/c633d02d47ccf2da20cfadd50e0b8e11f6e06811/crates/s3drive-core/src/manifest.rs).
This does not convert its historical receipt into a new upload receipt; the new
mixed-tree verifier must admit explicit inherited provenance. Shrinking/growing
a partial data object changes its length
and hash, so verify the complete old chunk and prepare new bytes. Preserve the
existing root/page and whole-payload/page-digest distinctions.

On restart, never rebase a dirty view onto the coordinator's latest payload.
Retain its original binding and CAS guard; a different current payload is a
conflict. Define and test authorized re-opening of the persisted historical base
in W1/W2, including revoked/rotated credentials, missing root and changed source
route. Persist no credentials. An inherited-reference proof is not a network
send permit; W4 still requires the configured, source-bound provider write policy.

These phases are a proposed implementation sequence, not implemented APIs,
schema migrations, mounted write support or provider acceptance. W1-W5 must
converge on W6; library-only milestones do not complete the drive goal.

### Native snapshot identity repair (2026-10-18)

Actual source d98 Linux CI reached the real FUSE test and failed after a new
4-byte root publication: fstat on an old130MiB handle returned4. The shared inode
identity and path-refresh metadata are insufficient when the kernel omits a file
handle or updates shared inode attributes. Do not remove the old-size assertion.

Repair draft in codex/manifest-snapshot-inodes: read-only manifest node identities
include their authenticated payload reference; rename without content change
keeps identity. Cache only the validated lookup version's metadata on that inode.
New path lookup and file open still authenticate online. Check the expected
payload reference both before and after fresh snapshot open, refusing a race
instead of opening a different version under the old inode. Legacy/mutable file
identities remain unchanged. Add portable publication/rename/race controls and
retain the actual native size/bytes check on Linux and Windows. Source d98's
cohort is terminal FAILURE (Windows native passed2.39s; Linux native failed);
its artifacts cannot certify the repair. The original watcher closed exit1.

Repair checkpoint: full local gate passed (session68406, unchanged257inputs),
including both stale/open-race controls and all existing portable regressions.
The13member indexed local archive retains the old failed cohort and negative
control alongside the corrected source. Actual repaired native acceptance is
pending; do not activate chunk writes or promote this source on the local gate.


Native read acceptance checkpoint (2026-10-18): repaired source `c633d02` has
completed own run39178517491 attempt1 with all3 jobs successful, both dedicated
real native manifest controls passing, reviewed raw logs, own strict21/14
artifacts/project oracles and all35 raw-field/type matches. Indexed26member hosted
evidence is verified; see the [native read contract](native-manifest-reads.md).
This is scoped native read acceptance. Independent W1a development proceeds from
the accepted library reader in its own checkout while preserving this source;
chunk writes, provider conformance and actual reboot remain later requirements.


### W1a independent reference-proof foundation (2026-10-18)

Develop from accepted reader7c2b454 in codex/manifest-reference-proofs while the
separate repaired native cohort39178517491 finishes. Scope is an opaque proof
of one data/hole chunk's reachability from an authorized File snapshot, bounded
metadata-only export and pure revalidation against the same exact base. Encoded
proofs contain no credentials and confer no upload authority or byte-health
receipt. Rename preserves content identity; a changed root or another file must
reject the proof. Aggregate holes still yield one clipped logical chunk.
This is W1a, not complete inherited-page reuse, historical base reopening,
sparse durable edits, new publication proofs or mounted writes. Native integration
continues to require acceptance of its own frozen source. Require focused real
SDK/authority controls and full local/own hosted gates before integration.


Source correction for later page reuse: `Namespace::root_key` delegates to
`key(Role::Page, ...)`; roots do not have a separate physical key role. An old
root can be inherited as a child when the new parent expects its exact file,
start, length, level and hash (`VerifiedPage::verify_child`). Do not require
unconditional re-materialization merely because it was once a root. New mixed
publication proofs still need explicit inherited provenance, not a fabricated
new upload receipt. W1a's data/hole witness does not yet export subtree reuse.


### Combined read-only snapshot format audit (2026-10-18)

Merge accepted native52f591c and locally green W1a ed2e93b. Add controls for old
metadata-empty and legacy snapshots across manifest publication, legacy
republication and format reversal. Compare inode attributes and old bytes with
fresh lookup/open; reject stale opens and races before returning a mismatched
snapshot. First reproduce any failure without changing production code. If
confirmed, extend version identities/lookup attributes to all files in the
read-only chunk-capable backend and guard both manifest and legacy reader opens.
Keep direct, writable and missing-capability fallback behavior scoped unchanged.
Require real Linux/Windows controls plus a fresh combined local/hosted cohort;
prerequisite artifacts cannot certify this source.

Result: combined source8c3448f passed its full local gate and its own hosted
run39184629344, all3 jobs/raw reviewed. Actual FUSE1.63s and WinFsp2.53s include
all-format snapshots and original old-handle assertions. Own artifact ZIP
digests, strict21+14/project oracles and all35 raw fields/types passed; no prior
artifacts reused. The indexed29-member hosted archive is verified. W1b page
reachability passed independent local gate95188 and now combines
this accepted source; require its own combined gate before integration. Native chunk
writes, sparse SQLite views, historical bases and mixed-tree proofs remain later.

### W1b independent page reachability (2026-10-18)

Build from local-green W1a ed2e93b while combined8c3448f runs its own native
cohort. Export opaque root/subtree PageProof witnesses, bounded to two metadata
pages and revalidated against an authorized File. Prove a page containing a
requested offset at a specified level; return its complete authenticated span.
Encoded witnesses identify the actual page start/level, never an arbitrary
relocated subrange. Root pages already use Role::Page, so test valid old-root
reuse as a child plus invalid geometry, without fabricated upload receipts.
Published-empty manifests have a root page even though no data chunk exists;
metadata-empty files have no root page. No data GET/HEAD during proof export,
shared operation capacity/deadlines/cancellation, strict cross-kind/foreign-base
and malformed witness refusal, local/own hosted gates required. Historical base
reopen, sparse SQLite views, mixed publication and settlement remain separate.

W1b independent result: full local gate95188 EXIT0, all260 frozen inputs
unchanged;7reader,12publication,62S3,365state/1ignored,fmt/clippy/build/book.
Indexed16-member local archive verified. Combine accepted snapshot/proof
checkpoint da84d3a (runtime8c3448f, own all3/native/raw/typed21+14/raw35 passed),
then require a fresh exact combined gate and own hosted cohort.

Combined W1b candidate bf2e6ae includes accepted snapshot/proof checkpoint
da84d3a. Its own full gate68162 passed with261 unchanged frozen runtime inputs,
9reader/format1nativeignored,12publication,62S3,365state1ignored and all book/build
checks. The indexed19-member combined local archive is verified;
independent3890584 local evidence alone does not accept the combined source.
Own hosted Linux/Windows/book native and artifact acceptance is still required.

Candidatebf2e6ae is published on codex/manifest-page-proofs. Its
[own Linux/Windows/book run](https://github.com/marshallr12/s3drive/actions/runs/39187701991)
is in progress with the actual native checks retained. Frozen261 runtime hashes
match the tested local source. Own source-bound raw/artifact acceptance remains
pending; accepted8c3448f artifacts cannot certify this new helper.

W1b result: runtimebf2e6ae own run39187701991 ALL3 SUCCESS, all raw reviewed,
both actual native controls and own ZIP digests/typed21+14/project oracles/all35
raw fields/types passed. Indexed31-member hosted archive verified. No older
artifacts reused; all261 frozen inputs match. Next combine this accepted source
with local-green W1c1 e18d6f5, then implement its documented W1c2 reader reopen
contract before sparse edit views. W1b does not activate writes or provider calls.

### W1c1 authenticated historical manifest metadata (2026-10-18)

Source audit at accepted `da84d3a`: `namespace_uploads` already retains immutable
reservation descriptors and their committed publication payloads. Add a bounded
primary-key lookup in one SQLite read transaction, with an additive
`manifest-history-v1` capability and `ManifestPayload` request/reply. Require
volume, node, reservation and exact original payload-reference hash. Validate
the retained reservation operation/receipt, its manifest geometry/store route,
published payload equality and expected reference. Unpublished/unknown records,
wrong file/volume/reference and malformed or damaged history fail explicitly.
No schema migration or full operation-log scan is needed.

The existing volume bearer authentication applies to history too. A deleted or
renamed current node does not rewrite an already published immutable version;
history lookup grants no namespace mutation, S3 write policy or settlement.
Clients must preserve their original CAS guard. Later reader integration must
reacquire current provider credentials, HEAD/GET/hash the original root and
revalidate the configured store route; it must not accept a cached arbitrary
Node as an authorized File or silently substitute the latest payload.

W1c1 scope is authority metadata only, in codex/payload-history-authority from
accepted da84d3a while separate W1b CI runs. Require old-version lookup after
replacement/rename/removal/restart, exact bad-binding/record-corruption refusal,
unpublished and unsupported legacy refusal, bearer/volume enforcement and unchanged
sequence/records. Run the full local gate. Persisted base-token/reader reopening,
legacy/metadata-empty recovery, sparse edit views, mixed-tree publication,
exact-view acknowledgment and native chunk writes remain subsequent work.

### W1c2 pinned base reopening — proposed next slice (2026-10-18)

After W1c1's local gate and W1b's own acceptance, combine their sources before
implementing the reader. Proposed descriptor: 88 fixed bytes, a distinct 8-byte
format marker, volume/node/reservation IDs (16 bytes each) and original payload
reference (32 bytes). Decoding is untrusted intent, not authorization. Persist
no credentials, caller-supplied S3 key or cached Node as a way to construct File.
The original edit's CAS expected value is this payload's own reference, **not**
`payload.reservation.guard.expected`, which identifies its predecessor.

Separate a private immutable content version from named File metadata. A proposed
BaseFile can expose size, bounded reads and chunk/page proofs, without fabricating
historical pathname, revision or inode attributes. Existing File APIs delegate
to the same version; clones retain the same snapshot permit and operation budget.
This avoids restoring a cached or synthesized Node as trusted namespace metadata.
Keep the history capability optional for ordinary current-version reads, and
reject reopening when it is absent. Do not replace the legacy reader or native
callback API in this slice.

Reopening must bind descriptor volume/file/reservation/reference to the authenticated
ManifestPayload reply and configured store route, then HEAD/GET/hash/verify the
original root with current credentials and existing snapshot/deadline bounds.
It may succeed after rename, newer content or deletion without rebasing the
original CAS. Missing/revoked/changed roots and routes fail explicitly; a witness
cannot turn into an upload receipt or network-send permit. Require process/server
restart, credential rotation/denial, malformed and cross-file descriptors, root
and child corruption, shared capacity/cancellation and exact old-byte/proof checks.
Use fresh full local and combined hosted/native gates before W2 uses this API.
Legacy and metadata-empty restart recovery remain explicit later cases, not
permission to omit them from eventual native write acceptance.

W1c1 result: full local gate60956 EXIT0, all262 frozen inputs unchanged;
368state/1ignored,11 authority namespace,8reader/format1nativeignored,12publication,
62S3,fmt/clippy/build/book passed. The indexed20-member local archive is verified.
No reader activation or own combined hosted acceptance follows from this alone.

Combined baseline5231113 check9230 and focused reader77295 both EXIT0,
9passed/1nativeignored10.69s; all264 runtime inputs unchanged. This baseline
compile/integration result does not replace the future W1c2 full/native gate.
This was the pre-implementation baseline; the draft below now implements the
proposal and requires its own complete acceptance.

W1c2 implementation draft: see [pinned base reopening](manifest-base-reopen.md).
File metadata and immutable BaseFile versions are separate;88-byte descriptor
intent is reauthorized through exact history and fresh root/provider checks.
Initial production all-target check and focused restart process test passed;
full portable gate95974 now passed with all266 frozen inputs unchanged:368state/
1ignored,11reader/1nativeignored,11authority namespace,12publication,62S3,
format/Clippy/build/doclinks/book. Indexed21-member local archive verified.
Own hosted/native acceptance remains pending. No sparse SQLite schema or native
chunk-write activation is included.

### Earlier canonical integration checkpoints

The following records describe their original checkpoints; later results above
supersede their pending statuses. Both lineages are retained during integration.


Reader implementation checkpoint (2026-10-18): controller474 and helperfef are
accepted and combined in source `88d02f7` on `codex/manifest-range-reader`.
Its full local gate passed (256 unchanged source hashes,12 publication/source,
5 reader,62 S3,365 state/1ignored,64chapters). The new reader, diagram and indexed
local evidence are documented in the
[reader branch book source](https://github.com/marshallr12/s3drive/blob/88d02f7a6605bf13bfd212d67c6609ae16b0ebfd/docs/src/manifest-range-reader.md).
Its [own hosted cohort](https://github.com/marshallr12/s3drive/actions/runs/39171113280)
is running; raw/artifact acceptance remains pending. Primary controller sources
stay unchanged until that acceptance. Native chunk lifecycle and real-provider
acceptance remain separate. Continue in its isolated worktree and read its HANDOFF.


Native read-integration draft checkpoint (2026-10-18): reader88d's own cohort
has passed all3 jobs and strict21/14 + raw35 checks at reader checkpoint7c2b454.
Implementation proceeds in /home/marshall/src/worktrees/s3drive-native-manifest-reads
on codex/native-manifest-reads. Six focused controls passed, including mixed formats,
two stores, sparse64GiB metadata, old roots and namespace faults. Local real FUSE
readiness failed OperationNotPermitted under NoNewPrivs1. The first full gate
failed an existing setup Info TLS connect timeout; its unchanged isolated case
passed. The second complete gate is running with top-level tests serialized and
all production timeouts/inside-test concurrency unchanged. No fullgate/native
acceptance or draft publication is claimed yet. Read that checkout's handoff.


Native read publication checkpoint (2026-10-18): source `d98ea2f` is pushed on
codex/native-manifest-reads after the full local gate passed with257 unchanged
source hashes,69documentationfiles/65chapters. Two fixture first-line markers
were repaired after serial-libtest framing was reproduced, without relaxing any
production deadline or assertion. The
[own hosted OS cohort](https://github.com/marshallr12/s3drive/actions/runs/39176574738)
is running; both Rust jobs explicitly execute real native manifest reads/remount.
No native success is inferred from the portable gate. Primary runtime stays at
its accepted controller source until the native phase is accepted. Chunk-write
integration remains later work; continue in the native branch's isolated checkout.


Combined snapshot/proof checkpoint (2026-10-18): accepted nativec633 at52f591c
and local W1a ed2e93b are combined with a demonstrated empty/legacy snapshot
repair in source `8c3448f`, on codex/manifest-reference-proofs. An old empty
inode's4-vs0 size failure was reproduced before production changes. Readonly
chunk-capable inodes now represent every payload version, with attributes pinned
and guards before/after both readers. Stable authority IDs remain distinct from
version inodes. Existing writable/direct/fallback scope remains unchanged.

The fresh full local gate passed all259 frozen inputs,8reader/format controls
(one native ignored),12publication,62S3,365state/one ignored and book/build checks.
An older readonly engine fixture now performs fresh path lookup after replacement
and preserves global-ID/old-byte/rename/delete assertions. Indexed25member local
evidence retains failures and exact source. The
[own combined OS cohort](https://github.com/marshallr12/s3drive/actions/runs/39184629344)
passed all3 jobs with raw review, actual FUSE1.63s/WinFsp2.53s, own ZIP digests,
typed21+14/project oracles and all35 raw fields/types. Accepted checkpoint
[da84d3a](https://github.com/marshallr12/s3drive/commit/da84d3a05e0562bed00f77f3ea5aece935e52d32)
retains the indexed29-member hosted archive. Earlier artifacts were not reused.

W1b3890584 independently passed the full local gate with260 unchanged inputs,
7reader/12publication/62S3/365state1ignored and book/build checks. It exports
bounded opaque PageProof witnesses and proves valid prior-root reuse as a child
with invalid geometry refusal, no data fetch or receipt fabrication. The combined
sourcebf2e6ae includes accepted da84d3a and passed fresh full gate68162 with261
unchanged frozen inputs,9reader/format1nativeignored,12publication,62S3,
365state1ignored and all book/build checks. Indexed19-member local archive verified.
Accepted [checkpoint2e7b6ca](https://github.com/marshallr12/s3drive/commit/2e7b6ca020f5c4068291f7db779e6f359e04f515)
retains the results and indexed31-member hosted archive. Its
[own Linux/Windows/book cohort](https://github.com/marshallr12/s3drive/actions/runs/39187701991)
passed all3 jobs/raw review, actual FUSE1.13s/WinFsp3.04s, own ZIP digests,
typed21+14/projectoracles/all35 raw fields/types. No earlier artifacts reused.
Historical base reopen, sparse SQLite edits, mixed-tree proofs, exact-view
settlement, native chunk writes and provider/reboot acceptance remain.

Independent W1c1 draft in codex/payload-history-authority adds authenticated
exact-reference historical manifest lookup from existing retained publication
and reservation records. No schema migration, operation-log scan or S3 call.
Each of five BLOB fields has a16KiB SQL guard before Rust copying; a padded-valid-
JSON negative reproduced acceptance before the fix. Three state controls
(17 corrupt/oversized scenarios) and one production WireClient/auth HTTP restart/
no-rebase control passed. Full local gate60956 passed with262 unchanged frozen
inputs,368state1ignored,11authority namespace,8reader/format1nativeignored,
12publication,62S3 and all format/Clippy/build/book checks. Local checkpoint
e18d6f5 is clean with matching committed hashes; not pushed or hosted-accepted.
The indexed20-member local archive is verified. This is not reader reopen or
native acceptance. W1c2 must separate immutable
content from namespace attributes, verify old roots with current credentials and
retain the own-payload CAS reference rather than the predecessor reservation guard.

W1c2 preparation: local5231113 combines W1c1e18d6f5 and accepted W1b2e7b6ca in
codex/payload-history-authority.264 runtime inputs frozen; baseline all-target
check9230 passed plus9focused reader controls/1nativeignored10.69s, all264
unchanged. Reader implementation has not begun. The combination is not yet hosted-
accepted. Implement the documented metadata-free pinned version/88-byte untrusted
descriptor and authority-bound original-root re-open contract, retaining original
own-payload CAS and current provider auth; then require own full/hosted gates.

Canonical integration,2026-10-18: marshallr12/s3drive main and the primary checkout
receive accepted checkpoint2e7b6ca/runtimebf2e6ae. All261 frozen runtime inputs
and all268 non-document tracked files match the accepted source exactly. Own
run39187701991 native/raw/artifact acceptance and local gate68162 apply to that
runtime; final book/link checks cover the documentation-only integration delta.
See [integration evidence](evidence/canonical-manifest-read-integration-20261018.json).
The history/reopen draft remains separate, with no native-write activation.

### W2 proposal: bounded immutable SQLite edit views (2026-10-18)

Planning only, while W1c2's own hosted gate is pending. No schema or write path
is activated by this section. The existing `logical.rs` whole-file writer and
`chunked.rs` full-snapshot planner remain intact. The new layer must not pass a
remote-backed sparse view to either as if it were a complete local snapshot.

Proposed first representation: each immutable view has its original manifest
base identity, logical length, parent view, operation identity and an ordered,
nonoverlapping extent map covering exactly its logical length. Extents refer to
verified base ranges, immutable local patch bytes or explicit zeros. One base or
zero extent can cover many gigabytes without allocating those bytes. A write
splits only intersecting extents and substitutes patch ranges; a truncate removes
the tail; later growth appends zeros. Removing tail extents on shrink prevents
old remote or patched bytes from reappearing after regrowth. Pinned older views
retain their original extent maps and bytes.

Store bounded immutable patch blocks in the same local SQLite transaction as the
new view, extent map, operation result and compare-and-set head transition. This
is a proposed simple first backend: it avoids a new filesystem/SQLite dual-commit
protocol. Measure database/WAL growth and commit behavior before selecting final
block and quota defaults. Candidate bounds are one-MiB patch blocks and at most
eight MiB per mutation/read, matching the current chunk geometry and read bound;
they are proposals, not public guarantees. No allocation or row count may scale
with a sparse file's full logical byte length.

Require explicit independent quotas for dirty patch bytes, views, operations,
extents and bounded traversal. A materialized per-view extent map is acceptable
for the first slice only with a small, documented metadata ceiling and measured
copy cost. Exhaustion must reject the entire mutation while preserving every
old view, head and operation receipt. No automatic dirty eviction, hidden full-
file copy or unbounded ancestor scan. Patches shared by several views count once
in byte accounting, and no reclamation is authorized merely because the live
head changed. Reclamation and settlement require separate proven retention rules.

A mutation carries an operation key, immutable target/base identity, expected
parent/head and exact arguments. Persist the result atomically. Repeating the
same key and arguments returns that operation's original view, even after a
newer edit; a reused key with changed arguments fails. A competing head change
fails explicitly without regenerating the operation key or rebasing the base.
Verify the complete persisted map, identity, lengths, local block hashes and
receipt binding before returning a successful replay. Corrupt or missing rows
must not be silently repaired or recreated from the caller's request.

The descriptor codec currently lives in the managed reader, which depends on
state; state cannot import managed without a dependency cycle. Plan a small W2a
prerequisite moving the pure descriptor representation/codec into core and
re-exporting the existing managed API. Preserve its exact88-byte format and
failure behavior, and require its own gate. The SQLite layer records local
intent; neither a decoded descriptor nor a locally hashed view proves remote
authorization. The managed composer must reauthorize the exact historical base
with the accepted reader, compare volume/node/base reference and configured
route, then supply verified base bytes. Persist no credential or synthesized Node.

For read-your-writes, select a bounded range of extents and compose into a bounded
output buffer. Verify each local patch block before returning any of its bytes;
fetch and verify complete remote data chunks before using partial bytes. Zero
extents need no provider access. Missing or revoked base access remains an error
for requested base ranges; it never substitutes a newer root. A fully local
patched range can be read without fetching unrelated base data. Cached proofs
remain reachability evidence, not upload or settlement receipts.

Proposed independently green slices: W2a codec/layering; W2b SQLite identity,
immutable maps, patches, mutation replay and bounds; W2c managed composition and
exact-base restart recovery. Before acceptance, prove64GiB sparse grow/shrink/
regrow, overlapping writes, zero-length/EOF/overflow cases, pinned older views,
same-key lost-reply replay, competing heads, process death on both sides of
commit, SQLite-full rollback, trigger/row/block corruption and bounded quota
exhaustion. Show measured dirty byte and metadata growth, and reject accidental
whole-file staging. Legacy and metadata-empty bases still need an explicit
recovery contract before eventual native write acceptance. W2 supplies no root
publication or clean acknowledgment; W3-W6 remain required.

W2a layering detail from source inspection: core's `manifest::Error` already has
Encoding and Binding cases, while managed callers currently downcast descriptor
decode failures to `ReadFailure`. A bare type re-export would change that error
contract. Prefer a core untrusted identity/codec plus the existing managed
`BaseDescriptor` wrapper, delegating accessors/encoding and mapping decode errors
back to the current ReadFailure variants. Preserve exact-length/magic rejection,
reserved-ID rejection,88-byte vectors, redacted Debug, equality and every existing
reader export/reopen test. A core constructor accepts identity intent only; it
must not construct BaseFile, authorize a history request or create a send permit.
State can use the core representation without depending on authority or managed.
No dependency change is required for the pure codec. Keep the authority-backed
lookup and fresh S3 root verification in managed. This corrects the earlier
shorthand "re-export"; preserving the managed API includes its error semantics.


W1c2 accepted checkpoint,2026-10-18: runtime `0d2dfce` passed own
[run39194683513](https://github.com/marshallr12/s3drive/actions/runs/39194683513),
all3 jobs/raw review/own ZIP API digests/independent typed21+14/project oracles/
all35 raw fields and types. Linux manifest FUSE/remount1.60s; Windows manifest
WinFsp/remount1.96s, paired startup81.70s. Linux368state1ignored/Windows363state
1ignored,11reader1nativeignored and62S3 passed. Indexed34-member hosted archive
and all266 frozen runtime inputs verified. See [base reopen evidence](manifest-base-reopen.md).
W2 consumers may now be implemented against this accepted base. Begin the pure
core codec/managed error-preserving wrapper prerequisite, then bounded SQLite
sparse views and verified composition. New native chunk writes remain unsupported.

Canonical history/reader integration uses the accepted0d2dfce runtime unchanged;
see [integration evidence](evidence/canonical-manifest-history-integration-20261018.json).
Only documentation/evidence follow the tested source. Publish by normal fast-
forward and verify remote/main and the clean primary checkout against all266 inputs.


W2a implementation draft,2026-10-18, from accepted main41188dd in
codex/base-identity-core: `s3drive_core::manifest::base::Descriptor` implements
only the88-byte structural identity codec and canonical hex constructor. Managed
BaseDescriptor wraps it, preserves its existing accessors/encoding/redacted Debug
and maps core Encoding to ReadFailure::Integrity and reserved IDs to Binding.
Actual authenticated history/store/root checks remain in the existing reader.
No dependency/schema/wire/native change. Three core controls include a literal
persisted vector, every truncation length, marker/extra/reserved-ID errors,
canonical hex refusal and explicitly untrusted alternative identities. Focused
core tests passed; existing reader controls and a fresh full local gate are still
required. This prepares SQLite dependency layering, not edit-view implementation.

W2b footprint findings,2026-10-19, from the accepted state sources: this is not a standalone
module-only addition. State currently migrates schema28 and validates its modules
in the same opening transaction. Namespace initialization and chunk-publication
role checks enumerate local tables; the latter deliberately excludes legacy
logical/namespace state. New sparse-view tables must participate in schema,
role-isolation and integrity checks, migration rollback and old-schema fixtures.
They must not silently become compatible with the existing complete-snapshot
publication/settlement role. Inspect these lists before declaring W2b's write
footprint. The existing `Owner::call` worker queue can run transactional state
methods; do not create another SQLite connection or unbounded blocking work in
an async read callback. These are inspected integration constraints, not a new
schema or an implemented sparse-view API.


W2a local result,2026-10-19: full gate33550 passed with all267 frozen runtime
inputs unchanged.37core tests include the literal descriptor vector;368state
(one ignored),11reader (one native ignored),62S3 and all format/Clippy/build/
book checks passed. Indexed12-member local archive verified; see
[codec evidence](evidence/base-identity-core-20261018.json). Own hosted/native
acceptance is pending. W2b's local storage-only slices may build on this green
structural codec in isolation, while their eventual combined source still needs
its own complete platform gates before activation. Main stays at accepted41188dd.


W2a accepted,2026-10-19: sourcec279e0f passed own
[run39198380488](https://github.com/marshallr12/s3drive/actions/runs/39198380488),
all3 raw logs/own ZIP digests/strict typed21+14/project oracles/all35 raw fields
and types. Indexed36-member hosted archive and all267 frozen inputs verified.
Linux manifest FUSE/remount1.19s, Windows WinFsp/remount14.11s;37core/11reader
1nativeignored/62S3 on both. No performance/native chunk-write claim. Canonical
integration keeps that runtime unchanged. W2b1 is a separate schema29 draft in
codex/sparse-edit-views; focused storage/migration controls passed and its full
local gate3 96485 has passed. That new source has no hosted acceptance yet.

W2b1 progress: its first full gate48732 exposed the native snapshot local-table
allowlist missing the four new EMPTY tables. The narrow repair retains unknown/
nonempty/view/64-table refusals. Snapshot CLI2 and state9 controls passed after
correcting only the synthetic orphan fixtures. The second full gate44679 actually terminated143; the complete third
gate96485 passed all275 frozen inputs in its isolated checkout. No schema29 acceptance yet.

### W2b1 first SQLite slice — declared contract (2026-10-19)

Preparation in codex/sparse-edit-views, based on locally green core runtimec279e0f.
The core's own hosted source remains frozen in a separate checkout. First persist
private local target/base intent, an immutable initial extent map and exact begin
operation/result in SQLite. Expose bounded metadata range planning and reopening;
no provider call or invented historical Node. A64GiB initial remote-backed view
must use bounded metadata, not allocate its logical byte length. Decoding a base
identity still does not establish remote authorization or current availability.

Require same-key/same-arguments replay, changed-key/target/base conflict refusal,
full-row integrity and bounds, private-owner/worker integration, SQL failure and
migration rollback, old-schema preservation and reciprocal legacy/authority role
isolation. Inspect every relevant table list. The existing fake-old-schema tests
must remove new tables when constructing old fixtures; keep migration collision
and corruption failures strict. The next migration and two Windows schema
expectations move together. Generic staging remains available for future mixed
uploads; its validation must not accidentally become a legacy-only constructor.

W2b2 then adds immutable patch storage and atomic write/truncate/grow transitions,
read-your-writes plans, older pinned views, newer-head replay, process death,
SQLite-full rollback and bounded dirty/metadata quotas. W2c supplies verified
network/local composition. W2b1 alone is not editable storage or native-write
acceptance. W3-W6 and legacy/metadata-empty recovery remain in scope.

W2b2 migration constraint from the actual W2b1 source: schema29 intentionally
admits one initial view/operation per file. `sparse_views.target` is UNIQUE and
the validator requires equal file/view/operation counts. Editable immutable
versions require a real migration and validator change, preserving existing
root record bytes, hashes, operation results and stable volume/file targets.
Do not invent another target for the same file to evade the constraint. Test
migration with populated cyclic file-head/view references and actual foreign-key
enforcement; preserve strict collision rollback. New child views need explicit
parent/head comparison and exact request identity, plus independent view,
operation, extent and dirty-byte quotas. A local mutation result remains distinct
from a provider root receipt and clean acknowledgment.


W2b1 local-green checkpoint,2026-10-19: full gate3 96485 passed all275 frozen
inputs unchanged.378state1ignored/37core/11reader1nativeignored/62S3 plus all
format/Clippy/build/71MD link/67book chapter checks passed. First gate exposed
and repaired the narrow snapshot allowlist omission; second actually terminated
143, cause unknown. The complete third run used a PTY/four libtest threads with
unchanged source/deadlines/assertions. Indexed37-member local archive verified.
See [local base-view evidence](sparse-base-views.md). Publish this exact new source
for its own platform gates before acceptance. W2b2 remains a real multi-version
migration and patch-mutation implementation, not an activation of W2b1 metadata.


W2b1 accepted,2026-10-19: runtime1681ee6 passed own
[run39203016390](https://github.com/marshallr12/s3drive/actions/runs/39203016390),
all3 raw logs/own ZIP API digests/strict typed21+14/project oracles/all35 raw fields
and types. Indexed44-member hosted archive and275 runtime inputs verified.
Linux378state/Windows373state1ignored,37core/11reader1nativeignored/62S3 passed;
real manifest FUSE1.62s/WinFsp2.71s and paired no-systemd/Windows recovery passed.
Canonical integration keeps this runtime unchanged. W2b2a schema30 follow-up in
codex/sparse-mutations has full local gate6893 green and requires its own combined
platform gates; it enables no patch API. W2b2b mutations and W2c/W3-W6 remain.


### W2b2 patch mutations — declared next phase (2026-10-19)

Implement in an isolated worktree from locally green W2b1, keeping its hosted
source frozen. Verify populated cyclic-FK migration before choosing SQL. Preserve
old root records, request/result bytes, hashes and IDs. Add bounded immutable
patch blocks and multiple versions; each write/resize names the exact parent/head
and operation key. Repeating a known operation returns its own old result after
a newer head; changed arguments conflict. Mutation, head update and receipt are
one transaction. No remote rebasing, clean acknowledgment or provider access.

Compose new extent maps from the parent: replace writes at their offsets, append
explicit zeros across holes, truncate removed tails, and keep growth zeroed after
shrinking. Read pieces return verified local patch bytes and bounded base/zero
placeholders; W2c must authorize and fetch the exact remote base. Enforce separate
file/version/extent/metadata/dirty-byte limits, bounded individual mutation/range
sizes and blocks. Preserve older views and unknown/conflicting dirty work. Require
SQL-full/trigger rollback, corruption, concurrent/late replay, restart/process
kill and sparse64GiB controls before the full gate. New role tables join all
snapshot/authority/legacy isolation checks and native schema assertions.


W2b2 refinement,2026-10-19: the actual SQLite experiment exposed commit787 for
single-table replacement with deferred FKs. First deliver W2b2a, a schema30
copy/empty/rebuild/restore prerequisite with FK enforcement on and byte-exact
populated29 migration/replay/rollback controls. Keep initial-only validation and
public APIs until W2b2b supplies complete multi-version/patch semantics. This
small independently green checkpoint does not reduce the declared W2b2 contract.
See [migration discussion](sparse-view-migration.md). No new bytes or native writes
are activated by relaxing the SQL target constraint alone.


W2b2a local result,2026-10-19: full gate2 6893 passed all276 frozen inputs,
380state1ignored/37core/11reader1nativeignored/62S3 plus all format/Clippy/build/
72MD links/68book checks. Gate1 failed only an older page-layout-dependent
managed disk-full fixture (ack succeeded with available space). A narrow test-only
receipt-update pressure trigger now forces real SQLITE_FULL and verifies rollback,
dirty retention and successful retry. The failed source/log and repaired full
pass are retained in the indexed21-member local archive. No mutation API or native
chunk-write activation. Publish this exact schema30 source for its own combined
gates; W2b2b remains next. See [migration evidence](sparse-view-migration.md).


### W2b2b immutable patches and mutations (declared2026-10-19)

Build in codex/sparse-patches from local-green8141996, keeping its own CI source
frozen. Add schema31 and <=1MiB immutable raw-digest blocks, multiple retained
versions, exact-parent/head writes and resizes with exact-operation replay after
newer edits. Old root serialization/IDs remain unchanged. Reconstruct every child
from its request and parent; require one complete chain to the head, no orphan/
fork/cycle, and every block referenced by a retained write. Preserve old literal
record/request BLOBs in post-write comparisons. Normalize spans and never revive
a removed tail. Return bounded local bytes only after full-block verification;
base/zero pieces remain metadata until W2c authenticates actual remote content.

Use separate soft file/view/operation/extent/metadata/dirty quotas, hard ceilings,
8MiB mutations/ranges and64GiB logical files. Quota refusal retains all old bytes
and exact replays bypass newly narrowed admission limits. Add the patch table to
all role/snapshot/legacy guards and update both Windows schema assertions. The
old schema30 migration must validate initial rows without requiring a patch table
that does not exist yet. Require realistic populated migration, exact replay and
competing-head controls, overlaps/truncate/regrow/64GiB holes, corruption, actual
SQLite-full/trigger rollback, worker and process-kill recovery, then the complete
local gate. No native chunk writes or clean settlement in this slice.


W2b2b local result,2026-10-19: full gate2 40129 passed all284 frozen inputs,
393state1ignored/37core/12reader1nativeignored/62S3 plus all format/Clippy/build/
73MD links/69book checks. Immutable patches, exact-parent write/resize/replay,
retained chains, verified bounded local pieces and independent quotas now have
local controls including six actual process-death boundaries,48 independent-model
edits,35 combined corruption cases, trigger/SQLite-full rollback and worker races.
The schema30 parent's own Linux cohort failed a post-body accounting race in the
existing reader fixture; its Windows/book success is not acceptance. This source
adds bounded owned-handler settlement and a deliberately delayed accounting
control, preserving the exact nine-chunk assertion. Gate1 was only a Clippy
let-chain cleanup; failed and passed sources/logs and the reviewed diagram are
retained in the indexed44-member local archive. Publish this exact schema31 source
for its own combined native/reference gates. See [patch evidence](sparse-patches.md).
W2c and W3-W6/legacy/metadata-empty recovery remain; no mounted chunk writes yet.


W2c next: inspect the implemented reader/base APIs and filesystem authorization
policy before coding composition. Freshly authorize the exact historical base
and compare actual size/store/source fingerprint/root before serving edited file
bytes, including all-local/all-zero requests. Never hold the SQLite owner across
network waits. Preserve pinned older views, bounded shared resources/deadlines,
cancellation and credential-rotation behavior. Verify through owned SDK + real
Rust authority + local-journal fixtures and the complete local/own combined gates.
Legacy and metadata-empty admission/restart contracts remain required before W6;
this sequence does not narrow the final drive to existing manifest files only.


### W2c composition — declared contract (2026-10-19)

Implement in isolated codex/sparse-composition from local-green d37096c, freezing
its own live CI source. Derive binding from authenticated BaseFile, reopen a stored
view via exact history/root verification and compare actual size/store/source/
fingerprint/root/principal. Compose bounded local pieces with verified base
chunks under one shared reader active permit, snapshot permit and overall
operation deadline. Fresh authority history and pinned-root HEAD must precede
returning any nonempty bytes, including all-local/zero reads. EOF/zero-length
reads may retain the existing network-free policy because no bytes escape.

A per-fragment call to read_range would reset budgets and duplicate downloads.
Use one request-local verified page/data cache, one traversal/data budget and
exact reference/length validation on cache hits (S3 object keys omit hashes).
Never hold SQL owner across network waits or expose partial output on late
faults. Bound ranges/resources and preserve older views; never silently rebind
credential generations. Verify through owned SDK + real Rust authority + private
SQLite worker, then full local and own combined platform/native gates. No native
chunk-write/publication/settlement activation. Legacy/metadata-empty support and
W3-W6 remain in the final goal.


W2c local result,2026-10-19: authenticated sparse composition full gate56076
passed all286 frozen inputs unchanged:393state1ignored/37core/16reader1native
ignored/62S3 plus all format/Clippy/build/74MD links/70book checks. Controls cover
mixed/page-boundary and40-fragment reads, exact nine-chunk8MiB output,64GiB zero
tails, all-local/zero authorization, wrong bindings/principals, late denied or
corrupt data, local corruption after open, independent-process/authority restart,
shared active/snapshot limits, cancellation/deadline and SQL progress under a
network gate. Indexed19-member local archive and reviewed architecture SVG are
retained. Publish this exact source for its own combined gates; accepted parent
main6c9f0b5/runtimed37096c is unchanged and does not certify this new code. See
[composition evidence](sparse-composition.md). W3-W6 and legacy/metadata-empty
support remain; native chunk writes stay disabled.

Historical main72ee64f pointer (superseded by accepted checkpoint below):

Current follow-up,2026-10-19: immutable patches and exact-parent write/resize
versions are implemented in published `codex/sparse-patches` runtime `d37096c`,
with full local gate40129 green/all284 frozen inputs.393state1ignored/37core/
12reader1nativeignored/62S3 and all format/Clippy/build/book checks passed. See
[feature implementation and diagram](https://github.com/marshallr12/s3drive/blob/codex/sparse-patches/docs/src/sparse-patches.md)
and its [own combined run39209163191](https://github.com/marshallr12/s3drive/actions/runs/39209163191).
Main stays on accepted B1/schema29; this is separate pending-platform source.
The schema30 parent's own run39205603137 failed Linux reader byte accounting
while Windows/book passed. The new source has bounded handler settlement and a
controlled post-body accounting test, preserving the exact9-chunk assertion.
It must pass its own all3/raw/native/own-artifact checks before acceptance.
W2c verified remote/local composition, mixed publication, exact settlement,
legacy/metadata-empty recovery and real native chunk writes remain required.


W2b2b accepted,2026-10-19: runtime d37096c passed own
[run39209163191](https://github.com/marshallr12/s3drive/actions/runs/39209163191),
all3 raw logs/own ZIP API digests/typed21+14/project oracles/all35 raw fields/types.
Indexed60-member hosted archive and all284 source inputs verified. Linux393state/
Windows388state1ignored,37core/12reader1nativeignored/62S3 passed; real manifest
FUSE1.60s/WinFsp2.38s and paired/no-systemd recovery passed. Canonical integration
keeps these runtime inputs unchanged. Separate codex/sparse-composition adds W2c
and has full local gate56076 green, including16reader1nativeignored.
It needs its own combined acceptance. W3-W6 and legacy/metadata-empty remain.

W2c publication,2026-10-19: source84da0ee includes accepted main6c9f0b5 ancestry,
all286 inputs exact from green gate56076. Its own
[run39213440204](https://github.com/marshallr12/s3drive/actions/runs/39213440204)
is in progress. Retain frozen source; require all3 terminal outcomes, full raw
review, own artifact digests/typed21+14/project/all35-record checks before
integration. W3-W6 and legacy/metadata-empty remain; no native writes activated.

### W3a bounded data preparation — declared contract (2026-10-19)

Implement in isolated `codex/incremental-data` from composition `b884a1a`;
parent runtime84da0ee remains frozen for own run39213440204. Prepare one logical
chunk of one pinned immutable edited view. Fresh local ledger validation and
exact-history/rootHEAD authorization precede every nonempty result. A wholly
unchanged same-length chunk returns its opaque authenticated reachability proof,
with zero data GETs; it is not a new upload receipt or current data-health proof.
Changed/truncated/grown chunks compose <=1MiB exact bytes, verifying complete old
data objects before slicing. Return a hole only after verified zero content.
Bind the result to exact view ID, full base binding, offset and logical length;
private result constructors prevent arbitrary-reference adoption. Share reader
active limits/deadline; release SQL before any network wait. No file-wide buffer.

Done evidence: one-byte edit downloads only its original changed chunk; unchanged
siblings and authenticated holes download no data; partial final shrink/growth
never reuses a prefix;127/128 chunk boundary and64GiB sparse tail; old-view result
after newer head/reopen; authorization/corrupt-tail/late-denial controls; shared
busy/cancel/deadline and independent SQL progress. Then full local and this new
source's own combined gates. W3a is the data-preparation building block, not all
W3: persisted cursor/mixed graph and subtree reuse remain W3b, then W4 exact-CAS
publication, W5 exact-view settlement and W6 native writes. Retain the old full-
snapshot planner/verifier unchanged; never manufacture its upload/source receipts.

W3a focused result,2026-10-19:3 tests passed in3.83s against real authority/SDK/
private SQLite. Full local gate81440 passed with288 frozen inputs unchanged. The candidate
module adds no schema or dependency; it reuses the existing composer and private
proof traversal under the same permit/deadline. New final-length controls reject
old-object prefix reuse, verify unread old tails and preserve truncated bytes as
zeros after regrowth. Updated [architecture and contract](incremental-data.md)
explain pending W3b mixed-tree persistence and W4-W6. Parent W2c own39213440204
all3/raw/source/own ZIP digests/typed21+14/project/raw checks passed and parent
is accepted as main057c8ae. No runtime source changed during the gates. W3a full result:393state1ignored/
37core/19reader1nativeignored/62S3 plus format/Clippy/build/75MD links/71book
chapters. Indexed16-member local archive verified. Publish exact source for its
own combined platform/native/source checks; parent source remains distinct.

W3a publication,2026-10-19: frozen source `9544193908967ea2852121bef7d91cdce89ed042`
is pushed with mainabdda37 ancestry and all288 exact inputs from the green gate.
Its own [run39215314627](https://github.com/marshallr12/s3drive/actions/runs/39215314627)
is running; all3/raw/own-artifact/source acceptance remains pending. Documentation-
only follow-ups preserve the frozen source; prior cohorts do not certify it.

W2c accepted,2026-10-19: source84da0ee passed own39213440204 all3 full raw/source/
own ZIP API digests/typed21+14/project/all35 field/type checks. Indexed62-member
hosted archive retained. All286 inputs exact; canonical integration changes only
docs/evidence after the tested runtime. Linux393state/Windows388state1ignored,
37core/16reader1nativeignored/62S3 and real manifest FUSE/WinFsp remount, legacy
native and paired/no-systemd recovery passed. See [composition evidence](sparse-composition.md).
Separate W3a9544193/288 inputs has own39215314627 book/Linux green and Windows
running; no W3a/native chunk writes activated by this promotion. W3b/W4-W6 and
legacy/metadata-empty/provider/physical acceptance remain.

W3b source finding,2026-10-19: reader cache identity and upload Session.binding_id
both call credential_binding with the full configuration fingerprint and pinned
credentials. Therefore compare Executor.recovery_target to the exact sparse base
source/fingerprint and generated object key; do not use storage routing identity
alone or silently rotate credentials. The persisted journal proves local intent/
integrity; the controller must reauthorize historical provenance before uploads.

W3a ancestry update,2026-10-19: merge accepted main057c8ae into preparation branch
with docs/evidence-only delta from source9544193 and all288 inputs unchanged.
Its own39215314627 book/Linux passed, Windows is running. Parent acceptance does
not substitute for this source's own combined checks.

W3a accepted,2026-10-19: runtime9544193 passed own39215314627 all3/full raw/source/
own API ZIP digests/typed21+14/project/all35 raw field/type checks. Indexed64-member
hosted archive retained; all288 frozen inputs exact. Linux393state/Windows388state
1ignored,37core/19reader1nativeignored/62S3; real legacy mounts, manifest FUSE/WinFsp
remount and paired/no-systemd recovery passed. See [preparation evidence](incremental-data.md).
Next W3b local draft in /home/marshall/src/worktrees/s3drive-mixed-plans is unwired,
uncompiled and untested. Existing source remains schema31; no mixed publication,
settlement or native edited writes are activated by this integration.

### W3b durable mixed-tree preparation — declared phase (2026-10-19)

Implement in isolated `codex/mixed-plans` from `c1f7fbb`; keep parent W3a9544193
frozen288 inputs for own39215314627. Ownership: state sparse_views/preparation
modules, additive032 migration, state open/test-schema/native schema assertions,
private-owner client, managed preparation controller and owned state/CLI tests;
docs/evidence/diagrams move with the phase. No legacy full-snapshot verifier,
wire/provider policy or native activation changes.

Freeze one exact immutable view, operation, new object generation and original
base root context; derive local binding from the stored view. Exact replay after
new heads/restart must preserve all original input BLOBs and CAS identity.
Persistent cursor distinguishes input coverage from bounded bottom-up page work.
New data/pages and inherited chunk/page witnesses have distinct serialized tags;
never manufacture old upload/source receipts. Validate witness geometry/store/
file scope and unchanged view coverage. Reuse only complete same-length objects;
an old root can be a new child only at matching level/span. The output root is
new, with its own immutable generation/key, even when child references are old.

Use whole unchanged subtrees and structural zero runs, bounded local data and
shared witness pages;64GiB sparse growth must not enumerate or allocate the
whole file. New data is at most1MiB per step, pages at most128 children. Persist
exact dirty bytes/hash and cursor atomically; no SQL across network. Admission
quotas never prevent recovery/inspection of old work. Validate complete retained
rows and every post-write effect so ignored/foreign/rewrite triggers, SQLite-full,
cancellation/unknown replies and killed workers cannot falsely acknowledge work.
Preserve both local edits and preparation on conflicts/corruption; no repair,
rebase, cleanup or silent overwrite. Independent worker restart resumes progress.

Local state establishes offline intent/integrity, not remote authorization or
current data health. The managed producer must use the actual authenticated base
and private exact-view candidates; restart reopens exact history and compares
full original binding/root before further preparation. The later W4 controller
must revalidate the same source/provenance and source-bound write policy before
new sends. W5 exact-view acknowledgment remains separate. This phase is complete
only after mixed roots build/reopen with correct bytes and measured unchanged
reference reuse, partial-final edits,127/128 boundary/height growth, sparse64GiB,
wrong proofs/bindings/quota/corrupt media/SQL/crash/replay controls, full local gate
and its own combined platform/native/source acceptance. W4-W6 and legacy/empty
admission remain required for the full drive goal.

W3b draft checkpoint,2026-10-19: local `codex/mixed-plans` contains an unwired
format and offline witness-validation draft in sparse_views/preparation.rs and
preparation/{model,proof}.rs. Explicit rustfmt parsed/formatted these three files;
no Rust type checking, schema, ledger/journal/controller or tests exist for this
draft yet. Do not claim a baseline gate covers it. Full acceptance remains the
complete declared W3b phase above. Its current HANDOFF records the exact next
work and validation conditions. Parent W3a9544193 is now accepted as main3086038.

W3b implementation checkpoint,2026-10-19: schema32 ledger/cursor/tree builder and
managed producer/reopen/checked dependency boundary are wired and focused tests
passed. State remains offline intent. False root context and correct-root false
new bytes/holes fail at the managed boundary. Fourteen killed-process boundaries,
SQLite-full, rollback, corruption, quota, path sharing/two-level proofs,64GiB zero
coverage and real authority/SDK restart/authorization/resource controls passed.
A page-domain-vs-raw-SHA bug was caught by core root verification and corrected;
failed and successful evidence retained. See [implementation and diagram](mixed-plans.md).
Full local gate34846 passed on all300 frozen inputs (408state/22reader, one
ignored each;37core/62S3;76Markdown/72chapters). Empty-only snapshot-table schema
adaptation and its nonempty refusal cases are included. The indexed329-member
local archive preserves exact source and both failed and successful runs. Own
combined gates subsequently passed; see the dated acceptance below. Parent main3086038 was accepted
schema31; this source does not activate uploads, settlement or native writes.


W3b accepted,2026-10-19: exact runtimee140508b19aef4082f9565990402852d4f27cdb4,
schema32, all300 frozen runtime/contract inputs unchanged. Own39225921755 attempt1
book/Linux/Windows SUCCESS, full raw/source/own API ZIP/independent typed21+14/
project/all35 raw-field/type acceptance passed. Linux408state, Windows403state,
37core/22reader/62S3 plus native legacy/manifests and paired session/supervisor
regressions passed; ignored tests and warnings remain documented. Indexed78-member
hosted archive306210bytes SHAdc9fe9b4381633f403525a79943a645d8196606b62964c2acc78b2bb2e7364f1.
See [mixed preparation](mixed-plans.md) and dated evidence. Main's doc-only
successor pointer is reconciled without runtime changes. W4a is a separate draft
with a live local gate and a known observation-only restart-construction gap;
W4b/W5/W6, legacy/metadata-empty origins and provider/physical session acceptance
remain. No chunk writes or file-flush semantics follow from W3b preparation.


### W4a mixed dependency uploads — declared phase (2026-10-19)

Implement in isolated `codex/mixed-uploads`, retaining W3b's frozen source and
its separate own platform acceptance. This phase must turn a completed mixed
plan into exact physical upload receipts without reserving/publishing an
authority root or settling a local view. Keep existing full-snapshot contracts
unchanged. Use the same private SQLite owner for sparse views, preparation and
physical staging; refuse pairing a preparation with another executor owner.

Derive stable operation keys from the complete frozen plan/root/object identity;
use existing `begin_operation` staging records, with no new schema solely for a
redundant mapping. New data/pages are physical uploads; inherited data/subtrees
and proved holes never produce PUTs or fake upload receipts. Enumerate admitted
records with bounded pagination, not one command per absent descendant of a
reused subtree or each chunk in a large structural zero run. Every new parent
requires exact historical receipts for its new children and validated inherited
provenance. The final distinct mixed-tree receipt validates the complete retained
graph/new-object receipts; it is not a full-snapshot TreeReceipt, root commit,
source acknowledgment, fresh authorization or file flush.

Before fresh staging/send, the managed controller must check full source/route/
credential identity, provider create policy and checked exact-view semantics.
Fresh parent pages also require authenticated inherited provenance/hole checks.
Recovery of a claimed/uncertain attempt must enter observation before fresh
composition or policy admission; never replace its target, bytes, guard or ID,
and never issue another PUT. Persisted prepared attempts still require fresh
admission before their first send. SQL must end before all network operations.
Keep raw BLOB SHA distinct from manifest page digests. Preserve newer dirty
heads, old input bytes, failed/uncertain operations, quotas and caller cancellation.
No automatic delete/reclamation, retry under a new generation or unconditional
PUT fallback. Staging collisions, triggers, disk-full and corrupt mappings must
fail without false acknowledgment.

Acceptance: actual SDK/owned S3 writes show only changed data plus new pages/root,
unchanged subtree reuse/zero runs cause no descendant sends, complete mixed graph
verifies, direct parent out-of-order sends fail, partial/empty/Unicode boundaries,
wrong owner/binding/policy/provenance, lost PUT/restart/cancellation and retained
quota/SQLite failures. Tests must show recovery preserves one attempt and does
not require fresh original data for an already claimed dependency. Run full local
and own Linux/Windows/book/native regression cohort on exact source. W4b original
content-CAS root publication/recovery, W5 settlement/retention and W6 actual native
writes, legacy/metadata-empty origins and provider/physical session gates remain.


W4a review checkpoint,2026-10-19: the first SDK flow passed, and the new-transfer
walk passed its SDK regression. Four focused state controls passed. The pre-recovery full
local gate passed on303 frozen inputs. This is an intermediate draft, not
phase acceptance. In addition to remaining empty/partial/cancel/busy/fault cases,
process-restart recovery needs an independent observation-only constructor:
current `MixedUploader` owns an authenticated `Preparation`, whose normal reopen
requires fresh original-base authorization. A claimed physical attempt must be
recoverable without that reopen. Build the recoverer from its exact private
worker/client, executor binding and saved physical identity; refuse Prepared or
missing attempts, use observation-only `Executor::reconcile`, and acknowledge
only its exact Verified operation. It must never call a path that can freshly
claim or PUT, including under narrowed/expired policy. Test actual worker/process
restart with source metadata denied and unchanged PUT counts. Root CAS and local
settlement remain W4b/W5; no historical receipt authenticates a fresh root request.


W4a restart/control implementation,2026-10-19: added separate `MixedRecovery`,
which owns no source reader/authority/preparation handle, binds the same private
SQL owner and exact physical intent, refuses unclaimed/Prepared work, and uses
observation-only reconcile plus exact Verified acknowledgment. Actual nested
process recovery succeeded with source metadata denied and expired/narrow policy;
only the saved object's HEAD/GET were allowed, with unchanged operation/generation
and PUT counts. SDK empty/partial/growth/Unicode/zero cases, forged data/hole and
unread-tail refusal, shared four-slot busy/cancel/local SQL progress, and conditional
racing-creator rejection/restart passed. Historical object receipt, complete-tree
receipt, authority root commitment and local clean acknowledgment remain distinct.
This source needs a new full local and own combined cohort; the prior303-input
local archive covers only the pre-recovery checkpoint, not this changed source.


W4a final local verification,2026-10-19: owned gate4 completed exit0 on304 exact
runtime inputs after the scoped opt-in fixture correction.412state1ignored,
37core/22reader1nativeignored/62S3/18publication cases, formatting/Clippy/build,
77Markdown links and73book chapters passed. The indexed324-member final local
archive2060824bytes SHA27fd5c537e227ca1e401140a42970755592b23f5474de86ba0a34617b3cfca53
retains all304 sources and failed/focused/full evidence. Own combined/native
source acceptance remains required before promotion.

### W4b root publication: reviewed constraints and next phase (2026-10-19)

This is a proposal, not implemented mixed-root publication. Current source review
confirms `ReserveManifest` uses a content guard, and Publish inherits that guard
from its reservation. The authority accepts an older observed sequence for these
content-guarded requests; the payload reference must still match. Ordinary rename
or unrelated namespace changes therefore must not cause the publisher to replace
its original content CAS. Deletion and changed content remain explicit conflicts.
See `namespace::publication::{uses_content_guard,base_matches}` and
`namespace::Journal::apply` in the current state crate. The frozen descriptor's
`payload_reference` names the actual base; its reservation's own `guard.expected`
names a predecessor and must never be substituted as the new CAS expectation.

Save exact reserve and publish Apply requests/IDs before RPCs. The reserve ID must
match the new manifest generation, as enforced by `ManifestRoot::context`. Physical
root body size/raw SHA belong in the reservation; logical file length and the
domain-separated page digest belong in the root descriptor. Keep complete-tree
physical receipt, authorized root commit and exact-view clean settlement as
separate types; never manufacture the old full-snapshot SourceReceipt.

Before a fresh root publication request, validate the complete mixed plan's
source semantics as well as physical receipts. A historical TreeReceipt is
explicitly not source authentication. Replaying an already saved request must
happen before fresh source/policy/current-head probes and must preserve the
original guard, IDs and bytes. Two independent machines, lost reserve/root replies,
rename/delete/content races, quota/full disk, corrupt receipts and killed workers
need actual SDK/authenticated-authority controls before acceptance.

Resolve the metadata-only restart opener before implementation: normal private
state opening validates preparation BLOBs. Root recovery after a saved publication
request must not require S3 credentials or local source-body reads. A narrowly
typed metadata-only owner/client may be needed; it must retain the private
kernel lock and role/schema checks, expose no fresh reserve/send/prepare path,
and never weaken normal data-integrity validation. Record the chosen design and
its tests. W5 clean-base transition/reclamation and legacy/metadata-empty origin
admission remain required before W6 activation; this phase must not hide them
behind one-object fallback or declare mounted chunk writes supported early.


### W4b edited manifest root publication — design checkpoint (2026-10-19)

User reaffirmed immutable chunks plus manifests (D041). The next phase must
prove exact-view semantics, reserve the new manifest with the original base
payload guard, and durably replay exact saved authority requests after unknown
replies. Read the [publication design and acceptance contract](mixed-root-publication.md)
before implementation. The metadata-only recovery owner must not depend on a
normal state open that validates potentially damaged source BLOBs. Schema33,
semantic-check ledger and publisher/recovery controller are proposals, not
implemented behavior. W4a304-input runtime remains frozen in its separate
worktree pending its own complete hosted cohort. No native activation or local
clean transition is authorized by a physical upload receipt.


W4b1 independently green slice, declared2026-10-19: add a bounded offline
`check_data` verifier to the preparation journal/private worker. It must compare
actual admitted Data candidates with exact immutable view ranges and full
original-chunk witness/bytes; reject unused witness material, false base spans,
wrong paths and discarded-tail corruption. It returns no durable proof or remote
capability. State remains unchanged after checks. Validate new heads/restart,
partial/grown/all-local/all-zero/hole and multi-level ancestry cases plus the
full local and own hosted cohort. Schema32 unchanged. W4b2 must persist semantic
coverage and exact root requests/recovery; this slice does not replace W4b/W5/W6.


W4a accepted,2026-10-19: runtime3fd2a87/schema32/all304 frozen inputs. Own
39235506209 attempt1 book/Linux/Windows SUCCESS; full raw/source/own API ZIP/
independent typed21+14/project/all35 raw-field/type checks passed. Own84-member
archive342113bytes SHAccafd84462a1d08c6814d5ef164811b668a94721a69b0e0d46bbe64ba2420e81.
Linux412state/Windows407state,37core/22reader/62S3/18publication cases and real
legacy/manifests/paired sessions passed. See [mixed dependency uploads](mixed-uploads.md)
for scope, warnings, exact evidence and retained failures. Physical mixed upload
completion is accepted; W4b root CAS, W5 settlement, origins and W6 mounted chunk
writes remain. No new Wasabi or physical workstation session acceptance follows.


W4b1 local gate passed,2026-10-19: runner271909 EXIT0,306 unchanged inputs,
417state1ignored/37core/22reader1nativeignored/62S3/18publication cases, format/
Clippy/build/78Markdown/74chapters. Source dbdfd61 plus documentation-only accepted
parent merge8220555; indexed source/focused/failed/full evidence retained.
[Verifier evidence](evidence/mixed-semantics-20261019.json). Own hosted/native/
source cohort pending. W4b2 root journal/controller/recovery still unimplemented.


W4b1 accepted,2026-10-19: runtime5112fd2/schema32/all306 inputs; own39239546495
attempt1 book/Linux/Windows SUCCESS, complete raw/source/own API ZIP/typed21+14/
project/all35 raw fields/types passed. Indexed86-member hosted archive346729bytes
SHAd2b9ca34b9520b52359389b32fcabb7181a96173487557b03295ec11c7f1ae04.
Linux417state/Windows412state and inherited native/session regressions passed;
see [acceptance evidence](evidence/mixed-semantics-20261019.json). No root commit,
clean receipt or native chunk write follows. W4b2 remains separately unaccepted.


### W4b2 durable mixed-root publication — declared2026-10-19

Implement in codex/mixed-root-journal, separate from the frozen306-input W4b1
own cohort. Own additive schema033, prep/publication journal and semantic
coverage, same private owner API plus restricted existing-only metadata recovery
owner, managed publisher/recovery and exact authority/SDK/fault/restart tests.
Follow [the complete publication contract](mixed-root-publication.md). Preserve
strict normal state opening and old full-snapshot contracts. Exact original
content guard, physical receipt and semantic coverage precede saved Reserve/
Publish; after saved requests recover those exact operations before fresh
source/policy probes. Never replace IDs or invent clean receipts. Migration,
SQLite-full/quota/trigger/corruption, wrong owner/root/route and two-client
conflicts must retain dirty bytes. Empty-only snapshot and native fixture schema
expectations move together. Full local plus own platform/native/source acceptance
required. W5 clean transition/reclamation, origins and W6 native writes remain.


W4b2 implementation checkpoint,2026-10-19: schema33 state publication/semantic
coverage journal, same-owner API, restricted existing-only metadata owner and
managed publisher/recovery are wired in codex/mixed-root-journal. Eleven focused
state tests (including24 killed-owner boundaries) and two authenticated SDK/
authority cases passed; retained earlier import, isolation, harness and Clippy
failures are documented. Full local and own platform/native/source gates remain
required. This does not settle sparse views or activate native chunk writes.


W4b2 final local gate passed,2026-10-19: source d72f665/schema33/all317 inputs
unchanged. Owned gate2 EXIT0:429state1ignored/37core/22reader1nativeignored/62S3/
21publication, format/Clippy/build/78Markdown/74chapters. Exact failed gate1 and
corrected final sources are in the675-member local archive (SHA
756f32f0ecc4694ff44c2a9282e013c5c4c4674258fc4826e8aec53a0c07a2e9).
[Root evidence](evidence/mixed-roots-20261019.json). Own hosted/native/source
acceptance remains required. W5/origins/W6 are not replaced by root publication.


W4b2 own acceptance completed2026-10-19: exact2d9e213/schema33/all317,
run39249045883 attempt1 all3 successful. Complete raw/source/own API ZIP SHA,
independent typed21+14/project/all35 raw field/type comparison passed;97-member
hosted archive SHA d09d4d8200ad89c9b9919f7e00f8b1fbb7627603e1c713b00be825b5d7d9bbe2.
Linux429/Windows424state,21publication, real legacy FUSE9/WinFsp7 and native
manifest/paired startup passed. [Exact evidence](mixed-root-publication.md).
W5a successor in codex/mixed-publication-lineage has schema34 repeated-local-root
lineage; its full local gate2 is pending after an isolated test-dispatch factory
fix. W5b clean epochs and bounded retirement, origins and W6 remain.


### W5a exact publication lineage (declared, 2026-10-19)

Implement the [lineage contract](mixed-publication-lineage.md) in the isolated
`codex/mixed-publication-lineage` worktree. Existing W4b2 inputs stay frozen for
their own acceptance. W5a enables subsequent descendant publications against an
exact known local root, with strict foreign-content refusal and byte-preserved
old intents. W5b still must add clean-base epochs and bounded local retirement;
W5a alone must not be exposed as a successful native clean flush.


W5a complete local gate passed2026-10-19: exact71242f5/schema34/all319,
434state1ignored/37core/22reader1nativeignored/62S3/22publication; schema fixtures,
format/Clippy/build/79MD/75chapters passed. A non-inlined scenario factory fixed
a retained old test-dispatch stack overflow without increasing stack limits.
[Local lineage evidence](mixed-publication-lineage.md) includes both exact319
sources in the663-member archive SHA
33d07eb2df7f6742dd7bc56a70d86cbb182400de6feb19e3b24e0f3256543359.
Own hosted/native/source acceptance remains required before W5a runtime promotion.
W5b clean epochs/retention and explicit origins/W6 remain.


W5a own cohort39253634685 attempt1 is running for exact source50ee01c/schema34.
Main runtime remains accepted W4b2/schema33 until full own source/native/artifact
acceptance. [Current successor evidence](mixed-publication-lineage.md).


W5a own acceptance completed2026-10-19: exact50ee01c/schema34/all319,
run39253634685 attempt1 all3 success. Full raw/source/own API ZIP SHA/independent
typed21+14/project/all35 raw fields/types passed. Indexed99-member archive SHA
3509706b384b8943e24512bb08019f3465f433050eb6a23b8feeba6d40b0e9f2.
[Exact lineage acceptance](mixed-publication-lineage.md). W5b1 successor source
3d5de01/schema35/all324 has11 focused state cases and actual SDK cycles passed;
its full local gate2 is running after a single unused-unit Clippy fix. It does not
activate native chunks. Bounded retention and fresh external-base adoption,
explicit origins/W6 and provider/physical session gates remain required.


### W5b1 clean epochs — declared2026-10-19

Implement [the exact acknowledgment/epoch contract](mixed-clean-epochs.md) in
codex/mixed-clean-epochs, isolated from W5a's frozen319-input own cohort. Own
schema35 record/view extension, metadata/source validators, source-mutation identity
guards, atomic same-owner acknowledgment and scoped current-head facade, managed
local acknowledgment and SDK/fault/recovery tests. Preserve literal old formats,
source reads and exact remote recovery. Full local and own hosted/native/source
gates precede promotion; no reclamation or native chunk-write activation.


Cross-machine clean-base observation is an additional required boundary before
W6: adopt a fresh authoritative manifest only against a proven clean local head,
with no unresolved/dirty work, retaining prior snapshots. It is distinct from
W5b1 acknowledgment of our own publication. See [the epoch contract](mixed-clean-epochs.md).
W5b1 full gate1 stopped at a single unused-unit Clippy diagnostic with all324
inputs unchanged; only that expression was removed. A fresh full gate follows.


W5b1 full local gate3 passed2026-10-19: exact652f938/schema35/all324,
446state1ignored/37core/22reader1nativeignored/62S3/23publication, format/Clippy/
build/80MD/76chapters. Both old-schema regression tests and all12 new ack cases
passed. The1004-member local archive preserves final324 and both failed324 sources;
SHA9c46f7b89c352a5b3b97297c39c7fd5aca4980b4683d85bdc6254d07acbcb48b.
[Exact epoch evidence](mixed-clean-epochs.md). Own hosted/native/source acceptance
remains required before promotion; no mounted chunk writes or reclamation.


W5b1 own cohort39263559455 attempt1 is running for exacta357499/schema35/all324.
Full local446state/23publication passed. Source/native/artifact acceptance remains
required before runtime promotion; main remains W5a/schema34. [Current evidence](mixed-clean-epochs.md).

W5b1 own acceptance completed2026-10-19: exacta357499/schema35/all324,
run39263559455 attempt1 all3 success. Full raw/source/own API ZIP SHA/independent
typed21+14/project/all35 raw fields/types passed. Indexed104-member hosted archive
SHA2dbf5b3c542b57f7dff302cea67ccb210a38f4dcac08768b20539dd1fc37de9a.
[Exact epoch acceptance](mixed-clean-epochs.md). W5b2a scoped operations are on
an isolated sourcea3028bf/schema36 branch with their own local/hosted gates running;
they are not promoted by W5b1 acceptance. Pins/seals/GC, fresh external-base
adoption, origins/W6 and provider/physical session acceptance remain.

W5b2a scoped-operation sourcea3028bf/schema36 is isolated and unaccepted. Fourteen
focused state cases/14 killed-owner boundaries, SQLITE_FULL and the scoped SDK
cycle passed. Complete local gate1 stopped101 on two existing SDK upload Timeouts
(22publication cases passed); all329 inputs unchanged. Own CI39267986023 remains
live for that exact source. Investigation uses a separate repair worktree; no
runtime change is promoted. [Current scoped contract](scoped-source-operations.md).
