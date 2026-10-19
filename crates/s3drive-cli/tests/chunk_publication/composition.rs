//! Public same-owner composition using real SDK/TCP fixtures; no native mount.
use super::*;
use s3drive_state::{chunk_mount, namespace_client as names, sparse_views};
pub async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let v = info(&s.direct).await;
    let Reply::Store(store) = ask(
        &s.direct,
        Request::Store {
            volume: v.id.clone(),
            store: id(200),
        },
    )
    .await
    else {
        panic!()
    };
    let target = e.recovery_target("writable/.s3drive/binding").unwrap();
    let declared = chunk_mount::Input {
        volume: v.id.clone(),
        controls: vec![names::Control {
            store: store.clone(),
            source: *target.source(),
            fingerprint: target.fingerprint().into(),
        }],
    };
    let st = e.storage();
    let combined = st.chunk_mount(declared.clone()).await.unwrap();
    assert!(combined.same_storage(&st));
    let owner = combined.key().clone();
    let ns = combined.namespace();
    let opens = combined.opens();
    opens.start_session(id(80), v.id.clone()).await.unwrap();
    let parent = node(&s.direct, &v.id, s3drive_authority::ROOT).await;
    let creator = s3drive_managed::creation::Creator::bind(
        e.clone(),
        s.wire.clone(),
        v.id.clone(),
        store.clone(),
        s3drive_state::manifest_creation::Limits::default(),
        Duration::from_secs(60),
    )
    .unwrap();
    let draft = s3drive_managed::creation::Draft {
        operation: id(40),
        expected_sequence: v.sequence,
        parent: s3drive_authority::NodeGuard {
            id: parent.id,
            revision: parent.revision,
        },
        name: "café 日本語 🦀 %2F.bin".into(),
    };
    let created = creator.create(draft.clone()).await.unwrap();
    let before = node(&s.direct, &v.id, &id(40)).await;
    let reader = s3drive_managed::chunks::Reader::open(
        s.wire.clone(),
        v.id.clone(),
        vec![(id(200), e.reader())],
    )
    .await
    .unwrap();
    let base = reader.open_file(&id(40)).await.unwrap().pinned_base();
    let local = st.sparse_views(sparse_views::Options::default()).unwrap();
    let clean = local
        .begin([1; 32], base.sparse_binding().unwrap(), 0)
        .await
        .unwrap();
    let root_node = node(&s.direct, &v.id, s3drive_authority::ROOT).await;
    let current = info(&s.direct).await;
    let request = names::Input {
        request: Apply {
            volume: v.id.clone(),
            operation: id(41),
            expected_sequence: current.sequence,
            mutation: Mutation::MoveEntry {
                node: s3drive_authority::NodeGuard {
                    id: before.id.clone(),
                    revision: before.revision,
                },
                source_parent: s3drive_authority::NodeGuard {
                    id: root_node.id.clone(),
                    revision: root_node.revision,
                },
                parent: s3drive_authority::NodeGuard {
                    id: root_node.id,
                    revision: root_node.revision,
                },
                name: "renamed 日本語 🦀.bin".into(),
                destination: None,
                destination_removal: None,
            },
        },
        store: store.clone(),
        source: *target.source(),
        fingerprint: target.fingerprint().into(),
        entries: vec![names::Entry::from_node(&before).unwrap()],
        destination_control: None,
    };
    ns.begin(request.clone()).await.unwrap();
    let Reply::Applied(receipt) = ask(
        &s.direct,
        Request::Apply {
            request: request.request.clone(),
        },
    )
    .await
    else {
        panic!()
    };
    // A later local write must survive acknowledgment of the earlier rename.
    let offset = 8 * 1024 * 1024 + 3;
    let dirty = local
        .write([2; 32], clean.id(), offset, Arc::from("日本語".as_bytes()))
        .await
        .unwrap();
    ns.acknowledge(id(41), Ok(receipt.clone())).await.unwrap();
    assert_eq!(
        local
            .current_head(dirty.binding().clone())
            .await
            .unwrap()
            .id(),
        dirty.id()
    );
    let edit = reader
        .reopen_edited(local.clone(), dirty.id())
        .await
        .unwrap();
    assert_eq!(
        edit.read_range(offset - 2, 11).await.unwrap(),
        [&[0, 0][..], "日本語".as_bytes()].concat()
    );
    let now = node(&s.direct, &v.id, &id(40)).await;
    assert_eq!(now.name, "renamed 日本語 🦀.bin");
    let mut blocked = request.clone();
    blocked.request.operation = id(42);
    blocked.request.expected_sequence = info(&s.direct).await.sequence;
    if let Mutation::MoveEntry { node, name, .. } = &mut blocked.request.mutation {
        node.revision = now.revision;
        *name = "blocked.bin".into();
    }
    blocked.entries = vec![names::Entry::from_node(&now).unwrap()];
    assert!(ns.begin(blocked).await.is_err());
    assert_eq!(
        creator.create(draft).await.unwrap().metadata(),
        created.metadata()
    );
    assert_eq!(ns.get(id(41)).await.unwrap().outcome, Some(Ok(receipt)));
    drop((edit, local, reader, ns, opens, combined, creator, e));
    st.close().await.unwrap();
    drop(st);
    let st = storage(root, "client").await;
    let combined = st.chunk_mount(declared).await.unwrap();
    assert_eq!(combined.key(), &owner);
    assert!(
        combined
            .opens()
            .obsolete_session(id(80), v.id)
            .await
            .unwrap()
    );
    let local = st.sparse_views(sparse_views::Options::default()).unwrap();
    assert_eq!(
        local
            .current_head(dirty.binding().clone())
            .await
            .unwrap()
            .id(),
        dirty.id()
    );
    drop((local, combined));
    st.close().await.unwrap();
}
