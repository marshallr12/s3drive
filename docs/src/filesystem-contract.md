# Filesystem behavior contract

This is the broader **proposed acceptance contract**. See the narrower
[implemented Linux contract](linux-mount.md) for current behavior. The first
working release should expose a conservative read-only filesystem. Writable
operation support must be advertised individually, with unsupported operations
returning the appropriate OS error rather than succeeding silently. The narrower
[version-2 existing-file write contract](native-writes.md) records the current
integration and remaining native acceptance. Creation and rename/delete remain
unsupported.

## What S3 guarantees, and what it does not

Amazon S3 documents strong consistency for object operations and atomic updates
of a single key. That does not create a transaction spanning several keys or
a distributed file lock. Other S3-compatible providers must be measured against
their own documented behavior.
[AWS consistency model](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Welcome.html#ConsistencyModel)

General portable S3 operations do not offer an in-place byte-range update.
Stage edits locally and replace the complete object, using multipart upload
where appropriate. A rename implemented by copying to a new key and deleting
the old one has observable intermediate states. AWS now has `RenameObject`, but
it is restricted to S3 Express One Zone directory buckets; it is not a generic
S3-compatible primitive.
[PutObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html),
[RenameObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_RenameObject.html)

## Operation matrix

| Operation | Read-only milestone | Later direct writable mode |
| --- | --- | --- |
| Lookup, stat, directory listing | Synthetic directories; bounded cache TTL and paginated listing | Same, with local pending changes merged consistently |
| Open/read/seek | Range GETs pinned to an object revision | Read own staged changes; never mix remote revisions |
| Create/write/truncate | Read-only error | Local staging plus conditional full-object publication |
| `fsync` / `FlushFileBuffers` | No dirty writes to commit | Default proposal waits for remote commit; failures are returned |
| Close/release | Release handle/cache references | Queue work as needed; close alone is not the remote durability promise |
| Unlink | Read-only error | Capability-gated conflict-safe deletion; retain open-handle bytes |
| File rename | Read-only error | Unsupported initially; later opt-in copy/delete or native verified capability |
| Directory rename | Read-only error | Unsupported until an explicit non-atomic recursive policy or managed mode |
| Cross-source rename | Cross-device error | Cross-device error; explicit copy is a different operation |
| Empty directories | Existing folder markers interpreted by policy | Marker convention requires its own interoperability tests |
| chmod/chown, ACLs, xattrs, links, streams | Synthetic access policy; mutation unsupported | Explicitly specified subset only |
| Byte-range locks and shared writers | Local handle bookkeeping only | No distributed locking promise |
| mmap, executable files, databases | Not an initial compatibility claim | Separate workload tests; never advertise general POSIX/NTFS equivalence |

Office-style applications often save a temporary file and rename it over the
original. Consequently, writable bytes alone are insufficient for a general
desktop-drive claim. Test real save/replace workflows during P5; document
unsupported applications until their required semantics exist.

## Names and directories

S3 object keys are case-sensitive UTF-8 strings, limited by AWS to 1,024 bytes,
and prefixes only suggest a directory hierarchy. Both `a` and `a/b` can exist
as object keys. Windows has additional reserved characters, device names and
path conventions. These models need an explicit projection layer.
[S3 keys](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-keys.html),
[Windows names](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file)

Proposed rules:

1. Never interpret remote keys as local cache paths. Cache files use generated
   IDs; preserve the original key separately. Do not normalize `..`, decode `%2F`,
   or merge repeated separators into a different remote object.
2. Fail ambiguous directory enumeration with a visible diagnostic instead of
   arbitrarily selecting `a` over directory `a/`, or `Readme` over `README` on a
   case-insensitive view. A later reversible escaping view may recover these
   names; its mapping must be bijective and stable before it is enabled.
3. Preserve Unicode key bytes. Define Windows comparison using the selected
   filesystem's rules, not an assumption that Rust lowercase equals NTFS folding.
4. Synthesize directories from delimited listings. Specify marker-object behavior,
   stable inode/file IDs, directory cookies and tombstone lifetimes before mounting.
5. Report synthetic capacity honestly: remote capacity is not local free cache
   space. Local staging limits must still enforce disk-pressure errors.

P0 only validates portable ASCII aliases and conservative configured prefixes;
the `resolve` command is not the final cross-platform name projection. The current adapters reject ambiguous remote listing projections.

## Coherence, conflicts and errors

For an open reader, prefer version-ID GETs when available. Otherwise send a
revision precondition on each range request. If the object changes or a pinned
version disappears before its bytes are cached, fail rather than splice ranges
from different versions. A fully cached open handle retains its immutable
authorized snapshot until release; each new open still authorizes online. A directory listing assembled across pages is not assumed to be a
snapshot during concurrent mutation.
[GetObject API](https://docs.aws.amazon.com/AmazonS3/latest/API/API_GetObject.html)

For later writes, conditional create (`If-None-Match: *`) and replacement
(`If-Match`) must reach the remote commit, including multipart completion.
A precondition failure creates a persistent conflict; never retry it as an
unconditional write. Treat ambiguous timeout and conflict statuses separately.
[S3 conditional writes](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html)

Map access denied to permission errors, confirmed missing objects to not-found,
timeouts to retryable I/O errors, and local disk exhaustion to no-space. A HEAD
request may return 403 instead of 404 when listing permission is absent; do not
infer that such an object is missing or safe to overwrite.
[HeadObject permissions](https://docs.aws.amazon.com/AmazonS3/latest/API/API_HeadObject.html)
