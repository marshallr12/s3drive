-- Intent keys bind complete edit plans to one generation across lost replies.
-- No remote execution or acknowledgement state is introduced here.
CREATE TABLE staged_operations (
    operation_key BLOB NOT NULL PRIMARY KEY CHECK (length(operation_key)=32),
    generation_id INTEGER NOT NULL UNIQUE REFERENCES staged_generations(generation_id) ON DELETE CASCADE
) STRICT;
