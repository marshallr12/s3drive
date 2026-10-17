use super::*;

fn guard(n: &Node) -> NodeGuard {
    NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    }
}
struct World {
    // Fields drop in declaration order: release SQLite/ownership before cleanup.
    state: State,
    tmp: tempfile::TempDir,
    volume: String,
    next: u64,
    store: String,
}
impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let mut state = State::open_managed(&tmp.path().join("authority")).unwrap();
        let volume = state
            .namespace_initialize_with_policy(names::PORTABLE)
            .unwrap()
            .id;
        let mut w = Self {
            tmp,
            state,
            volume,
            next: 1,
            store: String::new(),
        };
        w.store = w
            .apply(Mutation::RegisterStore {
                route: "a".repeat(64),
                root: "internal/".into(),
            })
            .unwrap()
            .artifact
            .and_then(|a| match a {
                Artifact::Store(s) => Some(s.id),
                _ => None,
            })
            .unwrap();
        w
    }
    fn request(&mut self, mutation: Mutation) -> Apply {
        self.next += 1;
        Apply {
            volume: self.volume.clone(),
            operation: format!("{:032x}", self.next),
            expected_sequence: self.state.namespace_volume().unwrap().sequence,
            mutation,
        }
    }
    fn apply(&mut self, m: Mutation) -> Result<Receipt> {
        let r = self.request(m);
        self.state.namespace_apply(&r)
    }
    fn node(&self, id: &str) -> Node {
        self.state.namespace_node(&self.volume, id).unwrap()
    }
    fn root(&self) -> Node {
        self.node(ROOT)
    }
    fn create(&mut self, parent: &Node, name: &str, kind: Kind) -> Node {
        self.apply(Mutation::CreateEntry {
            parent: parent.id.clone(),
            parent_revision: parent.revision,
            store: (kind == Kind::EmptyFile).then(|| self.store.clone()),
            name: name.into(),
            node_kind: kind,
        })
        .unwrap()
        .node
    }
    fn file(&mut self, name: &str) -> Node {
        self.create(&self.root(), name, Kind::EmptyFile)
    }
    fn publish(&mut self, n: &Node) -> Node {
        let reservation = self
            .apply(Mutation::ReserveContent {
                node: n.id.clone(),
                expected_revision: n.revision,
                store: self.store.clone(),
                size: 1,
                sha256: "b".repeat(64),
                expected_payload: payload_reference(&n.payload),
            })
            .unwrap();
        let Some(Artifact::Reservation(r)) = reservation.artifact else {
            panic!()
        };
        self.apply(Mutation::Publish {
            reservation: r.id,
            upload_operation: "c".repeat(64),
            etag: "\"tag\"".into(),
            version: None,
        })
        .unwrap()
        .node
    }
    fn mv(&mut self, n: &Node, p: &Node, name: &str, target: Option<&Node>) -> Apply {
        self.request(Mutation::MoveEntry {
            destination_removal: None,
            node: guard(n),
            source_parent: guard(&self.node(n.parent.as_ref().unwrap())),
            parent: guard(p),
            name: name.into(),
            destination: target.map(guard),
        })
    }
    fn rm(&mut self, n: &Node) -> Apply {
        self.request(Mutation::RemoveEntry {
            node: guard(n),
            parent: guard(&self.node(n.parent.as_ref().unwrap())),
        })
    }
    fn reopen(&mut self) {
        // The owning state must release its directory lock before reopening.
        let placeholder = State::open_managed(&self.tmp.path().join("placeholder")).unwrap();
        drop(std::mem::replace(&mut self.state, placeholder));
        self.state = State::open_managed(&self.tmp.path().join("authority")).unwrap();
        assert_eq!(
            self.state
                .namespace_initialize_with_policy(names::PORTABLE)
                .unwrap()
                .id,
            self.volume
        );
    }
}
#[test]
fn replacement_retains_both_payloads_and_exact_receipt_across_restart() {
    let mut w = World::new();
    let a = w.file("temporary 🦀");
    let a = w.publish(&a);
    let b = w.file("café");
    let b = w.publish(&b);
    let request = w.mv(&a, &w.root(), "cafe\u{301}", Some(&b));
    w.file("unrelated change");
    let receipt = w.state.namespace_apply(&request).unwrap();
    assert_eq!(receipt.node.id, a.id);
    assert_eq!(receipt.node.name, "cafe\u{301}");
    assert_eq!(receipt.node.payload, a.payload);
    assert_eq!(receipt.node.placement, a.placement);
    let Some(Artifact::Namespace { displaced: Some(t) }) = &receipt.artifact else {
        panic!()
    };
    assert_eq!(t.id, b.id);
    assert!(t.deleted);
    assert_eq!(t.payload, b.payload);
    assert_eq!(t.placement, b.placement);
    assert_eq!(t.revision, receipt.sequence);
    assert_eq!(w.node(&b.id), *t);
    let removed = w.rm(&receipt.node);
    let removed_receipt = w.state.namespace_apply(&removed).unwrap();
    assert!(removed_receipt.node.deleted);
    assert_eq!(removed_receipt.node.payload, a.payload);
    w.reopen();
    // Replaying the original replacement never resurrects the now-unlinked source.
    let sequence = w.state.namespace_volume().unwrap().sequence;
    assert_eq!(w.state.namespace_apply(&request), Ok(receipt.clone()));
    assert!(w.node(&a.id).deleted);
    assert_eq!(w.state.namespace_volume().unwrap().sequence, sequence);
    assert_eq!(
        w.state.namespace_operation(&w.volume, &request.operation),
        Ok(Some(Ok(receipt)))
    );
    assert_eq!(w.state.namespace_apply(&removed), Ok(removed_receipt));
}
#[test]
fn stale_source_target_parent_and_absence_guards_never_overwrite() {
    let mut w = World::new();
    let a = w.file("a");
    let b = w.file("b");
    let stale_source = w.mv(&a, &w.root(), "free", None);
    let old_target = w.mv(&b, &w.root(), "a", Some(&a));
    let a = w.publish(&a);
    assert_eq!(w.state.namespace_apply(&stale_source), Err(Error::Conflict));
    assert_eq!(w.state.namespace_apply(&old_target), Err(Error::Conflict));
    let absent = w.mv(&a, &w.root(), "winner", None);
    let winner = w.file("WINNER");
    assert_eq!(w.state.namespace_apply(&absent), Err(Error::AlreadyExists));
    let d = w.create(&w.root(), "parent", Kind::Directory);
    let nested = w.create(&d, "child", Kind::EmptyFile);
    let source_parent = w.mv(&nested, &w.root(), "child", None);
    let destination_parent = w.mv(&a, &d, "inside", None);
    let move_dir = w.mv(&d, &w.root(), "moved parent", None);
    w.state.namespace_apply(&move_dir).unwrap();
    assert_eq!(
        w.state.namespace_apply(&source_parent),
        Err(Error::Conflict)
    );
    assert_eq!(
        w.state.namespace_apply(&destination_parent),
        Err(Error::Conflict)
    );
    // A retained rejection is stable even if the name later becomes available.
    let rm = w.rm(&winner);
    w.state.namespace_apply(&rm).unwrap();
    assert_eq!(w.state.namespace_apply(&absent), Err(Error::AlreadyExists));
    assert_eq!(w.node(&a.id), a);
    assert_eq!(w.node(&b.id), b);
}
#[test]
fn ignored_and_failed_sql_updates_roll_back_replacement_and_dedup() {
    for trigger in [
        "CREATE TRIGGER fail BEFORE UPDATE ON namespace_nodes WHEN NEW.deleted=1 BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER fail BEFORE UPDATE ON namespace_nodes WHEN NEW.name='target' AND NEW.deleted=0 BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER fail BEFORE INSERT ON namespace_operations BEGIN SELECT RAISE(ABORT,'injected'); END;",
        "CREATE TRIGGER fail BEFORE INSERT ON namespace_operations BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER fail BEFORE UPDATE ON namespace_volume BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER fail BEFORE UPDATE ON namespace_volume BEGIN SELECT RAISE(ABORT,'injected'); END;",
    ] {
        let mut w = World::new();
        let a = w.file("a");
        let b = w.file("target");
        let r = w.mv(&a, &w.root(), "target", Some(&b));
        let sequence = w.state.namespace_volume().unwrap().sequence;
        w.state.connection.execute_batch(trigger).unwrap();
        assert_eq!(
            w.state.namespace_apply(&r),
            Err(Error::Internal),
            "{trigger}"
        );
        assert_eq!(w.node(&a.id), a);
        assert_eq!(w.node(&b.id), b);
        assert_eq!(w.state.namespace_volume().unwrap().sequence, sequence);
        assert_eq!(
            w.state.namespace_operation(&w.volume, &r.operation),
            Ok(None)
        );
        w.state
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        assert!(w.state.namespace_apply(&r).is_ok());
    }
}
#[test]
fn directory_rules_cycle_case_only_moves_and_wrong_parent_are_guarded() {
    let mut w = World::new();
    let d = w.create(&w.root(), "dir", Kind::Directory);
    let nested = w.create(&d, "nested", Kind::Directory);
    let child = w.create(&nested, "child", Kind::EmptyFile);
    let r = w.mv(&d, &nested, "cycle", None);
    assert_eq!(w.state.namespace_apply(&r), Err(Error::Cycle));
    let r = w.rm(&nested);
    assert_eq!(w.state.namespace_apply(&r), Err(Error::NotEmpty));
    let file = w.file("file");
    let r = w.mv(&d, &w.root(), "file", Some(&file));
    assert_eq!(w.state.namespace_apply(&r), Err(Error::NotDirectory));
    let r = w.mv(&file, &w.root(), "dir", Some(&d));
    assert_eq!(w.state.namespace_apply(&r), Err(Error::IsDirectory));
    let other = w.create(&w.root(), "other", Kind::Directory);
    let r = w.mv(&other, &d, "nested", Some(&nested));
    assert_eq!(w.state.namespace_apply(&r), Err(Error::NotEmpty));
    let r = w.request(Mutation::RemoveEntry {
        node: guard(&child),
        parent: guard(&w.root()),
    });
    assert_eq!(w.state.namespace_apply(&r), Err(Error::Conflict));
    let r = w.mv(&file, &w.root(), "FILE", Some(&file));
    let renamed = w.state.namespace_apply(&r).unwrap();
    assert_eq!(renamed.node.id, file.id);
    assert_eq!(renamed.node.name, "FILE");
    assert_eq!(
        renamed.artifact,
        Some(Artifact::Namespace { displaced: None })
    );
    let rm = w.rm(&child);
    w.state.namespace_apply(&rm).unwrap();
    let r = w.mv(&other, &d, "nested", Some(&nested));
    let receipt = w.state.namespace_apply(&r).unwrap();
    assert_eq!(receipt.node.id, other.id);
    assert_eq!(receipt.node.parent, Some(d.id));
    assert!(w.node(&nested.id).deleted);
    let r = w.request(Mutation::RemoveEntry {
        node: guard(&w.root()),
        parent: guard(&w.root()),
    });
    assert_eq!(w.state.namespace_apply(&r), Err(Error::Root));
}

#[test]
fn real_sqlite_full_rolls_back_both_names_and_leaves_exact_retry_pending() {
    let mut w = World::new();
    let mut requests = Vec::new();
    for i in 0..64 {
        let a = w.file(&format!("temporary-{i} 🦀"));
        let b = w.file(&format!("destination-{i} café"));
        let request = w.mv(&a, &w.root(), &b.name, Some(&b));
        requests.push((request, a, b));
    }
    w.state
        .connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let pages: i64 = w
        .state
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    w.state
        .connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let mut reached_full = false;
    for (request, a, b) in requests {
        let sequence = w.state.namespace_volume().unwrap().sequence;
        match w.state.namespace_apply(&request) {
            Ok(_) => (),
            Err(Error::Internal) => {
                assert_eq!(w.node(&a.id), a);
                assert_eq!(w.node(&b.id), b);
                assert_eq!(w.state.namespace_volume().unwrap().sequence, sequence);
                assert_eq!(
                    w.state.namespace_operation(&w.volume, &request.operation),
                    Ok(None)
                );
                w.state
                    .connection
                    .pragma_update(None, "max_page_count", pages + 1024)
                    .unwrap();
                let receipt = w.state.namespace_apply(&request).unwrap();
                assert_eq!(receipt.node.id, a.id);
                assert!(w.node(&b.id).deleted);
                reached_full = true;
                break;
            }
            e => panic!("unexpected replacement result: {e:?}"),
        }
    }
    assert!(
        reached_full,
        "must reach real SQLITE_FULL during replacement"
    );
}

#[test]
fn corrupt_keys_future_hints_changed_request_and_wrong_volume_fail_closed() {
    let mut w = World::new();
    let a = w.file("a");
    let b = w.file("café");
    let request = w.mv(&a, &w.root(), "CAFÉ", Some(&b));
    w.state
        .connection
        .execute(
            "UPDATE namespace_nodes SET name_key='wrong' WHERE node_id=?",
            [&b.id],
        )
        .unwrap();
    assert_eq!(w.state.namespace_apply(&request), Err(Error::Internal));
    assert_eq!(w.node(&a.id), a);
    assert_eq!(
        w.state.namespace_operation(&w.volume, &request.operation),
        Ok(None)
    );
    w.state
        .connection
        .execute(
            "UPDATE namespace_nodes SET name_key=? WHERE node_id=?",
            params![names::collision_key(&b.name), b.id],
        )
        .unwrap();
    let receipt = w.state.namespace_apply(&request).unwrap();
    let mut changed = request.clone();
    let Mutation::MoveEntry { name, .. } = &mut changed.mutation else {
        panic!()
    };
    *name = "different".into();
    assert_eq!(
        w.state.namespace_apply(&changed),
        Err(Error::OperationMismatch)
    );
    let mut future = w.rm(&receipt.node);
    future.expected_sequence += 1;
    assert_eq!(w.state.namespace_apply(&future), Err(Error::Conflict));
    let mut wrong = w.rm(&receipt.node);
    wrong.volume = "f".repeat(32);
    assert_eq!(w.state.namespace_apply(&wrong), Err(Error::WrongVolume));
    assert_eq!(w.node(&a.id), receipt.node);
}

#[test]
fn destination_child_limit_allows_replacement_but_refuses_growth() {
    let mut w = World::new();
    let source = w.file("source");
    let folder = w.create(&w.root(), "full folder", Kind::Directory);
    let target = w.create(&folder, "target", Kind::EmptyFile);
    {
        let tx = w.state.connection.transaction().unwrap();
        for i in 1..MAX_CHILDREN {
            let name = format!("retained-{i}");
            tx.execute("INSERT INTO namespace_nodes(node_id,parent_id,name,kind,revision,deleted,name_key) VALUES(?1,?2,?3,'directory',0,0,?4)",params![format!("{:032x}",10000+i),folder.id,name,names::collision_key(&name)]).unwrap();
        }
        tx.commit().unwrap();
    }
    let growing = w.mv(&source, &folder, "new child", None);
    assert_eq!(w.state.namespace_apply(&growing), Err(Error::Limit));
    assert_eq!(w.node(&source.id), source);
    let replacement = w.mv(&source, &folder, "target", Some(&target));
    assert!(w.state.namespace_apply(&replacement).is_ok());
    assert_eq!(
        w.state
            .namespace_list(&w.volume, &folder.id)
            .unwrap()
            .children
            .len(),
        MAX_CHILDREN
    );
}
