-- Immutable initial sparse views. No provider authorization or settlement.
CREATE TABLE sparse_view_role (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    format INTEGER NOT NULL CHECK(format=1),
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE sparse_view_files (
    target BLOB PRIMARY KEY NOT NULL CHECK(length(target)=32),
    binding BLOB NOT NULL CHECK(length(binding) BETWEEN 1 AND 4096),
    head BLOB NOT NULL UNIQUE CHECK(length(head)=32)
        REFERENCES sparse_views(view_id) DEFERRABLE INITIALLY DEFERRED,
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE sparse_views (
    view_id BLOB PRIMARY KEY NOT NULL CHECK(length(view_id)=32),
    target BLOB NOT NULL UNIQUE REFERENCES sparse_view_files(target),
    record BLOB NOT NULL CHECK(length(record) BETWEEN 1 AND 1048576),
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE sparse_view_operations (
    operation BLOB PRIMARY KEY NOT NULL CHECK(length(operation)=32),
    view_id BLOB NOT NULL UNIQUE REFERENCES sparse_views(view_id),
    request BLOB NOT NULL CHECK(length(request) BETWEEN 1 AND 4096),
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
