-- Immutable local patches. Records/operations reference these by raw SHA-256.
-- Existing root BLOBs, hashes and IDs are not rewritten.
CREATE TABLE sparse_view_blocks (
    block BLOB PRIMARY KEY NOT NULL CHECK(length(block)=32),
    data BLOB NOT NULL CHECK(length(data) BETWEEN 1 AND 1048576)
) STRICT;
