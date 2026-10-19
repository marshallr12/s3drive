-- Durable private empty-manifest creation intent; no provider permission.
CREATE TABLE manifest_creation_role (
 singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
 records INTEGER NOT NULL CHECK(records BETWEEN 1 AND 128),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE manifest_creations (
 operation TEXT PRIMARY KEY NOT NULL CHECK(length(operation)=32),
 record BLOB NOT NULL CHECK(length(record) BETWEEN 1 AND 65536),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
