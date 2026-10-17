CREATE TABLE windows_open_journal (
 open_id TEXT PRIMARY KEY CHECK(length(open_id)=32),
 owner_id TEXT NOT NULL CHECK(length(owner_id)=32),
 input BLOB NOT NULL CHECK(length(input)<=16384),
 event_count INTEGER NOT NULL CHECK(event_count>=0 AND event_count<=4096),
 head_hash BLOB CHECK(head_hash IS NULL OR length(head_hash)=32),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE retained_windows_opens (
 open_id TEXT PRIMARY KEY CHECK(length(open_id)=32),
 input_hash BLOB NOT NULL CHECK(length(input_hash)=32)
) STRICT;
CREATE TABLE windows_open_events (
 event_id TEXT PRIMARY KEY CHECK(length(event_id)=32),
 open_id TEXT NOT NULL REFERENCES windows_open_journal(open_id),
 ordinal INTEGER NOT NULL CHECK(ordinal>0 AND ordinal<=4096),
 action BLOB NOT NULL CHECK(length(action)<=16384),
 previous_hash BLOB CHECK(previous_hash IS NULL OR length(previous_hash)=32),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32),
 UNIQUE(open_id,ordinal)
) STRICT;
