-- Offline expected objects, never remote receipts or send permits.
CREATE TABLE chunk_plans (
 plan_id BLOB NOT NULL PRIMARY KEY CHECK(length(plan_id)=32),
 scope_hash BLOB NOT NULL UNIQUE CHECK(length(scope_hash)=32),
 input BLOB NOT NULL CHECK(length(input) BETWEEN 1 AND 4096),
 input_hash BLOB NOT NULL CHECK(length(input_hash)=32),
 reserved_objects INTEGER NOT NULL CHECK(reserved_objects BETWEEN 1 AND 66053),
 generation_id INTEGER UNIQUE REFERENCES staged_generations(generation_id),
 manifest_hash BLOB CHECK(manifest_hash IS NULL OR length(manifest_hash)=32),
 next_chunk INTEGER NOT NULL CHECK(next_chunk>=0),
 build_level INTEGER NOT NULL CHECK(build_level BETWEEN -1 AND 7),
 next_slot INTEGER NOT NULL CHECK(next_slot>=0),
 root BLOB CHECK(root IS NULL OR length(root)=120),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32),
 CHECK((generation_id IS NULL)=(manifest_hash IS NULL))
) STRICT;
CREATE TABLE chunk_objects (
 plan_id BLOB NOT NULL REFERENCES chunk_plans(plan_id),
 level INTEGER NOT NULL CHECK(level BETWEEN -1 AND 7),
 slot INTEGER NOT NULL CHECK(slot>=0),
 ordinal BLOB NOT NULL CHECK(length(ordinal)=8),
 offset_bytes INTEGER NOT NULL CHECK(offset_bytes>=0),
 length_bytes INTEGER NOT NULL CHECK(length_bytes>=0),
 hole INTEGER NOT NULL CHECK(hole IN (0,1)),
 sha256 BLOB NOT NULL CHECK(length(sha256)=32),
 body BLOB CHECK(body IS NULL OR length(body) BETWEEN 64 AND 10432),
 record_hash BLOB NOT NULL CHECK(length(record_hash)=32),
 PRIMARY KEY(plan_id,level,slot),
 UNIQUE(plan_id,level,ordinal),
 CHECK((level=-1 AND body IS NULL) OR (level>=0 AND body IS NOT NULL AND hole=0))
) STRICT;
