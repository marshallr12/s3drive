# Remaining beta completion gates

Research and implementation checkpoint: 2026-10-25. The user requested all four
items below, using the cloud development workspace where suitable and standard
public GitHub Actions otherwise. No paid larger runners, weakened durability
checks, reduced original workloads or increased original deadlines.

Baseline main `3fd2f05e6d833d45cc1bfb70eabe6aa754524402` passed independent full
[source, logs and artifacts review](https://github.com/marshallr12/s3drive/actions/runs/40157712514).
This is fresh existing-linked-file retention, not the four remaining milestones.
Candidate acceptance and subsequent merged-main verification remain separate.

## 1. Creation and namespace lifecycle cleanup

Existing `owned-unlinked-v1` implements create, rename, replacement, unlink and
open-file lifecycle. The `linked-retention-v1` profile deliberately refuses
creator and namespace admission. Extending cleanup requires permanent identity
expiry for namespace/creator admissions, compact original-operation evidence,
closed dependency groups and reader-safe physical reclamation. Simply deleting
terminal journal rows or removing the current refusal is unsafe: they remain
provenance for original opens, owned epochs and exact retries.

Acceptance must cover repeated create/save/rename/replace/unlink with held
handles, last Close, remount, exact cold bytes and bounded local rows/bytes on
both FUSE and WinFsp. Dirty, uncertain or conflicting work and cloned readers
must retain history or impose backpressure, never force a cut. Remote object GC
is not implied by local journal reclamation.

## 2. Cleanup after recovery

Recovery itself exists. Its observer session can differ from every recovered
open's original session. Session pruning must preserve both kinds of identity
while recovery records survive. The current safety correction adds this edge
to both session-admission and open-retirement pruning, with pending and finished
recovery fixtures across a third exclusive owner. Local compilation succeeds;
local durable execution is blocked by tmpfs, not passed. Hosted proof is pending.

A following unaccepted candidate reclaims only complete, finished no-effect
recovery groups: no creation/admission, namespace closure, owned source epoch or
frozen publication. A clone-held reader defers collection; deletion refusal must
preserve all records; consumed open IDs cannot be admitted again. This is not
cleanup of recovered publishers or owned/namespace histories. The new tests
compile locally; hosted results remain pending.

Broader cleanup needs complete recovery-group proofs, original dirty-source
and namespace settlement validation, unchanged Close semantics, expired replay
identities and transactional removal. Recovery abandonment is not an observed
native Close and cannot fabricate its completion suffix. Unknown outcomes,
unprepared dirty epochs and live logical/physical readers remain protective.

## 3. Performance and larger/longer soak

Accepted main Windows debug300 took1131.24 seconds against1200. Preserve that
exact gate and add workloads rather than replace it. Optimize measured repeated
work without carrying validated graph proofs across writes, awaits or owner
commands. Pure byte derivations are not authority or successful validation.

Additional acceptance must exercise more files, larger content and more saves,
including cold owners and storage bounds. Report exact source, profile, workload,
per-segment timings, DB/WAL/staging peaks and final cold-read result. Diagnostic
CPU counts and green job summaries alone are not acceptance.

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

### SQL compilation/dispatch candidate (unaccepted)

The accepted main's two-save Callgrind profile showed repeated empty-role table
queries and staged/remote row preparation as measurable costs. A new candidate
combines the exact excluded-table sets into one SQL expression per guard and
uses rusqlite's bounded prepared-statement cache for those two guards plus the
four staged/remote row shape/data queries. This caches SQL bytecode, **not** query
results, decoded graphs or successful proofs. Every invocation executes again;
shape checks still precede allocation and all semantic/hash checks remain.

Resolved rusqlite0.40.2 source was inspected: cached statements are removed while
borrowed, clear bindings when returned and are connection-owned. Enabling its
`cache` feature adds only locked hashlink0.12.2 (MIT OR Apache-2.0); no existing
locked version changes. In-memory negative controls visit each excluded table,
committed writes, rollback and drop/recreate schema changes in both ordinary and
combined modes. These two tests pass locally. Native/full performance acceptance
is pending; no measured speedup is claimed from this change alone.

### Additional soak fixtures (acceptance pending)

Two new, separate standard-runner release workloads supplement, never replace,
the unchanged original six300-save fixtures:1000 actual saves across100 remounts,
and300 actual saves round-robin across four64KiB files/30 remounts. Every prior
file value is verified before overwrite; every new value is read after sync and
Close, and every final file is verified in a new cold owner. Every segment checks
exact completed count, sparse PUT schedule, retained rows and unchanged4MiB DB /
8MiB WAL /2MiB staged-payload ceilings. Each native syscall segment still has its
original120-second bound and subprocess180-second bound. The additional longer
whole-workload steps have their own45-minute limits; original deadlines remain.

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

### Recovery atomicity and current validation status

The next candidate adds killed-owner checks at eligibility, evidence erasure,
post-validation and commit. Precommit deaths must preserve the complete original
snapshot; committed deletion must remove the complete group. Every precommit
case then retries from a fresh owner, followed by another cold validation.
A coherently resealed observer session sharing the original runtime must not
turn a same-owner open into an abandoned one. These are modeled protocol tests,
not native mount or provider acceptance.

Candidate `57bda5d471da329562be9ea4dd31f9342c87a149` is running
[the expanded suite](https://github.com/marshallr12/s3drive/actions/runs/40203142413).
Its predecessor's package smoke assumed the wrong CLI version prefix; this
candidate corrects it to the exact metadata package name. Actual relocated CLI
smoke passed locally; complete hosted log review remains pending. The SQL-only
candidate's fixed two-save Callgrind count fell from25,571,642,463 to23,032,359,387
instructions (about9.9%); this is diagnostic evidence, not a wall-time speedup or
acceptance. No candidate has replaced accepted main yet.

The next negative-test correction does not alter runtime cleanup behavior:
`1c02382` passed47 focused cases per OS, including killed-owner retirement, but
its resealed-session fixture failed early on SQLite's existing unique runtime
constraint. The corrected fixture instead rebinds/reseals the recovery observer
and its exact membership key atomically, leaving session uniqueness and foreign
keys intact, to exercise the intended same-runtime reclamation refusal.
