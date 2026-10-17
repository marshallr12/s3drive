# Atomic deferred creation

**Implementation contract, 2026-10-17.** The verified
[Windows create/handle reference](windows-deferred-reference.md) requires new
delete-on-close files to be created with deferred ownership in one operation.
Native Windows callbacks remain disabled during this library/authority phase.

`CreateDeferredEntry` uses the existing guarded parent, portable name,
empty-file/directory kind and exact store placement, plus this client's stable
owner. The operation ID is both the new node ID and the fence intent. Node,
deferred reservation, origin event, sequence and exact result commit together.
There is no ordinary-create/prepare gap. Failed/ignored SQL, quota or full disk
must leave no new node or reservation. Successful directory creation may admit
children, and file content remains owner-fenced in deferred/pending modes.

The client persists the complete input before RPC. An unacknowledged creation
blocks work on its predictable node ID, even though it has no before-node entry.
It must not invent ownership, cancel an unknown origin, adopt a foreign owner,
or prepare publication through a new bind while creation remains unknown.
Acknowledged ownership comes from the exact create receipt and original frozen
store/source/fingerprint. Reserve two closing slots for an unknown or active
deferred create. Preserve original hashes and requests; schema 16/config 6 stay
unchanged. Advertise the separate `deferred-create-v1` authority feature.

After a lost reply or process death, replay the original request/ID and recover
the same node/fence. Cancellation preserves the created node and any dirty
data; it does not silently undo an uncertain create. Pending promotion, final
commit and retained conflicts keep their existing guards. Late old receipts
must not restore an expired epoch or rewind a newer observation.

Acceptance requires files/directories, Unicode/collision races, wrong
parent/store/owner, corrupted proof, ignored effects, real SQLite-full rollback,
closing quotas, killed authority/client owners and exact replay. Actual SDK/HTTP
tests must publish binary/empty data with this created fence, recover uncertain
results without duplicate PUTs and preserve later epochs. Run the repository
gate and terminal exact-tip hosted Linux/Windows/book CI, including unchanged
native/reference/startup suites. No real Wasabi mutation is included.

Next implement same-fence owner rename/move and persistent per-open/node-wide
Windows integration. Deferred destination replacement still needs explicit
proof that an old target's cleanup cannot delete its replacement. This atomic
creation phase is a prerequisite, not the final native deletion feature.

## Local verification (2026-10-17)

The complete repository gate passed: formatting, strict Clippy, build, 183 Linux
state tests, eight authority HTTP tests, twelve managed SDK cases, documentation
links, mdBook build and chapter tests. This includes ignored-SQL and real
SQLite-full rollback, quota headroom, a killed authority process and four SDK
client process stages. The SDK case recovered the same file/directory creation
IDs and fences, published binary and empty content with exactly three PUTs,
preserved a directory child after cancellation and rejected a late receipt's
attempt to restore an expired ownership epoch. The objects belonged to the
loopback fixture; this is not a new Wasabi capability measurement.

Publication requires all three hosted jobs to finish successfully for the exact
commit, including real Linux/Windows mounts and startup plus fourteen frozen
NTFS traces, four selected upstream WinFsp legacy tests and nine fixture
rejections. Native delete-on-close callbacks remain unsupported.
