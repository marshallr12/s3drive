# Implemented Linux read-only mount

The P2 implementation adds `s3drive-fs`, a platform-neutral namespace and file
handle engine, and a Linux-only `fuser` 0.18 adapter. It exposes one directory
per source alias. No object data is persisted locally and no S3 mutation API is
exposed. SQLite remains reserved for the later cache/recovery phase.

## Run

Create a valid configuration and an existing empty directory owned by the
mounting user. The host needs `/dev/fuse` and `fusermount3`; neither a root daemon
nor `allow_other` is needed. A symlink or nonempty mountpoint is rejected.

```bash
mkdir -p "$HOME/S3Drive"
cargo run --locked -- mount s3drive.local.toml "$HOME/S3Drive"
# In another terminal:
ls "$HOME/S3Drive"
cat "$HOME/S3Drive/wasabi/example.txt"
fusermount3 -u "$HOME/S3Drive"
```

The process stays in the foreground. Ctrl-C and SIGTERM request unmount and wait
for callback threads to exit. Outstanding network futures are canceled with
`ECANCELED`, not retryable `EINTR`. Close idle open files to let a detached
mount finish draining. Normal external unmount also ends the process.
SIGKILL cannot run cleanup; if a crashed daemon leaves a disconnected mount, use
`fusermount3 -u` on that mountpoint before restarting. Unmount can require closing
open files and leaving working directories inside the mount.

## Namespace and handles

The virtual root contains the configured aliases. Directory listings use S3's
prefix/delimiter scheme and consume all pages before publishing the directory.
Each open directory holds a stable, sorted vector and offset cookies for its
lifetime. Separate opens can see later changes. Pagination is not an atomic
snapshot of a changing bucket; repeated entries/tokens fail rather than silently
selecting a result. New lookups fetch a fresh directory listing, so this initial
implementation favors explicit freshness over request efficiency.

Linux names preserve case and UTF-8 bytes, including spaces, plus signs and
literal percent sequences. Empty, dot, parent, slash, backslash, control and
components exceeding 255 UTF-8 bytes are rejected. A zero-byte marker matching
the directory prefix is ignored; a nonempty marker is an error. File/prefix
collisions such as `a` plus `a/b` make that directory inaccessible with an error.
Nothing is silently renamed or hidden. The Windows adapter applies the
additional [native name/case policy](windows-mount.md).

Inodes are stable per source/key/type for the lifetime of the mount and are
never recycled. The engine retains these identities until unmount, with a cap;
it does not evict on kernel `forget`. This avoids stale references identifying
another object. Open files retain independent S3 revision-bound handles.

Each file open performs HEAD. Reads use its version/ETag preconditions and exact
range checks. A changed unversioned object yields Linux `ESTALE`; reopen to see
the new revision. A pinned version remains readable only if the provider still
serves it. Open-handle attributes use the pinned size; ordinary path attributes
are fresh HEAD results. Attribute/entry TTLs are zero. Files use FUSE direct I/O
to prevent the kernel page cache from mixing revisions from different opens.
Shared writable mappings and executable files are not supported.

## Limits and error behavior

| Limit | Current value / result |
| --- | --- |
| Directory entries | 10,000; larger directories return an error |
| Listing pages | 64 maximum; token cycles/repeated entries fail |
| Whole directory listing deadline | 60 seconds, including all pages |
| Mount-wide inode identities | 32,768, retained until unmount; remount to reset |
| Open file + directory handles | 256 total |
| Entries retained by open directory handles | 100,000 references total |
| Concurrent FUSE callbacks | Four worker threads using one Tokio runtime |
| One S3 read | At most 8 MiB; existing transport deadline applies |

Limit failures return `ENOSPC` or `EMFILE`, never a truncated successful listing.
Invalid projections return `EILSEQ`, collisions/invalid provider responses `EIO`,
permission failures `EACCES`, missing objects `ENOENT`, and timeouts `ETIMEDOUT`.
There is no special UI for diagnostics yet. S3 quota/free space is unknown;
`statfs` reports zero capacity rather than inventing available local disk space.

Mount options are read-only, owner-only, `nodev`, `nosuid`, `noexec`, `noatime`
and kernel default permission checks. Directories report mode 0500 and files
0400 for the mounting user's ownership. Permission metadata and timestamps are
synthetic except object modification times. The service has no write path;
read-only `flush`/`fsync` only validate the handle, not a pending upload.

## Verification

The explicit native gate is:

```bash
cargo test -p s3drive-cli --test linux_mount --locked -- --ignored --nocapture
```

It mounts a temporary directory against an isolated loopback S3 HTTP fixture
with dummy profile credentials, and exercises ordinary kernel filesystem calls.
It covers two bucket aliases, a configured prefix, nested/empty/Unicode names,
310-entry paginated directory enumeration, sequential/random/concurrent reads,
EOF, rejected mutations, an object replacement while open, ambiguity failures,
external unmount/process teardown, remount, and SIGTERM during a stalled GET. No real cloud account is used.

Portable workspace tests do not run this ignored test; a native gate must invoke
it explicitly. Linux CI also runs it after installing the FUSE helper.
Local native acceptance passed on 2026-10-16 on Linux 6.12.29-amd64 x86_64
with fusermount3 3.14.0: both mount sessions exited successfully, and the mount
directory was empty after each teardown. The expanded Unicode gate passed
with 145 GET/HEAD requests on 2026-10-16. It preserves exact directory entries
and contents for 11 filenames: NFC `café.txt` and NFD `cafe\u0301.txt` as
distinct keys, Japanese, Chinese, Cyrillic, Arabic, Devanagari, supplementary
emoji, an emoji sequence with a zero-width joiner, and literal percent/plus
characters. It also traverses `目录-📁/résumé-🎉.txt`. The shared corpus is
`crates/s3drive-cli/tests/support/unicode.rs`; both native gates use it.
Wasabi acceptance
remains separate: the fixture tests our HTTP/kernel contract, not Wasabi's
service, authentication implementation or concurrency semantics.
