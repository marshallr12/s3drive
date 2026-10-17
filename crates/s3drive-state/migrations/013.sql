-- Node-wide reservations for reversible deletion disposition. Payload bytes,
-- names and all prior serialized receipts/bindings remain unchanged.
CREATE TABLE namespace_removals (
 node_id TEXT PRIMARY KEY REFERENCES namespace_nodes(node_id) CHECK(length(node_id)=32),
 intent_id TEXT NOT NULL UNIQUE CHECK(length(intent_id)=32),
 owner_id TEXT NOT NULL CHECK(length(owner_id)=32)
) STRICT;
