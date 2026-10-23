//! Retained-history work measurements use real journals, not synthetic SQL rows.
//! Timings are diagnostic; acceptance is based on deterministic work counts.
use super::*;
use crate::sparse_views::preparation::native::metadata;
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
            let visits = metadata::epoch_source_visits();
            metadata::validate(db)?;
            let metadata_loads = opens::load_count() - loads;
            let epoch_visits = metadata::epoch_source_visits() - visits;
            let metadata_us = start.elapsed().as_micros();
            let start = std::time::Instant::now();
            let loads = opens::load_count();
            crate::chunk_mount::require(db, &key)?;
            let require_loads = opens::load_count() - loads;
            let require_us = start.elapsed().as_micros();
            assert!(metadata_loads >= count);
            assert!(epoch_visits >= count);
            eprintln!("NATIVE_HISTORY_PROFILE opens={opens_count} epochs={count} plans={plans} views={views} metadata_open_loads={metadata_loads} epoch_source_visits={epoch_visits} require_open_loads={require_loads} metadata_us={metadata_us} require_us={require_us}");
            Ok(())
        }).await.unwrap();
    }
    n.storage.close().await.unwrap();
}
