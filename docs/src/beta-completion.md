# Beta completion status

## Current accepted baseline (2026-10-27)

Main `74e1540bd765fd8f2a61000d073e15780f455f31` independently passed
[run40270589324](https://github.com/marshallr12/s3drive/actions/runs/40270589324):
all nine complete job logs, 810 exact source files and six verified artifacts;
Linux 1143 / Windows 1135 workspace tests and all original/extended gates.
Windows shared/debug/release 300-save times were 684.33/1093.25/348.67 seconds;
the unchanged 1200-second debug limit retained 106.75 seconds of margin.
The lifecycle/recovery cleanup remains fresh-only opt-in `component-retention-v1`;
existing defaults/profiles and protection of dirty, uncertain and pinned work are
unchanged. Hosted Git CLI application I/O and relocated package/startup acceptance
are complete, not Office GUI, physical reboot or live Wasabi acceptance.

The owner selected GPL version 3 only on 2026-10-27, superseding the earlier
MIT choice for this revision. The new
[Windows MSI](windows-installer.md) and its additional CI gate have hosted candidate acceptance at `1593ac9`
([run40553716413](https://github.com/marshallr12/s3drive/actions/runs/40553716413));
all ten full logs,835 source blobs and six artifact digests/reports were reviewed.
The documentation successor and any merge still require exact-source verification;
no public binary release or real-machine acceptance is implied. Third-party notices remain
under their own terms. Earlier candidate/pending-main statements below are dated
historical checkpoints, superseded by this independent main result, not requests
to rerun completed phases. Real hardware/provider acceptance remains incomplete.

## Historical candidate acceptance (2026-10-25)

Exact source `59a10fe36b98d85d200bd24f7a51c79147e7a200`, tree
`7c8bab19508d6b0ff1ebf55e59fdec2c1892ec7d`, passed independent review of
[run40229890964](https://github.com/marshallr12/s3drive/actions/runs/40229890964): all nine complete job logs, 808 exact source files,
six artifact digests, all four schema46–50 compatibility reports and 35 typed
Windows reference observations. Linux1141/Windows1133 parent-harness
workspace tests passed. The two warm-cache schema30 regressions are corrected;
the original migration tests and all earlier acceptance gates remain unchanged.

Original shared/debug/release300 workloads passed in 610.45/758.85/160.83s on Linux and
707.23/1102.82/354.93s on Windows, with every remount segment and final cold read verified.
Windows debug300 retains 97.18s below its unchanged1200s limit;
this is limited headroom, not a hardware-independent performance guarantee.
Additional release1000-save/100-remount and four64KiB-file300-save workloads
passed on both OS. Each OS also passed four100-cycle component runs:800 combined
cycles,400 mounts and400 closed SQLite integrity/FK checks. Component permanent
row maxima were1, DB/WAL maxima778240/4161232 bytes and physical payload320 bytes.
Diagnostic instructions were21,363,286,243,16.46% below the accepted-main baseline.

Lifecycle and recovery cleanup are candidate-accepted for the **fresh opt-in
`component-retention-v1`** profile. Unrelated open/dirty/read-protected work,
uncertain outcomes and unfinished recovery remain protected; safe closed groups
are reclaimed without fabricating Close. Defaults and the existing linked/owned
profiles are unchanged. No remote object GC or alpha-store migration is added.

Previous accepted baseline main is `3fd2f05e6d833d45cc1bfb70eabe6aa754524402`
([run40157712514](https://github.com/marshallr12/s3drive/actions/runs/40157712514)).
Candidate evidence must not be substituted for the separate exact merge/main
verification. A documentation-only acceptance update does not change runtime,
tests or CI. The merge commit requires its own complete source/log/artifact review.

Real Wasabi headroom/provider acceptance, physical MiniAir driver/native-application
UI/startup installation/reboot and public release/licensing remain incomplete.
Private developer packaging and relocated CLI startup passed hosted Linux/Windows;
that is not installation or physical/provider acceptance. Private validation
archives, credentials and developer bundles remain unpublished.

## 1. Lifecycle cleanup: hosted code acceptance

The fresh component profile covers native creation, rename, replacement, unlink,
write/read through held handles, last Close, repeated cold re-admission and bounded
retention while unrelated dirty writers and cloned readers remain active. It
selects complete dependency groups and revalidates the surviving graph after
transactional deletion. Consumed open/session/namespace/creator/source identities
cannot be resurrected after eviction; permanent monotonic frontiers stay bounded.
Dirty, uncertain, incomplete or reader-pinned groups defer cleanup or retain
backpressure. Prospective atomic creator/open admission remains unsupported; the
ordinary native creation route is tested. This does not add remote object GC.

## 2. Recovery cleanup: hosted code acceptance

Observer-session identities remain protected while recovery records survive.
Finished no-effect groups and wholly settled recovered publishers can be reclaimed;
unfinished groups crossing selected nodes, pending dirty epochs, unknown remote
outcomes and live logical/physical readers cannot. Recovery abandonment never
stands in for native Close. Original retry and authority identities remain exact.
Process-kill cutpoints, deletion refusal, genuine SQLite page exhaustion, stale
identity rejection and cold reload tests passed without weakening durability,
foreign-key checks, corruption detection or rollback requirements.

## 3. Performance and longer/larger workloads: hosted acceptance

SQL compilation reuse is bounded to128 statements per State connection. It caches
bytecode, not rows, decoded graphs, validation outcomes or authority. Every query
binds and executes again; mutable PRAGMAs stay fresh. Prepare-only schema probes
and schema-presence/error-fallback detection remain uncached. Warm-cache negative
controls cover changed proof/source bytes, triggers, schema changes and rollback.
The diagnostic instruction reduction above is measurable; Windows wall-time
headroom remains limited and must be monitored on actual hardware.

Additional1000-save and four64KiB-file300-save workloads supplement rather than
replace the original six300-save gates. All cold bytes, exact PUT schedules and
4MiB DB/8MiB WAL/2MiB physical-payload ceilings passed. Original120-second native
segments,180-second child bounds and all original overall deadlines are unchanged.
The component stress preserves external read-only SQLite observers. The proven
Linux POSIX lock-lifetime defect is fixed by retaining verification descriptors
until SQLite closes, then closing them before releasing directory ownership.

Historical rejected sources remain in Git and private evidence: c98496f exposed
an intermittent malformed-database failure;0dab6bd fixed a deterministic SHM lock
hazard but failed Windows debug300's deadline;faa6b0b improved performance but
falsely refused two warmed old-schema fixtures.59a10fe corrects the last regression.
No failure was dismissed, no deadline was enlarged, and no uncertain data was
silently discarded. These diagnostics do not establish physical corruption of
the original failure database, whose raw DB/WAL was not retained.

## Hosted item4 candidate acceptance (2026-10-26; main proof separate)

The preceding code scope is now also independently verified on main
`f2410ec8b25609b34926b87b5474d7ad3671c647` in
[run40241620855](https://github.com/marshallr12/s3drive/actions/runs/40241620855).
The user requested further item4 tests on standard GitHub Actions, not MiniAir.

New test-only coverage runs genuine Git CLI subprocesses against the mounted
filesystem: Unicode document checkout, forced replacement, rename, deletion,
retained-reader checks, Windows exclusive sharing refusal, application reopen,
and exact cold reads after terminating the owned mount daemon. Git metadata lives
outside the drive, with no remote, credential helper, commit identity or network
operation. This is **CLI application I/O**, not Office/Notepad UI acceptance or a
claim that a complete Git repository can safely reside on the drive. Commands
follow official [checkout-index](https://git-scm.com/docs/git-checkout-index),
[mv](https://git-scm.com/docs/git-mv) and [rm](https://git-scm.com/docs/git-rm)
documentation reviewed2026-10-26. The loopback object fixture is not Wasabi.

Private packaging now compares two independently assembled archives of the same
built binary, records archive/binary/source digests, runs relocated config checks
and idempotent schema50 state initialization with integrity/FK validation, then
uses that relocated binary for actual mounts and the shipped startup/session
tests. Deterministic archive assembly is not reproducible compiler output across
machines. Existing Linux desktop/terminal-loss and Windows Task Scheduler separate
logon tests are rerun, not replaced by a synthetic supervisor mock. An owned
process interruption is not a physical reboot. Archives remain unpublished.
Exact candidate `34e42a41b28512a045e5e813cc8eaf071b363edd`, tree
`4555e7493fa7b6e6735fce8e259d9ef45e8222d0`, passed
[run40258999976](https://github.com/marshallr12/s3drive/actions/runs/40258999976):
all nine complete logs, 810 exact source files, six artifact digests/reports,
Linux1143/Windows1135 workspace tests and all original and extended gates.
Original shared/debug/release300 times were360.00/429.37/99.90s on Linux and
689.96/1096.64/341.19s on Windows; debug retains103.36s under unchanged1200s.
Both OSes passed400 component cycles/200 cold integrity checks, with all nine
permanent table maxima1. Long1000-save/large64KiB workloads were474.55/284.59s
on Linux and1245.61/610.74s on Windows. Product runtime/defaults are unchanged.
This documentation-only update preserves accepted runtime/tests/CI. The merge
commit still requires its own independent complete main verification.

Git2.55.0 on Linux and2.56.0.windows.2 performed the tested operations. The
provenance checker derives bucket/prefix composition from the actual fixture
configuration, verifies three creators/reservations/publications and exactly
nine canonical objects, including exact payloads and root/page hashes. Two pure
checker tests cover configured prefixes/buckets and missing/extra/corrupt evidence.
Windows additionally passed all nine relocated native/startup cases. Its known
OS-held fixture-profile error32 used the existing explicit disposable-VM policy:
functional/security checks and unmount must pass, no fixture-owned process may
remain, dummy credentials must be removed, and every other cleanup error fails.
Only profile deletion is deferred to hosted VM destruction, not silently claimed.

Rejected hosted test fixtures are preserved: e7ad8bb omitted loopback path-style
configuration; d4b2f27 incorrectly counted a nonexistent setup seed PUT;6dccc49
omitted the configured source prefix in its new provenance checker;d7e51f8 omitted
the existing disposable-runner cleanup flag from the new startup invocation.
Corrections affect only tests/CI, not product guards or existing deadlines. The
final exact candidate above passed all gates; prior green subsets were not used
as candidate or main acceptance.


## 4. Real-use beta and packaging

Hosted native/startup tests are not real provider or physical-machine evidence.
Prepare a reproducible package, isolated installation/startup smoke checks,
missing-driver behavior, native application save/reopen/rename/delete cases and
an explicit physical reboot/sign-in checklist. Never install on the user's
machine, alter startup/security or reboot merely because hosted CI passed.

Proposed live scope, **writes still blocked**: an existing disposable
unversioned bucket, fresh empty `s3drive-beta/<random-run-id>/` prefix; at most64
keys,256 PUT attempts,2000 total HEAD/GET/LIST calls and64 exact-key cleanup DELETEs;
64MiB uploaded and256MiB downloaded. Preflight, retries and pagination all count.
Stop before any cap; preserve uncertain objects. Cleanup requires recorded exact
identity/content and no unrelated keys. No recursive bucket deletion, bucket or
account configuration change, or use of exhausted historical Wasabi permissions.

The user permits bucket selection and requires staying within the existing1TB
Wasabi plan, with no incremental overages. Actual usage/headroom and retained
deleted-storage accounting are not verified; the account console is not signed
in. No price or claim of zero cost is supplied. A capability renewal probe, if needed, requires
its own isolated reserved conformance scope and explicit allocation within the
approved totals; an application prefix is not a capability probe prefix.
Credentials must stay in private supported configuration, never chat or Git.

MiniAir was authorized for testing and read-only readiness inspection found
Windows11 x64, ample NTFS space and no WinFsp. The user has no desktop access
currently; interactive driver installation, temporary startup registration and
reboot remain pending specific approval and access. Native UI automation is not
available through the current host tools. No installation or reboot occurred.
A reboot interrupts sessions and unsaved applications. Linux MX requires separate
host authorization. Record blocked gates honestly while independent work proceeds.

### Private test packaging (not a public release)

`scripts/package-test.py` creates a deterministic, exclusive-output ZIP from a
clean tracked checkout, a built CLI and an explicit source-file allowlist. Its
manifest records source commit/tree, compiler/target, lockfile digest and every
file's SHA256/size. A dependency license inventory is informational, not legal
clearance. WinFsp/FUSE binaries, credentials, configurations, databases and local
validation archives are not included. Output must be outside the repository.

Portable tests reject changed bytes, missing/extra/duplicate members, symlinks,
unsafe paths and overwriting an existing archive. Hosted packaging runs the
verified bundled CLI after relocating it into a fresh directory, checking
`--version` and `--help` only. This does not prove driver mounting, installation,
application saves or reboot. Bundles remain private and are not uploaded as
public CI artifacts. Project licensing, signing, installer/upgrade behavior and
physical-machine acceptance remain separate release gates.

Example after building the exact clean source with the pinned compiler:

```text
cargo metadata --locked --format-version 1 > <private-directory>/dependencies.json
python scripts/package-test.py --binary <built-CLI> --output <private-directory>/test.zip --target <rustc-host> --dependencies <private-directory>/dependencies.json
python scripts/package-test.py --verify <private-directory>/test.zip
```
