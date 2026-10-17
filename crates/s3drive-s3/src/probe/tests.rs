use super::*;
use aws_sdk_s3::config::{Credentials, SharedCredentialsProvider};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
    task::{JoinHandle, JoinSet},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Normal,
    IgnoreGuards,
    DropAfterCommit,
    DropBeforeCommit,
    DeleteAfterCommit,
    Status503,
    MissingEtag,
    CorruptGet,
    WrongMetadata,
    StallPut,
    NullVersion,
    HideListing,
    StallAfterCommit,
    StallGet,
    Reject409,
    FalseReceipt,
    OtherAfterCommit,
    ShortGet,
    RevokeReads,
}
#[derive(Clone)]
pub(crate) struct Object {
    pub(crate) bytes: Vec<u8>,
    pub(crate) tag: String,
    pub(crate) operation: String,
    pub(crate) checksum: String,
}
pub(crate) struct Model {
    pub(crate) objects: BTreeMap<String, Object>,
    requests: Vec<(String, String)>,
    serial: u64,
    pub(crate) mode: Mode,
}
pub(crate) struct Server {
    pub(crate) endpoint: String,
    pub(crate) model: Arc<Mutex<Model>>,
    task: JoinHandle<()>,
    pub(crate) received: Arc<Notify>,
    versioned: Arc<AtomicBool>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    pub(crate) async fn new(mode: Mode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let model = Arc::new(Mutex::new(Model {
            objects: BTreeMap::new(),
            requests: vec![],
            serial: 0,
            mode,
        }));
        let data = model.clone();
        let received = Arc::new(Notify::new());
        let signal = received.clone();
        let versioned = Arc::new(AtomicBool::new(false));
        let versions = versioned.clone();
        let task = tokio::spawn(async move {
            let mut workers = JoinSet::new();
            loop {
                tokio::select! {
                    socket=listener.accept()=> {
                        let (mut socket,_)=socket.unwrap();let data=data.clone();let signal=signal.clone();let versioned=versions.load(Ordering::SeqCst);
                        workers.spawn(async move {
                            tokio::time::timeout(Duration::from_secs(10),async move {
                                let mut raw=vec![];
                                while !raw.ends_with(b"\r\n\r\n") {assert!(raw.len()<32*1024);raw.push(socket.read_u8().await.unwrap());}
                                let raw=String::from_utf8(raw).unwrap();
                                let headers=raw.lines().skip(1).filter_map(|line|line.split_once(':')).map(|(k,v)|(k.to_ascii_lowercase(),v.trim().to_string())).collect::<HashMap<_,_>>();
                                assert!(headers["authorization"].contains("Credential=TESTACCESS/"));
                                assert!(!headers.contains_key("transfer-encoding"),"unexpected streaming transfer framing");
                                if headers.get("expect").is_some_and(|v|v.eq_ignore_ascii_case("100-continue")) {socket.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await.unwrap();}
                                let length=headers.get("content-length").map(|v|v.parse::<usize>().unwrap()).unwrap_or(0);assert!(length<=transport::MAX_PUT);
                                let mut body=vec![0;length];socket.read_exact(&mut body).await.unwrap();
                                let mut words=raw.lines().next().unwrap().split_whitespace();let method=words.next().unwrap();let target=words.next().unwrap();
                                let url=url::Url::parse(&format!("http://fixture{target}")).unwrap();
                                let path=percent_encoding::percent_decode_str(url.path()).decode_utf8().unwrap().to_string();
                                let query=url.query_pairs().into_owned().collect::<HashMap<_,_>>();
                                let reply={let mut model=data.lock().unwrap();model.requests.push((method.into(),target.into()));respond(&mut model,method,&path,&query,&headers,body,versioned)};
                                signal.notify_one();
                                match reply {
                                    Reply::Drop=>{},
                                    Reply::Stall=>{let _=socket.read(&mut [0]).await;},
                                    Reply::Data(bytes)=>{let _=socket.write_all(&bytes).await;},
                                }
                            }).await.unwrap();
                        });
                    },
                    result=workers.join_next(),if !workers.is_empty()=>{result.unwrap().unwrap();},
                }
            }
        });
        Self {
            endpoint,
            model,
            task,
            received,
            versioned,
        }
    }
    pub(crate) fn source(&self) -> Source {
        source_at(&self.endpoint)
    }
    pub(crate) fn reader(&self) -> S3Reader {
        S3Reader::with_credentials(
            self.source(),
            SharedCredentialsProvider::new(Credentials::new(
                "TESTACCESS",
                "TESTSECRET",
                Some("TESTTOKEN".into()),
                None,
                "test",
            )),
            Duration::from_secs(3),
            3,
        )
        .unwrap()
    }
    fn plan(&self) -> Plan {
        Plan::build(&self.source(), "07".repeat(16), 1).unwrap()
    }
    pub(crate) async fn session(&self) -> Session {
        Session::pin(self.reader()).await.unwrap()
    }
    pub(crate) fn count(&self, method: &str) -> usize {
        self.model
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|(m, _)| m == method)
            .count()
    }
}
pub(crate) fn source_at(endpoint: &str) -> Source {
    s3drive_core::Config::parse(&format!("version=1\nmode='read-only'\n[[sources]]\nalias='fixture'\nbucket='bucket'\nregion='us-east-1'\nprefix='root/'\nendpoint='{endpoint}'\npath_style=true\nallow_http_loopback=true\n")).unwrap().sources.remove(0)
}
enum Reply {
    Data(Vec<u8>),
    Drop,
    Stall,
}
fn response(status: u16, headers: &str, body: &[u8], length: usize) -> Reply {
    let mut result = format!(
        "HTTP/1.1 {status} Test\r\nConnection: close\r\nContent-Length: {length}\r\n{headers}\r\n"
    )
    .into_bytes();
    result.extend_from_slice(body);
    Reply::Data(result)
}
fn error(status: u16) -> Reply {
    let body = format!(
        "<Error><Code>{}</Code><Message>fixture</Message></Error>",
        if status == 412 {
            "PreconditionFailed"
        } else {
            "ServiceUnavailable"
        }
    );
    response(
        status,
        "Content-Type: application/xml\r\n",
        body.as_bytes(),
        body.len(),
    )
}
fn respond(
    model: &mut Model,
    method: &str,
    path: &str,
    query: &HashMap<String, String>,
    headers: &HashMap<String, String>,
    body: Vec<u8>,
    versioned: bool,
) -> Reply {
    if query.contains_key("versioning") {
        let body = format!(
            "<VersioningConfiguration xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">{}</VersioningConfiguration>",
            if versioned {
                "<Status>Enabled</Status>"
            } else {
                ""
            }
        );
        return response(
            200,
            "Content-Type: application/xml\r\n",
            body.as_bytes(),
            body.len(),
        );
    }
    if query.get("list-type").is_some_and(|v| v == "2") {
        let prefix = format!("/bucket/{}", query["prefix"]);
        let mut entries = String::new();
        for (key, obj) in &model.objects {
            if model.mode == Mode::HideListing {
                continue;
            }
            if key.starts_with(&prefix) {
                let encoded = percent_encoding::utf8_percent_encode(
                    key.strip_prefix("/bucket/").unwrap(),
                    percent_encoding::NON_ALPHANUMERIC,
                );
                entries.push_str(&format!(
                    "<Contents><Key>{encoded}</Key><Size>{}</Size><ETag>{}</ETag></Contents>",
                    obj.bytes.len(),
                    obj.tag
                ));
            }
        }
        let body = format!(
            "<ListBucketResult><EncodingType>url</EncodingType><IsTruncated>false</IsTruncated>{entries}</ListBucketResult>"
        );
        return response(
            200,
            "Content-Type: application/xml\r\n",
            body.as_bytes(),
            body.len(),
        );
    }
    if method == "PUT" {
        assert_eq!(headers.get("content-encoding"), None);
        assert!(headers.contains_key("x-amz-meta-s3drive-operation"));
        assert!(headers.contains_key("x-amz-meta-s3drive-sha256"));
        assert_eq!(
            headers["x-amz-meta-s3drive-sha256"],
            hex(Sha256::digest(&body).as_slice())
        );
        assert!(headers.contains_key("if-match") ^ headers.contains_key("if-none-match"));
        if model.mode == Mode::Status503 {
            return error(503);
        }
        if model.mode == Mode::Reject409 {
            return error(409);
        }
        if model.mode == Mode::DropBeforeCommit {
            return Reply::Drop;
        }
        if model.mode == Mode::StallPut {
            return Reply::Stall;
        }
        if model.mode != Mode::IgnoreGuards
            && ((headers.get("if-none-match").is_some_and(|v| v == "*")
                && model.objects.contains_key(path))
                || headers
                    .get("if-match")
                    .is_some_and(|tag| model.objects.get(path).is_none_or(|obj| obj.tag != *tag)))
        {
            return error(412);
        }
        model.serial += 1;
        let tag = format!("\"opaque-revision-{}\"", model.serial);
        model.objects.insert(
            path.into(),
            Object {
                bytes: body,
                tag: tag.clone(),
                operation: headers["x-amz-meta-s3drive-operation"].clone(),
                checksum: headers["x-amz-meta-s3drive-sha256"].clone(),
            },
        );
        if model.mode == Mode::DeleteAfterCommit {
            model.objects.remove(path);
            return Reply::Drop;
        }
        if model.mode == Mode::DropAfterCommit {
            return Reply::Drop;
        }
        if model.mode == Mode::StallAfterCommit {
            return Reply::Stall;
        }
        if model.mode == Mode::OtherAfterCommit {
            let other = model.objects.get_mut(path).unwrap();
            other.operation = "foreign".into();
            return Reply::Drop;
        }
        let version = if model.mode == Mode::NullVersion {
            "x-amz-version-id: null\r\n"
        } else {
            ""
        };
        return response(
            200,
            &if model.mode == Mode::MissingEtag {
                String::new()
            } else {
                format!(
                    "ETag: {}\r\n{version}",
                    if model.mode == Mode::FalseReceipt {
                        "\"false-receipt\""
                    } else {
                        &tag
                    }
                )
            },
            b"",
            0,
        );
    }
    let Some(obj) = model.objects.get(path) else {
        return error(404);
    };
    if method == "DELETE" {
        model.objects.remove(path);
        return response(204, "", b"", 0);
    }
    if method == "HEAD" || method == "GET" {
        if model.mode == Mode::RevokeReads {
            return error(403);
        }
        if method == "GET" && model.mode == Mode::StallGet {
            return Reply::Stall;
        }
        if method == "GET" && headers.get("if-match") != Some(&obj.tag) {
            return error(412);
        }
        let operation = if method == "GET" && model.mode == Mode::WrongMetadata {
            "other-operation"
        } else {
            &obj.operation
        };
        let version = if model.mode == Mode::NullVersion {
            "x-amz-version-id: null\r\n"
        } else {
            ""
        };
        let head = format!(
            "ETag: {}\r\nx-amz-meta-s3drive-operation: {operation}\r\nx-amz-meta-s3drive-sha256: {}\r\n{version}",
            obj.tag, obj.checksum
        );
        if method == "HEAD" {
            return response(200, &head, b"", obj.bytes.len());
        }
        let mut bytes = obj.bytes.clone();
        if model.mode == Mode::ShortGet {
            bytes.truncate(bytes.len() / 2);
            return response(200, &head, &bytes, obj.bytes.len());
        }
        if model.mode == Mode::CorruptGet && !bytes.is_empty() {
            bytes[0] ^= 1;
        }
        return response(200, &head, &bytes, bytes.len());
    }
    panic!("unexpected request {method} {path}");
}

#[tokio::test]
async fn complete_probe_verifies_guards_unicode_races_and_cleanup() {
    let server = Server::new(Mode::Normal).await;
    let plan = server.plan();
    let report = plan.run(server.session().await).await.unwrap();
    assert!(report.passed, "{:?}", report.errors);
    assert_eq!(report.checks.len(), 10);
    assert_eq!(server.count("PUT"), 14);
    assert_eq!(server.count("DELETE"), 6);
    assert!(server.model.lock().unwrap().objects.is_empty());
    assert!(
        server
            .model
            .lock()
            .unwrap()
            .requests
            .iter()
            .any(|(_, key)| key.contains("%252F"))
    );
    assert!(!report.to_toml().unwrap().contains("TESTSECRET"));
    assert!(report.acceptance(&plan).is_ok());
    // Exercise the public factory with a report produced by the complete wire
    // probe, rather than only the synthetic policy-parser fixtures.
    let report = report.to_toml().unwrap();
    let policy = format!(
        "version = 1\nsource_fingerprint = '{}'\nreport_sha256 = '{}'\nwritable_prefix = 'documents/'\nmax_object_bytes = 4096\noperations = 'create-and-replace'\nobject_metadata='s3drive-markers-only'\nevidence_max_age_seconds = 86400\n",
        source_fingerprint(&server.source()),
        hex(Sha256::digest(report.as_bytes()).as_slice()),
    );
    let policy = crate::uploads::WritePolicy::from_toml(
        &server.source(),
        &policy,
        &plan.to_toml().unwrap(),
        &report,
    )
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let storage = s3drive_state::worker::Storage::open(
        &temp.path().join("state"),
        s3drive_state::cache::Options::default(),
        s3drive_state::journal::Options::default(),
    )
    .await
    .unwrap();
    let executor = crate::uploads::Executor::open(server.reader(), storage.clone(), policy)
        .await
        .unwrap();
    assert_eq!(server.count("PUT"), 14); // Opening never writes or re-probes.
    let target = executor.target("documents/日本語.txt").unwrap();
    let staging = storage.staging();
    let id = staging
        .begin(target, s3drive_state::journal::Base::Missing, None, 0)
        .await
        .unwrap();
    staging.seal(id).await.unwrap();
    assert_eq!(
        executor.submit(id).await.unwrap().phase,
        s3drive_state::remote::Phase::Verified
    );
    assert_eq!(
        executor.reconcile(id).await.unwrap().phase,
        s3drive_state::remote::Phase::Verified
    );
    assert_eq!(server.count("PUT"), 15);
    assert_eq!(server.count("DELETE"), 6);
    storage.close().await.unwrap();
}

#[tokio::test]
async fn literal_null_versions_remain_mutable_and_a_hidden_existing_key_still_blocks() {
    let server = Server::new(Mode::NullVersion).await;
    let plan = server.plan();
    assert!(plan.run(server.session().await).await.unwrap().passed);
    let server = Server::new(Mode::HideListing).await;
    let plan = server.plan();
    server.model.lock().unwrap().objects.insert(
        format!("/bucket/{}", plan.keys[0]),
        Object {
            bytes: vec![1],
            tag: "\"old\"".into(),
            operation: String::new(),
            checksum: String::new(),
        },
    );
    assert!(plan.run(server.session().await).await.is_err());
    assert_eq!(server.count("PUT"), 0);
    assert_eq!(server.count("DELETE"), 0);
}
#[tokio::test]
async fn an_ignoring_provider_fails_capability_checks_and_only_owned_fixtures_are_removed() {
    let server = Server::new(Mode::IgnoreGuards).await;
    let plan = server.plan();
    let report = plan.run(server.session().await).await.unwrap();
    assert!(!report.passed);
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.name == "existing-key-create-rejected" && !c.passed)
    );
    assert!(
        report
            .checks
            .iter()
            .any(|c| c.name == "concurrent-replace-one-winner" && !c.passed)
    );
    assert!(report.remaining_keys.is_empty());
    assert!(server.model.lock().unwrap().objects.is_empty());
}
#[tokio::test]
async fn lost_or_invalid_success_replies_are_unknown_and_verify_without_another_put() {
    for mode in [Mode::DropAfterCommit, Mode::MissingEtag] {
        let server = Server::new(mode).await;
        let plan = server.plan();
        let session = server.session().await;
        let intent = plan.intent(0, 0, Condition::Missing);
        assert!(matches!(
            session.put(&intent).await.unwrap(),
            Outcome::Unknown(_)
        ));
        assert!(session.verify(&intent).await.unwrap().is_some());
        assert_eq!(server.count("PUT"), 1);
        session.cleanup(&plan, 0).await.unwrap();
    }
}
#[tokio::test]
async fn absence_does_not_distinguish_no_commit_from_a_later_external_delete() {
    for mode in [Mode::DropBeforeCommit, Mode::DeleteAfterCommit] {
        let server = Server::new(mode).await;
        let plan = server.plan();
        let session = server.session().await;
        let intent = plan.intent(0, 0, Condition::Missing);
        assert!(matches!(
            session.put(&intent).await.unwrap(),
            Outcome::Unknown(_)
        ));
        assert!(session.verify(&intent).await.unwrap().is_none());
        assert_eq!(server.count("PUT"), 1);
    }
}
#[tokio::test]
async fn service_errors_and_cancellation_do_not_automatically_replay() {
    let server = Server::new(Mode::Status503).await;
    let plan = server.plan();
    let session = server.session().await;
    assert!(matches!(
        session
            .put(&plan.intent(0, 0, Condition::Missing))
            .await
            .unwrap(),
        Outcome::Unknown(_)
    ));
    assert_eq!(server.count("PUT"), 1);
    let server = Server::new(Mode::StallPut).await;
    let plan = server.plan();
    let session = server.session().await;
    let intent = plan.intent(0, 0, Condition::Missing);
    let mut future = Box::pin(session.put(&intent));
    tokio::select! {result=&mut future=>panic!("stalled put completed: {}",result.is_ok()),_=server.received.notified()=>{}}
    drop(future);
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(server.count("PUT"), 1);
    assert!(server.model.lock().unwrap().objects.is_empty());
}
#[tokio::test]
async fn corrupt_or_foreign_observations_are_never_acknowledged_or_deleted() {
    for mode in [Mode::CorruptGet, Mode::WrongMetadata] {
        let server = Server::new(mode).await;
        let plan = server.plan();
        let session = server.session().await;
        let intent = plan.intent(0, 0, Condition::Missing);
        session.put(&intent).await.unwrap();
        assert!(session.verify(&intent).await.is_err());
        assert!(session.cleanup(&plan, 0).await.is_err());
        assert_eq!(server.count("DELETE"), 0);
    }
    let server = Server::new(Mode::Normal).await;
    let plan = server.plan();
    let session = server.session().await;
    let intent = plan.intent(0, 0, Condition::Missing);
    session.put(&intent).await.unwrap();
    server
        .model
        .lock()
        .unwrap()
        .objects
        .values_mut()
        .next()
        .unwrap()
        .operation = "other-writer".into();
    assert!(session.verify(&intent).await.unwrap().is_none());
    assert!(session.cleanup(&plan, 0).await.is_err());
    assert_eq!(server.count("DELETE"), 0);
}
#[tokio::test]
async fn occupied_or_versioned_scopes_refuse_before_any_mutation() {
    for versioned in [false, true] {
        let server = Server::new(Mode::Normal).await;
        let plan = server.plan();
        let session = server.session().await;
        if versioned {
            server.versioned.store(true, Ordering::SeqCst);
        } else {
            server.model.lock().unwrap().objects.insert(
                format!("/bucket/{}unowned", plan.full_prefix()),
                Object {
                    bytes: vec![1],
                    tag: "\"old\"".into(),
                    operation: String::new(),
                    checksum: String::new(),
                },
            );
        }
        assert!(plan.run(session).await.is_err());
        assert_eq!(server.count("PUT"), 0);
        assert_eq!(server.count("DELETE"), 0);
    }
}
#[tokio::test]
async fn invalid_conditions_and_payloads_fail_before_dispatch() {
    let server = Server::new(Mode::Normal).await;
    let plan = server.plan();
    let session = server.session().await;
    for value in ["*", "", "W/\"weak\"", "\"a\",\"b\"", "\"bad\r\nheader\""] {
        let intent = plan.intent(0, 0, Condition::Match(value.into()));
        assert!(session.put(&intent).await.is_err());
    }
    let mut intent = plan.intent(0, 0, Condition::Missing);
    intent.bytes = vec![0; transport::MAX_PUT + 1].into();
    intent.checksum = hex(Sha256::digest(&intent.bytes).as_slice());
    assert!(session.put(&intent).await.is_err());
    assert_eq!(server.count("PUT"), 0);
}
#[test]
fn plans_are_strict_source_bound_and_regenerate_targets() {
    let source = s3drive_core::Config::parse(
        "version=1\nmode='read-only'\n[[sources]]\nalias='x'\nbucket='bucket'\nregion='us-east-1'",
    )
    .unwrap()
    .sources
    .remove(0);
    let plan = Plan::prepare(&source).unwrap();
    let serialized = plan.to_toml().unwrap();
    let decoded = Plan::from_toml(&serialized).unwrap();
    decoded.validate(&source, &plan.digest().unwrap()).unwrap();
    assert_ne!(
        Plan::prepare(&source).unwrap().full_prefix(),
        plan.full_prefix()
    );
    assert!(Plan::from_toml(&(serialized.clone() + "\nunknown=true")).is_err());
    assert!(Plan::from_toml(&"x".repeat(MAX_PLAN_BYTES + 1)).is_err());
    let mut changed = plan.clone();
    changed.keys[0] = "existing-user-data".into();
    assert!(
        changed
            .validate(&source, &changed.digest().unwrap())
            .is_err()
    );
    let mut source = source;
    source.bucket = "other-bucket".into();
    assert!(plan.validate(&source, &plan.digest().unwrap()).is_err());
}
