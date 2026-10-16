-- Local, rebuildable read metadata only. No write journal exists yet.
CREATE TABLE cached_objects (
    source_alias TEXT NOT NULL,
    object_key TEXT NOT NULL,
    etag TEXT,
    version_id TEXT,
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    PRIMARY KEY (source_alias, object_key)
) STRICT;
