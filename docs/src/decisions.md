# Decisions and open questions

Recorded 2026-10-16. “Proposed” decisions guide the plan and can be changed before
their implementation phase. They are not additional requirements attributed
to the user.

| ID | Status | Decision and reason |
| --- | --- | --- |
| D001 | Adopted for scaffold | Rust Cargo workspace with shared core and separate OS adapters; mdBook holds research and plans |
| D002 | Adopted for scaffold | Local SQLite via rusqlite; bundled SQLite avoids relying on a host-installed library |
| D003 | Implemented; native acceptance recorded separately | FUSE/fuser on Linux and WinFsp on Windows for genuine filesystem mounts and drive letters |
| D004 | Historical read/compatibility prototype; target superseded by D031 | Direct objects preserved existing S3 interoperability; retain existing configurations without implicit migration |
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
| D026 | Implemented P5b2a local journal | Commit a single-use send claim before issuing its in-memory permit; never reissue after response loss/restart. Preserve uncertain/rejected attempts and local bytes. Bind descendant guards to verified parent receipts; observation of absence does not authorize replay. Network execution, explicit resolution and reclamation remain separate phases |
| D027 | Implemented P5b2b executor boundary | A private send function consumes the journal permit; reconciliation never sends. Verify complete operation-bound bytes even after a success receipt. Compile the executor normally but keep construction unavailable until provider and write-policy acceptance; local wire fixtures cannot authorize ordinary cloud writes |
| D028 | Proposed multipart extension | Use provider-specific limits and immutable part plans; journal upload-ID and completion uncertainty separately. Guard completion once and reconcile the operation before any new publication. Unknown or expired upload IDs cannot prove publication did not occur; multipart needs separate live acceptance and authorization |
| D029 | Implemented P5b2c library opt-in | Construct the executor only from separate explicit source/prefix/byte/operation/metadata policy and complete plan-bound local evidence. Pin matching credentials; bound new claims by evidence age. Expired/narrowed policy still permits observation-only recovery. Local files are trusted owner input, not signed authorization; existing mount configuration remains read-only |
| D030 | Implemented P5b2d generation-specific flush | Require exact remote verification and atomic local receipt/ancestor settlement before flush succeeds. Keep retained sealed-intent rows, payloads, immutable manifests and remote identities. Never clear a newer head or acknowledge an unresolved result. Pending enumeration validates settlement coverage before suppressing work; payload reclamation and writable callbacks remain separate acceptance phases |
| D031 | User selected, 2026-10-16 | Managed layout with stronger cross-machine coordination: authoritative namespace and immutable S3 generations. Ordinary S3 tools see internal keys; no implicit migration. Turso remains an optional backend subject to acceptance |
| D032 | Implemented P6m1 prototype | Persistent SQLite namespace authority with stable IDs, global sequence CAS and transactional operation/request/result deduplication. Loopback service and exact-byte Unicode policy are development boundaries; payload publication is now covered by D033; native writes remain disabled; P6m3a read-only integration is explicit |
| D033 | Implemented P6m2 library | New immutable create-only blob roots per logical file revision, with exact reservation request persisted before HTTP, verified SDK bytes, authority file/volume CAS and separate local logical acknowledgment. Retain every uncertain/conflicting generation. Store routing digests exclude machine-local profiles and do not prove a provider-global principal |
| D034 | Implemented P6m3a, 2026-10-17 | Version-3 managed mounts are read-only, with authority-defined paths, store-to-local-source routing and bounded verified in-memory snapshots. Separate private-token authority CLI; no implicit migration, remote deployment or managed writable callbacks |
| D035 | Implemented P6m3b1 library, 2026-10-17 | Keep volume/node local heads separate from S3 generation targets. Persist exact logical edit/publication inputs, bind operation IDs exclusively, verify whole bytes during atomic seal, and require physical plus metadata receipts before atomic logical settlement. Preserve newer heads and all conflicts; native integration follows in D036 |
| D036 | Implemented P6m3b2, 2026-10-17 | Explicit version-4 existing-file edits with policy-bound stores and an empty-file default store. Add full-payload-reference CAS, preserve legacy guards/hashes, and require the authority capability. Store verified clean-base observations in schema 11; dirty conflicts retain bytes. Serialize mutation/base adoption and publication initially; native create/rename/delete/save-replace remain unsupported |
| D037 | Native creation accepted, 2026-10-17 | New portable-collision-v1 volumes preserve display names but admit one Unicode-17 compatibility/case-closure alias per directory. Version 5 opts into creation/directories; no existing-volume conversion. Persist complete namespace intents and authoritative empty-file store placement before effects; recover exact operation IDs. Native rename/removal/save replacement remain separate phases |
| D038 | Authority entry CAS implemented, 2026-10-17; native integration pending | Guard source/destination identities and revisions plus both parent revisions; replace by an atomic move and retained target tombstone. Persist exact outcomes with the effect, detect ignored writes and preserve all payload references. Do not enable mounted rename/removal before journal, detached publication and native acceptance |

## Questions to resolve at the relevant phase

- **Before cache sizing and broader OS acceptance:** Typical file sizes, directory sizes and working
  set? How much local disk may the cache consume? Which Windows/Linux versions
  and CPU architectures need support?
- **Before Windows packaging:** Is a drive letter essential, or is an Explorer
  sync folder sufficient? Is the eventual app private/personal, open source,
  internally distributed, or commercially distributed?
- **Before managed native acceptance:** Which desktop applications and save/replace
  workflows must be supported? Managed storage is selected; independent S3 edits
  to internal generation objects are outside its coordination protocol.
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
