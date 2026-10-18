use super::*;
use crate::{
    probe::tests::{Mode, Object, Server},
    uploads::{Executor, policy},
};
use s3drive_core::manifest::{self, Entry};
use s3drive_state::{
    cache,
    journal::{self, Base},
    worker::Storage,
};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};

async fn executor(server: &Server, path: &Path) -> Executor {
    let s = server.session().await;
    let policy = policy::tests::fixture_policy(&s);
    let storage = Storage::open(path, cache::Options::default(), journal::Options::default())
        .await
        .unwrap();
    Executor::open(s.reader, storage, policy).await.unwrap()
}
async fn plan(e: &Executor, bytes: &[u8]) -> (chunked::Input, journal::GenerationId) {
    let input = chunked::Input {
        id: [7; 32],
        volume: [1; 16],
        node: [2; 16],
        store: [3; 16],
        generation: [4; 16],
        root: "managed/".into(),
        source: e.0.session.binding_id,
        fingerprint: e.0.session.reader.configuration_fingerprint().into(),
        parent: None,
        size: bytes.len() as u64,
        expected_content: Some([9; 32]),
    };
    let p = e.storage().chunked(chunked::Options::default()).unwrap();
    p.freeze(input.clone()).await.unwrap();
    let j = e.storage().staging();
    let g = j
        .begin_operation(
            input.staging_operation().unwrap(),
            input.local_target().unwrap(),
            Base::Missing,
            None,
            input.size,
        )
        .await
        .unwrap();
    let mut offset = 0;
    for b in bytes.chunks(manifest::CHUNK_BYTES as usize) {
        j.append_at(g, offset, Arc::from(b)).await.unwrap();
        offset += b.len() as u64;
    }
    j.seal(g).await.unwrap();
    p.attach(input.id, g).await.unwrap();
    p.prepare(input.id).await.unwrap();
    (input, g)
}
fn uploader(e: &Executor) -> Uploader {
    Uploader::new(e.clone(), chunked::Options::default()).unwrap()
}

#[tokio::test]
async fn binary_unicode_holes_and_empty_tree_uploads_match_verified_pages() {
    let mut body = (0..manifest::CHUNK_BYTES)
        .map(|n| (n % 251) as u8)
        .collect::<Vec<_>>();
    body.resize(2 * manifest::CHUNK_BYTES as usize, 0);
    body.extend_from_slice("tail café 🦀 日本語\0".as_bytes());
    for bytes in [body.as_slice(), b"", &[0; 17]] {
        let server = Server::new(Mode::Normal).await;
        let tmp = tempfile::tempdir().unwrap();
        let e = executor(&server, &tmp.path().join("state")).await;
        let (i, logical) = plan(&e, bytes).await;
        let p = e.storage().chunked(chunked::Options::default()).unwrap();
        let uploaded = uploader(&e).upload_tree(i.id).await.unwrap();
        let root = p.bytes(i.id, 0, 0).await.unwrap().unwrap();
        let page = uploaded.root().verify_page(&root.bytes).unwrap();
        assert_eq!(page.page().length(), bytes.len() as u64);
        let root_expect = p.transfer(i.id, 0, 0).await.unwrap().unwrap();
        assert_ne!(
            root_expect.payload_sha256(),
            root_expect.reference().sha256()
        );
        assert_eq!(
            uploaded.root_receipt().info().payload_sha256,
            *root_expect.payload_sha256()
        );
        let objects = server.model.lock().unwrap().objects.clone();
        assert_eq!(
            objects[&format!("/bucket/root/{}", root.target.object_key())].bytes,
            root.bytes
        );
        let mut at = 0;
        for entry in page.page().entries() {
            match entry {
                Entry::Hole { length } => {
                    assert!(bytes[at..at + *length as usize].iter().all(|b| *b == 0))
                }
                Entry::Data { length, object } => {
                    let slot = at as u64 / manifest::CHUNK_BYTES;
                    let t = p.transfer(i.id, -1, slot).await.unwrap().unwrap();
                    assert_eq!(t.reference(), object);
                    let got = &objects[&format!("/bucket/root/{}", t.target().object_key())].bytes;
                    assert_eq!(got, &bytes[at..at + *length as usize]);
                    assert_eq!(<[u8; 32]>::from(Sha256::digest(got)), *object.sha256());
                }
                Entry::Child { .. } => panic!("unexpected parent"),
            }
            at += entry.length() as usize;
        }
        assert_eq!(at, bytes.len());
        assert_eq!(server.count("PUT"), objects.len());
        let counts = (
            server.count("PUT"),
            server.count("GET"),
            server.count("HEAD"),
        );
        assert_eq!(
            uploader(&e).upload_tree(i.id).await.unwrap().root(),
            uploaded.root()
        );
        assert_eq!(
            counts,
            (
                server.count("PUT"),
                server.count("GET"),
                server.count("HEAD")
            )
        );
        assert!(
            e.storage()
                .uploads()
                .flush_receipt(logical)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            e.storage()
                .staging()
                .pending(None, 100)
                .await
                .unwrap()
                .iter()
                .any(|g| g.id == logical)
        );
        e.storage().close().await.unwrap();
    }
}

#[tokio::test]
async fn parent_refused_until_child_is_verified_and_locally_acknowledged() {
    let server = Server::new(Mode::Normal).await;
    let tmp = tempfile::tempdir().unwrap();
    let e = executor(&server, &tmp.path().join("state")).await;
    let (i, _) = plan(&e, b"child").await;
    let p = e.storage().chunked(chunked::Options::default()).unwrap();
    assert!(uploader(&e).upload_object(i.id, 0, 0).await.is_err());
    assert_eq!(server.count("PUT"), 0);
    assert!(
        p.transfer(i.id, 0, 0)
            .await
            .unwrap()
            .unwrap()
            .generation()
            .is_none()
    );
    let child = p
        .materialize(i.id, -1, 0)
        .await
        .unwrap()
        .unwrap()
        .generation()
        .unwrap();
    assert_eq!(
        e.submit(child).await.unwrap().phase,
        remote::Phase::Verified
    );
    assert!(uploader(&e).upload_object(i.id, 0, 0).await.is_err());
    assert_eq!(server.count("PUT"), 1);
    e.flush(child).await.unwrap();
    uploader(&e)
        .upload_object(i.id, 0, 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(server.count("PUT"), 2);
    e.storage().close().await.unwrap();
}

#[tokio::test]
async fn lost_commit_response_and_restart_keep_each_physical_identity_without_reput() {
    let server = Server::new(Mode::DropAfterCommit).await;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let e = executor(&server, &path).await;
    let (i, _) = plan(&e, "café 🦀".as_bytes()).await;
    let uploaded = uploader(&e).upload_tree(i.id).await.unwrap();
    let checksum = *uploaded.root_receipt().checksum();
    let p = e.storage().chunked(chunked::Options::default()).unwrap();
    let before = [
        p.transfer(i.id, -1, 0).await.unwrap().unwrap(),
        p.transfer(i.id, 0, 0).await.unwrap().unwrap(),
    ];
    assert_eq!(server.count("PUT"), 2);
    e.storage().close().await.unwrap();
    let e = executor(&server, &path).await;
    assert_eq!(
        uploader(&e)
            .upload_tree(i.id)
            .await
            .unwrap()
            .root_receipt()
            .checksum(),
        &checksum
    );
    let p = e.storage().chunked(chunked::Options::default()).unwrap();
    assert_eq!(
        before,
        [
            p.transfer(i.id, -1, 0).await.unwrap().unwrap(),
            p.transfer(i.id, 0, 0).await.unwrap().unwrap()
        ]
    );
    assert_eq!(server.count("PUT"), 2);
    e.storage().close().await.unwrap();
}

#[tokio::test]
async fn collision_retains_rival_bytes_rejected_child_blocks_parent_without_retry() {
    let server = Server::new(Mode::Normal).await;
    let tmp = tempfile::tempdir().unwrap();
    let e = executor(&server, &tmp.path().join("state")).await;
    let (i, _) = plan(&e, b"ours").await;
    let p = e.storage().chunked(chunked::Options::default()).unwrap();
    let t = p.transfer(i.id, -1, 0).await.unwrap().unwrap();
    let key = format!("/bucket/root/{}", t.target().object_key());
    server.model.lock().unwrap().objects.insert(
        key.clone(),
        Object {
            bytes: b"rival".to_vec(),
            tag: "\"rival\"".into(),
            operation: "foreign".into(),
            checksum: "foreign".into(),
        },
    );
    assert!(uploader(&e).upload_tree(i.id).await.is_err());
    let g = p
        .transfer(i.id, -1, 0)
        .await
        .unwrap()
        .unwrap()
        .generation()
        .unwrap();
    assert_eq!(
        e.storage()
            .uploads()
            .for_generation(g)
            .await
            .unwrap()
            .unwrap()
            .phase,
        remote::Phase::Rejected
    );
    assert!(uploader(&e).upload_object(i.id, 0, 0).await.is_err());
    assert!(uploader(&e).upload_tree(i.id).await.is_err());
    assert_eq!(server.count("PUT"), 1);
    assert_eq!(server.model.lock().unwrap().objects[&key].bytes, b"rival");
    assert_eq!(
        e.storage().staging().read(g, 0, 100).await.unwrap(),
        b"ours"
    );
    e.storage().close().await.unwrap();
}

#[tokio::test]
async fn uncertain_absent_child_is_not_resent_and_blocks_its_page() {
    let server = Server::new(Mode::DropBeforeCommit).await;
    let tmp = tempfile::tempdir().unwrap();
    let e = executor(&server, &tmp.path().join("state")).await;
    let (i, _) = plan(&e, b"retain me").await;
    assert!(uploader(&e).upload_tree(i.id).await.is_err());
    server.model.lock().unwrap().mode = Mode::Normal;
    assert!(uploader(&e).upload_tree(i.id).await.is_err());
    assert!(uploader(&e).upload_object(i.id, 0, 0).await.is_err());
    assert_eq!(server.count("PUT"), 1);
    assert!(server.model.lock().unwrap().objects.is_empty());
    e.storage().close().await.unwrap();
}

#[tokio::test]
async fn policy_scope_and_binding_fail_before_physical_staging_or_network() {
    let server = Server::new(Mode::Normal).await;
    let tmp = tempfile::tempdir().unwrap();
    let e = executor(&server, &tmp.path().join("state")).await;
    let (i, _) = plan(&e, b"child").await;
    let session = server.session().await;
    let (text, plan, report) = policy::tests::documents(&session);
    let text = text.replace("writable_prefix = ''", "writable_prefix = 'elsewhere/'");
    let narrowed =
        policy::WritePolicy::from_toml(&session.reader.source, &text, &plan, &report).unwrap();
    let denied = Executor::open(session.reader, e.storage(), narrowed)
        .await
        .unwrap();
    assert!(uploader(&denied).upload_tree(i.id).await.is_err());
    let p = e.storage().chunked(chunked::Options::default()).unwrap();
    assert!(
        p.transfer(i.id, -1, 0)
            .await
            .unwrap()
            .unwrap()
            .generation()
            .is_none()
    );
    // A distinct source binding is frozen in a second valid plan; it must not be
    // staged under this executor even though its object key falls in scope.
    let mut wrong = i.clone();
    wrong.id = [8; 32];
    wrong.generation = [5; 16];
    wrong.source = [99; 32];
    p.freeze(wrong.clone()).await.unwrap();
    let j = e.storage().staging();
    let g = j
        .begin_operation(
            wrong.staging_operation().unwrap(),
            wrong.local_target().unwrap(),
            Base::Missing,
            None,
            wrong.size,
        )
        .await
        .unwrap();
    j.append_at(g, 0, Arc::from(b"child".as_slice()))
        .await
        .unwrap();
    j.seal(g).await.unwrap();
    p.attach(wrong.id, g).await.unwrap();
    p.prepare(wrong.id).await.unwrap();
    assert!(uploader(&e).upload_tree(wrong.id).await.is_err());
    assert!(
        p.transfer(wrong.id, -1, 0)
            .await
            .unwrap()
            .unwrap()
            .generation()
            .is_none()
    );
    assert_eq!(server.count("PUT"), 0);
    e.storage().close().await.unwrap();
}

#[tokio::test]
async fn restart_claimed_object_recovers_after_policy_expiry_but_cannot_send_parent() {
    let server = Server::new(Mode::DropAfterCommit).await;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let e = executor(&server, &path).await;
    let (i, _) = plan(&e, b"recover exact object").await;
    let p = e.storage().chunked(chunked::Options::default()).unwrap();
    let g = p
        .materialize(i.id, -1, 0)
        .await
        .unwrap()
        .unwrap()
        .generation()
        .unwrap();
    let info = e.storage().uploads().prepare(g).await.unwrap();
    let permit = e
        .storage()
        .uploads()
        .claim(info.operation, info.target.clone())
        .await
        .unwrap();
    let intent = e.intent(&info).await.unwrap();
    assert!(matches!(
        e.0.session.put(&intent).await.unwrap(),
        crate::transport::Outcome::Unknown(_)
    ));
    drop(permit);
    e.storage().close().await.unwrap();
    let session = server.session().await;
    let (text, plan, report) = policy::tests::documents(&session);
    let mut plan: toml::Value = toml::from_str(&plan).unwrap();
    plan["created_unix_seconds"] = 1.into();
    let plan = toml::to_string(&plan).unwrap();
    let mut report: toml::Value = toml::from_str(&report).unwrap();
    report["plan_digest"] = crate::probe::Plan::from_toml(&plan)
        .unwrap()
        .digest()
        .unwrap()
        .into();
    report["observed_unix_seconds"] = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - 86401)
        .into();
    let report = toml::to_string(&report).unwrap();
    let mut text: toml::Value = toml::from_str(&text).unwrap();
    text["report_sha256"] = crate::transport::hex(&Sha256::digest(report.as_bytes())).into();
    text["writable_prefix"] = "elsewhere/".into();
    text["max_object_bytes"] = 1.into();
    let policy = policy::WritePolicy::from_toml(
        &session.reader.source,
        &toml::to_string(&text).unwrap(),
        &plan,
        &report,
    )
    .unwrap();
    let storage = Storage::open(
        &path,
        cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap();
    let e = Executor::open(session.reader, storage, policy)
        .await
        .unwrap();
    let receipt = uploader(&e)
        .upload_object(i.id, -1, 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(receipt.info().generation, g);
    assert_eq!(server.count("PUT"), 1);
    assert!(uploader(&e).upload_tree(i.id).await.is_err());
    assert!(
        e.storage()
            .chunked(chunked::Options::default())
            .unwrap()
            .transfer(i.id, 0, 0)
            .await
            .unwrap()
            .unwrap()
            .generation()
            .is_none()
    );
    assert_eq!(server.count("PUT"), 1);
    e.storage().close().await.unwrap();
}

#[tokio::test]
async fn sparse_multilevel_tree_uploads_children_before_root_with_only_four_objects() {
    let server = Server::new(Mode::Normal).await;
    let tmp = tempfile::tempdir().unwrap();
    let e = executor(&server, &tmp.path().join("state")).await;
    let (old, previous) = plan(&e, b"").await;
    let mut i = old;
    i.id = [8; 32];
    i.generation = [5; 16];
    i.parent = Some(previous);
    i.size = 128 * manifest::CHUNK_BYTES + 1;
    let p = e.storage().chunked(chunked::Options::default()).unwrap();
    p.freeze(i.clone()).await.unwrap();
    let j = e.storage().staging();
    let g = j
        .begin_operation(
            i.staging_operation().unwrap(),
            i.local_target().unwrap(),
            Base::Missing,
            i.parent,
            i.size,
        )
        .await
        .unwrap();
    j.append_zero_at(g, 0, i.size - 1).await.unwrap();
    j.append_at(g, i.size - 1, Arc::from(b"z".as_slice()))
        .await
        .unwrap();
    j.seal(g).await.unwrap();
    p.attach(i.id, g).await.unwrap();
    p.prepare(i.id).await.unwrap();
    assert!(uploader(&e).upload_object(i.id, 1, 0).await.is_err());
    assert_eq!(server.count("PUT"), 0);
    let got = uploader(&e).upload_tree(i.id).await.unwrap();
    assert_eq!(got.root().length(), i.size);
    assert_eq!(server.count("PUT"), 4);
    let root = p.bytes(i.id, 1, 0).await.unwrap().unwrap();
    let verified = got.root().verify_page(&root.bytes).unwrap();
    assert_eq!(verified.page().entries().len(), 2);
    for slot in 0..2 {
        let leaf = p.bytes(i.id, 0, slot).await.unwrap().unwrap();
        let t = p.transfer(i.id, 0, slot).await.unwrap().unwrap();
        assert!(
            e.storage()
                .uploads()
                .flush_receipt(t.generation().unwrap())
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            server.model.lock().unwrap().objects
                [&format!("/bucket/root/{}", leaf.target.object_key())]
                .bytes,
            leaf.bytes
        );
    }
    e.storage().close().await.unwrap();
}
