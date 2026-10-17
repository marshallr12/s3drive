-- Keep write_journal as the retained sealed-intent marker. Acknowledgment does
-- not delete payloads, manifests, attempt history or newer pending generations.
CREATE TABLE remote_flushes (
    operation_id BLOB NOT NULL PRIMARY KEY REFERENCES remote_intents(operation_id),
    generation_id INTEGER NOT NULL UNIQUE REFERENCES staged_generations(generation_id),
    covered_generations INTEGER NOT NULL CHECK(covered_generations BETWEEN 1 AND 65536),
    receipt_hash BLOB NOT NULL CHECK(length(receipt_hash)=32)
) STRICT;
CREATE TABLE write_settlements (
    generation_id INTEGER NOT NULL PRIMARY KEY REFERENCES staged_generations(generation_id),
    covering_operation BLOB NOT NULL REFERENCES remote_flushes(operation_id),
    manifest_hash BLOB NOT NULL CHECK(length(manifest_hash)=32),
    settlement_hash BLOB NOT NULL CHECK(length(settlement_hash)=32)
) STRICT;
