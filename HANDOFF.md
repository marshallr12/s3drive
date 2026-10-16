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

The user selected **Wasabi**, test bucket `s3drivetest`, region `us-east-2`
(N. Virginia). Correct endpoint: `https://s3.us-east-2.wasabisys.com`.
Recommended ignored config: `s3drive.local.toml`, with profile `wasabi-s3drive`
in `~/.aws/credentials`. User is preparing configuration; the file was absent
at this turn's start. No credentials were read and no live provider accessed.
Explained `[[sources]]`, named credential profiles and prefix mapping in chat.
Keep credentials out of chat, logs and Git. See [Wasabi plan](docs/src/wasabi.md).

## P3 in progress

Windows adapter and native acceptance harness are implemented on a working
branch for native CI verification. Do not claim P3 passed until the Windows
runner completes mount/read/unmount tests. `docs/src/windows-mount.md` records
APIs, limits and evidence boundaries. Linux and Windows mount tests now share
`crates/s3drive-cli/tests/support/fixture.rs`; rerun Linux native acceptance.
The previous main-branch results below do not certify these edits.

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
