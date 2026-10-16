# S3Drive handoff

Updated: 2026-10-16. P3 read-only mounts have passed native acceptance on Linux
and Windows. Scoped live Wasabi reads also passed. Goal active; cache, recovery,
safe writes and packaging remain future phases.

## Goal and scope

Enable S3-compatible storage to appear as local drives/filesystems on Linux and
Windows using Rust. Research, seven-crate workspace, mdBook and private GitHub
repository `marshallr12/s3drive` are complete. The first usable read-only
milestone is verified. Do not call the broader editable-drive goal complete
until the documented safe-write contract and its OS acceptance gates pass.

## Current implementation

- Shared bounded namespace engine; one directory per configured source alias.
- Linux FUSE and Windows WinFsp foreground mounts use the same S3 reader.
  Owner-only/read-only permissions, revision-pinned file handles, bounded
  directory snapshots, explicit collisions and cancellation paths are enforced.
- S3 exposes LIST/HEAD/range GET only. It validates ranges, lengths and revisions,
  bounds deadlines and uses explicit endpoint/region/profile precedence.
- Windows uses native UTF-16 case comparison and owner SID security descriptors.
  Invalid/case-colliding names fail instead of being silently renamed/hidden.
  The GPL-3.0 WinFsp wrapper is used for private prototyping; future application
  distribution terms and packaging remain undecided.
- `config-check`, `resolve`, `list`, `stat`, `read`, `init-state`, `status` and
  both native `mount` paths work. Windows requires WinFsp 2.1 Developer files
  for a source build and the installed runtime/driver for mounting.
- SQLite schema 1 is initialized but has no live cache entries. Before cache
  adoption, add persistent authenticated source identity and migrate the P0
  reserved table. A configuration fingerprint is not an account identity.
- No uploads, mutations, disk cache, offline support or desktop packaging yet.

## User configuration and live Wasabi evidence

User-selected Wasabi bucket `s3drivetest`, region `us-east-2` (N. Virginia),
endpoint `https://s3.us-east-2.wasabisys.com`, alias `wasabi`. Ignored local
`s3drive.local.toml` passes validation. The user corrected credential field
names to `aws_access_key_id` / `aws_secret_access_key` in local profile
`wasabi-s3drive`. Key values were neither displayed nor modified.

The user explicitly approved creating/deleting three fixtures under
`s3drive-conformance/20261016-lxrfkg8p/`. SDK pagination, HEAD, exact full/range/
EOF reads and Linux FUSE sequential/concurrent reads passed. Fixtures covered
524288 binary bytes, an empty file, and `nested/a%2Fb + café.txt`.
All three were removed, the isolated prefix was confirmed empty, and the
mount exited cleanly. Bucket versioning is disabled. Stale validators,
versioned reads, Windows against Wasabi and all write/recovery semantics remain
unverified. Do not infer broader mutation authorization from this limited test.

Live testing found Wasabi listing spaces encoded as `+`. Explicit
`list_url_encoding = "form"` was added to the ignored local config and Wasabi
example. Default percent-only decoding remains unchanged. Dialect participates
in configuration fingerprint v2; wire tests cover exact decoding/containment.
See `docs/src/wasabi.md` and `docs/src/evidence/wasabi-2026-10-16.json` for hashes,
source scope, test manifest and exclusions. Private ephemeral logs/fixture
files are under `/tmp/s3drive-wasabi-fixtures-lxrfkg8p/`.

## Native acceptance

The shared HTTP fixture uses ephemeral ports and isolated dummy credentials.
It checks signing-header shape, not an independent SigV4 implementation.
Both native gates exercise two buckets, configured prefixes, 310-entry
pagination, sequential/random/concurrent reads, EOF, stale handles/fresh reopen,
rejected writes, and teardown during an in-flight GET. Windows additionally
checks case/device-name rejection, sharing, owner ACLs, PowerShell, Explorer's
Shell.Application namespace and the upstream read-only `getvolinfo_test`.

At the user's request, both gates include a shared Unicode corpus in
`crates/s3drive-cli/tests/support/unicode.rs`: 11 filenames and a Unicode
folder, covering Japanese, Chinese, Cyrillic, Arabic, Devanagari, supplementary
emoji, a zero-width joiner sequence and literal percent/space/plus characters.
NFC/NFD accent spellings remain separate keys with distinct bytes. The gates
verify exact listings, directory traversal and correct contents for every key.

Local Linux gate passed with 145 GET/HEAD requests/two mounts on Linux
6.12.29-amd64, fusermount3 3.14.0 and Rust 1.99.1. Logs:
`/tmp/s3drive-unicode-linux.log`, `/tmp/s3drive-unicode-gate.log`.
No local mount remains. Commands:

```bash
bash scripts/gate.sh
cargo test -p s3drive-cli --test linux_mount --locked -- --ignored --nocapture
```

Windows native acceptance passed at commit
`25235a72e83d75389f0f95d53ff998b4bab6c80c`, CI run
[38928960490](https://github.com/marshallr12/s3drive/actions/runs/38928960490).
The Windows command is:

```powershell
cargo test -p s3drive-cli --test windows_mount --locked -- --ignored --nocapture
```

CI installs digest-pinned WinFsp 2.1.25156/runtime/SDK and upstream test binary.
The native test requires `S3DRIVE_WINFSP_TEST_EXE`; the installed WinFsp `bin`
directory must be on PATH for the test executable's direct DLL import. CI
checks its startup before mounting. PowerShell isolates its module path from
PowerShell 7. Both normal stdin-EOF shutdown and cancellation remove the drive.
Actual console Ctrl+C/Break events and a visually inspected Explorer window
are not covered; Shell COM access is. Read `docs/src/windows-mount.md` for limits.

Earlier failed CI runs: 38924822475 (ambiguous WinFsp guard type), 38925797029
(PowerShell module path), 38927733040 (upstream test DLL loading). Each defect
was fixed and the complete native gate rerun; none is acceptance evidence.

## Publication and next work

P3 was developed on `windows-read-only` from P2 commit
`2c0830d6102f4f9b54507f8f15a2b359be21e805`. The source checkpoint above has
native acceptance evidence; status/documentation updates also passed the local
gate (`/tmp/s3drive-p3-checkpoint-gate.log`). Publication to main requires the
exact branch tip's terminal CI success, then a fast-forward. Check current Git
and Actions state when resuming; preserve ignored user configuration.

Next is P4: design authenticated cache identity, SQLite migration, revision-
bound clean data cache, exclusive process ownership, eviction/pinning and
restart/disk-full/offline-miss tests. Keep cache/state outside mounted trees.
Writes require separate staged generations and a durable journal (P5a),
provider conditional commits/recovery (P5b), then desktop mutations (P5c).
Do not expose unconditional overwrite as a shortcut. Broader live provider
probes must use explicitly scoped disposable fixtures.

Read the mdBook implementation plan, provider/OS contracts and decisions.
No subagents were dispatched. The overall goal is not complete.
