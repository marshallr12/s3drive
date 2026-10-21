# Sparse node-wide effect barriers

Implementation date: 2026-10-21. This is the narrow barrier prerequisite for
phase 2 of the [owned-unlinked sparse plan](owned-unlinked-sparse-plan.md), not
completion of typed ownership admission, sparse owned publication, or native
lifecycle support. The schema remains 46, and every existing wire/record format
is unchanged. No provider, native feature flag, or deployment is enabled.

## Contract

The local barrier covers every admitted sparse preparation for a volume/node,
including incomplete preparations, prepared trees without root-publication
records, consumed physical claims, published roots without source settlement,
and rejected requests. It validates the complete bounded preparation/source/
publication graph before filtering by node. A clean current source head or one
settled plan cannot conceal a second unresolved plan on the same node.

Only the existing exact successful source acknowledgment releases that plan's
barrier. A remote receipt alone is insufficient. An acknowledgment of an older
view retains newer dirty views under the existing settlement rules; this change
neither discards data nor introduces an abandonment/no-effect settlement API.
A rejected or unfinished preparation therefore remains conservatively blocked
until existing successful recovery completes or a separately designed explicit
settlement path becomes available. No local result proves current remote
ownership or authorizes another physical send.

In a chunk-composed owner the barrier runs before fresh namespace admission,
including removal and replacement targets, and within the namespace admission
transaction. It also runs inside the transactions preparing or finishing
obsolete-session abandonment and releasing live open capacity through retirement.
The finish step rechecks work admitted after recovery preparation. Ordinary
legacy whole-generation owners retain their previous behavior.

An already persisted namespace request, completed session proof, or sealed
retirement replays as historical evidence before this new admission check.
Subsequent work cannot retroactively invalidate a completed proof. No open/event
rows, dirty bytes, physical identities, request bodies or source views are
rewritten or deleted by a refusal.

The reciprocal preparation-admission check runs in the original immediate
transaction after exact existing-operation replay and before inserting a new
plan. The current unowned preparation API refuses new work behind an unresolved
namespace effect or acknowledged removal ownership. Future typed owned-unlinked
admission must supply retained original-open/current-event provenance rather
than bypassing this check.

## Accepted verification — 2026-10-21

Runtime/test source `2b08169f5e85d92bb7cc3a5834913ae490c0f11b` passed its own
[Linux/Windows/book run 39673658957](https://github.com/marshallr12/s3drive/actions/runs/39673658957).
All three jobs are terminal success. The source ZIP matches all 753 tracked files
byte for byte (ZIP SHA256
`efad6deddb4b0b99149640416aa307936639c00dc8a8a504d0ef0a841f0843d4`).
The full logs and both downloaded artifacts were reviewed, not just job status.

- Workspace suites: Linux 933 passed/0 failed/15 ignored; Windows 929
  passed/0 failed/31 ignored, each across 37 cohorts. All eleven new barrier
  tests and the readiness regression passed on both platforms.
- The original eight authority kill boundaries, fourteen preparation boundaries,
  eleven settlement boundaries and actual SQLite-full rollback cases passed.
- Separate FUSE/WinFsp, Unicode/chunk writes, manifest ranges/remount,
  replacement/disposition and startup/recovery gates passed unchanged.
- All 21 replacement plus 14 disposition artifact records match the reference
  fields, types and values, their log records and the exact candidate SHA.
  Downloaded ZIP digests were verified. JSON SHA256 values are
  `042ed122db7f98cbba55ed1e8eb7fb93189e1f75eb7100128a0431223cfe7ac7`
  and `5186a0a3a0fa2b000979229671489547ec02cafeb9b6a894e3b686c50b86018f`.
- All 19 slow-test notices resolve to passes. Existing WinFsp no-import linker
  warnings, toolchain selection notices and Actions Node20 deprecation warnings
  remain; no failed test or native step is waived.

Local fmt, all-target Clippy with warnings denied, workspace build, schema-fixture
checks, documentation links and mdBook build/test passed. The marker regression
also ran locally; the built production CLI contains none of its fault selectors.
The cloud workstation uses unsupported overlayfs/tmpfs for durable staging.
Local durable-state tests are recorded as blocked/failing for that reason, not
accepted, and the storage guard remains intact. Suitable standard GitHub runners
are authorized for the full Linux/Windows and separate native regression gates.

The first hosted candidate `49d1c6c`, [run39670781416](https://github.com/marshallr12/s3drive/actions/runs/39670781416), passed all eleven new
Linux barrier cases but failed an existing authority crash-fixture readiness
assertion: the parent observed the marker between file creation and its write.
The checkpoint now writes a temporary file and atomically renames it, as the
other crash fixtures do. A deterministic regression pauses before rename to
assert that the complete temporary bytes exist while parent-visible readiness
is absent. A deterministic negative control that exposes the old direct-write
creation window fails that exact assertion; the restored implementation passes.
The eight original kill boundaries and strong byte assertions remain.
Source review also found the fixture hook was not compile-time test-only; it is
now guarded by cfg(test), with an inert non-test stub. No ordinary authority
mutation semantics change. The first Windows job passed its full/native gates,
but that run remains failed diagnostic evidence. Acceptance uses only the
corrected candidate's own complete run. The local evidence archive stays
unpublished; integrate only after a fresh head comparison and verify main CI.

Eleven new durable-journal cases cover preparation-only clean heads, claimed
physical uploads and complete trees without root records, multiple same-node
plans, rejection, cross-node filtering plus corrupt excluded records, reciprocal
namespace admission, acknowledged removal ownership, exact old replay, retirement,
obsolete-session admission and finish-time rechecking. Their authority/upload
receipts are modeled, not real provider or newly enabled native lifecycle proof.

The existing preparation process-kill suite additionally checks that the barrier
appears only at committed admission. Existing source-settlement process-kill and
actual SQLite-full suites additionally check that it releases only at committed
acknowledgment, including historical acknowledgments preserving newer dirty data.
The complete standard CI above includes unchanged FUSE/WinFsp, Unicode/chunk
I/O, remount and startup/recovery regressions. This accepts only the barrier
prerequisite, not the subsequent typed owned-unlinked contract.

## Next boundary

Typed sparse preparation/publication must still bind the retained original
open and removal fence to the exact current mode/event, preserve predecessor/
successor/source-settlement provenance, and define explicit rejected/no-effect
settlement. Schema/format forward guards and compatibility/rollback checks belong
to that next persisted contract. SDK recovery and new native lifecycle acceptance
remain subsequent independently green phases. Physical workstation sign-in/
reboot and real provider/independent-host validation remain unverified.
