# Experimental quiescent lifecycle retention

Design checkpoint: 2026-10-25. **Unaccepted state-protocol prototype**, not a
completed beta milestone or a mounted configuration option. The accepted
`linked-retention-v1`, `owned-unlinked-v1` and absent/default profiles remain
unchanged. No old-store conversion and no remote object deletion are proposed.

## Why a different cut

Existing operations already implement creation, rename, replacement, unlink,
retained handles and recovery. Their local records remain dependencies of open
activation, original dirty epochs, namespace ownership and exact retries. The
linked-file anchor deliberately cannot summarize those dependencies. In
particular, abandonment of an obsolete session is not an observed native Close.

This prototype introduces a separate fresh combined-owner contract, admitted by
`chunk_mount::admit_quiescent`, while keeping the existing version2 owner and
monotonic admission frontier. Its explicit additional mode field prevents
ordinary or linked admission from silently adopting this owner. The mode cannot
contain linked-retention anchors. It currently exposes a state API only.

Instead of manufacturing a smaller proof for surviving data, a successful cut
removes an entire validated local dependency forest. No local source survives
the cut that could need the erased provenance. Subsequent access must acquire
current verified authority metadata and remote content again. The owner identity,
consumed identity frontier, scoped/retry mode and source bindings remain.

## Admission and expiry

Direct namespace, canonical creator and cold source-import requests use separate owner-bound domains
of the permanent monotonic frontier. Exact retained retries precede consumption;
a forgotten original request cannot be inserted again. Existing callback policy
uses deterministic children of an original open ID, callback and fixed role.
Such namespace admission requires the complete original retained event chain and
exact linked input; a retired open cannot admit a new child. Erasing the parent
therefore expires its callback children as well. No callback identity is replaced
merely to retry an uncertain operation. A source re-import consumes a fresh
ordinal even if its remote descriptor is unchanged, so an erased source parent
ID cannot be recreated by retrying its original token. The initial graph and
all child mutation IDs therefore differ after a completed cut.

Prospective creator/open atomic admission remains explicitly refused in this
prototype. It needs its own paired reservation proof, rather than consuming two
IDs in separate transactions. No native application coverage is implied until
its actual creation route has been tested.

## Current conservative eligibility

`publication::retention::quiescent::retire` uses one immediate transaction:

1. Revalidate the exact mount key/mode, full source/preparation/root graphs and
   namespace/creator/open proofs. Refuse application triggers.
2. Require no logical pin or physical reader, no linked anchor, and every source
   head clean. A close barrier, partial preparation, dirty epoch or unknown root
   does not become clean because a process exited.
3. Require each preparation's root receipt and acknowledgment, every namespace
   outcome known, no surviving owned removal, every original open terminal and
   retired, and every recovery group finished and wholly selected. Recovery
   observers must belong to a different runtime from original members.
4. Require every creator outcome and physical witness settled. The exact set of
   known creator/transfer/root generations must equal all staged generations;
   unrelated staging blocks the cut rather than being erased.
5. End the read proof, remove the selected source/lifecycle/physical records,
   then construct an entirely fresh proof. The permanent binding, identity/session rows, source bindings and mode/reader
   counters must be byte-for-byte unchanged; source, publication, namespace,
   creator, recovery/open and physical journals must be empty. Cyclic source foreign keys are deferred
   only within the transaction and explicitly checked before commit; checking
   is never disabled.

At present the selected forest is the **whole local store**. One unrelated live
handle or reader can defer all reclamation. This is conservative backpressure,
not completed cleanup under arbitrary concurrent or long-held-open workloads.
Per-connected-component cuts and bounded repeated native lifecycle workloads
remain required before claiming the broader lifecycle milestone complete.

## Acceptance still required

The prototype has modeled tests for separate admission profiles, expired source
re-imports, physical clone/acquisition races, exact retained
retries, active-open and reader-clone deferral, dirty/incomplete preservation,
closed publisher eviction and completed recovery without fabricating Close.
Compilation alone is not durable execution. Hosted tests must establish these
claims on actual supported storage before acceptance.

Before native enablement/acceptance, add and inspect:

- Exact stale namespace, creator, open, source command and owner rejection after
  a cut; unchanged owner/frontier and cold live namespace/content reload.
- Every transactional cutpoint under killed owners, SQLite-full rollback,
  deletion refusal, corruption and independent concurrent logical/physical reads.
- Completed creation, rename, replacement and unlink dependency groups, including
  original held-open dirty handles, rejected effects and unknown authority replies.
- Safe component selection while unrelated files stay open; bounded rows/bytes
  when dependencies close and explicit quota/backpressure otherwise.
- Actual Linux FUSE and Windows WinFsp repeated lifecycle tests, remounts, exact
  cold bytes, original Close/retry/crash/read gates, unchanged deadlines and the
  existing six300-save plus added larger/longer workload acceptance.

The filesystem must invalidate only its own evicted cached file entries; it
cannot drop independent readers to force eligibility. Package/startup/provider
and physical-machine evidence remains a separate gate in the
[beta completion plan](beta-completion.md).


### First hosted state-protocol result (not acceptance)

On exact `e11c5d66b6c8da37285b61618b947248f6218fe7`,
[run40206741871](https://github.com/marshallr12/s3drive/actions/runs/40206741871)
passed45 Linux focused diagnostic cases, including six initial quiescent tests.
Complete focused logs were reviewed. The full suite and Windows remain pending;
the follow-on source-expiry/post-cut/concurrent-acquisition additions are newer
code and do not inherit that result. No mounted option is enabled.
