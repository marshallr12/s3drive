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
| D020 | Implemented storage; mount integration pending | SQLite BLOBs for clean snapshots up to 1 MiB make bytes/checksum/metadata atomic. Use one bounded worker, explicit byte/entry limits and pins. Bulk files and staged writes remain separate future storage; no performance or offline-access claim |

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
