//! Test-only fixture setup: remove current sparse tables before deliberately
//! constructing an older schema. Production migrations keep collision failures.
pub(crate) fn legacy_sql(sql: &str) -> String {
    format!(
        "DROP TABLE sparse_view_operations; DROP TABLE sparse_views; DROP TABLE sparse_view_files; DROP TABLE sparse_view_role; {sql}"
    )
}
