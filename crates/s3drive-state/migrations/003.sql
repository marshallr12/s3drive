-- Clean whole-file snapshots only. Never store dirty/staged writes here.
-- Bytes and their independent integrity checksum commit in the same transaction.
CREATE TABLE clean_blobs (
    cache_key BLOB NOT NULL PRIMARY KEY CHECK (length(cache_key) = 32),
    source_id BLOB NOT NULL REFERENCES source_bindings(source_id),
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0 AND size_bytes <= 1048576),
    checksum BLOB NOT NULL CHECK (length(checksum) = 32),
    data BLOB NOT NULL CHECK (length(data) = size_bytes),
    last_used INTEGER NOT NULL CHECK (last_used >= 0)
) STRICT;
CREATE INDEX clean_blobs_lru ON clean_blobs(last_used, cache_key);
