-- Explicit same-owner chunk / namespace / per-open composition.
CREATE TABLE chunk_mount_role (
 singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
 record BLOB NOT NULL CHECK(length(record) BETWEEN 1 AND 65536),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
