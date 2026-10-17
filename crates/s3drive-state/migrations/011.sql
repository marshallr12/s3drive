ALTER TABLE logical_files ADD COLUMN observed_base BLOB CHECK(observed_base IS NULL OR length(observed_base)<=32768);
