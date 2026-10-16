-- P0 rows had only an alias, with no credential binding. Preserve them for
-- inspection but never promote them into the active cache namespace.
ALTER TABLE cached_objects RENAME TO legacy_cached_objects_v1;

CREATE TABLE source_bindings (
    source_id BLOB NOT NULL PRIMARY KEY CHECK (length(source_id) = 32),
    config_fingerprint TEXT NOT NULL CHECK (
        length(config_fingerprint) = 64
        AND config_fingerprint NOT GLOB '*[^0-9a-f]*'
    )
) STRICT;
