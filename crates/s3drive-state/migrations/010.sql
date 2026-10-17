-- Local managed file identity is distinct from every generated S3 object key.
CREATE TABLE logical_files (
 volume TEXT NOT NULL,
 node TEXT NOT NULL,
 input BLOB NOT NULL CHECK(length(input)<=32768),
 settled_generation INTEGER REFERENCES staged_generations(generation_id),
 settled_publication TEXT,
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32),
 PRIMARY KEY(volume,node),
 CHECK((settled_generation IS NULL)=(settled_publication IS NULL))
) STRICT;
CREATE TABLE logical_edits (
 operation_id TEXT PRIMARY KEY,
 volume TEXT NOT NULL,
 node TEXT NOT NULL,
 input BLOB NOT NULL CHECK(length(input)<=32768),
 generation_id INTEGER UNIQUE REFERENCES staged_generations(generation_id),
 publication BLOB CHECK(publication IS NULL OR length(publication)<=32768),
 receipt BLOB CHECK(receipt IS NULL OR length(receipt)<=32768),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32),
 FOREIGN KEY(volume,node) REFERENCES logical_files(volume,node),
 CHECK(receipt IS NULL OR publication IS NOT NULL)
) STRICT;

CREATE TABLE logical_publication_links (
 operation_id TEXT PRIMARY KEY CHECK(length(operation_id)=32),
 edit_id TEXT NOT NULL REFERENCES logical_edits(operation_id)
) STRICT;
