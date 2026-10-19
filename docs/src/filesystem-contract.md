# Filesystem behavior contract

This is the broader **proposed acceptance contract**. See the narrower
[implemented Linux contract](linux-mount.md) for current behavior. The first
working release should expose a conservative read-only filesystem. Writable
operation support must be advertised individually, with unsupported operations
returning the appropriate OS error rather than succeeding silently. The narrower
[version-2 existing-file write contract](native-writes.md) records the current
integration. [Regular-file creation](native-creation.md) also passed native
acceptance. Mounted directory creation and rename/delete remain unsupported.
The user selected [managed layout](managed-layout.md); its namespace prototype
is implemented separately and not yet connected to native callbacks.

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

| Operation | Existing direct implementation | Selected managed target |
| --- | --- | --- |
| Lookup/stat/list | Synthetic S3-key hierarchy; online checks and bounded listing | Authority namespace, stable volume/node IDs and revision-aware listing |
| Read/seek | Revision-pinned GET or authorized cached snapshot | Immutable published payload reference; pinned readers |
| File create/write/truncate | Opt-in guarded file creation and conditional full-object writes | Stage/upload/verify a new immutable blob, then metadata CAS; publication pending |
| Remote flush | Verified S3 operation plus local acknowledgment | Verified blob plus committed reference and local acknowledgment; pending |
| Directory create | Unsupported on main; marker spike archived | One metadata transaction; prototype implemented, native integration pending |
| File/subtree rename | Unsupported | Atomic namespace move; prototype supports no replacement; desktop replace-save pending |
| Unlink | Unsupported | Atomic metadata tombstone; prototype empty removal only, open-reader retention pending |
| Cross-volume rename | Cross-device error | Cross-device error; separate copy operation |
| Close/release | Release handle/cache references; not a remote durability promise | Same; preserve outstanding staged work |
| chmod/chown, ACLs, xattrs, links, streams | Fixed owner policy; mutation unsupported | Explicit subset needs specification/acceptance |
| Distributed byte-range locks / writable databases | Unsupported | Deferred; atomic namespace does not establish these semantics |
| mmap/executables/general POSIX/NTFS equivalence | Not advertised | Separate workload/platform acceptance required |

Office-style applications often save a temporary file and rename it over the
original. Consequently, writable bytes alone are insufficient for a general
desktop-drive claim. Test real save/replace workflows during managed native acceptance; document
unsupported applications until their required semantics exist.

## Names and directories

The managed prototype stores byte-exact Unicode names and rejects structural
path traversal, without deriving physical S3 keys from them. Before managed
native mounts, freeze a portable comparison policy and verify Windows collisions.
Directory nodes are real authority records; no S3 folder marker is needed.
The following key-projection rules describe the existing **direct** mode.


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

Managed publication needs two independent durable outcomes: immutable object
verification and a committed metadata-reference CAS. SQL and S3 do not form one
transaction. A stale metadata writer cannot overwrite the published reference;
its uploaded blob and local edit stay recoverable. Namespace operation IDs
resolve unknown commits. Leases alone never authorize replay or overwrite.
The next paragraphs describe existing direct-object reads and writes.


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


In the current combined read-only chunk-capable draft, distinguish the stable
logical authority node ID from a content-version inode. After a publication,
applications resolve the path again to obtain the replacement inode; stale
inode opens fail explicitly. Existing descriptors retain pinned bytes and
attributes, and a rename with unchanged content preserves the version identity.
The engine regression now checks both distinct version inodes and unchanged
logical IDs. This draft still requires its own real platform gates; it does not
change the writable lifecycle identity contract.
