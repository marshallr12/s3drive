-- Prepared intent is not permission to send. Only a durable claim can issue
-- the single in-memory send permit; uncertain claims must never be replayed.
CREATE TABLE remote_intents (
    generation_id INTEGER NOT NULL PRIMARY KEY REFERENCES staged_generations(generation_id),
    operation_id BLOB NOT NULL UNIQUE CHECK(length(operation_id)=32),
    source_id BLOB NOT NULL REFERENCES source_bindings(source_id),
    config_fingerprint TEXT NOT NULL CHECK(length(config_fingerprint)=64),
    object_key TEXT NOT NULL CHECK(length(CAST(object_key AS BLOB)) BETWEEN 1 AND 1024),
    manifest_hash BLOB NOT NULL CHECK(length(manifest_hash)=32),
    payload_sha256 BLOB NOT NULL CHECK(length(payload_sha256)=32),
    size_bytes INTEGER NOT NULL CHECK(size_bytes BETWEEN 0 AND 8388608),
    guard_etag TEXT CHECK(guard_etag IS NULL OR length(CAST(guard_etag AS BLOB)) BETWEEN 1 AND 8192),
    anchor_operation BLOB REFERENCES remote_intents(operation_id),
    anchor_receipt BLOB,
    intent_hash BLOB NOT NULL CHECK(length(intent_hash)=32),
    phase TEXT NOT NULL CHECK(phase IN ('prepared','in-flight','uncertain','verified','rejected','cancelled')),
    reason TEXT NOT NULL CHECK(reason IN ('none','interrupted','transport','timeout','response','observation')),
    observation TEXT NOT NULL CHECK(observation IN ('none','absent','other')),
    rejection_status INTEGER,
    remote_etag TEXT CHECK(remote_etag IS NULL OR length(CAST(remote_etag AS BLOB)) BETWEEN 1 AND 8192),
    remote_version TEXT CHECK(remote_version IS NULL OR length(CAST(remote_version AS BLOB)) BETWEEN 1 AND 8192),
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32),
    CHECK((anchor_operation IS NULL AND anchor_receipt IS NULL) OR
          (anchor_operation IS NOT NULL AND anchor_receipt IS NOT NULL AND length(anchor_receipt)=32)),
    CHECK((phase IN ('prepared','in-flight','cancelled') AND reason='none' AND observation='none' AND rejection_status IS NULL AND remote_etag IS NULL AND remote_version IS NULL) OR
          (phase='uncertain' AND reason<>'none' AND rejection_status IS NULL AND remote_etag IS NULL AND remote_version IS NULL) OR
          (phase='rejected' AND reason='none' AND observation='none' AND rejection_status IS NOT NULL AND rejection_status IN (409,412) AND remote_etag IS NULL AND remote_version IS NULL) OR
          (phase='verified' AND reason='none' AND observation='none' AND rejection_status IS NULL AND remote_etag IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX remote_target_blocked ON remote_intents(config_fingerprint,object_key)
WHERE phase IN ('prepared','in-flight','uncertain','rejected');
-- Never silently forget an attempt that may have reached the service.
CREATE TABLE retained_remote_intents (
    operation_id BLOB NOT NULL PRIMARY KEY REFERENCES remote_intents(operation_id)
) STRICT;
