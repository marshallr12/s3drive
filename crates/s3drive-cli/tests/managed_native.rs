#[path = "support/fixture.rs"]
pub mod fixture;
#[path = "support/managed.rs"]
#[allow(dead_code)]
mod managed;
#[path = "support/writes.rs"]
#[allow(dead_code)]
mod writes;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn managed_native_child() {
    managed::child().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn managed_engine_cli_authority_and_two_bucket_snapshots() {
    managed::portable().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn managed_engine_durable_existing_file_edits_and_content_cas() {
    managed::portable_writes().await;
}
