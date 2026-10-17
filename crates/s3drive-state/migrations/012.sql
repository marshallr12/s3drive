-- Existing exact-name volumes are preserved, never converted implicitly.
ALTER TABLE namespace_volume RENAME TO namespace_volume_v11;
CREATE TABLE namespace_volume (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 volume_id TEXT NOT NULL UNIQUE CHECK(length(volume_id)=32),
 sequence INTEGER NOT NULL CHECK(sequence>=0),
 name_policy TEXT NOT NULL CHECK(name_policy IN ('exact-utf8-v1','portable-collision-v1'))
) STRICT;
INSERT INTO namespace_volume SELECT * FROM namespace_volume_v11;
DROP TABLE namespace_volume_v11;
ALTER TABLE namespace_nodes ADD COLUMN placement BLOB CHECK(placement IS NULL OR length(placement)<=2048);
ALTER TABLE namespace_nodes ADD COLUMN name_key TEXT CHECK(name_key IS NULL OR length(CAST(name_key AS BLOB))<=4096);
CREATE UNIQUE INDEX namespace_portable_names ON namespace_nodes(parent_id,name_key) WHERE deleted=0 AND name_key IS NOT NULL;
CREATE TABLE namespace_intents (
 operation_id TEXT PRIMARY KEY CHECK(length(operation_id)=32),
 input BLOB NOT NULL CHECK(length(input)<=8192),
 outcome BLOB CHECK(outcome IS NULL OR length(outcome)<=65536),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
