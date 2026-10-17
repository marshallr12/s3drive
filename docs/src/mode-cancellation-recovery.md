# Cancellation after an unknown mode change

**Implementation contract, 2026-10-17.** This phase extends the
[deferred-removal foundation](deferred-removal.md) without enabling native
Windows deletion. Schema 16 and mount configuration version 6 remain unchanged.

An acknowledged owner may explicitly cancel its reservation while a transition
of that same node and fence remains unknown locally. The narrow exception must
match the original volume, store, source and credential fingerprint. Unknown
arm, commit, ordinary namespace effects, foreign fences and prepared publications
remain barriers. Cancellation retains the file and its staged bytes in either
authority ordering: transition then cancel, or cancel then late transition.

Successful cancellation is exact, durable evidence that this epoch is closed.
It does not reveal whether the preceding transition committed. Derive a typed
`TransitionCancellation` proof from the retained, checked operation history;
do not rewrite requests, hashes or unknown outcomes. The original unknown row
remains in the pending audit listing. Proof inspection distinguishes it from
work that may still act, and ordinary admission ignores only the proven closed
transition. Publisher retries return an explicit cancelled-transition error
without another HTTP request. A fresh owner/fence is independent of that proof.

At exhausted history capacity the authority may refuse a transition without
retaining an exact result. Cancellation must still use the reserved closing
slot; the unknown original remains unknown. Do not manufacture a server receipt
or rejection. Late exact successful/rejected results may be recorded while
preserving newer observed metadata, current ownership and dirty generations.

The proof must validate the complete acknowledged ownership lifecycle and the
closing input/receipt, not just find an operation with the same node name.
Missing or corrupted history is an error. Cancellation acknowledgment and local
file observation stay in the existing SQLite transaction. Failure/full disk
retains the unknown cancellation and protected work for exact recovery.

Acceptance requires both authority orderings, response loss/restart, a successor
fence, unknown quota rejection, late observations, wrong owner/binding and
unrelated unknown operations, prepared-publication barriers, corruption,
ignored/full-disk acknowledgments and actual SDK client-death recovery. Run the
repository gate and exact-tip hosted Linux/Windows/book CI, including existing
native/reference/startup suites. Real Wasabi mutation is outside this phase.

Next add atomic deferred create and owner-fenced rename, then durable
per-descriptor/node-wide Windows lifecycle integration and full native deletion
acceptance. This recovery foundation does not substitute for those operations.


Local verification passed on 2026-10-17: final repository gate exit 0, 173 state
tests, seven authority HTTP tests and 11 managed-publication cases. Six new
state tests cover both transition directions and authority orderings, quota
without an exact stored rejection, retained dirty bytes, foreign bindings,
missing/corrupt proof, unrelated unknown commit/origin barriers, prepared
publication and actual SQLite-full cancellation rollback. Two actual SDK/TCP
cases kill clients at the transition and cancellation boundaries, restart,
inspect the typed proof, reject replay before HTTP and publish a successor
without duplicate PUTs. Unicode filenames and binary/empty data are included.
The unchanged native/reference/startup suites must additionally pass for the
exact published commit on hosted Linux and Windows; this source note does not
substitute for the workflow result. No native Windows deletion is enabled.
