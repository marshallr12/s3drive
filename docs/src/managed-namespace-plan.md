# Managed namespace and application saves

Implementation status and remaining plan, **2026-10-17**. P6m3b2 existing-file editing is
published at `683ec51a405198bfedc64f3a3b1c5eed7e7ed218`, with terminal
[Linux/Windows/book CI](https://github.com/marshallr12/s3drive/actions/runs/39010367651).
Creation passed both native steps at the [recorded checkpoint](development.md#creation-native-checkpoint); terminal exact-tip CI is required for publication.
The remaining namespace/save work is required before calling this an ordinary
editable desktop drive.

![Native create persists an exact local intent before an atomic authority transaction; empty-file placement precedes its first S3 payload.](images/managed-namespace.png)

## Delivery and acceptance

1. **P6m3b3a: creation and directories — native acceptance passed.** New, explicitly selected portable
   volumes; version-5 managed mount configuration; authority entry CAS; durable
   client namespace intents and exact-request recovery; real FUSE/WinFsp file
   and directory creation. Existing volumes/configurations retain their policy.
   Acceptance includes Unicode, two clients racing for a name, policy bounds,
   lost metadata replies/process death, no incidental S3 PUT/DELETE for empty
   metadata, nested file write/reopen, collisions, and both native drivers.
2. **P6m3b3b: rename, replacement and removal.** Guard source and destination
   node revisions/identities, parent revisions and expected target absence.
   Commit replacement plus target tombstone atomically. Flush pending source
   bytes before exposing a replacement. Retain displaced/removed objects and
   dirty local generations. Open handles keep their node identity and bytes;
   detached publication must update only a tombstone, never resurrect a path.
   Namespace effects and local acknowledgment need an atomic recovery bridge.
3. **P6m3b3c: real save workflows and recovery.** Exercise application-style
   temporary-file/write/fsync/replace saves on both OSes, directory moves,
   open-unlinked readers/writers, remote races, uncertain metadata outcomes,
   crash recovery and retained conflicts. Do not enable a callback before its
   acceptance passes. Reference-aware garbage collection remains disabled.

This split establishes actual native creation first, then uses the same intent
and authority protocol for full saves; it does not redefine the editable-drive
objective around creation alone.

## Portable collision policy proposal

New `portable-collision-v1` volumes retain the exact UTF-8 display name but reject
unaddressable Windows components and competing names with the same collision
key. The key uses Unicode-17 NFKC plus locale-independent lower/upper closure;
it is a deliberately conservative project collision rule, **not** Windows ordinal
comparison or the Unicode standard's NFKC_Casefold algorithm. Linux lookup remains
exact; Windows lookup uses its native case comparison. Rejecting a broader set
of aliases prevents ambiguity without rewriting filenames.

Pin normalization data and Rust character tables to Unicode 17. The newest
normalization crate inspected locally (0.1.26) uses Unicode 18, while Rust 1.99.1
uses Unicode 17; use inspected 0.1.25 and fail compilation on table-version drift.
Audit the collision rule against every BMP upcase mapping on native Windows CI;
reject the policy rather than assume cross-platform equivalence. Windows 10/11
workstation acceptance remains separate from Server 2025 evidence.

Windows reserved characters, trailing dots/spaces and device names require
validation, including the documented superscript COM/LPT digit aliases.
[Microsoft filename rules](https://learn.microsoft.com/windows/win32/fileio/naming-a-file)
Unicode compatibility normalization can merge distinctions; it is used only for
collision admission, never to rename existing display strings.
[Unicode 17 implementation guidance](https://www.unicode.org/versions/Unicode17.0.0/core-spec/chapter-5/)

## Namespace intent protocol proposal

Persist the complete request, volume, default-store identity and frozen local
credential/configuration binding before network. A request carries an entry ID,
parent revision, collision-checked name and authoritative file-store placement, so unrelated volume changes need not
conflict. The authority commits its effect and exact dedup outcome together.
Record the matching outcome locally; unknown transport/commit results remain
pending and replay only the same request. Bound and checksum retained rows.

Metadata creation is distinct from uploading content: an empty file can be
represented durably at the authority before its first immutable payload. Its store descriptor is recorded at creation; other clients must follow it regardless of their own default store. Reservations for a different store are refused. Legacy unplaced file creation is refused on a portable volume. Policies
still gate admission; version 5 explicitly opts into namespace changes. No S3
DELETE, unconditional overwrite, layout conversion, live Wasabi mutation or
Turso deployment is part of this phase.

WinFsp deletion must follow its cleanup lifecycle; its resolved Rust trait says
`set_delete` records intent and must not delete immediately. That integration
belongs to the replacement/removal phase, not an invented POSIX-style callback.
[WinFsp API header](https://github.com/winfsp/winfsp/blob/master/inc/winfsp/winfsp.h)


## Current creation contract

Use `authority-serve --name-policy portable-collision-v1` for a **new** private
authority and retain that flag when restarting it. Selecting another policy for
an initialized volume fails; no existing names are silently converted. Version-5
managed mount configuration requires that policy and `namespace-create-v1`.
Versions 3/4 retain their existing read/edit behavior. The
[configuration template](../../examples/managed-namespace.toml) is a template,
not a live write policy or a conversion command.

Schema 12 preserves schema-11 rows, adds collision keys and optional authoritative
placement to nodes, and stores private client namespace intents. Legacy node
serialization omits absent placement, preserving existing local receipt hashes.
Checksums cover complete client inputs/outcomes; corrupt sibling keys prevent a
new create from bypassing collision rules. Parent/name admission and authority
idempotency commit together. A receipt failure/full disk leaves the local intent
pending. Mount recovery replays the same request before serving callbacks.
Removing the namespace opt-in does not authorize forgetting an unresolved intent.

The portable test uses the actual CLI authority and SDK fixture, creates nested
Unicode entries, writes/reopens content, verifies collision refusal, and kills a
client after a forwarded create has committed while the proxy withholds its
reply. Restart must adopt the original result without another volume-sequence
increment or S3 PUT. A second client with another default store must publish into
the file's authoritative store. Authority restart must preserve the namespace.
Native acceptance is the separate sixth case in each ignored suite, including
real exclusive creation, nested directories, normal remount, a new file's dirty
restart and a killed mount during that lost metadata reply. Run native suites
serially as documented; portable tests do not certify either driver.

Initial limits remain 8 MiB per file, four retained file snapshots, bounded
node/intent counts and no garbage collection. New opens authorize online;
unpublished empty nodes use authority authorization plus the selected publisher's
policy because there is no object yet. Creation can commit before an open-handle
allocation or subsequent read fails; retained intent IDs resolve that effect.
No native rename/delete, symlinks, ACL mutation or desktop replacement-save
acceptance is implied by creation alone.
