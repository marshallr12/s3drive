-- Explicit retry-history safety opt-in; no history is removed.
CREATE TABLE sparse_retry_mode (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    format INTEGER NOT NULL CHECK(format=1),
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
