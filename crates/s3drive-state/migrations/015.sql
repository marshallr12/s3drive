CREATE TABLE managed_rejections (
 operation_id TEXT PRIMARY KEY REFERENCES managed_publications(operation_id),
 rejection BLOB NOT NULL CHECK(length(rejection)<=32768),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE logical_conflicts (
 edit_id TEXT PRIMARY KEY REFERENCES logical_edits(operation_id),
 publication_id TEXT NOT NULL UNIQUE REFERENCES managed_publications(operation_id),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
