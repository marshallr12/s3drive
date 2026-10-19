-- Mixed-root historical semantic checks and exact saved authority requests.
-- No source clean settlement or payload reclamation is implied.
CREATE TABLE sparse_publication_role (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 admitted INTEGER NOT NULL CHECK(admitted BETWEEN 0 AND 256),
 checked INTEGER NOT NULL CHECK(checked BETWEEN 0 AND 262144),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE sparse_publications (
 plan_id BLOB PRIMARY KEY NOT NULL REFERENCES sparse_preparations(plan_id) CHECK(length(plan_id)=32),
 reserve_id TEXT UNIQUE NOT NULL CHECK(length(reserve_id)=32),
 publish_id TEXT UNIQUE NOT NULL CHECK(length(publish_id)=32),
 record BLOB NOT NULL CHECK(length(record) BETWEEN 1 AND 65536),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
CREATE TABLE sparse_publication_checks (
 plan_id BLOB NOT NULL REFERENCES sparse_preparations(plan_id) CHECK(length(plan_id)=32),
 slot INTEGER NOT NULL CHECK(slot BETWEEN 0 AND 65535),
 record BLOB NOT NULL CHECK(length(record) BETWEEN 1 AND 2048),
 state_hash BLOB NOT NULL CHECK(length(state_hash)=32),
 PRIMARY KEY(plan_id,slot)
) STRICT;
