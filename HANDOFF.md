# S3Drive handoff

Updated: 2026-10-16. Current phase: P2 Linux read-only mount implemented and
locally verified. P1 live-provider conformance remains pending. Goal active.

## Goal and scope

Enable S3-compatible storage to appear as local drives/filesystems on Linux and
Windows using Rust. Research, workspace, mdBook and private GitHub repository
`marshallr12/s3drive` are complete. Continue toward real usable mounts on both
platforms, then the documented safe-write contract; do not call the goal done
at a partial phase checkpoint.

## Current implementation

- Seven crates: configuration, read-only S3 transport, shared filesystem engine,
  local SQLite state, Linux FUSE, Windows stub, CLI.
- Linux `mount CONFIG MOUNTPOINT` is implemented. It serves per-source aliases
  through FUSE with owner-only read-only permissions, stable inode IDs, bounded
  directory snapshots and revision-pinned direct-I/O file handles.
- SDK LIST/HEAD/range GET are the only remote operations exposed. It validates
  exact ranges, lengths and revisions, sanitizes errors, bounds deadlines and
  uses explicit source endpoint/region/profile precedence.
- Root/alias configuration and `list`, `stat`, `read`, `config-check`, `resolve`,
  `init-state`, `status` work. Windows `mount` remains explicitly unsupported.
- SQLite schema 1 is initialized but not used for live cache entries. Add a
  persistent authenticated source identity and migration before cache adoption;
  a configuration fingerprint alone is not an account identity.
- No uploads, mutations, cache, offline support or packaging yet. Do not expose
  unconditional writes as a shortcut to provider conformance.

## Current user input

The user configured **Wasabi**, test bucket `s3drivetest`, region `us-east-2`
(N. Virginia), endpoint `https://s3.us-east-2.wasabisys.com`, alias `wasabi`.
Ignored `s3drive.local.toml` now exists and passes validation. The user corrected
credential field names to `aws_access_key_id` / `aws_secret_access_key` in the
`wasabi-s3drive` local profile. The values were neither displayed nor modified.

On 2026-10-16 the user explicitly approved creating/deleting three fixtures
under `s3drive-conformance/20261016-lxrfkg8p/`. The live SDK passed pagination,
HEAD, full/range/EOF reads and exact byte comparisons. A real Linux FUSE mount
passed sequential/concurrent reads of the binary, empty and Unicode/literal
name objects, then cleanly unmounted. All three objects were removed; the
isolated prefix is empty. Bucket versioning is disabled. Stale validators,
versioned reads and Windows against Wasabi remain unverified.

The live test found that Wasabi encodes listing spaces as `+`. Added explicit
`list_url_encoding = "form"` to the ignored local config and Wasabi template;
default percent-only decoding is unchanged for other sources. The dialect is
part of configuration fingerprint v2. A wire regression covers spaces, pluses,
literal percent sequences, directories and containment. See the durable
`docs/src/evidence/wasabi-2026-10-16.json` and `docs/src/wasabi.md`.

## P3 in progress

Windows adapter/harness are on `windows-read-only`; main remains the P2
checkpoint. First CI found an ambiguous host guard type, fixed explicitly.
Second CI passed build/Clippy/portable tests and actual Windows filesystem
reads, sharing, mutation refusal and name/revision checks; the later PowerShell
ACL step failed loading its module, not a filesystem assertion. Fix the test's
PowerShell 5/7 environment isolation and rerun the full native gate, including
Shell namespace, upstream volume info, removal and cancellation. Do not claim
P3 complete until all pass. Runs: 38924822475, 38925797029 failed. Third run
38927733040 passed PowerShell/ACL/Shell checks, then the upstream executable
failed before emitting output. Inspection of the verified release binary found
a direct `winfsp-x64.dll` import; CI now adds the installed runtime to PATH
and checks executable startup before mounting. Failure diagnostics include exit
status. Inspect Actions for the next exact commit. CI caches even on failure.

The user requested broader Unicode filename testing. Added a shared corpus of
11 filenames and a Unicode directory (CJK, Cyrillic, Arabic, Devanagari, emoji,
ZWJ sequence, distinct NFC/NFD accents, literal percent/space/plus). Exact
listing preservation and per-name bytes passed through real Linux FUSE on
2026-10-16 (145 requests/two mounts); Windows runs the same corpus next.
Local log: `/tmp/s3drive-unicode-linux.log`.

Linux and Windows mount tests share `tests/support/fixture.rs`. The refactored
Linux native test passed with 62 requests/two mounts. The full local gate after
the Wasabi fix passed (30 portable tests on Linux); log
`/tmp/s3drive-p3-wasabi-gate.log`. No mounts or fixture objects remain. Local
private fixture/log files are in `/tmp/s3drive-wasabi-fixtures-lxrfkg8p/`.

## Verification

Local native FUSE gate passed on 2026-10-16, Linux 6.12.29-amd64 x86_64,
fusermount3 3.14.0, Rust 1.99.1, fuser 0.18.0:

```bash
cargo test -p s3drive-cli --test linux_mount --locked -- --ignored --nocapture
```

It made 62 GET/HEAD requests to a loopback fixture using dummy credentials:
two buckets, configured prefix, nested/empty/Unicode/literal-percent keys,
310-entry pagination, sequential/random/concurrent reads, EOF, read-only
failures, stale open handles, fresh reopen and ambiguous directory errors.
External unmount and remount/SIGTERM during a stalled GET both exited cleanly;
the underlying temporary directory was empty afterward. No FUSE mount/process
was left behind. Log: `/tmp/s3drive-p2-mount.log` (ephemeral).

An initial shutdown test found retryable EINTR caused caller retry/liveness
problems. Cancellation now returns ECANCELED and the native regression passes.
The fixture proves kernel/HTTP behavior, not independent SigV4 verification or
Wasabi compatibility. Idle open descriptors can delay detached-mount drainage;
close them to complete graceful shutdown.

Portable tests total 28 plus one intentionally ignored native test on Linux.
`bash scripts/gate.sh` passed format, Clippy, tests/build and mdBook; the P2 log
is `/tmp/s3drive-p2-gate.log`. CI includes native Linux/Windows Rust jobs, book,
and an explicit Linux real-mount step; inspect the pushed commit's terminal
[Actions result](https://github.com/marshallr12/s3drive/actions).

## Next work

1. Reuse the shared `s3drive-fs` engine for a Windows WinFsp adapter. Inspect the
   exact resolved APIs and licensing first. Prove an actual native Windows
   mount/read/unmount, not just a cross-compile; an isolated Windows CI runner
   with the driver is a possible acceptance host.
2. When Wasabi test configuration is supplied, run the documented read-only
   conformance checks, record endpoint/region/versioning and exact source scope.
   Never mutate the user's ordinary objects or probe writes during mount startup.
3. Add cache identity/migrations and recovery only as separate green phases.
   Windows naming/case/sharing rules need native treatment; Linux's current
   case-sensitive name policy is not Windows acceptance evidence.

Read [plan](docs/src/implementation-plan.md), [Linux contract](docs/src/linux-mount.md),
[provider contract](docs/src/provider-contract.md), and [decisions](docs/src/decisions.md).
The native test is intentionally separate from the portable gate. Use normal
unmount first, and do not delete a directory that still contains a live mount.
No subagents were dispatched. The overall goal is not complete.
