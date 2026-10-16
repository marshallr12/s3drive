# Decisions and open questions

Recorded 2026-10-16. “Proposed” decisions guide the plan and can be changed before
their implementation phase. They are not additional requirements attributed
to the user.

| ID | Status | Decision and reason |
| --- | --- | --- |
| D001 | Adopted for scaffold | Rust Cargo workspace with shared core and separate OS adapters; mdBook holds research and plans |
| D002 | Adopted for scaffold | Local SQLite via rusqlite; bundled SQLite avoids relying on a host-installed library |
| D003 | Proposed | FUSE/fuser on Linux and WinFsp on Windows for genuine filesystem mounts and drive letters |
| D004 | Proposed | Direct object layout first, preserving existing S3 interoperability |
| D005 | Adopted for scaffold/first milestone | Read-only mode; no implicit write or deletion support |
| D006 | Proposed | One alias per bucket/prefix under a virtual root; no union, sharding or replication |
| D007 | Adopted for P1 reads | AWS SDK implements list/head/range reads; mutation/recovery requirements still need evaluation |
| D008 | Proposed | Explicit flush means remote durability by default; offline local durability must be a separate mode |
| D009 | Adopted | Unknown capabilities remain unknown; no fallback from failed conditional writes to unconditional overwrite |
| D010 | Deferred | Windows binding/distribution license and installer arrangement; P0 links neither runtime nor wrapper |
| D011 | Deferred | Full POSIX/NTFS compatibility, distributed locks, writable database workloads and cross-source atomicity |
| D012 | Deferred | Tray UI and optional Cloud Files surface until the shared engine is reliable |
| D013 | Adopted for P1 | Source configuration controls S3 endpoint/region; explicit profiles do not fall back to ambient keys; numeric-loopback HTTP is test-only opt-in |

## Questions to resolve at the relevant phase

- **Before provider integration:** Which providers and regions are first (AWS,
  self-hosted or another S3 service)? What authentication and private CA setup
  must work? Which disposable bucket/prefix is available for conformance tests?
- **Before the first mount:** Typical file sizes, directory sizes and working
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
