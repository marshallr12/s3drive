-- Exact-view mixed preparation. These records are offline intent only, not
-- authority authentication, upload receipts or logical-file clean settlement.
CREATE TABLE sparse_preparations (
    plan_id BLOB PRIMARY KEY NOT NULL CHECK(length(plan_id)=32),
    operation BLOB UNIQUE NOT NULL CHECK(length(operation)=32),
    view_id BLOB NOT NULL REFERENCES sparse_views(view_id) CHECK(length(view_id)=32),
    input BLOB NOT NULL CHECK(length(input) BETWEEN 1 AND 65536),
    state BLOB NOT NULL CHECK(length(state) BETWEEN 1 AND 4096),
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE sparse_preparation_objects (
    plan_id BLOB NOT NULL REFERENCES sparse_preparations(plan_id) CHECK(length(plan_id)=32),
    level INTEGER NOT NULL CHECK(level BETWEEN -1 AND 2),
    slot INTEGER NOT NULL CHECK(slot BETWEEN 0 AND 65535),
    record BLOB NOT NULL CHECK(length(record) BETWEEN 1 AND 65536),
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32),
    PRIMARY KEY(plan_id,level,slot)
) STRICT;
CREATE TABLE sparse_preparation_blobs (
    blob BLOB PRIMARY KEY NOT NULL CHECK(length(blob)=32),
    data BLOB NOT NULL CHECK(length(data) BETWEEN 1 AND 1048576)
) STRICT;
