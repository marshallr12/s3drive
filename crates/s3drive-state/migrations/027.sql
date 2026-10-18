-- An isolated chunk publication client never becomes a legacy/native client.
-- The retained count detects a lost publication row, including completed rows.
CREATE TABLE chunk_client_role (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 admitted INTEGER NOT NULL CHECK(admitted BETWEEN 1 AND 256),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE chunk_publications (
 plan_id BLOB PRIMARY KEY NOT NULL REFERENCES chunk_plans(plan_id) CHECK(length(plan_id)=32),
 reserve_id TEXT NOT NULL UNIQUE CHECK(length(reserve_id)=32),
 publish_id TEXT NOT NULL UNIQUE CHECK(length(publish_id)=32),
 record BLOB NOT NULL CHECK(length(record) BETWEEN 1 AND 65536),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32),
 CHECK(reserve_id<>publish_id)
) STRICT;
