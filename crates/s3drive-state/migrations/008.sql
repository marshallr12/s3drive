-- Separate from local source bindings: these IDs are authority-wide identities.
-- One volume per authority DB. No payload references exist in this phase.
CREATE TABLE namespace_volume (
 singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
 volume_id TEXT NOT NULL UNIQUE CHECK(length(volume_id) = 32),
 sequence INTEGER NOT NULL CHECK(sequence >= 0),
 name_policy TEXT NOT NULL CHECK(name_policy = 'exact-utf8-v1')
) STRICT;
CREATE TABLE namespace_nodes (
 node_id TEXT PRIMARY KEY CHECK(length(node_id) = 32),
 parent_id TEXT REFERENCES namespace_nodes(node_id),
 name TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('directory', 'empty-file')),
 revision INTEGER NOT NULL CHECK(revision >= 0),
 deleted INTEGER NOT NULL CHECK(deleted IN (0, 1)),
 CHECK((parent_id IS NULL AND name = '' AND kind = 'directory' AND deleted = 0)
    OR (parent_id IS NOT NULL AND name != ''))
) STRICT;
CREATE UNIQUE INDEX namespace_names ON namespace_nodes(parent_id, name) WHERE deleted = 0;
CREATE UNIQUE INDEX namespace_root ON namespace_nodes((1)) WHERE parent_id IS NULL;
CREATE TABLE namespace_operations (
 operation_id TEXT PRIMARY KEY CHECK(length(operation_id) = 32),
 request BLOB NOT NULL,
 outcome BLOB NOT NULL
) STRICT;
