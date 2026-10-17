# Native Windows lifecycle bridge

Design date: 2026-10-17. **Under implementation; not accepted or enabled on
published main.** Baseline `971c6fb` passed terminal CI39066548525 with schema22,
config6. Its complete proof is `/tmp/s3drive-owned-target-publication.json` on the
development workstation. Repository visibility was restored to private before
that publication; continue checking it before and after future publication.

Version7 selects `managed.windows-disposition = "posix-v1"` explicitly. Versions
1–6 keep their existing unsupported native deletion paths. This selects the
[measured WinFsp POSIX profile](windows-replacement-reference.md), including its
documented differences from NTFS. Do not claim NTFS parity. Require full native
DoC, SetDelete, clear/reassert, duplicate/multiple file objects, directory,
early-cleanup, late-I/O, rename/replacement and killed-owner acceptance before
publishing this profile.

One durable open ID represents a WinFsp file context, not each duplicate Windows
handle. Root and read-only objects do not acquire a writable disposition record.
Existing-file DoC freezes exact PrepareRemoval admission; create+DoC freezes
CreateDeferredEntry atomically. Registration/activation and planned return values
never prove delivery to the kernel. Preserve uncertain admissions until exact
reconciliation or separate exclusive-owner abandonment; invent no Cleanup/Close.

Callbacks refer to stable node/open IDs. Cleanup flags and Close observations
must enter the local SQLite journal before network reconciliation. Namespace
changes, disposition changes and writes share publication-then-edit locking;
controller locks follow those locks. Raw callback capture precedes that locking.
Cleanup/Close cannot return status: failures retain their exact journal work and
dirty data for recovery. No best-effort unconditional overwrite is permitted.

POSIX deleting cleanup may unlink while another file object remains active.
Retain old-target snapshots and detached late writes through Close. The final
non-deleting cleanup may release an owned intent; a queued deleting cleanup has
priority. Renamed file objects follow current authoritative identity, not the
original callback pathname. Directory emptiness and foreign ownership conflicts
remain authoritative. Normal listings include pending children, while ordinary
Node access rejects them; replacement inspects the exact owned reservation rather
than treating a failed normal lookup as absence.

A multi-store mount can have different physical/credential controls for source
and destination. Freeze the destination control separately when it differs;
validate it against the original target ownership proof and current mount
publisher. Metadata replacement does not move or rebind either payload. Schema23
adds optional control/profile proof fields without rewriting older hashes.

Native acceptance runs on disposable Windows Server2025 and Linux runners.
Portable/controller tests are additional evidence, never substitutes. The local
MX workstation has NoNewPrivs=1; do not bypass it. No additional Wasabi mutations,
real startup installation, credential changes or Turso provisioning are implied.


## Native test boundary

The test-only WinFsp feature exposes safe wrappers for disposition and exact
directory creation requests. Unsafe calls remain in the audited native module.
The directory probe uses NtCreateFile with synchronous directory/Create/DoC
options, DELETE access and initialized NT structures; it first runs the same
owned request on local NTFS to validate the oracle. This is based on Microsoft's
[NtCreateFile contract](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile),
read2026-10-17. Actual target behavior remains unverified until the native run.


## Portable checkpoint

The local repository gate passed with 255 state tests, ten HTTP and seventeen
SDK cases, plus the new shared-filesystem lifecycle engine, lint, build and book
checks. The engine proves early cleanup with another object, retained late I/O,
non-deleting cleanup, two-store replacement and renamed nonempty directories.
Paired-control/profile/migration checks pass; older serialized fields stay absent
by default. An initial unboxed dispatch was corrected. A later reproducer traced a
remaining overflow to nested S3 HEAD state during reopen; generated HEAD/GET
send futures now stay on the heap inside their deadlines. The original test
passes without diagnostic instrumentation or larger stack limits.

The new Windows-only suite remains unverified here. It includes an actual NTFS
directory-create control, atomic DoC creation, duplicates/multiple objects,
regular cancellation, explicit extended-cancellation refusal, POSIX early cleanup, four replacement
modes, cross-store retained I/O and native lost-response/killed-owner recovery.
It must compile and pass on the actual runner along with the frozen reference
matrices and existing startup/mount gates before config7 is published to main.


Candidate `6199af5` (CI39069635074) passed Linux/book but stopped Windows Clippy
on misplaced Open/Create context initialization and typed flag composition.
Those compile errors are corrected locally; no native runtime success is inferred.
A valid narrowed policy also exposed a missing metadata-only DoC creation guard:
the public and worker paths now require the selected store's write permission,
and the loopback test proves both file/directory denial without PUT or namespace
admission. Windows-only native acceptance now runs in a dedicated early step;
the later native suite excludes that one case to avoid repeating it.


The corrected local gate passed exit0 (`/tmp/s3drive-lifecycle-gate6.log`): 255
state tests, ten HTTP, seventeen SDK, lifecycle/policy and original mutation
regression, formatting, Clippy, builds and book. No stack limits changed. Native
Windows compilation/runtime and exact-tip hosted acceptance remain required.

## Observed kernel limit

Research and native observation2026-10-17: candidate366828a reached the actual
WinFsp mount. Atomic file/directory DoC, duplicate lifetime and ordinary
SetDelete clear/reassert passed before extended ON_CLOSE cancellation returned
HRESULT0x80070032 (not supported). The pinned
[WinFsp2.1 disposition implementation](https://github.com/winfsp/winfsp/blob/v2.1/src/sys/fileinfo.c#L1493-L1495)
rejects this flag in the kernel before posting a userspace callback. A Rust
SetDelete implementation cannot implement that rejected request. The profile
therefore explicitly excludes extended ON_CLOSE cancellation, unlike the frozen
NTFS reference. Acceptance must assert the exact refusal, retained bytes and
unchanged DoC effect at Close. Ordinary SetDelete cancellation remains required;
no silent conversion of the rejected extended request is permitted.

Candidate10899f2 exposed a separate native open failure: an authorized snapshot
could be older than a concurrent local disposition effect. A loopback regression
pauses the authority's Node reply during a new open and races SetDelete and
actual non-deleting Cleanup. It reproduced the missing serialization before the
fix. Read authorization now acquires publication/edit locks before Node/HEAD/GET,
and retains them through logical adoption. Callback facts are still captured
before those locks; their effects wait for the authorized open. The monotonic
observation guard remains unchanged. Bounded online reads now serialize with
local mutations; remote deadlines still apply. Native reacceptance is required.
The complete local gate passed after this change, including both barrier cases
and the existing mutation/reopen regressions
(`/tmp/s3drive-lifecycle-gate7.log`, exit0). No native acceptance is inferred.
