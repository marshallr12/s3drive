# Empty-manifest client integration

Source review: 2026-10-19. **Proposed next integration phase; not implemented.**
The [atomic authority operation](atomic-empty-manifest-create.md) is accepted
at exact source29eb237/schema39 after its complete local and hosted gates. It deliberately cannot prove an S3 upload:
the client must make and verify that upload before submitting the metadata request.
This work must lead into normal native create/write/flush, rather than introduce
another disconnected write profile.

## Reuse and limits found in the current sources

The accepted [S3 executor](https://github.com/marshallr12/s3drive/blob/12a2f2a7d836aab518c7b0d60675234d6470ef97/crates/s3drive-s3/src/uploads.rs)
already provides source-bound create policy checks, exact staging targets,
single-use send claims and observation after uncertainty. Repeated submission of
an already claimed generation observes its original object; it does not acquire
another PUT permission. Reuse that executor for the canonical64-byte page body.
The logical file is empty, but the staged/uploaded object is not.

The [local namespace journal](https://github.com/marshallr12/s3drive/blob/12a2f2a7d836aab518c7b0d60675234d6470ef97/crates/s3drive-state/src/namespace_client.rs)
accepts a fixed list of older namespace mutations; CreateEmptyManifest is not
one of them. Its constructor also enforces legacy-role restrictions. Merely adding
the mutation to that list does not establish safe coexistence with sparse views,
manifest publication and Windows open-handle recovery.

The [shared storage owner](https://github.com/marshallr12/s3drive/blob/12a2f2a7d836aab518c7b0d60675234d6470ef97/crates/s3drive-state/src/worker.rs)
already owns the connection, lock and bounded command queue. Generic immutable
staging can serve physical objects. Keep creation records and staging on that
same owner; do not open a second connection against its private directory or
relax legacy logical-writer isolation. The [native bridge review](native-chunk-integration.md)
identifies the separate typed composition and lifecycle changes still required.

## Required durable sequence

1. Bind the exact authority volume, configured store, S3 route, local credential
   generation and create-operation ID. Negotiate atomic-empty-manifest-v1 before
   any fresh upload permission. Refuse unsupported or changed bindings explicitly.
2. Persist the full creation intent: parent/revision, portable name, expected
   sequence, operation IDs and immutable store identity. Derive the canonical root
   locally with node and root generation equal to the create-operation ID. Retain
   a stable, domain-separated staging-operation key; a lost local reply must not
   allocate another object, node or name.
3. Seal precisely the canonical page bytes under the exact derived target.
   Link the generation to its creation intent durably. Before a fresh send, apply
   the existing policy and source checks; use create-only conditional upload and
   full-byte verification. Reject nonmatching remote bytes or identity.
4. Persist the verified physical receipt and then the exact CreateEmptyManifest
   request, including its ETag/version and upload-operation binding, before RPC.
   These are distinct stages: the final metadata request cannot be manufactured
   before a real physical proof exists. SQLite failure after upload retains the
   original generation for observation/recovery.
5. Submit/recover only that saved metadata request. Validate the complete returned
   node, canonical manifest, placement, publication history and operation receipt.
   Save the receipt without substituting a newer live node. Name collision or a
   changed parent is an explicit retained outcome; never choose a new name or
   silently replace the competitor.
6. Initialize the ordinary authenticated manifest edit view through the typed
   native bridge. Historical creation success alone does not prove the name is
   still live: subsequent removal or rename must remain visible as such. Bind
   filesystem handles to the actual node and content epoch, not just a pathname.

Known local receipts should replay before narrowed quotas or fresh feature probes.
A metadata-only recovery facade must work without S3 credentials or source bytes
once the exact metadata request is saved. Earlier unknown physical outcomes still
need the original source-bound observer. Neither path may invent a provider receipt,
resend a claimed PUT, change an admitted parent/name, or erase retained uncertainty.
Orphan uploads remain retained until a separately verified cleanup protocol exists.

## Acceptance tied to the native outcome

Use deterministic SDK/TCP and authority fixtures for fresh creation, Unicode and
empty content, same-name races, absent feature before PUT, wrong route/credentials,
root corruption, policy expiry, lost local replies and physical/metadata responses,
restart and cancellation. Kill the real local owner around staging/link/proof/
request/receipt boundaries and verify SQLITE_FULL rollback and quota behavior.
Exercise historical receipt recovery after rename/removal without resurrecting it.

Then test the composed native path on actual FUSE and hosted WinFsp: create,
write beyond8MiB, sparse extension/truncate, read-your-writes, flush, reopen,
save/replace, rename/delete, open-unlinked handles, owner death and remount. The
same existing-file chunk path must be used by created files. This integration
must preserve the old read-only and whole-generation profile regression suites.
A real Wasabi write test needs a fresh concrete user-approved fixture scope;
physical MX Linux and Windows10/11 sign-in/reboot checks remain separate.
