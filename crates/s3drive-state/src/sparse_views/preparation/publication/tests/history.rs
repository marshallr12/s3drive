//! Retained-history work measurements use real journals, not synthetic SQL rows.
//! Timings are diagnostic; acceptance is based on deterministic work counts.
use super::*;
use crate::sparse_views::preparation::native as validation;
use s3drive_core::manifest::{self, Page};

async fn add_epoch(n: &Native, index: u8) {
    let input = n.token.input().clone();
    let key = n.key.clone();
    let (id, view, root, page) = n
        .storage
        .cache()
        .owner
        .call(move |store| {
            let mut f = fixture();
            let node = [index; 16];
            let page = Page::new(
                [1; 16],
                node,
                0,
                0,
                vec![manifest::Entry::Data {
                    length: 17,
                    object: manifest::Reference::new([5; 16], [7; 16], 0, digest(&f.bytes))?,
                }],
            )?;
            let root = RootDescriptor::new(
                [1; 16],
                node,
                17,
                manifest::Reference::new([5; 16], [7; 16], 1 << 56, page.digest())?,
            )?;
            f.node.id = hex(&node);
            let payload = f.node.payload.as_mut().unwrap();
            payload.reservation.node = f.node.id.clone();
            payload.reservation.key = Namespace::new(f.store.root.clone(), [1; 16], node, [5; 16])?
                .root_key(&root)?
                .as_str()
                .into();
            payload.reservation.sha256 = hex(&digest(&page.encode()));
            payload.reservation.size = page.encode().len() as u64;
            payload.reservation.manifest = Some(ns::ManifestRoot::new(root.clone()).unwrap());
            assert!(payload.valid());
            let reference = ns::payload_reference(&f.node.payload).unwrap();
            f.binding = Binding::new(
                Descriptor::from_hex(&hex(&[1; 16]), &f.node.id, &hex(&[7; 16]), &reference)?,
                [5; 16],
                [6; 32],
                "a".repeat(64),
                f.store.root.clone(),
                17,
            )?;
            let mut input = input;
            input.id = hex(&[index; 16]);
            input.node = f.node.id;
            let s = &mut store.state;
            let mut journal = opens(s, &key);
            journal.prepare(input.clone())?;
            journal.activate(&input.id, &hex(&[index.wrapping_add(64); 16]))?;
            let view = views::Journal::new(s, views::Options::default())?.begin_scoped(
                views::ScopedOperation::new(&f.binding, [index; 32])?,
                f.binding,
                17,
            )?;
            Ok((input.id, view, root.encode().to_vec(), page.encode()))
        })
        .await
        .unwrap();
    let token = n.client.bind(id).await.unwrap();
    let view = n
        .client
        .write(
            token.clone(),
            Context::new(&view, root.clone(), page.clone()).unwrap(),
            [index.wrapping_add(32); 32],
            5,
            vec![255],
        )
        .await
        .unwrap();
    n.client
        .prepare(token, Context::new(&view, root, page).unwrap())
        .await
        .unwrap();
}

#[tokio::test]
async fn native_sparse_history_profile_counts_retained_epochs_plans_and_opens() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("history profile café 🦀"), 32).await;
    let view = dirty(&n, &n.view, 2, 5).await;
    n.client
        .prepare(n.token.clone(), context(&view))
        .await
        .unwrap();
    for count in 1..=8usize {
        if count > 1 {
            add_epoch(&n, count as u8 + 10).await;
        }
        if ![1, 4, 8].contains(&count) {
            continue;
        }
        let key = n.key.clone();
        n.storage.cache().owner.call(move |store| {
            let db = &store.state.connection;
            let opens_count: usize = db.query_row("SELECT count(*) FROM windows_open_journal", [], |r| r.get::<_, u32>(0).map(|v| v as usize))?;
            let plans: usize = db.query_row("SELECT count(*) FROM sparse_preparations", [], |r| r.get::<_, u32>(0).map(|v| v as usize))?;
            let views: usize = db.query_row("SELECT count(*) FROM sparse_views", [], |r| r.get::<_, u32>(0).map(|v| v as usize))?;
            assert_eq!((opens_count, plans, views), (count, count, count * 2));
            let start = std::time::Instant::now();
            let loads = opens::load_count();
            let visits = validation::epoch_source_visits();
            validation::validate_metadata(db)?;
            let metadata_loads = opens::load_count() - loads;
            let epoch_visits = validation::epoch_source_visits() - visits;
            let metadata_us = start.elapsed().as_micros();
            let start = std::time::Instant::now();
            let loads = opens::load_count();
            crate::chunk_mount::require(db, &key)?;
            let require_loads = opens::load_count() - loads;
            let require_us = start.elapsed().as_micros();
            assert_eq!(metadata_loads, count * 2, "one original replay plus one independent source-role replay per open");
            assert_eq!(epoch_visits, views + count, "index each source once and look up each epoch once");
            assert_eq!(require_loads, count * 3, "scope inspection must not replay active originals twice");
            eprintln!("NATIVE_HISTORY_PROFILE opens={opens_count} epochs={count} plans={plans} views={views} metadata_open_loads={metadata_loads} epoch_source_visits={epoch_visits} require_open_loads={require_loads} metadata_us={metadata_us} require_us={require_us}");
            Ok(())
        }).await.unwrap();
    }
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_history_scope_replays_live_and_retired_opens_once_each() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("retained scope"), 16).await;
    let other = second(&n).await;
    close_native(&n, &n.token, 71).await.unwrap();
    n.client
        .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
        .await
        .unwrap();
    let key = n.key.clone();
    let retired = n.token.input().id.clone();
    let live = other.input().id.clone();
    n.storage.cache().owner.call(move |store| {
        opens(&mut store.state, &key).retire(&retired)?;
        let db = &store.state.connection;
        assert!(opens::sparse_retired(db, &retired)?);
        assert!(!opens::sparse_retired(db, &live)?);
        for _ in 0..2 {
            let before = opens::load_count();
            assert!(crate::chunk_mount::compatible(db)?);
            assert_eq!(opens::load_count() - before, 2,
                "each fresh role inspection must replay both live and retired evidence exactly once");
        }
        Ok(())
    }).await.unwrap();
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_history_scope_rechecks_retired_unselected_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("retained corruption"), 16).await;
    let _other = second(&n).await;
    close_native(&n, &n.token, 71).await.unwrap();
    n.client
        .complete(n.token.clone(), hex(&[71; 16]), hex(&[72; 16]))
        .await
        .unwrap();
    let key = n.key.clone();
    let retired = n.token.input().id.clone();
    n.storage.cache().owner.call(move |store| {
        opens(&mut store.state, &key).retire(&retired)?;
        let db = &store.state.connection;
        assert!(crate::chunk_mount::compatible(db)?);
        // The unrelated live open must not hide changed retired event evidence.
        assert!(db.execute("UPDATE windows_open_events SET state_hash=zeroblob(32) WHERE open_id=?", [&retired])? > 0);
        assert!(crate::chunk_mount::compatible(db).is_err());
        assert!(validation::validate_metadata(db).is_err());
        let changed: u32 = db.query_row("SELECT count(*) FROM windows_open_events WHERE open_id=? AND state_hash=zeroblob(32)", [&retired], |r| r.get(0))?;
        assert!(changed > 0, "validation must not repair corrupted retired history");
        Ok(())
    }).await.unwrap();
    n.storage.close().await.unwrap();
}

#[tokio::test]
async fn native_sparse_history_many_plans_keep_one_original_replay_and_fresh_checks() {
    let temp = tempfile::tempdir().unwrap();
    let n = setup(&temp.path().join("one epoch many plans"), 32).await;
    let mut view = n.view.clone();
    let mut oldest = None;
    for count in 1..=8usize {
        view = dirty(&n, &view, 20 + count as u8, 5).await;
        let plan = n
            .client
            .prepare(n.token.clone(), context(&view))
            .await
            .unwrap();
        oldest.get_or_insert(plan.id().unwrap());
        if ![1, 4, 8].contains(&count) {
            continue;
        }
        n.storage.cache().owner.call(move |store| {
            let db = &store.state.connection;
            let loads = opens::load_count();
            let visits = validation::epoch_source_visits();
            let start = std::time::Instant::now();
            validation::validate_metadata(db)?;
            let metadata_us = start.elapsed().as_micros();
            let loads = opens::load_count() - loads;
            let visits = validation::epoch_source_visits() - visits;
            assert_eq!(loads, 2, "more retained plans must not reload their common original");
            assert_eq!(visits, count + 2, "visit each source once plus its single epoch lookup");
            eprintln!("NATIVE_HISTORY_SHARED opens=1 epochs=1 plans={count} views={} metadata_open_loads={loads} epoch_source_visits={visits} metadata_us={metadata_us}", count + 1);
            Ok(())
        }).await.unwrap();
    }
    let oldest = oldest.unwrap();
    n.storage
        .cache()
        .owner
        .call(move |store| {
            let db = &store.state.connection;
            validation::validate_metadata(db)?;
            assert_eq!(
                db.execute(
                    "UPDATE sparse_preparations SET state_hash=zeroblob(32) WHERE plan_id=?",
                    [oldest.as_slice()]
                )?,
                1
            );
            assert!(
                validation::validate_metadata(db).is_err(),
                "the oldest retained plan must be rechecked after mutation"
            );
            let hash: Vec<u8> = db.query_row(
                "SELECT state_hash FROM sparse_preparations WHERE plan_id=?",
                [oldest.as_slice()],
                |r| r.get(0),
            )?;
            assert_eq!(
                hash,
                vec![0; 32],
                "validation must not repair historical plan bytes"
            );
            Ok(())
        })
        .await
        .unwrap();
    n.storage.close().await.unwrap();
}
