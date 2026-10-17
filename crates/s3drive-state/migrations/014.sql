-- Client-local operational identity. The separate anchor detects missing or
-- independently changed identity rows; neither record is an authentication key.
CREATE TABLE client_identity (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 owner_id TEXT NOT NULL CHECK(length(owner_id)=32),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE retained_client_identity (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
