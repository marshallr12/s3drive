# Purpose and current status

**Goal:** enable S3-compatible storage to back local virtual drives/filesystems
on Linux and Windows, using Rust. Files should remain remotely stored while
ordinary desktop applications access them through familiar filesystem paths.
One drive may expose several buckets or prefixes.

**Research date: 2026-10-16.** This book is the durable record of research,
proposed behavior, implementation phases, and discussions. External technical
facts have primary-source links. Architectural choices are our recommendations,
not promises made by the cited projects.

**Selected target:** [managed layout with stronger coordination](managed-layout.md),
chosen by the user on 2026-10-16. A shared authority owns the namespace; immutable
S3 objects hold file generations. Existing direct mounts stay available without
automatic conversion. The SQLite authority and [immutable publication library](managed-publication.md)
are implemented. [Managed read-only integration](managed-native.md) now connects
the native adapters and passed Linux/Windows native acceptance; [managed existing-file editing](managed-write-plan.md) has a verified Linux/Windows native checkpoint, with exact-tip publication gates recorded separately. [Managed creation/directories](managed-namespace-plan.md) passed real FUSE/WinFsp acceptance; [version-6 rename/replacement and Linux removal](managed-native-mutations.md) now add retained-handle recovery. Windows deletion, full application compatibility and remote deployment remain work.

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
Begin with explicit, read-only bucket roots such as:

```text
Linux:   ~/S3Drive/documents/report.pdf
Windows: S:\documents\report.pdf

documents -> configured endpoint / bucket / desktop/report.pdf
photos    -> another endpoint / another bucket / image.jpg
```

Linux and Windows now implement this namespace; native acceptance is recorded
separately for each adapter.
Preserve native object names and content in the initial design, accepting a
documented subset of filesystem semantics. Random writes, durable upload,
conflict handling and rename are separate milestones. The managed target now uses an authoritative metadata service for atomic
namespace edits; direct-key behavior below describes the existing compatibility implementation.

## What this repository implements

| Feature | Status |
| --- | --- |
| Rust workspace and CLI | Implemented |
| TOML parsing and source/namespace validation | Implemented; no network checks |
| Alias + prefix + file-path resolution | Implemented as a planning utility |
| Local SQLite state | Schema 12; logical managed edits, namespace intents, portable collision keys, authoritative empty-file placement and publication/authority tables plus bindings, clean BLOBs, staged generations, remote attempts, flush receipts and private process ownership |
| mdBook research, design and acceptance plan | Implemented |
| S3 transport and credentials | Implemented; [Wasabi read acceptance](wasabi.md) passed; broader conformance pending |
| Linux filesystem mount | Implemented read-only FUSE; see [mount contract](linux-mount.md) and native evidence in handoff |
| Windows drive mount | Implemented WinFsp adapter; [native read-only acceptance passed](windows-mount.md) |
| Linux sign-in startup, including MX without systemd | [Detached launcher implemented](startup-and-sessions.md); native desktop/login-hook/terminal-loss checks passed |
| Windows startup across login sessions | [Scheduled task and shared NTFS folder implemented](startup-and-sessions.md); native cross-logon reads/removal passed; workstation reboot/SSH authentication pending |
| Managed local edits | [Durable logical heads and settlement](managed-edits.md); storage/SDK library; [native integration](managed-write-plan.md) adds version-4 existing-file edits |
| Managed file publication | [Real SDK/HTTP/SQLite library](managed-publication.md); existing-file edits and creation passed native acceptance; remote deployment/live managed-provider acceptance pending |
| Managed namespace authority | [SQLite and authenticated loopback development service](managed-layout.md); private token CLI and explicit version-3 read-only mounts |
| Managed native reads | [Authority namespace and verified immutable payloads](managed-native.md); real FUSE/WinFsp acceptance passed |
| Managed rename/replacement and Linux removal | [Version-6 contract and acceptance](managed-native-mutations.md); retained handles and exact recovery; Windows deletion remains unsupported |
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
| Rename/delete, complete save replacement, new offline opens, tray UI | Planned |

The initial research/scaffold delivery is complete. Work has advanced to the
[read-only provider adapter](provider-contract.md). The overall product goal
remains open until both OS adapters and the
agreed filesystem contract have passed their acceptance tests.

## Assumptions for planning

An individual user runs one process per volume with local SSD space available.
The first namespace is one folder per configured source, not a merged union,
RAID array, automatic sharding, or replication scheme. Existing direct configurations retain ordinary S3 keys. The selected managed
layout uses internal generation keys and requires explicit import/export. Read-only access is the first usable
milestone. Wasabi is the selected first hosted provider in `us-east-2`; isolated
test fixtures and local credential profiles are configured. Workload sizes,
cache budget and packaging requirements remain open; [open questions](decisions.md)
record those choices.
