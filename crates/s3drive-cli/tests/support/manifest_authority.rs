//! Protocol metadata and explicit legacy-client refusal; this does NOT attest
//! uploaded chunks. The authority trusts its authenticated client's proof.
use super::*;
use s3drive_core::manifest::{Entry, Page, Reference, objects::RootDescriptor};
fn binary(id: &str) -> [u8; 16] {
    std::array::from_fn(|i| u8::from_str_radix(&id[2 * i..2 * i + 2], 16).unwrap())
}
fn unsupported<T>(result: s3drive_state::Result<T>) {
    let e = match result {
        Ok(_) => panic!("unsupported manifest was accepted"),
        Err(e) => e,
    };
    assert!(
        matches!(
            e.downcast_ref::<s3drive_managed::Failure>(),
            Some(s3drive_managed::Failure::UnsupportedContent)
        ),
        "{e}"
    );
}
pub async fn run(config: &s3drive_core::Config, root: &Path, wire: &WireClient, volume: &str) {
    let Reply::Features(features) = ask(wire, Request::Features {}).await else {
        panic!()
    };
    assert!(features.iter().any(|f| f == "manifest-root-v1"));
    let source = &config.sources[0];
    let s3 = s3drive_s3::S3Reader::from_source(source).await.unwrap();
    change(
        wire,
        200,
        Mutation::RegisterStore {
            route: s3.storage_fingerprint(),
            root: "writable/.s3drive/".into(),
        },
    )
    .await;
    let n = change(
        wire,
        10,
        Mutation::CreateEntry {
            parent: s3drive_authority::ROOT.into(),
            parent_revision: 0,
            store: Some(id(200)),
            name: "manifest café 日本語 🦀".into(),
            node_kind: Kind::EmptyFile,
        },
    )
    .await
    .node;
    let publisher = publisher(config, root, "client", wire.clone(), volume.into()).await;
    let reader = s3drive_managed::Reader::open(wire.clone(), volume.into(), vec![(id(200), s3)])
        .await
        .unwrap();
    let page = Page::new(
        binary(volume),
        binary(&n.id),
        0,
        0,
        vec![Entry::Hole {
            length: 9 * 1024 * 1024,
        }],
    )
    .unwrap();
    let body = page.encode();
    let root = RootDescriptor::new(
        binary(volume),
        binary(&n.id),
        page.length(),
        Reference::new(binary(&id(200)), binary(&id(20)), 1 << 56, page.digest()).unwrap(),
    )
    .unwrap();
    let request = Apply {
        volume: volume.into(),
        operation: id(20),
        expected_sequence: info(wire).await.sequence,
        mutation: Mutation::ReserveManifest {
            node: n.id.clone(),
            expected_revision: n.revision,
            store: id(200),
            size: body.len() as u64,
            sha256: sha(&body),
            manifest: Box::new(s3drive_authority::ManifestRoot::new(root).unwrap()),
            guard: s3drive_authority::ContentGuard {
                expected: None,
                detached: false,
                removal: None,
            },
        },
    };
    // Ordinary one-object staging must never accept a root and bypass the
    // separate durable chunk-plan/dependency/settlement controller.
    assert!(
        publisher
            .stage(request.clone(), id(21), Arc::from(body))
            .await
            .is_err()
    );
    assert!(
        publisher
            .storage()
            .staging()
            .pending(None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        publisher
            .storage()
            .staging()
            .drafts(None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    let Reply::Applied(reserved) = ask(
        wire,
        Request::Apply {
            request: request.clone(),
        },
    )
    .await
    else {
        panic!()
    };
    let Some(s3drive_authority::Artifact::Reservation(reservation)) = reserved.artifact.clone()
    else {
        panic!()
    };
    assert_eq!(reservation.logical_size().unwrap(), 9 * 1024 * 1024);
    assert_eq!(
        ask(
            wire,
            Request::Reservation {
                volume: volume.into(),
                reservation: id(20)
            }
        )
        .await,
        Reply::Reservation(reservation.clone())
    );
    assert_eq!(
        ask(
            wire,
            Request::Apply {
                request: request.clone()
            }
        )
        .await,
        Reply::Applied(reserved)
    );
    let mut changed = request;
    changed.expected_sequence += 1;
    assert_eq!(
        ask(wire, Request::Apply { request: changed }).await,
        Reply::Error(s3drive_authority::Error::OperationMismatch)
    );
    let published = change(
        wire,
        21,
        Mutation::Publish {
            reservation: id(20),
            upload_operation: "d".repeat(64),
            etag: "\"synthetic-metadata-proof\"".into(),
            version: None,
        },
    )
    .await;
    assert_eq!(
        published.node.payload.as_ref().unwrap().reservation,
        reservation
    );
    unsupported(reader.node(&n.id).await);
    unsupported(reader.list(s3drive_authority::ROOT).await);
    unsupported(reader.read(&n.id).await);
    unsupported(publisher.read(&n.id).await);
    assert!(
        publisher
            .storage()
            .staging()
            .pending(None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    publisher.storage().close().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manifest_root_protocol_and_legacy_client_refusal_use_real_authenticated_tcp() {
    let f = fixture::Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let cfg = writes::setup(&f, tmp.path()).await;
    let before = f.requests.lock().unwrap().len();
    let output = tokio::time::timeout(
        Duration::from_secs(120),
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "managed_child", "--nocapture"])
            .env("S3DRIVE_MANAGED_MODE", "manifest_authority")
            .env("S3DRIVE_MANAGED_ROOT", tmp.path())
            .env("S3DRIVE_MANAGED_CONFIG", &cfg.config)
            .env("AWS_SHARED_CREDENTIALS_FILE", &cfg.credentials)
            .env("AWS_CONFIG_FILE", &cfg.aws_config)
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("manifest protocol subprocess deadline")
    .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        f.requests.lock().unwrap().len(),
        before,
        "unsupported reads/stage sent an S3 request"
    );
}
