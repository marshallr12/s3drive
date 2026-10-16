# Purpose and current status

**Goal:** enable S3-compatible storage to back local virtual drives/filesystems
on Linux and Windows, using Rust. Files should remain remotely stored while
ordinary desktop applications access them through familiar filesystem paths.
One drive may expose several buckets or prefixes.

**Research date: 2026-10-16.** This book is the durable record of research,
proposed behavior, implementation phases, and discussions. External technical
facts have primary-source links. Architectural choices are our recommendations,
not promises made by the cited projects.

## Recommendation

Build a per-user Rust process with a shared namespace/cache engine, a Linux FUSE
adapter, and a Windows WinFsp adapter. Use a local SQLite database for metadata
and later a persistent upload journal. Keep bulk file data in ordinary local
cache files. Begin with explicit, read-only bucket roots such as:

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
conflict handling and rename are separate milestones. If atomic shared
filesystem semantics become essential, revisit a managed object layout with
an authoritative metadata service rather than hiding weaknesses of copy/delete.

## What this repository implements

| Feature | Status |
| --- | --- |
| Rust workspace and CLI | Implemented |
| TOML parsing and source/namespace validation | Implemented; no network checks |
| Alias + prefix + file-path resolution | Implemented as a planning utility |
| Local SQLite state | Schema 2; credential-binding registry, atomic migration, private directory and exclusive process ownership |
| mdBook research, design and acceptance plan | Implemented |
| S3 transport and credentials | Implemented; [Wasabi read acceptance](wasabi.md) passed; broader conformance pending |
| Linux filesystem mount | Implemented read-only FUSE; see [mount contract](linux-mount.md) and native evidence in handoff |
| Windows drive mount | Implemented WinFsp adapter; [native read-only acceptance passed](windows-mount.md) |
| Linux sign-in startup, including MX without systemd | [Detached launcher implemented](startup-and-sessions.md); native desktop/login-hook/terminal-loss checks passed |
| Windows startup across login sessions | [Scheduled task and shared NTFS folder implemented](startup-and-sessions.md); native cross-logon reads/removal passed; workstation reboot/SSH authentication pending |
| Cross-machine coordination with Turso | [Research and design proposal](turso-coordination.md); no dependency or cloud service enabled |
| Read cache, upload journal, offline use, tray UI | Planned |

The initial research/scaffold delivery is complete. Work has advanced to the
[read-only provider adapter](provider-contract.md). The overall product goal
remains open until both OS adapters and the
agreed filesystem contract have passed their acceptance tests.

## Assumptions for planning

An individual user runs one process per volume with local SSD space available.
The first namespace is one folder per configured source, not a merged union,
RAID array, automatic sharding, or replication scheme. Existing objects should
remain useful through independent S3 tools. Read-only access is the first usable
milestone. Wasabi is the selected first hosted provider in `us-east-2`; isolated
test fixtures and local credential profiles are configured. Workload sizes,
cache budget and packaging requirements remain open; [open questions](decisions.md)
record those choices.
