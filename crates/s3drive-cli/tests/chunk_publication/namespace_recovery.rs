//! Dedicated existing-only namespace/open recovery, not native acceptance.
use super::*;
use s3drive_managed::{chunks::Reader, namespace_recovery::Recovery};
use s3drive_state::{namespace_client as names, namespace_client::opens, sparse_views};
#[derive(serde::Serialize, serde::Deserialize)]
struct Saved {
    input: names::Input,
    outcome: std::result::Result<s3drive_authority::Receipt, s3drive_authority::Error>,
    open: opens::OpenInput,
    events: Vec<(String, opens::Action)>,
}
pub async fn seed(config: &s3drive_core::Config, root: &Path, s: &Server, kind: &str) {
    let (e, chunks, creator, draft) = creator_import::setup_owner(config, root, s).await;
    let created = creator.create(draft).await.unwrap();
    let n = created.metadata().node.clone();
    let volume = chunks.key().volume().to_owned();
    let store = empty_creation::store(s).await;
    let target = e.recovery_target("writable/.s3drive/binding").unwrap();
    let o = chunks.opens();
    o.start_session(id(80), volume.clone()).await.unwrap();
    o.prepare(opens::OpenInput {
        id: id(71),
        session: id(80),
        volume: volume.clone(),
        node: n.id.clone(),
        store: store.clone(),
        source: *target.source(),
        fingerprint: target.fingerprint().into(),
        delete_on_close: false,
        controlled: true,
        session_bound: true,
        posix: false,
        admission: None,
        creation: None,
    })
    .await
    .unwrap();
    o.activate(id(71), id(72)).await.unwrap();
    let reader = Reader::open(s.wire.clone(), volume.clone(), vec![(id(200), e.reader())])
        .await
        .unwrap();
    let base = reader.open_file(&n.id).await.unwrap().pinned_base();
    let local = e
        .storage()
        .sparse_views(sparse_views::Options::default())
        .unwrap();
    local
        .begin([1; 32], base.sparse_binding().unwrap(), 0)
        .await
        .unwrap();
    // An unrelated sparse dirty source must not become a metadata dependency.
    // Store identity is the authority fixture's hexadecimal numeric ID.
    let mut store_bytes = [0u8; 16];
    for (i, b) in store_bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&store.id[i * 2..i * 2 + 2], 16).unwrap();
    }
    let binding = sparse_views::Binding::new(
        s3drive_core::manifest::base::Descriptor::from_hex(
            &volume,
            &id(90),
            &id(91),
            &"c".repeat(64),
        )
        .unwrap(),
        store_bytes,
        *target.source(),
        target.fingerprint().into(),
        store.root.clone(),
        4096,
    )
    .unwrap();
    let dirty = local.begin([2; 32], binding, 0).await.unwrap();
    local
        .write(
            [3; 32],
            dirty.id(),
            11 * 1024 * 1024 + 3,
            "日本語 🦀".as_bytes().into(),
        )
        .await
        .unwrap();
    let parent = node(&s.direct, &volume, s3drive_authority::ROOT).await;
    let mut input = names::Input {
        request: Apply {
            volume,
            operation: id(81),
            expected_sequence: info(&s.direct).await.sequence,
            mutation: Mutation::ArmRemoval {
                node: s3drive_authority::NodeGuard {
                    id: n.id.clone(),
                    revision: n.revision,
                },
                parent: s3drive_authority::NodeGuard {
                    id: parent.id,
                    revision: parent.revision,
                },
                owner: chunks.key().owner().into(),
            },
        },
        store,
        source: *target.source(),
        fingerprint: target.fingerprint().into(),
        entries: vec![names::Entry::from_node(&n).unwrap()],
        destination_control: None,
    };
    if kind == "cold_namespace_rejected" {
        input.request.expected_sequence += 1;
    }
    o.callback(id(71), id(73), opens::Callback::SetDelete { delete: true })
        .await
        .unwrap();
    let open = o.link(id(71), id(74), id(73), input.clone()).await.unwrap();
    chunks.namespace().begin(input.clone()).await.unwrap();
    let outcome = match ask(
        &s.direct,
        Request::Apply {
            request: input.request.clone(),
        },
    )
    .await
    {
        Reply::Applied(r) => Ok(r),
        Reply::Error(e) => Err(e),
        other => panic!("{other:?}"),
    };
    assert_eq!(outcome.is_err(), kind == "cold_namespace_rejected");
    if kind == "cold_namespace_known" {
        chunks
            .namespace()
            .acknowledge(id(81), outcome.clone())
            .await
            .unwrap();
    }
    std::fs::write(
        root.join("namespace-saved.json"),
        serde_json::to_vec(&Saved {
            input,
            outcome,
            open: open.input,
            events: open.events.into_iter().map(|e| (e.id, e.action)).collect(),
        })
        .unwrap(),
    )
    .unwrap();
    checkpoint("CRASH_ROOT");
    std::future::pending::<()>().await;
}
fn preserved(root: &Path) -> Vec<String> {
    let db = raw(root);
    [
        "sparse_view_files",
        "sparse_views",
        "sparse_view_blocks",
        "staged_chunks",
        "windows_mount_sessions",
        "windows_session_recoveries",
        "retired_windows_opens",
    ]
    .into_iter()
    .flat_map(|table| {
        let mut q = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY 1"))
            .unwrap();
        let cols = q.column_count();
        q.query_map([], |r| {
            Ok((0..cols)
                .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                .collect::<Vec<_>>()
                .join("|"))
        })
        .unwrap()
        .map(|r| format!("{table}:{}", r.unwrap()))
        .collect::<Vec<_>>()
    })
    .collect()
}
pub async fn recover(root: &Path, s: &Server) {
    let saved: Saved =
        serde_json::from_slice(&std::fs::read(root.join("namespace-saved.json")).unwrap()).unwrap();
    assert!(
        Storage::open(
            &root.join("client"),
            cache::Options::default(),
            journal::Options::default()
        )
        .await
        .is_err()
    );
    let bytes = preserved(root);
    let r = Recovery::open_existing(
        &root.join("client"),
        s.wire.clone(),
        saved.input.request.volume.clone(),
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    assert!(r.recover(id(999)).await.is_err());
    assert!(r.recover_open_link(id(999), id(75), id(81)).await.is_err());
    assert!(s.proxy.requests().is_empty());
    let initial = r
        .records()
        .await
        .unwrap()
        .into_iter()
        .find(|n| n.input.request.operation == id(81))
        .unwrap();
    if initial.outcome.is_none() {
        assert!(r.recover(id(81)).await.is_err());
        assert_eq!(s.proxy.applies(), vec![saved.input.request.clone()]);
        assert!(
            r.records()
                .await
                .unwrap()
                .into_iter()
                .find(|n| n.input.request.operation == id(81))
                .unwrap()
                .outcome
                .is_none()
        );
    }
    s.proxy.clear();
    s.proxy.mode.store(0, Ordering::SeqCst);
    let after = r.recover_open_link(id(71), id(75), id(81)).await.unwrap();
    assert_eq!(after.input, saved.open);
    assert_eq!(
        after.events[..saved.events.len()]
            .iter()
            .map(|e| (e.id.clone(), e.action.clone()))
            .collect::<Vec<_>>(),
        saved.events
    );
    assert_eq!(after.events.len(), saved.events.len() + 1);
    assert!(after.pending().is_empty());
    assert!(after.reply(&id(73)).is_none());
    assert!(!after.completed(&id(73)));
    assert!(after.cleanup().is_none());
    assert!(after.close().is_none());
    assert_eq!(
        s.proxy.requests().len(),
        usize::from(initial.outcome.is_none())
    );
    if initial.outcome.is_none() {
        assert_eq!(s.proxy.applies(), vec![saved.input.request.clone()]);
    }
    assert_eq!(
        r.recover(id(81)).await.unwrap().outcome,
        Some(saved.outcome)
    );
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    assert_eq!(
        r.recover_open_link(id(71), id(75), id(81)).await.unwrap(),
        after
    );
    assert!(r.recover_open_link(id(71), id(76), id(81)).await.is_err());
    assert!(s.proxy.requests().is_empty());
    assert_eq!(preserved(root), bytes);
    r.close().await.unwrap();
    assert!(r.records().await.is_err());
    assert!(
        Storage::open(
            &root.join("client"),
            cache::Options::default(),
            journal::Options::default()
        )
        .await
        .is_err()
    );
}
