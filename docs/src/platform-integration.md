# Linux and Windows integration

Linux FUSE and Windows WinFsp read-only adapters have passed their native gates. See the
[Windows contract](windows-mount.md). See [the Linux contract](linux-mount.md) for commands, limits and the
explicit native acceptance gate. [Opt-in existing-file editing](native-writes.md)
adds write/truncate/flush callbacks with separate native acceptance on both OSes.

## Linux: FUSE with fuser

The Linux-only `fuser` 0.18 dependency supplies lookup, getattr, directory,
open/read/release and flush callbacks; version 2 adds write and size changes. Inodes are retained under a
mount-wide cap instead of recycled on `forget`; directory handles keep stable
snapshots and offsets. Four FUSE threads run network futures on a shared Tokio
runtime. Signals cancel outstanding futures and request unmount. The exact
resolved source was inspected before implementing typed callback IDs and
session lifecycle APIs.
[fuser API](https://docs.rs/fuser/0.18.0/fuser/)

Use per-user mounts initially, with conservative `nodev`, `nosuid` and read-only
settings. Require an empty, owned mountpoint, access to `/dev/fuse`, and the
appropriate mount helper. Add a diagnostics command before service packaging.
Do not enable `allow_other` by default. Ensure the cache/state directory cannot
reside inside the mount, even through a symlink or bind-mount alias.

Test on a real Linux host with the driver available. Unit tests and a Docker
environment without `/dev/fuse` cannot demonstrate mounting. Exercise large
directories, canceled requests, inode lifetime, repeated mount/unmount and
process termination; check the resulting mount table and orphaned processes.

## Windows: WinFsp drive

WinFsp supplies the installed filesystem runtime/driver; the Rust process
implements its user-mode filesystem callbacks. The candidate wrapper exposes
`FileSystemHost` and `FileSystemContext`. It documents required delay-load
linking, and offers a `system` feature for installed libraries. Prototype on
`x86_64-pc-windows-msvc` with a matching WinFsp installation.
[WinFsp host API](https://docs.rs/winfsp/0.13.1+winfsp-2.1/winfsp/host/index.html),
[wrapper build instructions](https://github.com/SnowflakePowered/winfsp-rs)

Implement and test file information, directory enumeration, file contexts,
security descriptors, read offsets, sharing modes, cleanup/close ordering,
delete-pending behavior and case handling. Windows callbacks cannot be mapped
one-for-one to Unix assumptions. P5c1a adds overwrite/write/size/flush callbacks.
Rename replacement rules and byte-range lock policy remain subsequent work.

Separate driver installation from running the user agent. A Windows service's
session/account context may differ from the interactive desktop; verify that
the intended user sees the drive in Explorer. Start with a foreground process
in that user's session before considering a service or logon launcher. Detect
missing DLL/driver, occupied drive letter and unsupported runtime versions with
actionable diagnostics. Do not silently choose another drive letter.

Native acceptance requires Explorer enumeration, PowerShell reads, byte-exact
large-file reads, cleanup/unmount and the relevant WinFsp test suite subset on
a Windows machine. Linux cross-compilation is not Windows runtime evidence.
The wrapper's own test results are not results for S3Drive.

## Alternative: Cloud Files API

Microsoft's Cloud Files APIs support placeholder creation, hydration,
dehydration, pinning, sync-root registration and Explorer status. This is well
suited to an on-demand **folder within an existing local volume**. It still
requires our S3 synchronization and conflict engine; registering a provider
does not implement upload or coherence.
[Build a cloud sync engine](https://learn.microsoft.com/en-us/windows/win32/cfapi/build-a-cloud-file-sync-engine)

Choose this route if Explorer integration and offline pinning matter more than
a dedicated virtual volume. Use generated `windows` bindings with only the
required API features; verify COM/WinRT registration, callback lifetime and
the supported OS/filesystem prerequisites in a small native prototype.
Do not build both Windows surfaces before user requirements justify the cost.

ProjFS provides projected files but Microsoft notes that it targets fast
backing stores and lacks cloud-specific progress/offline presentation. This
makes it a weaker initial match for internet object storage.
[ProjFS guidance](https://learn.microsoft.com/en-us/windows/win32/projfs/projected-file-system)

## Distribution dependency decision

### Linux runtime baseline (observed 2026-10-18; packaging proposal)

The development environment reports Debian12 and glibc2.36. The rebuilt e51f737
debug executable runs `--version`; ELF inspection reports a highest required
glibc symbol version of2.34. See the [source/binary-bound observation](evidence/linux-runtime-baseline-20261018.json).
This is a local executable check, not a packaged-release or actual MX reboot
acceptance result. The user's MX/no-systemd requirement remains unchanged.

Proposed release gate: build and execute each Linux artifact against a baseline
compatible with Debian12/glibc2.36, record its actual loader/library/symbol
requirements, and retain the separate native FUSE and no-systemd session gates.
Validate the eventual artifact on the target workstation before claiming MX
support. Rust's [platform support table](https://doc.rust-lang.org/rustc/platform-support.html)
(read2026-10-18) describes toolchain support; treating the target triple alone
as proof for the complete application would exceed that evidence.

### Windows distribution terms

WinFsp is GPLv3 with a stated FLOSS linking/distribution exception and commercial
licensing options. The `winfsp-rs` repository has its own GPLv3 license. Review
the exact binding and runtime terms separately when selecting the application's
distribution model; do not assume the runtime exception applies to the wrapper.
[WinFsp license](https://github.com/winfsp/winfsp/blob/master/License.txt),
[winfsp-rs license](https://github.com/SnowflakePowered/winfsp-rs/blob/master/LICENSE.md)

The current Linux adapter links fuser (MIT). The Windows prototype links the GPL-3.0 winfsp wrapper;
no project distribution license has been selected. This
does not block private research. Before distributing a working Windows build,
resolve whether to use this wrapper under compatible terms, another reviewed
binding arrangement, or an alternative adapter. Also account for installer
signing, driver/runtime deployment, notices and update rollback.
