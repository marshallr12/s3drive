ALTER TABLE namespace_removals ADD COLUMN mode TEXT NOT NULL DEFAULT 'pending' CHECK(mode IN ('pending','deferred'));
ALTER TABLE namespace_removals ADD COLUMN event_id TEXT CHECK(event_id IS NULL OR length(event_id)=32);
CREATE TABLE namespace_removal_events (
 node_id TEXT NOT NULL REFERENCES namespace_nodes(node_id),
 operation_id TEXT NOT NULL REFERENCES namespace_operations(operation_id) DEFERRABLE INITIALLY DEFERRED,
 intent_id TEXT NOT NULL CHECK(length(intent_id)=32),
 owner_id TEXT NOT NULL CHECK(length(owner_id)=32),
 mode TEXT NOT NULL CHECK(mode IN ('pending','deferred','cancelled','committed')),
 sequence INTEGER NOT NULL CHECK(sequence>0),
 PRIMARY KEY(node_id,operation_id),
 UNIQUE(node_id,sequence)
) STRICT;
CREATE INDEX namespace_removal_event_operations ON namespace_removal_events(operation_id);
