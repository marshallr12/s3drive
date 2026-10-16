# Decisions and open questions

Recorded 2026-10-16. “Proposed” decisions guide the plan and can be changed before
their implementation phase. They are not additional requirements attributed
to the user.

| ID | Status | Decision and reason |
| --- | --- | --- |
| D001 | Adopted for scaffold | Rust Cargo workspace with shared core and separate OS adapters; mdBook holds research and plans |
| D002 | Adopted for scaffold | Local SQLite via rusqlite; bundled SQLite avoids relying on a host-installed library |
| D003 | Implemented; native acceptance recorded separately | FUSE/fuser on Linux and WinFsp on Windows for genuine filesystem mounts and drive letters |
| D004 | Adopted for reads | Direct object layout first, preserving existing S3 interoperability |
| D005 | Adopted for scaffold/first milestone | Read-only mode; no implicit write or deletion support |
| D006 | Adopted for P2 | One alias per bucket/prefix under a virtual root; no union, sharding or replication |
| D007 | Adopted for P1 reads | AWS SDK implements list/head/range reads; mutation/recovery requirements still need evaluation |
| D008 | Proposed | Explicit flush means remote durability by default; offline local durability must be a separate mode |
| D009 | Adopted | Unknown capabilities remain unknown; no fallback from failed conditional writes to unconditional overwrite |
| D010 | Private prototype; distribution deferred | P3 uses GPL-3.0 winfsp wrapper; application distribution license and installer arrangement remain undecided |
| D011 | Deferred | Full POSIX/NTFS compatibility, distributed locks, writable database workloads and cross-source atomicity |
| D012 | Deferred | Tray UI and optional Cloud Files surface until the shared engine is reliable |
| D013 | Adopted for P1 | Source configuration controls S3 endpoint/region; explicit profiles do not fall back to ambient keys; numeric-loopback HTTP is test-only opt-in |
| D014 | User selected and tested for reads | Wasabi `us-east-2`, bucket `s3drivetest`, local profile `wasabi-s3drive`; isolated test objects were approved, verified and removed |
| D015 | Adopted for P2 | Bounded, uncached read-only namespace; collisions fail the directory, handles pin revisions, Linux direct I/O avoids mixed kernel cache revisions |
| D016 | Adopted from live Wasabi evidence | Explicit `list_url_encoding = "form"` decodes raw listing plus signs as spaces before one percent decode; other providers retain percent-only default |
| D017 | User requirement; Linux and Windows launchers implemented | Start at sign-in after reboot; keep the mount accessible from later same-account SSH sessions and surviving tmux processes. Support MX Linux without systemd. Linux terminal-loss and Windows cross-logon checks passed; actual workstation reboot/SSH authentication acceptance remains pending |
| D018 | Proposed after research | Evaluate Turso Cloud behind an authoritative coordination API; retain local SQLite. Do not use offline last-push-wins sync for leases or publication, or treat SQL/S3 as one transaction |
| D019 | Implemented prerequisite for P4 | Bind each successful open to its resolved credential generation and source configuration. Require online HEAD on new opens; credential rotation creates a separate cache namespace. Preserve schema 1 rows as unbound legacy data during migration to schema 2. This does not establish a provider-global principal or enable offline access |
| D020 | Implemented | SQLite BLOBs for clean snapshots up to 1 MiB make bytes/checksum/metadata atomic. Use one bounded worker, explicit byte/entry limits and pins. Bulk caching remains future work; staged data follows D022. No performance or new offline-access claim |
| D021 | Implemented for P4b2b | Opt-in mounted clean cache: authorize every new open online, retain immutable fully cached snapshots until handle release, coalesce fills, and use direct reads for large files or pinned quota pressure. This permits already-open cached reads during network loss but denies new offline opens |
| D022 | Implemented local P5a1 foundation | Non-evictable bounded SQLite chunks, manifests and pending intent make local generation publication transactional across Linux/Windows. Local receipts do not acknowledge remote writes. External bulk files remain optional and need a separate durability proof |
| D023 | Implemented P5a2a | One async owner serves cache and journal commands on the same connection and process lock. Bound the queue and inputs; an accepted request with a lost reply has an unknown outcome. Inspect durable state and use guarded append/seal replay; never infer rollback from cancellation |
| D024 | Implemented P5a2b library | Immutable edit plans bind the complete fixed input and patch/truncate intent to a durable operation key. Hydrate unchanged S3 ranges through a pinned handle outside SQL, preserve sparse local extents and publish one complete local snapshot. Failed builders stay recoverable; no writable callback or remote commit is enabled |
| D025 | Implemented P5b1 probe boundary | Use one SDK mutation attempt and observe exact operation metadata plus full bytes after a lost result. Absence does not prove no prior commit. Confine provider tests and fixture-only plain DELETE cleanup to a reviewed six-key unversioned namespace; never enable application writes from compatibility claims |

## Questions to resolve at the relevant phase

- **Before cache sizing and broader OS acceptance:** Typical file sizes, directory sizes and working
  set? How much local disk may the cache consume? Which Windows/Linux versions
  and CPU architectures need support?
- **Before Windows packaging:** Is a drive letter essential, or is an Explorer
  sync folder sufficient? Is the eventual app private/personal, open source,
  internally distributed, or commercially distributed?
- **Before writes:** Must objects remain directly usable by other S3 tools?
  Can other computers or tools modify the same objects concurrently? Which
  desktop applications and rename behavior must be supported?
- **Before offline write-back:** Is local-only durability acceptable while
  disconnected? How should conflicts be named/presented and manually resolved?

These do not block research or P0. Do not ask for credentials in chat; configure
them through the provider mechanism when actual network tests are authorized.

## Rejected shortcuts

Do not use the mounted drive for its own cache or SQLite state. Do not call a
copy/delete pair an atomic rename, treat an ETag as a universal content hash,
or use one uploaded SQLite file as shared multi-client coordination. Do not
claim that SDK compatibility implies provider semantics. A drive that silently
loses edits is not an acceptable compromise for a shorter implementation plan.
