//! Test-only fixture setup: remove current sparse tables before deliberately
//! constructing an older schema. Production migrations keep collision failures.
pub(crate) fn legacy_sql(sql: &str) -> String {
    format!(
        "DROP TABLE sparse_preparation_objects; DROP TABLE sparse_preparations; DROP TABLE sparse_preparation_blobs; DROP TABLE sparse_view_blocks; DROP TABLE sparse_view_operations; DROP TABLE sparse_views; DROP TABLE sparse_view_files; DROP TABLE sparse_view_role; {sql}"
    )
}

/// Preserve populated old sparse views while removing only newer preparation DDL.
pub(crate) fn before_preparation(sql: &str) -> String {
    format!(
        "DROP TABLE sparse_preparation_objects; DROP TABLE sparse_preparations; DROP TABLE sparse_preparation_blobs; {sql}"
    )
}
