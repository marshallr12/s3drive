CREATE TABLE windows_mount_sessions (
 session_id TEXT PRIMARY KEY CHECK(length(session_id)=32),
 volume_id TEXT NOT NULL CHECK(length(volume_id)=32),
 runtime_id TEXT NOT NULL CHECK(length(runtime_id)=32),
 owner_id TEXT NOT NULL CHECK(length(owner_id)=32),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32),
 UNIQUE(volume_id,runtime_id)
) STRICT;
CREATE TABLE windows_session_recoveries (
 recovery_id TEXT PRIMARY KEY CHECK(length(recovery_id)=32),
 volume_id TEXT NOT NULL,
 node_id TEXT NOT NULL,
 input BLOB NOT NULL CHECK(length(input)<=65536),
 proof BLOB CHECK(proof IS NULL OR length(proof)<=65536),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE UNIQUE INDEX windows_pending_session_recovery
 ON windows_session_recoveries(volume_id,node_id) WHERE proof IS NULL;
CREATE TABLE recovered_windows_opens (
 open_id TEXT PRIMARY KEY REFERENCES windows_open_journal(open_id),
 recovery_id TEXT NOT NULL REFERENCES windows_session_recoveries(recovery_id)
) STRICT;
