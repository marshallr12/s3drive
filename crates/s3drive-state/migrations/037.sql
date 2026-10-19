-- Transient read retention only. No source-history or payload deletion.
CREATE TABLE source_read_pin_session (
 singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
 format INTEGER NOT NULL CHECK(format=1),
 owner TEXT UNIQUE NOT NULL CHECK(length(owner)=32),
 issued INTEGER NOT NULL CHECK(issued>=0),
 active INTEGER NOT NULL CHECK(active BETWEEN 0 AND 256),
 entries_hash BLOB NOT NULL CHECK(length(entries_hash)=32),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE source_read_pins (
 pin_id BLOB PRIMARY KEY NOT NULL CHECK(length(pin_id)=32),
 owner TEXT NOT NULL REFERENCES source_read_pin_session(owner) DEFERRABLE INITIALLY DEFERRED,
 sequence INTEGER UNIQUE NOT NULL CHECK(sequence>0),
 view_id BLOB NOT NULL REFERENCES sparse_views(view_id) CHECK(length(view_id)=32),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
