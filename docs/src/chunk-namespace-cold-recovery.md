# Restricted chunk namespace/open cold recovery

Implementation proposal, 2026-10-22, based on main
`1b77e207ed1b142e5516ea9b90b5e11df96a0fc6`. Not accepted and not native activation.

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
all plan/root/source barriers before final close. Dead sessions cannot submit
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
