-- Existing file discriminator remains 'empty-file' on disk; a non-NULL payload
-- distinguishes a published immutable file from an unbacked empty prototype.
ALTER TABLE namespace_nodes ADD COLUMN payload BLOB
 CHECK(payload IS NULL OR kind = 'empty-file');
CREATE TABLE namespace_stores (
 store_id TEXT PRIMARY KEY,
 descriptor BLOB NOT NULL
) STRICT;
CREATE TABLE namespace_uploads (
 reservation_id TEXT PRIMARY KEY,
 descriptor BLOB NOT NULL,
 publication BLOB
) STRICT;
-- Client-local protocol records. Never mix this table's populated contents with
-- an initialized authority volume. Immutable staged bytes stay in existing tables.
CREATE TABLE managed_publications (
 operation_id TEXT PRIMARY KEY,
 input BLOB NOT NULL,
 generation_id INTEGER REFERENCES staged_generations(generation_id),
 reservation BLOB,
 publication_request BLOB,
 receipt BLOB,
 state_hash BLOB NOT NULL CHECK(length(state_hash) = 32)
) STRICT;
