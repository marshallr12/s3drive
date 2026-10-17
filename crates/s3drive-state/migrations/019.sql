-- Retirement frees only live admission capacity. All original rows, anchors,
-- callbacks and namespace proofs remain in place and cannot be reused.
CREATE TABLE retired_windows_opens (
 open_id TEXT PRIMARY KEY REFERENCES windows_open_journal(open_id),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
