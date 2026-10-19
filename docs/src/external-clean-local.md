# Local clean-version adoption intent

Current status,2026-10-19: accepted after the complete local gate and this source's
own hosted/platform/source/artifact checks. This remains local intent only; the
authenticated facade has its own separate acceptance.


Declared 2026-10-19. Isolated phase `codex/external-clean-local`, based on accepted
schema37 main `dc0104a`. This phase adds local SQLite intent only. A later managed
facade must authenticate a fresh authority/manifest observation before calling it;
no local binding, operation or receipt grants remote authenticity.

Use schema38 to fence older binaries from the new record format. Preserve all
literal old records. Add a distinct format4 Observe(parent) derivation/hash domain,
with full Base coverage of the new binding length. Permit only the same target,
store, source principal/fingerprint and object-root route. Its parent must be an
exact initial full-base view, validated own-publication rebase or prior observation;
a write/resize is dirty even if it changes no bytes. No origins or GC in this slice.

Expose scoped intent admission and exact operation recovery on Journal and its
same-owner Client. Bind the token to the old parent epoch. Known exact replay is
checked before current head, fresh quotas or unresolved-work checks. Reusing the
operation with a different parent/binding conflicts. A fresh operation requires
permanent scoped mode, an exact current-head CAS and no unresolved preparation for
this file. Every retained preparation for the target must have a validated own
publication acknowledgment; missing publication, preparing, claimed, unknown and
rejected work all block. Acknowledged historical work may remain. Other files may
have pending work. Verify the publication/preparation/physical graph, not just a
row count or an acknowledgment flag. Historical preparation created after adoption
is allowed and must block the next fresh adoption until resolved.

If the exact binding is unchanged, return Unchanged without inserting any rows;
that result admits no operation and the nonce remains unused. A changed binding
returns Adopted with the immutable View, not a newly allocated read pin. Exact
recovery can return only an already admitted adoption; unknown returns None and
does not claim that an RPC was never sent. The managed facade must recover known
admission before making a new observation. Lost replies after commit preserve the
admitted result. Old readers, pins, blocks, pending state and operation history stay
intact; failure never silently cancels work or overwrites dirty data.

Transactions cover graph validation, expected-head check, new view/operation,
binding/head update and exact post-state validation. Check unrelated preparation,
publication and pin state for silent SQL effects. No network inside the transaction.
Use existing bounded quotas, owner lock and worker. No new dependencies or workers.

Acceptance: independent expected Base/Zero/Local pieces for larger/smaller/empty
versions, old pinned reads, no-op dirty refusal, same-file pending work versus
other-file work, acknowledged retained history, post-adoption historical work,
competing head/adoption, wrong scopes/tokens, exact replay/recovery after dirty head
and narrowed quotas/restart, no-change boundedness, old literal/schema37 migration,
corruption/trigger refusal, real SQLITE_FULL rollback and process-kill boundaries.
Run focused state tests, all-target Clippy, then freeze the exact candidate for the
complete local gate. Own Linux/Windows acceptance is required before promotion.
Fresh network authorization/denial/SDK tests belong to the subsequent facade slice;
this phase cannot satisfy external adoption or native chunk-write acceptance alone.


## Focused evidence (2026-10-19)

The twelve local adoption tests passed in1.23s, including five actual killed-owner
boundaries, real SQLITE_FULL and a same-owner race with a retained read guard.
A separate publication fixture passed in0.86s: acknowledged historical work permits
adoption; a later preparation of an old pinned version blocks the next adoption
without invalidating the committed result. State all-target Clippy passed17.34s.
An initial check ran before the test module existed and failed with E0583; the
module was then supplied. No production assertion was relaxed. Full workspace and
own hosted/platform acceptance remain outstanding; no remote facade is enabled.


Full gate1 is running from frozen340-input source67a75e3047a867e2499ef08ded64fe00b72a4aa9,
runner1114325/child1114329, started2026-10-19T17:33:34Z. Own
[CI39295182975](https://github.com/marshallr12/s3drive/actions/runs/39295182975),
attempt1, has book118205713291/Linux118205712929/Windows118205713142 running.
GET-only observer1115366 tracks this cohort. Prepared acceptance tools are not
executed; they require13 new cases,494/489 state counts and exact340 inputs as
well as inherited native/source/artifact checks. No parent artifact reuse.


## Complete local gate (2026-10-19)

Source67a75e3/schema38 passed the complete local gate at17:46:14Z with all340
frozen inputs unchanged. Publication24/163.16s, state494 (one diagnostic ignored)/
256.86s, reader23 (one native-only ignored)/45.10s,37core/62S3, formatting,
workspace all-target Clippy/build and book/link checks passed. The log contains
90 Markdown-file link validation and86 chapter-test entries. Ordinary four test
threads; no reduced SDK admission or changed operation deadlines.

The [indexed local archive](evidence/external-clean-local-local-20261019.tar.gz)
contains355 verified members/6437122bytes: exact340 inputs, full gate/map/runner,
focused and Clippy logs, archive builder and two independently hash/index-verified
accepted-parent archives. SHA-256
`6771882a97cca90e85ca7570d6b05e6fa8ce4bda7e6956628d609cc10bacc582`.
Full log SHA-256
`4c9d2c03b1384b851295402053e266554079fc32324b299f0dfeaf11db75f342`.
[Dated evidence](evidence/external-clean-local-20261019.json) records the scope.
The one-use local archiver is DONE; never rerun it. Own book/Linux CI succeeded;
Windows is still running. Full own raw/source/native/artifact checks remain
required before promotion. The separate remote-observation candidate cannot be
accepted by this local intent result; it needs its own checks.


## Own hosted acceptance (2026-10-19)

Source67a75e3047a867e2499ef08ded64fe00b72a4aa9 passed
[CI39295182975](https://github.com/marshallr12/s3drive/actions/runs/39295182975),
attempt1: book118205713291/Linux118205712929/Windows118205713142 all succeeded.
Complete raw logs, exact340 committed/working inputs, own API ZIP digests,
independent typed21+14 records, project oracles and all35 raw fields/JSON types
passed. Own replacement12207128174/disposition12209851124 artifacts were used.
The [indexed hosted archive](evidence/external-clean-local-hosted-20261019.tar.gz)
contains107 verified members/407971bytes, SHA-256
`b97670d9733322847845c305d8e9fb6114673957f9348180f2bed9e156048902`.
The one-use raw review/source checker/comparator/hosted archiver are DONE; never
rerun them. Observer1115366 CLOSED0/all_success at18:07:07Z.

Linux494/Windows489 state cases (one diagnostic ignored each),24publication and
23reader (one native-only ignored),37core/62S3 passed. Five slow Windows cases
completed;15 interleaved reference tests had explicit completion. Separate actual
native runs passed FUSE9/51.59s, WinFsp7/101.13s, manifest ranges/remount1.08s/2.87s
and paired no-systemd23.57s/Windows77.39s. Actual images Ubuntu24.04
20261012.334.1 and WindowsServer2025 windows-2025-vs2026 20261007.264.1.

Three disposable Windows profiles stayed locked; dummy credentials were removed,
profile deletion deferred to hosted VM destruction. Unused WinFsp delay-import
warnings and tooling deprecations were retained and reviewed. This does not prove
native chunk writes, physical MX/Windows10/11 reboot, real Wasabi mutations or the
separate authenticated facade. Earlier running statuses above are historical.
