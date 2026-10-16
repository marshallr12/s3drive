# Windows read-only drive

Implementation checkpoint, researched 2026-10-16. The adapter is present on the
Windows implementation branch; **native acceptance is pending**. Do not infer
Windows runtime support from Linux builds or portable tests.

## Run

Use Windows x64 with Rust's MSVC toolchain, Visual Studio C++ tools and WinFsp
2.1 (2025), including its Developer files. Install the driver separately, then
run the foreground process in the desktop user's session:

```powershell
cargo run --locked -- mount s3drive.local.toml S:
```

Choose an unused drive letter. Ctrl+C or Ctrl+Break unmounts and stops the
process. Supervisors may pass `--unmount-on-stdin-close` and keep stdin open;
closing the pipe requests the same shutdown. Directory mountpoints, services,
automatic drive selection and packaged installers are not implemented.

The prototype links `winfsp` 0.13.1 and `windows` 0.61.3 only on Windows. Both
the adapter's and executable's build scripts emit the required delay-load
flags. This binding is GPL-3.0; no application distribution license is chosen.
Private implementation does not resolve future distribution terms.
[Binding build instructions and license](https://github.com/SnowflakePowered/winfsp-rs)

## Implemented behavior

Source aliases become directories at the volume root. The shared engine still
uses exact S3 keys; Windows translates case-insensitive requests back to the
original spelling after validating the entire parent listing. It uses native
NT single-UTF-16-unit uppercase mapping, including non-ASCII names, instead of
Rust's potentially expanding Unicode uppercase conversion.
[Microsoft mapping API](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-rtlupcaseunicodechar)

Reserved device names (including extensions and superscript digits), colons,
Windows forbidden characters, trailing periods/spaces and case-equivalent
siblings fail the directory operation. They are not renamed or silently hidden.
The shared 255-byte component limit also applies. Literal percent escapes stay
literal. Alternate streams, reparse points and short-name aliases are absent.
[Windows naming rules](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file)

Each file context pins the HEAD revision for subsequent ranged reads. Directory
contexts retain bounded snapshots with stable resume markers. The driver
receives read-only attributes and a read-only volume; callbacks also refuse
write/delete access. A protected DACL grants read/traverse to the mounting
account's SID. The process does not impersonate callers or share S3 credentials.
Windows enforces sharing modes through its native open handles.

Four synchronous WinFsp workers run I/O futures on the shared Tokio runtime.
All callback I/O has a 60-second deadline and cancellation path. Limits are
256 open contexts and 100,000 retained directory entries per mount, in addition
to shared namespace limits. Directory metadata currently requires HEAD per
file, so large remote listings can be slow and may reach the deadline. No disk
cache, uploads, offline operation or capacity estimation exists.

Unsafe Rust is confined to `s3drive-winfsp/src/native.rs`: process-token/SID
querying, construction/copying of a self-relative security descriptor, native
uppercase mapping and matching Windows allocation cleanup. Other modules
forbid unsafe code. The native binding itself remains a separate FFI dependency.

## Native acceptance

```powershell
cargo test -p s3drive-cli --test windows_mount --locked -- --ignored --nocapture
```

This must run with the actual installed driver. Set
`S3DRIVE_WINFSP_TEST_EXE` to the x64 executable from the official
`winfsp-tests-2.1.25156.zip` release; CI downloads and verifies it. The installed
WinFsp `bin` directory must be on `PATH` for this executable's direct DLL
imports; CI checks startup before mounting. It mounts two fixture buckets
using dummy credentials, verifies names, pagination, byte-exact sequential,
random and concurrent reads, EOF, revision changes, sharing, rejected mutations,
owner ACLs, PowerShell and Explorer's Shell.Application namespace. It also runs
the upstream `getvolinfo_test` subset against this drive; mutable upstream tests
are not appropriate to the current read-only contract. It rejects
an occupied letter, then removes the drive, remounts and cancels a stalled GET.
The shared Unicode corpus checks 11 filenames and a Unicode directory,
including CJK, Cyrillic, Arabic, Devanagari, surrogate-pair emoji, a zero-width
joiner sequence and distinct composed/decomposed accent spellings. Listings
must preserve exact names, and every file must return its own expected bytes.
Non-ASCII case-equivalent sibling names are separately rejected on Windows.
It does not prove a visually inspected interactive Explorer window, all Windows
applications, general NTFS conformance, provider credentials or Wasabi behavior.

CI installs a fixed official MSI after verifying its published SHA-256. Missing
driver, install failure or required reboot fails the gate. It never substitutes
a fake mount. Record the exact successful run before marking P3 verified.
[WinFsp 2.1 installer and published digest](https://github.com/winfsp/winfsp/releases/tag/v2.1)
