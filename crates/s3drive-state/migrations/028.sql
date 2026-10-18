-- Exact historical source acknowledgment; never a physical remote flush.
CREATE TABLE chunk_source_role (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    settled INTEGER NOT NULL CHECK(settled BETWEEN 1 AND 256),
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE chunk_source_settlements (
    plan_id BLOB PRIMARY KEY NOT NULL REFERENCES chunk_publications(plan_id) CHECK(length(plan_id)=32),
    snapshot_id INTEGER NOT NULL UNIQUE REFERENCES staged_generations(generation_id),
    record BLOB NOT NULL CHECK(length(record) BETWEEN 1 AND 4096),
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
