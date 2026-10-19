-- Explicit source-operation admission mode. Existing view/request/root bytes stay unchanged.
CREATE TABLE sparse_scoped_mode (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    format INTEGER NOT NULL CHECK(format=1),
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
