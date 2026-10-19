-- Permit multiple immutable versions of one file. No mutation API is enabled by
-- this migration. The caller validates schema29 and refuses attached triggers.
-- Keep foreign_keys enabled: temporarily empty the cyclic references before
-- rebuilding, then restore the exact BLOBs in the same transaction.
PRAGMA defer_foreign_keys=ON;
CREATE TEMP TABLE sparse_files_copy_v30 AS
    SELECT target,binding,head,state_hash FROM sparse_view_files;
CREATE TEMP TABLE sparse_views_copy_v30 AS
    SELECT view_id,target,record,state_hash FROM sparse_views;
CREATE TEMP TABLE sparse_operations_copy_v30 AS
    SELECT operation,view_id,request,state_hash FROM sparse_view_operations;
DELETE FROM sparse_view_operations;
DELETE FROM sparse_view_files;
DELETE FROM sparse_views;
DROP TABLE sparse_views;
CREATE TABLE sparse_views (
    view_id BLOB PRIMARY KEY NOT NULL CHECK(length(view_id)=32),
    target BLOB NOT NULL REFERENCES sparse_view_files(target),
    record BLOB NOT NULL CHECK(length(record) BETWEEN 1 AND 1048576),
    state_hash BLOB NOT NULL CHECK(length(state_hash)=32)
) STRICT;
INSERT INTO sparse_view_files SELECT * FROM sparse_files_copy_v30;
INSERT INTO sparse_views SELECT * FROM sparse_views_copy_v30;
INSERT INTO sparse_view_operations SELECT * FROM sparse_operations_copy_v30;
DROP TABLE temp.sparse_operations_copy_v30;
DROP TABLE temp.sparse_views_copy_v30;
DROP TABLE temp.sparse_files_copy_v30;
