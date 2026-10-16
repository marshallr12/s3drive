-- Non-evictable staged data. clean_blobs eviction must never touch these tables.
CREATE TABLE staged_generations (
    generation_id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id BLOB NOT NULL REFERENCES source_bindings(source_id),
    object_key TEXT NOT NULL CHECK (length(CAST(object_key AS BLOB)) BETWEEN 1 AND 1024),
    parent_id INTEGER REFERENCES staged_generations(generation_id) CHECK (parent_id IS NULL OR parent_id < generation_id),
    base_absent INTEGER NOT NULL CHECK (base_absent IN (0,1)),
    base_etag TEXT CHECK (base_etag IS NULL OR length(CAST(base_etag AS BLOB)) BETWEEN 1 AND 8192),
    base_version TEXT CHECK (base_version IS NULL OR length(CAST(base_version AS BLOB)) BETWEEN 1 AND 8192),
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    filled_bytes INTEGER NOT NULL DEFAULT 0 CHECK (filled_bytes BETWEEN 0 AND size_bytes),
    extent_count INTEGER NOT NULL DEFAULT 0 CHECK (extent_count >= 0),
    phase TEXT NOT NULL DEFAULT 'building' CHECK (phase IN ('building','sealed')),
    manifest_hash BLOB,
    CHECK ((base_absent=1 AND base_etag IS NULL AND base_version IS NULL) OR
           (base_absent=0 AND (base_etag IS NOT NULL OR base_version IS NOT NULL))),
    CHECK ((phase='building' AND manifest_hash IS NULL) OR
           (phase='sealed' AND manifest_hash IS NOT NULL AND length(manifest_hash)=32 AND filled_bytes=size_bytes))
) STRICT;
CREATE INDEX staged_generations_target ON staged_generations(source_id,object_key,phase,generation_id);
CREATE TABLE staged_chunks (
    chunk_hash BLOB NOT NULL PRIMARY KEY CHECK (length(chunk_hash)=32),
    data BLOB NOT NULL CHECK (length(data) BETWEEN 1 AND 1048576),
    created_by INTEGER NOT NULL
) STRICT;
CREATE TABLE retained_staged_chunks (
    chunk_hash BLOB NOT NULL PRIMARY KEY REFERENCES staged_chunks(chunk_hash)
) STRICT;
CREATE TABLE staged_extents (
    generation_id INTEGER NOT NULL REFERENCES staged_generations(generation_id) ON DELETE CASCADE,
    offset_bytes INTEGER NOT NULL CHECK (offset_bytes >= 0),
    length_bytes INTEGER NOT NULL CHECK (length_bytes > 0),
    chunk_hash BLOB REFERENCES staged_chunks(chunk_hash) DEFERRABLE INITIALLY DEFERRED,
    PRIMARY KEY (generation_id,offset_bytes)
) STRICT;
CREATE INDEX staged_extents_chunks ON staged_extents(chunk_hash);
CREATE TABLE staged_heads (
    source_id BLOB NOT NULL REFERENCES source_bindings(source_id),
    object_key TEXT NOT NULL,
    generation_id INTEGER NOT NULL REFERENCES staged_generations(generation_id),
    PRIMARY KEY (source_id,object_key)
) STRICT;
CREATE TABLE write_journal (
    generation_id INTEGER NOT NULL PRIMARY KEY REFERENCES staged_generations(generation_id),
    status TEXT NOT NULL CHECK (status='pending')
) STRICT;
