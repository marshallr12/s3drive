-- Offline commitments only: neither a frozen input nor a ready hash plan
-- acquires target ownership or grants any network permission.
CREATE TABLE multipart_plans (
    plan_id BLOB NOT NULL PRIMARY KEY CHECK(length(plan_id)=32),
    generation_id INTEGER NOT NULL UNIQUE REFERENCES staged_generations(generation_id),
    input BLOB NOT NULL CHECK(length(input) BETWEEN 1 AND 32768),
    input_hash BLOB NOT NULL CHECK(length(input_hash)=32),
    part_count INTEGER NOT NULL CHECK(part_count BETWEEN 1 AND 10000),
    phase TEXT NOT NULL CHECK(phase IN ('frozen','ready')),
    payload_sha256 BLOB CHECK(payload_sha256 IS NULL OR length(payload_sha256)=32),
    plan_hash BLOB CHECK(plan_hash IS NULL OR length(plan_hash)=32),
    CHECK((phase='frozen' AND payload_sha256 IS NULL AND plan_hash IS NULL) OR
          (phase='ready' AND payload_sha256 IS NOT NULL AND plan_hash IS NOT NULL))
) STRICT;
CREATE TABLE multipart_parts (
    plan_id BLOB NOT NULL REFERENCES multipart_plans(plan_id),
    part_number INTEGER NOT NULL CHECK(part_number BETWEEN 1 AND 10000),
    offset_bytes INTEGER NOT NULL CHECK(offset_bytes>=0),
    length_bytes INTEGER NOT NULL CHECK(length_bytes>0),
    sha256 BLOB NOT NULL CHECK(length(sha256)=32),
    PRIMARY KEY(plan_id,part_number)
) STRICT;
