# Beta completion status

## Candidate code accepted; real-use beta remains incomplete (2026-10-25)

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
