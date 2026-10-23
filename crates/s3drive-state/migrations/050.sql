-- Explicit permitted source cut edges. Empty until an authorized retirement.
CREATE TABLE sparse_retention_role (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    owner BLOB NOT NULL CHECK(length(owner)=32),
    entries INTEGER NOT NULL CHECK(entries BETWEEN 1 AND 256),
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE sparse_retention_anchors (
    target BLOB PRIMARY KEY NOT NULL CHECK(length(target)=32),
    view_id BLOB UNIQUE NOT NULL REFERENCES sparse_views(view_id) CHECK(length(view_id)=32),
    generation_id INTEGER UNIQUE NOT NULL REFERENCES staged_generations(generation_id),
    record BLOB NOT NULL CHECK(length(record) BETWEEN 1 AND 2097152),
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
