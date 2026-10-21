# Prospective creation admission: state layer

Accepted 2026-10-21 at runtime `20420d73bf07ccb0929c3b188e32c6d23a37dfec`.
**State layer only; no user installation.** This implements
step A of the [prospective admission design](creator-open-admission.md) in the
Rust state library, using schema 45 and creator format 3. The SDK entry point,
native lifecycle integration remain pending. Accepted
main was `02133e0` / runtime `3c38ad1` / schema 44 when this candidate began.

## Implemented local contract

`chunk_mount::Client::creations(limits)` returns a creator client retaining that
worker's typed chunk capability. A format-3 `manifest_creation::Input` includes
an original owner and `CreationOpen { id, session, delete_on_close, posix }`.
`Journal::for_chunks` is the corresponding synchronous state entry point.
Neither API sends a network request. The original source/store/volume must be
in the retained binding; the owner must match the capability, and the session
must belong to the current exclusive state acquisition.

One immediate transaction writes the creator and role projection, original
source binding, inactive controlled/session-bound open, and independent retained
open anchor. It then validates the complete graph and namespace quota before
commit. Any capacity failure, changed retry input, duplicate open or namespace
operation collision rolls back the transaction. Successful repeated admission
returns the same record without charging again. Generic creator admission
refuses format 3; generic open preparation cannot recreate its row. Historical
read/update recovery retains the original identities, without reviving an old
session as a fresh handle.

Unimported, nonrejected creators reserve three namespace slots: the future
import row plus two deferred closing slots. Namespace begin, acknowledgment and
import account for those reservations. Only the exact complete canonical import
converts the reservation into ordinary retained namespace/history accounting.
Other operations cannot steal its operation ID. Exact terminal authority
rejection releases the prospective namespace budget, while the open row remains
until explicit recovery closure and retirement. An unknown result retains both.

An unrelated admitted closing attempt can temporarily occupy its own reserved
credit while its result is unknown. A verified prospective import may still
convert its existing reservation if the complete projected budget is unchanged
and the physical namespace-row limit is respected. This exception permits no
fresh capacity and does not apply to legacy imports without a reservation.

The actual inactive row consumes the existing open-row quota and reserves the
existing controlled callback budget. No counters, tables, quotas, worker slots,
deadlines or stack limits were added or raised. Logical quotas do not reserve
filesystem space; later disk failure remains possible and recoverable.

## Proof and recovery boundaries

Creator loading checks its original raw open input/owner, row checksum and
retained anchor, without reducing open events. Open loading checks creator
metadata/projection and its selected original physical/open witness before
validating events. This avoids recursive role loaders. Namespace budget checks
validate the complete creator metadata projection and physical/open witnesses
for prospective creators; existing full-role APIs still validate all witnesses.
No historical input, stage key, source bytes or receipt is rewritten.

Fresh activation additionally requires the original current runtime session,
successful creator import, and the original fence still locally deferred.
Cancellation or a successor fence cannot activate the original inactive open.
Stored activation proofs are checked against the frozen creator origin; later
history does not erase already recorded activation facts. Local activation does
not prove remote liveness or grant a kernel handle.

Unknown creations, including successful remote creates whose receipt/import is
not locally complete, block session abandonment and retirement. This guard also
runs when loading recovery proofs, so forged no-effect recovery cannot bypass
the mutator's checks. A known rejected creation can close an obsolete session
without fabricating activation or a successful namespace entry. Successful
creations retain their exact closing work after import. No automatic abandon,
deletion, retention or GC is enabled.

## Schema compatibility

Schema 45 is a marker-only change. Format 3 uses a new staging-key hash domain;
its canonical initial 64-byte manifest and authority mutation are unchanged.
New optional fields are omitted from legacy serialization. Format-1/2 inputs,
stage keys, BLOBs, JSON and role hashes retain their original representation.

Before any transaction-local marker change, a bounded raw-shape guard rejects
prospective creators or creator-referencing opens under pre-45 markers. The
pre-44 canonical-import guard still runs against the original version. Valid
43/44 roles then receive a transaction-local current marker for strict role
validation; failure rolls everything back. Earlier unsupported role migrations
remain refusals. Saturated legacy format-1/2 imports still require their own
explicit recovery work; these records have no retrospective reservation.

## State verification

Workspace check and Clippy passed locally. The 15-case focused run passed in
46.17 seconds, followed by a separate concurrent public-worker admission case
(0.20 seconds) and the unrelated-pending-closing conversion case (23.30 seconds).
One of the 15 cases is the subprocess worker entry; the other
cases exercise actual state transactions. The initial workspace check failed
on one missing `creation: None` in an existing multiline test fixture; that
fixture was corrected, and both failed and successful logs are retained.

The tests cover real worker termination at four admission transaction/commit
boundaries, actual SQLITE_FULL rollback/retry, independently saturated open-row
and callback budgets, namespace capacity through import/promotion/final closure,
operation collisions, unknown remote outcomes, known rejection closure,
obsolete sessions, cancellation/successor refusal, schema-44 migration/rollback,
missing anchors and joint creator/open/anchor forgery against the independent
staging witness. Concurrent public-worker calls with the same open ID yield
one complete admission and one refusal, with no partial second creator.

Physical receipts and bulk namespace/open quota history in these state tests
are **modeled**. Worker termination, public worker serialization and SQLite
transactions/disk-full are real. These tests do not establish any provider,
mount, reboot or remote fencing behavior on their own. Exact-source hosted
completion and native regression artifact review subsequently passed below.
SDK step B and native
lifecycle acceptance remain additional prerequisites for integrating this into
mounted creation. Do not enable native flags on the strength of state tests.

## Full local gate and executable compatibility

Candidate `20420d73bf07ccb0929c3b188e32c6d23a37dfec` passed the full
`bash scripts/gate.sh` on 2026-10-21 in 983.01 seconds, exit 0, with all 733
inputs unchanged. All 37 cohorts and all 17 prospective cases passed; every
slow-test notice resolved to a later pass. Frozen runtime input count: 380,
with 25 changes from accepted main `02133e0`. Gate-log SHA256:
`ba753a63a14675cace65c3ca1fcdea8aa75d35ea95c57af40caa4f3b06c1e1cc`.

The actual accepted schema-44 Linux CLI refused a disposable schema-45 database
without changing its database bytes. The new CLI reopened it as schema 45.
Executable copies avoided any shared-target worktree switch. Old binary SHA256:
`4fce77cea8b52753c59bf55a5cde7f24374b7ef70ef58e1be4e3bdf289e6a6bc`.
An initial review incorrectly assumed the test-stage and final-build executable
digests would match. That check failed; both evidence cohorts were retained.
The final workspace-build executable was then checked independently against the
old binary and passed the same forward-refusal/reopen checks. Final binary SHA256:
`d08bd8bcdf785aa6c697ddcbccd42bd4a61d06c7fce6a7334502b14cbbd9ccc0`.
Its bytes remained unchanged through that check. No source, gate, deadline,
quota or schema guard was changed to address the digest assumption.

The [local evidence archive](evidence/creator-open-state-local-20261021.tar.gz)
contains all 380 runtime inputs, frozen source maps, development/gate logs,
the candidate patch and initial compatibility proof; every one of its 405
members was checked. SHA256:
`123b8b7e2d343726290004e77bbcaf5a5cc31d811e749cdc916ede44ec1a056a`.
The [five-member supplement](evidence/creator-open-state-local-supplement-20261021.tar.gz)
preserves the failed digest assumption, final executable checks and complete
local log review. Every member was checked; SHA256:
`7dbf2771616a26d332c5f174dc6d48a4420fd66761cfdf71456d640aac0ab617`.
Archive writers are complete and must not be rerun.

## Hosted acceptance and scope

Own [CI 39616964879](https://github.com/marshallr12/s3drive/actions/runs/39616964879)
tests candidate `20420d7`. The independently retrieved source ZIP matches all
733 frozen file digests (ZIP SHA256
`80c238b07a35c04ea5f1173cc92cb7bbb19dde7c8dc79df21fd144831b3f9220`).
All three jobs passed on attempt 1. Independent strict 21-replacement/14-disposition
checks, project PowerShell oracles and comparison of every raw-record field and
JSON type passed. Duplicate keys and nonfinite JSON are refused. All 17 new state
cases, 14 earlier import state cases and three earlier SDK cases have passing
completions in both Rust jobs (worker entry cases included). This adds no new
SDK-flow acceptance claim. Existing real FUSE/WinFsp chunk Unicode I/O, directory,
remount, 128KiB callback and startup regressions passed.

Full raw review found no test faults. All 17 slow notices had later passing
completions; 15 interleaved Windows reference completions were checked within
their bounded contexts. Three disposable-VM profile-cleanup warnings were
reviewed against their following passes and the source cleanup guard: only an
OS profile sharing violation after acceptance/unmount may defer profile deletion
to VM destruction; dummy credential/removal errors accumulate and throw before
PASS. Other warnings were tooling/node deprecations, git/toolchain hints and
unused WinFsp delay-import linker warnings. All remain in the evidence.

| Job | Raw lines / parsed outcomes | Raw-log SHA256 |
| --- | --- | --- |
| Linux 119280476660 | 1692 / 924 | `439d7eaf5cda928aa534f1d758b34a2823a963d91fcb968ad46ad0aef1467e60` |
| Windows 119280477007 | 2019 / 991 plus 15 interleaved | `9a73d3e5c04d149e06b5404e7b8a6eba0d35ce67a50ae2feca0d604465acbf3f` |
| Book 119280477165 | 651 / no Rust outcomes | `9e76c1d9b4e01f4eb25daac22ca5fe4e29de78284b98add3c366a9cc034da0a8` |

Runtime images were Ubuntu24.04/20261012.334.1 and
WindowsServer2025/windows-2025-vs2026/20261018.275.1; book used
Ubuntu24.04/20261018.339.1. These are hosted VM results, not physical
MX/Windows10/11 sign-in/reboot or live Wasabi acceptance.

Replacement artifact 12353607302 ZIP SHA256:
`7be98f3993a6e625167a08c51d82be70152b9ada3f7ac15db76eb90ff15350e4`.
Disposition artifact 12355085746 ZIP SHA256:
`d02894cf944725d5d2a0bb4d67df40a8a17a0b750366265ae7aa11f87c50120e`.
Both matched their API digests and exact source. The
[37-member hosted archive](evidence/creator-open-state-hosted-20261021.tar.gz)
retains API metadata, all raw logs, scans, review, source proof, artifacts and
verification tools; every member was checked. SHA256:
`0af4ed455c7a66da01c34c773d10e641687c9d2c5b0ea70debeafd521c4a6f2a`.
All three archive writers are complete. Later documentation/evidence edits must
preserve all 380 runtime digests; they are not new runtime acceptance evidence.

## SDK step B constraints

Source review on 2026-10-21 of the [current SDK flow](https://github.com/marshallr12/s3drive/blob/20420d73bf07ccb0929c3b188e32c6d23a37dfec/crates/s3drive-managed/src/creation.rs)
found the following constraints for the next implementation:

- Retain the original Creator/Recovery concurrency slot, deadline and boxed
  future boundary. Derive the owner from the same-worker chunk capability and
  commit typed admission before PUT or authority mutation; legacy entry points
  must retain format-1/2 inputs and stage keys.
- Preserve saved request/receipt recovery before fresh probes. An already
  claimed physical upload must remain observable after restart without a second
  PUT. Blindly replacing its exact generic begin with fresh typed admission would
  reject the obsolete session before observation. Distinguish that historical
  recovery from permission to start an unclaimed upload or grant a handle.
- Restricted flat `Recovery::open_existing` does not handle mixed chunk roles.
  Add capability-bound metadata-only recovery for saved requests/receipts, with
  no AWS credentials, write-policy file or staged body reads. Unknown physical
  outcomes still need an authorized provider observation. Validate complete role
  membership before any store-scoped enumeration; never adopt foreign bindings.

Real SDK/TCP tests must prove refusal before PUT/Apply on quota/session/binding
failure, unknown physical reply recovery without resend, saved receipt replay
without RPC, no-credential damaged-body metadata recovery and shared slot/deadline
behavior. Sparse owned-unlinked publication and native lifecycle gates follow.

Development/continuation evidence lives under `/tmp/s3drive-creator-open-state-*`.
No user database, credentials, bucket, startup installation or hosting was touched.
