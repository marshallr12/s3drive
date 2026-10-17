use super::*;
use namespace::RemovalMode;
pub(super) fn prepare(f: &mut Fixture, n: &Node) -> Input {
    let mut i = arm(f, n);
    let Mutation::ArmRemoval {
        node,
        parent,
        owner,
    } = i.request.mutation
    else {
        unreachable!()
    };
    i.request.mutation = Mutation::PrepareRemoval {
        node,
        parent,
        owner,
    };
    i
}
pub(super) fn transition(f: &mut Fixture, n: &Node, mode: RemovalMode) -> Input {
    let old = owned(f, n).unwrap();
    f.input(
        Mutation::TransitionRemoval {
            node: guard(n),
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            fence: old.fence,
            expected: old.mode,
            mode,
        },
        std::slice::from_ref(n),
    )
}
#[test]
fn mode_acknowledgments_are_atomic_keep_original_binding_and_preserve_dirty_data() {
    let mut f = Fixture::new();
    let n = f.create("deferred café 日本語 🦀");
    let dirty = f.stage(&n, 500, b"binary\0\xff");
    let initial = prepare(&mut f, &n);
    let mut before = apply(&mut f, &initial).node;
    let first = owned(&mut f, &n).unwrap();
    let mut history = Vec::new();
    for mode in [
        RemovalMode::Pending,
        RemovalMode::Deferred,
        RemovalMode::Pending,
    ] {
        let i = transition(&mut f, &before, mode);
        Journal::new(&mut f.client)
            .unwrap()
            .begin(i.clone())
            .unwrap();
        let r = f.authority.namespace_apply(&i.request).unwrap();
        let old = owned(&mut f, &n).unwrap();
        f.client.connection.execute_batch("CREATE TRIGGER fail BEFORE UPDATE ON namespace_intents BEGIN SELECT RAISE(IGNORE); END").unwrap();
        assert!(f.ack(&i, r.clone()).is_err());
        assert_eq!(owned(&mut f, &n).unwrap(), old);
        assert_eq!(f.file(&n).base, before);
        f.client
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        f.reopen_client();
        f.ack(&i, r.clone()).unwrap();
        let current = owned(&mut f, &n).unwrap();
        assert_eq!(current.input, first.input);
        assert_eq!(current.fence, first.fence);
        assert_eq!(current.mode, mode);
        assert_eq!(current.receipt, r);
        history.push((i, r.clone()));
        for (i, r) in &history {
            f.ack(i, r.clone()).unwrap();
        }
        assert_eq!(owned(&mut f, &n).unwrap(), current);
        assert_eq!(f.file(&n).head, dirty.generation);
        before = r.node;
    }
    let close = close(&mut f, &before, &first.fence, true);
    apply(&mut f, &close);
    assert!(owned(&mut f, &n).is_none());
    assert!(f.file(&n).base.deleted);
    assert_eq!(f.file(&n).head, dirty.generation);
}
#[test]
fn deferred_parent_admits_children_but_not_reserved_source_or_unknown_mode_changes() {
    let mut f = Fixture::new();
    let request = f.request(Mutation::CreateEntry {
        parent: ROOT.into(),
        parent_revision: 0,
        name: "directory".into(),
        node_kind: Kind::Directory,
        store: None,
    });
    let dir = f.authority.namespace_apply(&request).unwrap().node;
    let prepare = prepare(&mut f, &dir);
    let r = apply(&mut f, &prepare);
    let child = f.input(
        Mutation::CreateEntry {
            parent: dir.id.clone(),
            parent_revision: r.node.revision,
            name: "child 🦀".into(),
            node_kind: Kind::EmptyFile,
            store: Some(f.store.id.clone()),
        },
        &[],
    );
    Journal::new(&mut f.client)
        .unwrap()
        .begin(child.clone())
        .unwrap();
    let receipt = f.authority.namespace_apply(&child.request).unwrap();
    f.ack(&child, receipt.clone()).unwrap();
    let remove = f.remove(&r.node);
    assert!(Journal::new(&mut f.client).unwrap().begin(remove).is_err());
    let promote = transition(&mut f, &r.node, RemovalMode::Pending);
    Journal::new(&mut f.client)
        .unwrap()
        .begin(promote.clone())
        .unwrap();
    let mut next = child.clone();
    next.request.operation = id(9000);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .begin(next.clone())
            .is_err()
    );
    assert_eq!(
        f.authority.namespace_apply(&promote.request),
        Err(namespace::Error::NotEmpty)
    );
    Journal::new(&mut f.client)
        .unwrap()
        .acknowledge(&promote.request.operation, Err(namespace::Error::NotEmpty))
        .unwrap();
    Journal::new(&mut f.client).unwrap().begin(next).unwrap();
    assert_eq!(owned(&mut f, &dir).unwrap().mode, RemovalMode::Deferred);
}
#[test]
fn reserved_local_promotion_and_closure_fit_at_quota_without_losing_unknown_results() {
    let mut f = Fixture::new();
    let n = f.create("quota 🦀");
    let i = prepare(&mut f, &n);
    let r = apply(&mut f, &i);
    let template = f.input(
        Mutation::CreateEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            name: "fixture".into(),
            node_kind: Kind::EmptyFile,
            store: Some(f.store.id.clone()),
        },
        &[],
    );
    let tx = f.client.connection.transaction().unwrap();
    for operation in 10_000..14_093 {
        let mut input = template.clone();
        input.request.operation = id(operation);
        let record = Record {
            input,
            outcome: None,
        };
        tx.execute(
            "INSERT INTO namespace_intents VALUES(?1,?2,NULL,?3)",
            params![
                record.input.request.operation,
                serde_json::to_vec(&record.input).unwrap(),
                hash(&record).unwrap()
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .begin(template)
            .is_err()
    );
    let promote = transition(&mut f, &r.node, RemovalMode::Pending);
    Journal::new(&mut f.client)
        .unwrap()
        .begin(promote.clone())
        .unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .acknowledge(&promote.request.operation, Err(namespace::Error::Conflict))
            .is_err()
    );
    let receipt = f.authority.namespace_apply(&promote.request).unwrap();
    f.ack(&promote, receipt.clone()).unwrap();
    let defer = transition(&mut f, &receipt.node, RemovalMode::Deferred);
    assert!(Journal::new(&mut f.client).unwrap().begin(defer).is_err());
    let fence = owned(&mut f, &n).unwrap().fence;
    let finish = close(&mut f, &receipt.node, &fence, true);
    apply(&mut f, &finish);
    assert!(owned(&mut f, &n).is_none());
}
