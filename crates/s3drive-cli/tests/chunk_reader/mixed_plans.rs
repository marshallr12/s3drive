//! Real SDK/authority/private SQLite preparation. No provider uploads or mounts.
use super::*;
use composition::storage;
use s3drive_core::manifest::objects::RootDescriptor;
use s3drive_state::sparse_views::preparation::{self as plans, Dependency};
use std::{path::Path, sync::Arc};
async fn object(
    f: &Fixture,
    local: &plans::Client,
    plan: [u8; 32],
    generation: [u8; 16],
    ns: &Namespace,
    role: Role,
    reference: &Reference,
) -> Vec<u8> {
    if reference.generation() == &generation {
        let level = if role == Role::Data {
            -1
        } else {
            (reference.ordinal() >> 56) as i8 - 1
        };
        let slot = reference.ordinal() & 0x00ff_ffff_ffff_ffff;
        let Dependency::New {
            reference: actual,
            bytes,
            ..
        } = local.dependency(plan, level, slot).await.unwrap()
        else {
            panic!("new reference lacks new bytes")
        };
        assert_eq!(&actual, reference);
        bytes
    } else {
        let key = f.absolute(ns.key(role, reference).unwrap().as_str());
        f.s3.objects
            .lock()
            .unwrap()
            .get(&key)
            .unwrap()
            .bytes
            .clone()
    }
}
async fn verify(f: &Fixture, local: &plans::Client, info: &plans::Info) -> BTreeMap<u64, Vec<u8>> {
    let root = info.root().unwrap().unwrap();
    let plan = info.id().unwrap();
    let generation = *info.generation();
    let ns = Namespace::new(
        "managed/".into(),
        *root.volume(),
        *root.node(),
        bytes(&id(200)),
    )
    .unwrap();
    let body = object(f, local, plan, generation, &ns, Role::Page, root.page()).await;
    let mut stack = vec![root.verify_page(&body).unwrap()];
    let mut data = BTreeMap::new();
    while let Some(page) = stack.pop() {
        let mut offset = page.page().start();
        for (index, entry) in page.page().entries().iter().enumerate() {
            match entry {
                Entry::Child {
                    object: reference, ..
                } => {
                    let b = object(f, local, plan, generation, &ns, Role::Page, reference).await;
                    stack.push(page.verify_child(index, &b).unwrap());
                }
                Entry::Data {
                    object: reference, ..
                } => {
                    let b = object(f, local, plan, generation, &ns, Role::Data, reference).await;
                    page.verify_data(index, &b).unwrap();
                    assert!(data.insert(offset, b).is_none());
                }
                Entry::Hole { .. } => {}
            }
            offset += match entry {
                Entry::Child { length, .. }
                | Entry::Data { length, .. }
                | Entry::Hole { length } => *length,
            };
        }
    }
    data
}
pub async fn flow(f: &mut Fixture, path: &Path) {
    f.tree(
        10,
        130 * C + 17,
        BTreeMap::from([
            (0, pattern(0, C)),
            (127, pattern(127, C)),
            (128, pattern(128, C)),
            (130, pattern(130, 17)),
        ]),
    )
    .await;
    let r = f.reader().await;
    let base = r.open_file(&id(10)).await.unwrap().pinned_base();
    let local_path = path.join("mixed café 🦀");
    let (storage, local) = storage(&local_path).await;
    let initial = local
        .begin([1; 32], base.sparse_binding().unwrap(), base.size())
        .await
        .unwrap();
    let text = "café 🦀".as_bytes();
    let v = local
        .write([2; 32], initial.id(), 127 * C + 5, Arc::from(text))
        .await
        .unwrap();
    let edited = r.reopen_edited(local.clone(), v.id()).await.unwrap();
    let preparation = edited
        .begin_preparation([80; 32], [99; 16], plans::Options::default())
        .await
        .unwrap();
    let plan = preparation.id();
    let gets = data_gets(f);
    for _ in 0..128 {
        preparation.step().await.unwrap();
    }
    assert_eq!(preparation.info().await.unwrap().cursor().offset, 128 * C);
    assert_eq!(
        data_gets(f) - gets,
        1,
        "one-byte-region edit downloaded unchanged data"
    );
    let newer = local.resize([3; 32], v.id(), 0).await.unwrap();
    storage.close().await.unwrap();
    assert!(preparation.info().await.is_err());
    f.tree(10, 3, BTreeMap::from([(0, b"NEW".to_vec())])).await;
    f.change(
        180,
        Mutation::Move {
            node: id(10),
            parent: s3drive_authority::ROOT.into(),
            name: "renamed prepared 🦀".into(),
        },
    )
    .await;
    f.change(181, Mutation::Remove { node: id(10) }).await;
    f.restart_authority(path).await;
    let (storage, local) = composition::storage(&local_path).await;
    let r = f.reader().await;
    let resumed = r
        .reopen_preparation(local.clone(), plan, plans::Options::default())
        .await
        .unwrap();
    let done = resumed.prepare().await.unwrap();
    assert_eq!(
        data_gets(f) - gets,
        1,
        "restart downloaded already prepared data again"
    );
    assert_eq!(done.view(), &v);
    assert_eq!(local.head(v.binding().clone()).await.unwrap(), newer);
    let result = verify(
        f,
        &local.preparations(plans::Options::default()).unwrap(),
        &done,
    )
    .await;
    let mut changed = pattern(127, C);
    changed[5..5 + text.len()].copy_from_slice(text);
    assert_eq!(
        result,
        BTreeMap::from([
            (0, pattern(0, C)),
            (127 * C, changed),
            (128 * C, pattern(128, C)),
            (130 * C, pattern(130, 17))
        ])
    );
    let checked = resumed.dependency(-1, 127).await.unwrap();
    assert_eq!(checked.plan(), plan);
    assert_eq!(checked.view_id(), v.id());
    assert_eq!(checked.binding(), v.binding());
    assert!(matches!(
        checked.content(),
        Dependency::New {
            role: Role::Data,
            ..
        }
    ));
    assert!(matches!(
        resumed.dependency(0, 1).await.unwrap().content(),
        Dependency::Inherited {
            role: Role::Page,
            ..
        }
    ));
    assert!(matches!(
        resumed.dependency(1, 0).await.unwrap().content(),
        Dependency::New {
            role: Role::Page,
            ..
        }
    ));
    assert!(std::mem::size_of_val(&resumed.step()) <= 8192);
    assert!(
        f.s3.events()
            .iter()
            .all(|e| e.method == "GET" || e.method == "HEAD")
    );
    // An old full root becomes an unchanged child when the new tree grows.
    f.create(11, "grown root 🦀", 200, false).await;
    f.tree(
        11,
        128 * C,
        BTreeMap::from([(0, pattern(0, C)), (127, pattern(127, C))]),
    )
    .await;
    let b = r.open_file(&id(11)).await.unwrap().pinned_base();
    let old_root = b.prove_root().await.unwrap();
    let v = local
        .begin([4; 32], b.sparse_binding().unwrap(), 129 * C)
        .await
        .unwrap();
    let e = r.reopen_edited(local.clone(), v.id()).await.unwrap();
    let p = e
        .begin_preparation([81; 32], [100; 16], plans::Options::default())
        .await
        .unwrap();
    let gets = data_gets(f);
    assert_eq!(p.step().await.unwrap().cursor().offset, 128 * C);
    let done = p.prepare().await.unwrap();
    assert_eq!(data_gets(f), gets);
    let client = local.preparations(plans::Options::default()).unwrap();
    let Dependency::New { bytes, .. } = client.dependency(p.id(), 1, 0).await.unwrap() else {
        panic!()
    };
    let root = done.root().unwrap().unwrap().verify_page(&bytes).unwrap();
    let Entry::Child { object, .. } = &root.page().entries()[0] else {
        panic!()
    };
    assert_eq!(object, old_root.reference());
    assert_eq!(
        verify(f, &client, &done).await,
        BTreeMap::from([(0, pattern(0, C)), (127 * C, pattern(127, C))])
    );
    storage.close().await.unwrap();
}
pub async fn faults(f: &Fixture, path: &Path) {
    let tree = f.tree(10, 2 * C, data(0, 2)).await;
    let r = f.reader().await;
    let base = r.open_file(&id(10)).await.unwrap().pinned_base();
    let (storage, local) = storage(&path.join("local")).await;
    let initial = local
        .begin([1; 32], base.sparse_binding().unwrap(), 2 * C)
        .await
        .unwrap();
    let v = local
        .write([2; 32], initial.id(), 0, Arc::from(b"X".as_slice()))
        .await
        .unwrap();
    let edited = r.reopen_edited(local.clone(), v.id()).await.unwrap();
    let p = edited
        .begin_preparation([80; 32], [99; 16], plans::Options::default())
        .await
        .unwrap();
    let cursor = p.info().await.unwrap().cursor();
    for mode in 31..=37 {
        f.mode.store(mode, Ordering::SeqCst);
        let before = f.s3.events().len();
        assert!(p.step().await.is_err());
        assert_eq!(p.info().await.unwrap().cursor(), cursor);
        assert_eq!(f.s3.events().len(), before);
    }
    f.mode.store(0, Ordering::SeqCst);
    for fault in [Fault::HeadDenied, Fault::Missing, Fault::BigHead] {
        f.s3.fault(Target::Exact(tree.root.clone()), fault);
        assert!(p.step().await.is_err());
        assert_eq!(p.info().await.unwrap().cursor(), cursor);
        f.s3.clear();
    }
    let old =
        f.s3.objects
            .lock()
            .unwrap()
            .get(&tree.data[0])
            .unwrap()
            .clone();
    *f.s3
        .objects
        .lock()
        .unwrap()
        .get_mut(&tree.data[0])
        .unwrap()
        .bytes
        .last_mut()
        .unwrap() ^= 1;
    assert!(p.step().await.is_err());
    assert_eq!(p.info().await.unwrap().cursor(), cursor);
    f.s3.objects
        .lock()
        .unwrap()
        .insert(tree.data[0].clone(), old);
    // A self-consistent local root claim is not an authenticated original base.
    let proof = base.prove_root().await.unwrap();
    let root = RootDescriptor::new(
        bytes(&f.volume),
        bytes(&id(10)),
        base.size(),
        Reference::new(
            *proof.reference().store(),
            [88; 16],
            proof.reference().ordinal(),
            *proof.reference().sha256(),
        )
        .unwrap(),
    )
    .unwrap();
    let client = local.preparations(plans::Options::default()).unwrap();
    let fake = client
        .begin(plans::Request {
            operation: [81; 32],
            view: v.id(),
            generation: [89; 16],
            base_root: root.encode().to_vec(),
            base_page: proof.page().page().encode(),
        })
        .await
        .unwrap();
    let gets = data_gets(f);
    failed(
        r.reopen_preparation(local.clone(), fake.id().unwrap(), plans::Options::default())
            .await,
        ReadFailure::Binding,
    );
    assert_eq!(data_gets(f), gets);
    let actual = RootDescriptor::new(
        bytes(&f.volume),
        bytes(&id(10)),
        base.size(),
        proof.reference().clone(),
    )
    .unwrap();
    for (operation, view_id, hole) in [(83u8, v.id(), false), (84u8, initial.id(), true)] {
        let forged = client
            .begin(plans::Request {
                operation: [operation; 32],
                view: view_id,
                generation: [operation; 16],
                base_root: actual.encode().to_vec(),
                base_page: proof.page().page().encode(),
            })
            .await
            .unwrap();
        let mut bad = vec![0; C as usize];
        if !hole {
            bad[0] = b'X';
        }
        let after = client
            .append(
                forged.id().unwrap(),
                forged.cursor(),
                plans::Candidate::Bytes(bad),
            )
            .await
            .unwrap();
        let after = client
            .append(
                after.id().unwrap(),
                after.cursor(),
                plans::Candidate::InheritedChunk(base.prove_chunk(1).await.unwrap().encode()),
            )
            .await
            .unwrap();
        let done = client
            .step(after.id().unwrap(), after.cursor())
            .await
            .unwrap();
        assert!(done.root().unwrap().is_some());
        let reopened = r
            .reopen_preparation(local.clone(), done.id().unwrap(), plans::Options::default())
            .await
            .unwrap();
        failed(reopened.dependency(-1, 0).await, ReadFailure::Integrity);
        assert!(
            reopened.info().await.unwrap().root().unwrap().is_some(),
            "false intent was silently repaired"
        );
    }
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    f.s3.fault(Target::Data, Fault::Gate);
    let a = p.clone();
    let task = tokio::spawn(async move { a.step().await });
    f.s3.wait(arrived + 1).await;
    let newer = tokio::time::timeout(Duration::from_secs(3), local.resize([3; 32], v.id(), 7))
        .await
        .unwrap()
        .unwrap();
    f.s3.clear();
    f.s3.permits.add_permits(1);
    assert_eq!(task.await.unwrap().unwrap().cursor().offset, C);
    assert_eq!(local.head(v.binding().clone()).await.unwrap(), newer);
    let q = edited
        .begin_preparation([82; 32], [101; 16], plans::Options::default())
        .await
        .unwrap();
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    f.s3.fault(Target::Data, Fault::Gate);
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let a = q.clone();
        tasks.push(tokio::spawn(async move { a.step().await }));
    }
    f.s3.wait(arrived + 4).await;
    failed(q.step().await, ReadFailure::Busy);
    failed(base.read_range(0, 1).await, ReadFailure::Busy);
    failed(
        r.reopen_preparation(local.clone(), q.id(), plans::Options::default())
            .await,
        ReadFailure::Busy,
    );
    for task in tasks {
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }
    f.s3.clear();
    f.s3.permits.add_permits(4);
    f.s3.settle().await;
    assert_eq!(q.info().await.unwrap().cursor(), cursor);
    let short = f
        .reader_options(ReadOptions::new(Duration::from_secs(3)).unwrap())
        .await;
    let timeout = short
        .reopen_preparation(local.clone(), q.id(), plans::Options::default())
        .await
        .unwrap();
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    f.s3.fault(Target::Data, Fault::Gate);
    let task = tokio::spawn(async move { timeout.step().await });
    f.s3.wait(arrived + 1).await;
    failed(task.await.unwrap(), ReadFailure::Timeout);
    f.s3.clear();
    f.s3.permits.add_permits(1);
    f.s3.settle().await;
    assert_eq!(q.info().await.unwrap().cursor(), cursor);
    q.step().await.unwrap();
    // New credentials cannot adopt the original principal's private plan.
    std::fs::write(
        std::env::var_os("AWS_SHARED_CREDENTIALS_FILE").unwrap(),
        "[fixture]\naws_access_key_id=TESTROTATED\naws_secret_access_key=TESTROTATEDSECRET\n",
    )
    .unwrap();
    *f.s3.expected_key.lock().unwrap() = "TESTROTATED".into();
    assert!(p.step().await.is_err());
    let rotated = f.reader().await;
    failed(
        rotated
            .reopen_preparation(local.clone(), p.id(), plans::Options::default())
            .await,
        ReadFailure::Binding,
    );
    assert_eq!(local.head(v.binding().clone()).await.unwrap(), newer);
    storage.close().await.unwrap();
}
pub async fn boundaries(f: &Fixture, path: &Path) {
    let r = f.reader().await;
    let (storage, local) = storage(&path.join("boundaries")).await;
    // The structural zero run ends inside a chunk before a local Unicode patch.
    f.tree(10, 17, BTreeMap::from([(0, pattern(0, 17))])).await;
    let b = r.open_file(&id(10)).await.unwrap().pinned_base();
    let initial = local
        .begin([1; 32], b.sparse_binding().unwrap(), C + 10)
        .await
        .unwrap();
    let v = local
        .write([2; 32], initial.id(), C + 5, Arc::from("café".as_bytes()))
        .await
        .unwrap();
    let e = r.reopen_edited(local.clone(), v.id()).await.unwrap();
    let p = e
        .begin_preparation([80; 32], [99; 16], plans::Options::default())
        .await
        .unwrap();
    let gets = data_gets(f);
    let done = p.prepare().await.unwrap();
    assert_eq!(data_gets(f) - gets, 1);
    let mut first = pattern(0, 17);
    first.resize(C as usize, 0);
    let mut last = vec![0; 5];
    last.extend_from_slice("café".as_bytes());
    assert_eq!(
        verify(
            f,
            &local.preparations(plans::Options::default()).unwrap(),
            &done
        )
        .await,
        BTreeMap::from([(0, first), (C, last)])
    );
    f.create(11, "truncated tail 🦀", 200, false).await;
    let tree = f.tree(11, C, data(0, 1)).await;
    let b = r.open_file(&id(11)).await.unwrap().pinned_base();
    let v = local
        .begin([3; 32], b.sparse_binding().unwrap(), 7)
        .await
        .unwrap();
    let e = r.reopen_edited(local.clone(), v.id()).await.unwrap();
    let p = e
        .begin_preparation([81; 32], [100; 16], plans::Options::default())
        .await
        .unwrap();
    let original =
        f.s3.objects
            .lock()
            .unwrap()
            .get(&tree.data[0])
            .unwrap()
            .clone();
    *f.s3
        .objects
        .lock()
        .unwrap()
        .get_mut(&tree.data[0])
        .unwrap()
        .bytes
        .last_mut()
        .unwrap() ^= 1;
    assert!(p.step().await.is_err());
    assert_eq!(p.info().await.unwrap().cursor().offset, 0);
    f.s3.objects
        .lock()
        .unwrap()
        .insert(tree.data[0].clone(), original);
    let gets = data_gets(f);
    let done = p.prepare().await.unwrap();
    assert_eq!(data_gets(f) - gets, 1);
    assert_eq!(
        verify(
            f,
            &local.preparations(plans::Options::default()).unwrap(),
            &done
        )
        .await,
        BTreeMap::from([(0, pattern(0, 7))])
    );
    f.create(12, "regrown tail 🦀", 200, false).await;
    f.tree(12, 17, BTreeMap::from([(0, pattern(0, 17))])).await;
    let b = r.open_file(&id(12)).await.unwrap().pinned_base();
    let initial = local
        .begin([4; 32], b.sparse_binding().unwrap(), 17)
        .await
        .unwrap();
    let short = local.resize([5; 32], initial.id(), 7).await.unwrap();
    let v = local.resize([6; 32], short.id(), 33).await.unwrap();
    let e = r.reopen_edited(local.clone(), v.id()).await.unwrap();
    let p = e
        .begin_preparation([82; 32], [101; 16], plans::Options::default())
        .await
        .unwrap();
    let done = p.prepare().await.unwrap();
    let mut expected = pattern(0, 7);
    expected.resize(33, 0);
    assert_eq!(
        verify(
            f,
            &local.preparations(plans::Options::default()).unwrap(),
            &done
        )
        .await,
        BTreeMap::from([(0, expected)])
    );
    f.create(13, "empty root 🦀", 200, false).await;
    f.tree(13, 0, BTreeMap::new()).await;
    let b = r.open_file(&id(13)).await.unwrap().pinned_base();
    let v = local
        .begin([7; 32], b.sparse_binding().unwrap(), 0)
        .await
        .unwrap();
    let e = r.reopen_edited(local.clone(), v.id()).await.unwrap();
    let p = e
        .begin_preparation([83; 32], [102; 16], plans::Options::default())
        .await
        .unwrap();
    let gets = data_gets(f);
    let done = p.prepare().await.unwrap();
    assert_eq!(done.root().unwrap().unwrap().length(), 0);
    assert!(
        verify(
            f,
            &local.preparations(plans::Options::default()).unwrap(),
            &done
        )
        .await
        .is_empty()
    );
    assert_eq!(data_gets(f), gets);
    let huge = local.resize([8; 32], v.id(), 65536 * C).await.unwrap();
    let e = r.reopen_edited(local.clone(), huge.id()).await.unwrap();
    let p = e
        .begin_preparation([84; 32], [103; 16], plans::Options::default())
        .await
        .unwrap();
    f.mode.store(31, Ordering::SeqCst);
    assert!(p.step().await.is_err());
    assert_eq!(p.info().await.unwrap().cursor().offset, 0);
    f.mode.store(0, Ordering::SeqCst);
    let gets = data_gets(f);
    assert_eq!(p.step().await.unwrap().cursor().offset, 65536 * C);
    assert_eq!(data_gets(f), gets);
    storage.close().await.unwrap();
}
