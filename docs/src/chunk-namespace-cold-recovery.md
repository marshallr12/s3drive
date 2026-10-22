# Restricted chunk namespace/open cold recovery

Accepted bounded prerequisite, 2026-10-22, runtime/test/CI source
`cb568aa0417cf84d829a58236f9ac45c707b4ef8`, based on main
`1b77e207ed1b142e5516ea9b90b5e11df96a0fc6`. Not native activation.

The native integration review found a prerequisite: namespace/open clients use
normal `Storage`, which validates source bodies at startup. Existing restricted
creator and mixed-root recovery cannot replay namespace intents or record their
already-linked open acknowledgments. The legacy disposition controller requires
whole-generation `Publisher` and must not be composed with a chunk mount.

## Bounded contract

Add an existing-only, exclusive, metadata-only chunk namespace recovery owner.
It must refuse missing stores, wrong application/schema, non-WAL storage, unsafe
filesystems, triggers, corrupt ownership/control, namespace/open chains and
creator provenance. Validate the whole retained role before filtering results.
No migrations, session creation, open activation, callback observations, reply or
completion fabrication, fresh namespace admission, source acknowledgment,
publication, upload or deletion is available through this capability.

It can read retained namespace/open records, replay only an exact already saved
namespace request, persist its checked terminal outcome, and acknowledge that
same outcome against an already persisted open-effect link. Existing event IDs
remain immutable. Rejected namespace outcomes are retained, never reported as
successful effects. Unknown replies stay unknown. A link without a namespace
intent is insufficient permission for cold admission: return incomplete.

Metadata replay does not inspect or repair source/staging bodies. It leaves all
source views, physical attempts, root publications, session bindings, callbacks,
replies and retirement records unchanged. The caller must close this exclusive
owner before trying normal Storage. Source integrity failure can still prevent
a normal mount after metadata outcomes have been reconciled.

## Required evidence before acceptance

- Exact replay and idempotent acknowledgment, original identities and unchanged
  source bytes; modeled state receipts distinguished from authenticated SDK HTTP.
- Corrupt source and staging bodies with normal-owner refusal; exact saved RPC
  recovery without credentials, policy, feature probes or provider traffic.
- Wrong volume/control/operation/outcome, absent intent, corrupt excluded records,
  foreign role, unsupported schema and late triggers fail closed.
- Real SQLite-full rollback and killed-owner boundaries preserve atomic records.
- Full Linux and Windows workspace gates, unchanged native regressions, schema
  compatibility and exact-source/log/artifact review. These are prerequisite
  acceptance, not the new native lifecycle gates.

## Subsequent native work

A chunk-aware disposition engine must own original session-bound opens, serialize
namespace transitions with sparse publication, and route FUSE/WinFsp callbacks.
It must use typed owned preparation, preserve dirty retained handles and settle
all plan/root/source barriers before close completion or releasing ownership.
The actual Close observation must precede filesystem/network waits; atomic
original-publisher admission/finalization ordering is still a separate prerequisite. Dead sessions cannot submit
fresh owned work. Staged/unclaimed plans may remain retained conflicts; no
unconditional retry or synthesized successful close is permitted. Actual >8MiB
Unicode unlink/save-replace/dirty read/last close/crash/remount cases on both OSes
are required before enabling flags. GC, providers, physical startup and deployment
remain outside this slice.

## Candidate history

First candidate ea974014fce882d1a21337484fd369e3e1eac8e9,
[run39848217524](https://github.com/marshallr12/s3drive/actions/runs/39848217524),
failed its new SDK fixture gate: the generic child harness did not yet recognize
the three new intentional-kill modes, and the rejected-outcome fixture chose a
sequence older than its own node guard (invalid admission rather than an
authority rejection). The fix recognizes exactly those killed modes and uses a
valid future sequence for a real retained authority Conflict. Neither change
relaxes implementation checks. No acceptance is claimed from this failed run.

Second candidate b344890f4867e0dfffc19d6c083cb2566108cc4c,
[run39848748461](https://github.com/marshallr12/s3drive/actions/runs/39848748461),
passed six of seven state cases including all four killed transaction boundaries.
The real SQLiteFull case reached disk-full and verified rollback, then its retry
correctly rejected the fixture's duplicate authority sequence across independent
modeled receipts. Each modeled receipt now has a distinct sequence and matching
node revision; implementation validation remains unchanged. The suite additionally
checks missing stores/roles, foreign roles, non-WAL opening and corrupt unselected
namespace records. Failed candidate runs remain non-acceptance evidence.

## Accepted prerequisite evidence

Exact candidate cb568aa0417cf84d829a58236f9ac45c707b4ef8 passed
[CI39849211849](https://github.com/marshallr12/s3drive/actions/runs/39849211849):
Linux120070563474, Windows120070564098 and book120070563913 all completed
successfully. Linux966/Windows962 workspace tests passed in37 cohorts each. All
8 focused state cases (including the killed-child helper) and3 authenticated SDK
cases passed separately and in the full workspace suites, alongside the previous
16 owned sparse state and6 SDK cases. The four killed-transaction boundaries,
real SQLiteFull rollback/retry and credential-free exact replay are covered by
those dedicated prerequisite cases, not inferred from native regression success.

All764 source-archive files matched the exact candidate. Complete job logs were
reviewed. The3 artifact ZIP API digests matched, actual46/47 binary compatibility
preserved the complete legacy SQL dump and refusal bytes, and all35 typed native
records (21 replacement/14 disposition) matched independent fixtures and raw logs
field-for-field, including JSON types. Existing FUSE/WinFsp, chunk remount, paired
startup/recovery and teardown gates passed. These do not accept new owned sparse
native lifecycle behavior. Existing tooling/linker notices and3 disposable Windows
locked-profile cleanup notices were reviewed; dummy credentials were removed and
profile deletion deferred to runner destruction. No test failure was hidden.

Schema47, creator format3 and all native feature flags remain unchanged. Local
format/clippy,39 core tests, schema fixtures and documentation-link checks passed;
local overlayfs/tmpfs is not durable staging and was not used as durable acceptance.
Validation archives remain private. Merge only after a fresh main comparison and
verify the merged main's own full CI. Live native controller/adapter integration,
atomic source-epoch/Close ordering and dedicated new native OS gates remain open.
