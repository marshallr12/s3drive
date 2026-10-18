//! Ordinary-account fixture preparation and inspection; no real credentials.
use super::*;
use serde::{Deserialize, Serialize};

fn harness(root: &Path) -> Harness {
    let profile = std::fs::read_to_string(root.join("profile-path.txt")).unwrap();
    Harness {
        root: root.into(),
        config: root.join("managed.toml"),
        credentials: Path::new(&profile).join(".aws/credentials"),
        mount: root.join("managed-mounted"),
    }
}
pub(super) fn initialize(root: &Path) {
    let h = harness(root);
    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("managed-fixture.json")).unwrap()).unwrap();
    let endpoint = marker["endpoint"].as_str().unwrap();
    let address = url::Url::parse(endpoint).unwrap();
    assert_eq!(address.scheme(), "http");
    assert!(
        address
            .host_str()
            .unwrap()
            .parse::<std::net::IpAddr>()
            .unwrap()
            .is_loopback()
    );
    for which in ["first", "second"] {
        assert_eq!(
            cfg(&root.join(which).join("drive.toml")).sources[0]
                .endpoint
                .as_deref(),
            Some(endpoint)
        );
    }
    let aws = h.credentials.parent().unwrap();
    std::fs::create_dir_all(aws).unwrap();
    std::fs::write(&h.credentials,"[fixture]\naws_access_key_id=TESTPROFILE\naws_secret_access_key=TESTPROFILESECRET\n[fixture-alt]\naws_access_key_id=TESTPROFILE\naws_secret_access_key=TESTPROFILESECRET\n").unwrap();
    std::fs::write(aws.join("config"), "").unwrap();
    for which in ["first", "second"] {
        let path = root.join(which).join("drive.toml");
        let mut value: toml::Value =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        value["writes"]["directory"] = root
            .join(which)
            .join("direct-state")
            .to_str()
            .unwrap()
            .into();
        for name in ["policy", "plan", "report"] {
            value["writes"]["sources"][0][name] = root
                .join(which)
                .join(format!("{name}.toml"))
                .to_str()
                .unwrap()
                .into();
        }
        let text = toml::to_string(&value).unwrap();
        s3drive_core::Config::parse(&text).unwrap();
        std::fs::write(path, text).unwrap();
    }
}
fn read_only(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
            | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .unwrap()
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Frozen {
    remote: Vec<(Vec<u8>, Vec<u8>, String)>,
    logical: Vec<(String, Vec<u8>, Option<Vec<u8>>)>,
}
fn frozen(root: &Path) -> Frozen {
    let db = read_only(&root.join("managed-write-state/metadata.sqlite3"));
    let remote = db
        .prepare(
            "SELECT operation_id,intent_hash,object_key FROM remote_intents ORDER BY operation_id",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let logical = db
        .prepare(
            "SELECT operation_id,input,reservation FROM managed_publications ORDER BY operation_id",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    Frozen { remote, logical }
}
async fn replay(root: &Path, w: &WireClient, volume: &str) {
    let (request, original): (
        Apply,
        std::result::Result<s3drive_authority::Receipt, s3drive_authority::Error>,
    ) = serde_json::from_slice(
        &std::fs::read(root.join("authority-inputs/managed-replay.json")).unwrap(),
    )
    .unwrap();
    let before = info(w).await.sequence;
    assert_eq!(request.volume, volume);
    assert_eq!(
        w.request(&Request::Operation {
            volume: volume.into(),
            operation: request.operation.clone()
        })
        .await
        .unwrap(),
        Reply::Operation(Some(original.clone()))
    );
    assert_eq!(
        w.request(&Request::Apply { request }).await.unwrap(),
        Reply::Applied(original.unwrap())
    );
    assert_eq!(
        info(w).await.sequence,
        before,
        "original seed replay repeated metadata effect"
    );
}
pub(super) async fn worker(root: &Path, mode: &str) {
    let h = harness(root);
    let identity: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("authority-identity.json")).unwrap())
            .unwrap();
    let volume = identity["volume"].as_str().unwrap();
    let w = WireClient::https(
        identity["endpoint"].as_str().unwrap(),
        Token::from_private_file(&root.join("authority-inputs/authority.token")).unwrap(),
        Some(&std::fs::read(root.join("authority-inputs/cert.pem")).unwrap()),
    )
    .unwrap();
    tokio::time::timeout(
        Duration::from_secs(if mode == "Recover" { 100 } else { 20 }),
        async {
            loop {
                if let Ok(Reply::Info(v)) = w.request(&Request::Info {}).await {
                    assert_eq!(v.id, volume);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
        },
    )
    .await
    .unwrap();
    match mode {
        "Seed" => {
            let endpoint = cfg(&root.join("first/drive.toml")).sources[0]
                .endpoint
                .clone()
                .unwrap();
            h.configure_connection(
                identity["endpoint"].as_str().unwrap(),
                volume,
                &root.join("authority-inputs/authority.token"),
                Some(&root.join("authority-inputs/cert.pem")),
                &endpoint,
            );
            h.child("seed").await;
            h.configure_bootstrap(true, false).await;
            let db = read_only(&root.join("authority-state/metadata.sqlite3"));
            let bytes: (Vec<u8>, Vec<u8>) = db
                .query_row(
                    "SELECT request,outcome FROM namespace_operations WHERE operation_id=?1",
                    [id(10)],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            let request: Apply = serde_json::from_slice(&bytes.0).unwrap();
            let outcome: std::result::Result<s3drive_authority::Receipt, s3drive_authority::Error> =
                serde_json::from_slice(&bytes.1).unwrap();
            s3drive_state::create_private_document(
                &root.join("authority-inputs"),
                "managed-replay.json",
                &serde_json::to_vec(&(request, outcome)).unwrap(),
            )
            .unwrap();
            replay(root, &w, volume).await;
        }
        "Verify" | "Recover" => replay(root, &w, volume).await,
        "Snapshot" => {
            let snapshot = frozen(root);
            assert_eq!(snapshot.remote.len(), 1);
            assert_eq!(snapshot.logical.len(), 1);
            assert!(snapshot.logical[0].2.is_some());
            let db = read_only(&root.join("managed-write-state/metadata.sqlite3"));
            assert_eq!(
                db.query_row("SELECT phase FROM remote_intents", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "uncertain"
            );
            s3drive_state::create_private_document(
                &root.join("authority-inputs"),
                "managed-unknown.json",
                &serde_json::to_vec(&snapshot).unwrap(),
            )
            .unwrap();
        }
        "CheckRecovered" => {
            let original: Frozen = serde_json::from_slice(
                &std::fs::read(root.join("authority-inputs/managed-unknown.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                frozen(root),
                original,
                "coordinator recovery changed exact unknown operations"
            );
            let db = read_only(&root.join("managed-write-state/metadata.sqlite3"));
            assert_eq!(
                db.query_row("SELECT phase FROM remote_intents", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "verified"
            );
            replay(root, &w, volume).await;
        }
        "Rival" => publish(root, &w, "first", 200, 11, 1040, &data(1)).await,
        "CheckConflict" => {
            let c = cfg(&h.config);
            let fs = s3drive_fs::ReadOnlyFs::new(&c).await.unwrap();
            let d = fs.lookup(s3drive_fs::ROOT, DIR).await.unwrap();
            let n = fs.lookup(d.id, FILE).await.unwrap();
            assert!(fs.open_write(n.id).await.is_err());
            assert!(fs.flush_all().await.is_err());
            fs.cache_control().unwrap().close().await.unwrap();
            drop(fs);
            let storage = Storage::open(
                &root.join("managed-write-state"),
                cache::Options::default(),
                journal::Options::default(),
            )
            .await
            .unwrap();
            let local = storage
                .logical()
                .lookup(volume.into(), id(11))
                .await
                .unwrap()
                .unwrap();
            assert_ne!(
                local.head, local.settled,
                "reopening discarded the conflicting dirty head"
            );
            assert_eq!(
                storage
                    .staging()
                    .read(local.head.unwrap(), 0, 14)
                    .await
                    .unwrap(),
                b"dirty conflict"
            );
            storage.close().await.unwrap();
        }
        _ => panic!("unknown managed session fixture action"),
    }
}
