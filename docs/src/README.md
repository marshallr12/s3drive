# Purpose and current status

**Goal:** enable S3-compatible storage to back local virtual drives/filesystems
on Linux and Windows, using Rust. Files should remain remotely stored while
ordinary desktop applications access them through familiar filesystem paths.
One drive may expose several buckets or prefixes.

**Research dates: 2026-10-16–18.** This book records research, design,
implementation contracts and scoped acceptance evidence. External technical
claims have primary-source links; recommendations and unverified behavior are
labelled.

**Selected target:** [managed layout with stronger coordination](managed-layout.md),
chosen2026-10-16. One shared Rust authority owns SQLite namespace metadata;
immutable S3 objects hold file generations. Each computer has a separate private
SQLite cache/journal. Existing direct-key mounts remain available without an
automatic conversion.

Managed native reads, existing-file writes, directory/file creation and
rename/save replacement have accepted Linux/Windows checkpoints. Linux removal
and retained handles are implemented; Windows deletion uses the explicit
[version7/POSIX lifecycle profile](native-windows-lifecycle.md), with its measured
WinFsp limitations. [HTTPS transport](remote-authority.md) and
[store/configuration bootstrap](managed-bootstrap.md) are accepted through main
`bf9509e`. This is still a research/development filesystem with incomplete
application, platform and provider acceptance.

**Current main checkpoint:** coordinator startup, private offline
[snapshot creation/inspection](authority-backup-recovery.md), and bounded TLS
socket diagnostics passed at `bf9509e`,
[primary CI39113628548](https://github.com/marshallr12/s3drive/actions/runs/39113628548)
and an [unchanged-source repeat](https://github.com/marshallr12/s3drive/actions/runs/39115395230).
Exact uncertain identities, dirty conflicts, Unicode/binary/empty reads and
same-supervisor child replacement have scoped hosted acceptance. This does not
establish actual workstation reboot, restore/failover or elimination of the
historical intermittent Windows transport failures.

**Current work:** bounded [dispatch-boundary observations](owned-authority-client.md#dispatch-boundary-observations-candidate-2026-10-18)
add sender/body/budget hints to distinguish an active driver from body/header
progress. Focused controls pass; this candidate changes no scheduling or retry
policy and does not establish the historical Windows cause.

Its source2575374 attempt1 [hosted cohort](https://github.com/marshallr12/s3drive/actions/runs/39133178423)
passed Linux/Windows/book and its own strict21/14 reference artifact checks.
The corrected schema24 Windows session fixture passed natively. Its exact-source
attempt2 also passed all three jobs, reviewed own logs and fresh strict21/14
artifacts; see [separate repeat evidence](evidence/dispatch-repeat-20261018.json).
The historical transport cause remains unresolved. These checks do not certify
the newer manifest codec or promote runtime main.

The [large managed-file representation study](large-managed-files.md) audits
the remaining whole-file boundaries and compares one-object generations with
immutable chunks and authenticated manifests. The recommendation and diagram
explain the chunked format selected by the user on2026-10-18. An offline bounded
core codec now prototypes it. Existing mounted formats and limits remain.

[Offline immutable multipart plans](multipart-plan.md#offline-immutable-plans-candidate-implementation-2026-10-18)
freeze original bindings and hash verified staged bytes in bounded owner commands.
The candidate adds schema24 and preserves the existing8MiB upload/edit limits;
no multipart network execution is enabled. Local and source-bound platform
acceptance are recorded separately in the [dated evidence](evidence/multipart-plans-20261018.json).

The [owned authority client](owned-authority-client.md) replaces
the production metadata HTTP execution path on the candidate branch. Its
portable controls retain complete SQLite receipts through bounded shutdown;
its exact-source47e9dd4 Windows burst failed at step22 with an empty-header
timeout, while Linux/book passed. The later instrumented90a8357
[cohort](https://github.com/marshallr12/s3drive/actions/runs/39124027385) passed
Linux/Windows native gates and its own strict21/14 reference artifacts. This is
one cohort without a source-identical repeat or established failure cause.
The measured client-local
[pending-scan optimization](managed-edits.md#retained-history-scan-validation-candidate-2026-10-18)
at64aa073 passed its own Linux/Windows native/startup checks and strict21/14
reference reports. Its [unchanged-source repeat](https://github.com/marshallr12/s3drive/actions/runs/39127440399)
failed Windows paired startup while Linux/book passed. Its matched exchange
was actively polled but wrote no plaintext bytes before the server timed out.
It preserves checks on retained history while reusing proofs within one read
command; no causal transport correction is established.
The accepted main still uses the previous client. Multipart/large files,
writable restoration, real managed-provider acceptance and remote deployment
remain incomplete. No real user startup, remote hosting or Turso backend has
been installed.

## Recommendation

Build a per-user Rust process with a shared namespace/cache engine, a Linux FUSE
adapter, and a Windows WinFsp adapter. Use a local SQLite database for metadata
and a local pending-intent journal. An async owner shares the cache and journal
connection; local edit plans provide patch/truncate and pinned-base hydration.
The [local attempt journal](remote-journal.md) preserves single-use claims and
uncertain outcomes. A [journal-driven SDK executor](upload-executor.md) is tested
against local HTTP fixtures; its [explicit write policy](write-policy.md) binds
construction to source configuration and recorded evidence. An [explicit write-capability probe](conditional-uploads.md)
reserves disposable keys and records provider evidence without enabling mounted writes.
Store small clean snapshots in SQLite; bulk cache files remain future work.
The direct compatibility layout exposes explicit bucket roots such as:

```text
Linux:   ~/S3Drive/documents/report.pdf
Windows: S:\documents\report.pdf

documents -> configured endpoint / bucket / desktop/report.pdf
photos    -> another endpoint / another bucket / image.jpg
```

Linux and Windows now implement this namespace; native acceptance is recorded
separately for each adapter.
The managed target uses authoritative metadata for atomic namespace edits and
immutable S3 generations for content. The direct-key example describes the
compatibility layout. Supported behavior, explicit refusal paths and remaining
application limits are recorded in the [filesystem contract](filesystem-contract.md).

## What this repository implements

| Feature | Status |
| --- | --- |
| Rust workspace and CLI | Implemented |
| TOML parsing and source/namespace validation | Implemented; no network checks |
| Alias + prefix + file-path resolution | Implemented as a planning utility |
| Local SQLite state | Main schema23; offline multipart candidate schema24; separate authoritative namespace and client databases, immutable staged bytes, exact operation/receipt recovery, private ownership, publication/disposition/open-session journals and cache |
| mdBook research, design and acceptance plan | Implemented |
| S3 transport and credentials | Implemented; [Wasabi read acceptance](wasabi.md) passed; broader conformance pending |
| Linux filesystem mount | FUSE reads and explicit policy-bound writes/creation/managed mutation; [initial mount contract](linux-mount.md) and later managed acceptance |
| Windows drive mount | WinFsp reads, policy-bound writes and managed mutations; [initial acceptance](windows-mount.md) and explicit [version7 lifecycle](native-windows-lifecycle.md) |
| Linux sign-in startup, including MX without systemd | [Detached launcher implemented](startup-and-sessions.md); native desktop/login-hook/terminal-loss checks passed |
| Windows startup across login sessions | [Scheduled task and shared NTFS folder implemented](startup-and-sessions.md); native cross-logon reads/removal passed; workstation reboot/SSH authentication pending |
| Managed local edits | [Durable logical heads and settlement](managed-edits.md); storage/SDK library; [native integration](managed-write-plan.md) adds version-4 existing-file edits |
| Managed file publication | [Real SDK/HTTP/SQLite library](managed-publication.md); existing-file edits and creation passed native acceptance; remote deployment/live managed-provider acceptance pending |
| Authority client transport | [Owned TCP/TLS/HTTP execution candidate](owned-authority-client.md); source47 failed Windows; later64 passed its primary native/reference cohort but failed Windows paired startup on its unchanged-source repeat; historical cause unresolved and accepted main retains the previous client |
| Private authority snapshots | [Offline creation and inspection](authority-backup-recovery.md) accepted at bf9509e; integrity is not history freshness; writable restore/failover remain unsupported |
| Managed namespace authority | [Rust/SQLite service](managed-layout.md), authenticated [HTTPS](remote-authority.md), exact replay and explicit [store/configuration setup](managed-bootstrap.md); real remote deployment pending |
| Managed native reads | [Authority namespace and verified immutable payloads](managed-native.md); real FUSE/WinFsp acceptance passed |
| Managed rename/replacement and Linux removal | [Version6 contract](managed-native-mutations.md), retained handles and exact recovery; versions1–6 retain unsupported Windows deletion, version7 selects the measured lifecycle profile |
| Cross-machine coordination with Turso | [Research and design proposal](turso-coordination.md); no dependency or cloud service enabled |
| Clean-cache storage library | Bounded worker, checksums, LRU/pins and recovery tests |
| Mounted caching | Opt-in; online HEAD on new opens, revision snapshots, restart and integrity checks |
| Local staging/journal foundation | Immutable chunked generations and pending intent; [retained snapshots and local edits](staged-writes.md) |
| Conditional write probe | [Scoped Wasabi single-PUT acceptance passed](wasabi.md#conditional-put-acceptance--2026-10-16); all six fixtures removed |
| Durable remote-attempt journal | [Local single-use claims and uncertain-result recovery](remote-journal.md); executor described below |
| Journal-driven SDK executor | [Implemented and tested on Linux/Windows](upload-executor.md); explicit source/evidence-bound factory; native integration below |
| Generation-specific remote flush | [Library receipt and atomic acknowledgment](flush-receipts.md); retain newer heads and all snapshots |
| Existing-file write mounts | [Version-2 opt-in implementation](native-writes.md), native Linux/Windows acceptance passed; no real writable configuration installed |
| Regular-file creation in existing writable directories | [Guarded creation and durable rediscovery](native-creation.md); native Linux/Windows acceptance passed |
| Managed file/directory creation | [Version-5 portable volumes](managed-namespace-plan.md); native Linux/Windows creation and recovery passed |
| Coordinator startup and paired managed drive | [Pinned startup contract](authority-startup.md); hosted Linux/Windows paired recovery accepted at a7994e6; actual reboot remains separate |
| New offline opens, multipart/large files, reclamation, tray UI, backup/failover and full application compatibility | Remain work; no unverified path is enabled implicitly |

The initial research/scaffold delivery is complete. The overall product goal
remains active: native startup/recovery, broader target-machine/provider checks,
remote deployment and the remaining filesystem contract must be verified.
Main and the current candidate branch have different acceptance scopes; use the
linked evidence rather than assuming every candidate feature is published.

## Assumptions for planning

An individual user runs one process per volume with local SSD space available.
The first namespace is one folder per configured source, not a merged union,
RAID array, automatic sharding, or replication scheme. Existing direct configurations retain ordinary S3 keys. The selected managed
layout uses internal generation keys and requires explicit import/export. Read-only access is the first usable
milestone. Wasabi is the selected first hosted provider in `us-east-2`; isolated
test fixtures and local credential profiles are configured. Workload sizes,
cache budget and packaging requirements remain open; [open questions](decisions.md)
record those choices.
