use super::*;
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn private_owner_worker_reopens_exact_views_and_closes_all_clones() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("worker café 🦀");
        let storage = crate::worker::Storage::open(
            &path,
            crate::cache::Options::default(),
            crate::journal::Options::default(),
        )
        .await
        .unwrap();
        let client = storage.sparse_views(Options::default()).unwrap();
        let other = client.clone();
        let b = binding(2, 17);
        let (a, c) = tokio::join!(
            client.begin([7; 32], b.clone(), MAX_FILE_BYTES),
            other.begin([7; 32], b, MAX_FILE_BYTES)
        );
        let a = a.unwrap();
        assert_eq!(c.unwrap(), a);
        assert!(State::open_managed(&path).is_err());
        storage.close().await.unwrap();
        assert!(client.view(a.id()).await.is_err());
        let storage = crate::worker::Storage::open(
            &path,
            crate::cache::Options::default(),
            crate::journal::Options::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            storage
                .sparse_views(Options::new(1).unwrap())
                .unwrap()
                .view(a.id())
                .await
                .unwrap(),
            a
        );
        storage.close().await.unwrap();
    })
    .await
    .expect("bounded sparse-view worker acceptance timed out");
}
