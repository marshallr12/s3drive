//! Test-only fixture setup: remove current sparse tables before deliberately
//! constructing an older schema. Production migrations keep collision failures.
pub(crate) fn legacy_sql(sql: &str) -> String {
    format!(
        "DROP TABLE chunk_mount_role; DROP TABLE manifest_creations; DROP TABLE manifest_creation_role; DROP TABLE source_read_pins; DROP TABLE source_read_pin_session; DROP TABLE sparse_scoped_mode; DROP TABLE sparse_publication_checks; DROP TABLE sparse_publications; DROP TABLE sparse_publication_role; DROP TABLE sparse_preparation_objects; DROP TABLE sparse_preparations; DROP TABLE sparse_preparation_blobs; DROP TABLE sparse_view_blocks; DROP TABLE sparse_view_operations; DROP TABLE sparse_views; DROP TABLE sparse_view_files; DROP TABLE sparse_view_role; {sql}"
    )
}

/// Preserve populated old sparse views while removing only newer preparation DDL.
pub(crate) fn before_preparation(sql: &str) -> String {
    format!(
        "DROP TABLE chunk_mount_role; DROP TABLE manifest_creations; DROP TABLE manifest_creation_role; DROP TABLE source_read_pins; DROP TABLE source_read_pin_session; DROP TABLE sparse_scoped_mode; DROP TABLE sparse_publication_checks; DROP TABLE sparse_publications; DROP TABLE sparse_publication_role; DROP TABLE sparse_preparation_objects; DROP TABLE sparse_preparations; DROP TABLE sparse_preparation_blobs; {sql}"
    )
}

/// Preserve schema32 views/preparation/staging during additive publication migration.
pub(crate) fn before_publication(sql: &str) -> String {
    format!(
        "DROP TABLE chunk_mount_role; DROP TABLE manifest_creations; DROP TABLE manifest_creation_role; DROP TABLE source_read_pins; DROP TABLE source_read_pin_session; DROP TABLE sparse_scoped_mode; DROP TABLE sparse_publication_checks; DROP TABLE sparse_publications; DROP TABLE sparse_publication_role; {sql}"
    )
}

/// Test-only demotion from the current schema, preserving all legacy state.
/// Never discard a creation intent while manufacturing an old fixture.
pub(crate) fn without_creation(db: &rusqlite::Connection) {
    without_chunk_mount(db);
    let count:i64=db.query_row("SELECT (SELECT count(*) FROM manifest_creation_role)+(SELECT count(*) FROM manifest_creations)",[],|r|r.get(0)).unwrap();
    assert_eq!(
        count, 0,
        "old-schema fixture would discard a creation intent"
    );
    db.execute_batch("DROP TABLE manifest_creations; DROP TABLE manifest_creation_role")
        .unwrap();
}

/// Never discard a live composition binding when constructing schema40 fixtures.
pub(crate) fn without_chunk_mount(db: &rusqlite::Connection) {
    let count: i64 = db
        .query_row("SELECT count(*) FROM chunk_mount_role", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        count, 0,
        "old-schema fixture would discard chunk mount ownership"
    );
    db.execute_batch("DROP TABLE chunk_mount_role").unwrap();
}
