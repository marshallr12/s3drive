//! Real TCP/TLS, SQLite and independent clients. This is local wire acceptance,
//! not evidence of remote hosting, multiple physical hosts or Turso behavior.
use s3drive_authority::{
    Apply, Authority, Error, Kind, Mutation, ROOT, Reply, Request, ServerTls, Token, WireClient,
    WireError, WirePhase, WireRequest, serve_tls,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
};
use tokio_rustls::{
    TlsConnector,
    rustls::{
        ClientConfig, RootCertStore,
        pki_types::{CertificateDer, ServerName, pem::PemObject},
    },
};
const SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct Certificate {
    cert: String,
    key: String,
}
impl Certificate {
    fn acceptor(&self) -> tokio_rustls::TlsAcceptor {
        let certs = CertificateDer::pem_slice_iter(self.cert.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let key =
            tokio_rustls::rustls::pki_types::PrivateKeyDer::from_pem_slice(self.key.as_bytes())
                .unwrap();
        let config = tokio_rustls::rustls::ServerConfig::builder_with_provider(Arc::new(
            tokio_rustls::rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
        tokio_rustls::TlsAcceptor::from(Arc::new(config))
    }
    fn new(name: &str, expired: bool) -> Self {
        let key = rcgen::KeyPair::generate().unwrap();
        let mut params = rcgen::CertificateParams::new(vec![name.into()]).unwrap();
        if expired {
            params.not_before = rcgen::date_time_ymd(2000, 1, 1);
            params.not_after = rcgen::date_time_ymd(2001, 1, 1);
        }
        Self {
            cert: params.self_signed(&key).unwrap().pem(),
            key: key.serialize_pem(),
        }
    }
    fn server(&self) -> ServerTls {
        ServerTls::from_pem(self.cert.as_bytes(), self.key.as_bytes()).unwrap()
    }
    fn client(&self, url: &str) -> WireClient {
        WireClient::https(
            url,
            Token::new(SECRET.into()).unwrap(),
            Some(self.cert.as_bytes()),
        )
        .unwrap()
    }
    async fn raw(&self, url: &str) -> tokio_rustls::client::TlsStream<TcpStream> {
        let url = url::Url::parse(url).unwrap();
        let mut roots = RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(self.cert.as_bytes()) {
            roots.add(cert.unwrap()).unwrap();
        }
        let config = ClientConfig::builder_with_provider(Arc::new(
            tokio_rustls::rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        let tcp = TcpStream::connect(("127.0.0.1", url.port().unwrap()))
            .await
            .unwrap();
        TlsConnector::from(Arc::new(config))
            .connect(ServerName::try_from("127.0.0.1".to_owned()).unwrap(), tcp)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn tls_neither_follows_redirects_nor_replays_a_dropped_reply() {
    for redirect in [false, true] {
        let request = Request::Apply {
            request: Apply {
                volume: "1".repeat(32),
                operation: "2".repeat(32),
                expected_sequence: 0,
                mutation: Mutation::Create {
                    parent: ROOT.into(),
                    name: "never replay".into(),
                    node_kind: Kind::Directory,
                },
            },
        };
        let expected_body = serde_json::to_vec(&request).unwrap();
        let cert = Certificate::new("127.0.0.1", false);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("https://{}/v1/namespace", listener.local_addr().unwrap());
        let location = format!("https://{}/v1/namespace", target.local_addr().unwrap());
        let acceptor = cert.acceptor();
        let task = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut stream = acceptor.accept(tcp).await.unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                assert!(header.len() < 8192);
                header.push(stream.read_u8().await.unwrap());
            }
            let text = std::str::from_utf8(&header).unwrap();
            assert!(
                text.to_ascii_lowercase()
                    .contains("content-type: application/json\r\n")
            );
            assert!(!text.to_ascii_lowercase().contains("transfer-encoding:"));
            let length = text
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|n| n.parse::<usize>().ok())
                })
                .unwrap();
            assert!(length < 4096);
            let mut body = vec![0; length];
            stream.read_exact(&mut body).await.unwrap();
            assert_eq!(
                body, expected_body,
                "diagnostic changed serialized request bytes"
            );
            if redirect {
                stream.write_all(format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
                stream.flush().await.unwrap();
            }
            drop(stream);
            let (retry, follow) = tokio::join!(
                tokio::time::timeout(Duration::from_millis(500), listener.accept()),
                tokio::time::timeout(Duration::from_millis(500), target.accept())
            );
            assert!(retry.is_err(), "failed mutation was replayed by transport");
            assert!(follow.is_err(), "transport followed redirect");
        });
        let error = cert.client(&url).request(&request).await.unwrap_err();
        if redirect {
            assert_eq!(error, WireError::HttpStatus(307));
        } else {
            assert!(
                matches!(
                    error,
                    WireError::Transport {
                        request: WireRequest::Apply,
                        phase: WirePhase::Request,
                        timed_out: false,
                        body_frame_polled: true,
                        ..
                    }
                ),
                "{error:?}"
            );
        }
        assert_redacted(&error, &[SECRET, &url]);
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
    }
}

fn assert_redacted(error: &WireError, secrets: &[&str]) {
    let diagnostic = format!("{error:?} {error}");
    for secret in secrets {
        assert!(
            !diagnostic.contains(secret),
            "wire error disclosed private input"
        );
    }
    assert!(std::error::Error::source(error).is_none());
    assert!(diagnostic.contains("inspect the durable operation"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tls_wire_diagnostics_distinguish_failure_stages_without_disclosing_inputs() {
    #[derive(Debug, Clone, Copy)]
    enum Fault {
        Status,
        Oversize,
        InvalidReply,
        TruncatedBody,
        RequestTimeout,
        BodyTimeout,
    }
    // Prepare every synchronous certificate/client factory before any request
    // deadline starts. Keep blocking trust/crypto setup off runtime workers;
    // the six network exchanges below still run concurrently.
    let mut fixtures = Vec::new();
    for fault in [
        Fault::Status,
        Fault::Oversize,
        Fault::InvalidReply,
        Fault::TruncatedBody,
        Fault::RequestTimeout,
        Fault::BodyTimeout,
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("https://{}/v1/namespace", listener.local_addr().unwrap());
        let endpoint = url.clone();
        let setup = Instant::now();
        let (cert, client, acceptor) = tokio::task::spawn_blocking(move || {
            let cert = Certificate::new("127.0.0.1", false);
            let client = cert.client(&endpoint);
            let acceptor = cert.acceptor();
            (cert, client, acceptor)
        })
        .await
        .unwrap();
        fixtures.push((
            fault,
            listener,
            url,
            cert,
            client,
            acceptor,
            setup.elapsed(),
        ));
    }
    let mut cases = tokio::task::JoinSet::new();
    for (fault, listener, url, cert, client, acceptor, setup) in fixtures {
        cases.spawn(async move {
            const PRIVATE_BODY: &str = "private-response-marker";
            let stage = Arc::new(AtomicU8::new(0));
            let progress = stage.clone();
            let started = Instant::now();
            let (finished, done) = oneshot::channel();
            let mut server = tokio::spawn(async move {
                let (tcp, _) = listener.accept().await.unwrap();
                progress.store(1, Ordering::Relaxed);
                let mut stream = acceptor.accept(tcp).await.unwrap();
                progress.store(2, Ordering::Relaxed);
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") {
                    assert!(header.len() < 8192);
                    let byte = stream.read_u8().await.unwrap_or_else(|error| {
                        panic!("fixture {fault:?} header ended after {} bytes: {:?}", header.len(), error.kind())
                    });
                    header.push(byte);
                }
                progress.store(3, Ordering::Relaxed);
                let length = std::str::from_utf8(&header).unwrap().lines().find_map(|line| {
                    line.to_ascii_lowercase().strip_prefix("content-length: ")
                        .and_then(|n| n.parse::<usize>().ok())
                }).unwrap();
                assert!(length < 4096);
                let mut body = vec![0; length];
                stream.read_exact(&mut body).await.unwrap();
                assert!(matches!(serde_json::from_slice::<Request>(&body).unwrap(), Request::Info {}));
                progress.store(4, Ordering::Relaxed);
                let response = match fault {
                    Fault::Status => format!("HTTP/1.1 503 Service Unavailable\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PRIVATE_BODY}", PRIVATE_BODY.len()),
                    Fault::Oversize => "HTTP/1.1 200 OK\r\nContent-Length: 8388609\r\nConnection: close\r\n\r\n".into(),
                    Fault::InvalidReply => format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PRIVATE_BODY}", PRIVATE_BODY.len()),
                    Fault::TruncatedBody | Fault::BodyTimeout => format!("HTTP/1.1 200 OK\r\nContent-Length: 4096\r\nConnection: close\r\n\r\n{PRIVATE_BODY}"),
                    Fault::RequestTimeout => String::new(),
                };
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.flush().await.unwrap();
                progress.store(5, Ordering::Relaxed);
                if matches!(fault, Fault::RequestTimeout | Fault::BodyTimeout) {
                    done.await.unwrap();
                }
                drop(stream);
                assert!(tokio::time::timeout(Duration::from_millis(500), listener.accept()).await.is_err(), "diagnostic path retried request");
            });
            let result = tokio::time::timeout(Duration::from_secs(13), client.request(&Request::Info {})).await;
            let at_result = stage.load(Ordering::Relaxed);
            let elapsed = started.elapsed();
            let _ = finished.send(());
            let server_result = tokio::time::timeout(Duration::from_secs(3), &mut server).await;
            if server_result.is_err() {
                server.abort();
                let _ = server.await;
            }
            let reached = stage.load(Ordering::Relaxed);
            let context = format!("fault={fault:?}, stage_at_result={at_result}, final_stage={reached} (0=new,1=TCP,2=TLS,3=headers,4=body,5=fault-ready), setup={setup:?}, elapsed={elapsed:?}, result={result:?}");
            assert!(matches!(server_result, Ok(Ok(()))), "fixture did not complete: {context}; {server_result:?}");
            assert_eq!(reached, 5, "fault was not established: {context}");
            let error = result.expect(&context).expect_err(&context);
            assert_redacted(&error, &[SECRET, &url, &cert.cert, PRIVATE_BODY]);
            match fault {
                Fault::Status => assert_eq!(error, WireError::HttpStatus(503), "{context}"),
                Fault::Oversize => assert_eq!(error, WireError::ResponseTooLarge, "{context}"),
                Fault::InvalidReply => assert_eq!(error, WireError::InvalidReply, "{context}"),
                Fault::TruncatedBody => assert!(matches!(error, WireError::Transport { request: WireRequest::Info, phase: WirePhase::ResponseBody, timed_out: false, body_frame_polled: true, .. }), "{context}"),
                Fault::RequestTimeout => assert!(matches!(error, WireError::Transport { request: WireRequest::Info, phase: WirePhase::Request, timed_out: true, body_frame_polled: true, .. }), "{context}"),
                Fault::BodyTimeout => assert!(matches!(error, WireError::Transport { request: WireRequest::Info, phase: WirePhase::ResponseBody, timed_out: true, body_frame_polled: true, .. }), "{context}"),
            }
        });
    }
    let mut failures = Vec::new();
    while let Some(result) = cases.join_next().await {
        if let Err(error) = result {
            failures.push(error.to_string());
        }
    }
    assert!(failures.is_empty(), "wire fault cases failed: {failures:?}");
}
struct Server {
    url: String,
    stop: oneshot::Sender<()>,
    task: JoinHandle<std::io::Result<()>>,
}
impl Server {
    async fn start(authority: &Authority, cert: &Certificate) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("https://{}/v1/namespace", listener.local_addr().unwrap());
        let (stop, receive) = oneshot::channel();
        let client = authority.client();
        let tls = cert.server();
        let task = tokio::spawn(serve_tls(
            listener,
            client,
            Token::new(SECRET.into()).unwrap(),
            tls,
            async {
                let _ = receive.await;
            },
        ));
        Self { url, stop, task }
    }
    async fn stop(self) {
        self.stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), self.task)
            .await
            .expect("TLS drain timeout")
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn tls_requires_correct_trust_hostname_token_and_volume() {
    let temp = tempfile::tempdir().unwrap();
    let a = Authority::open(temp.path().join("authority"))
        .await
        .unwrap();
    let cert = Certificate::new("127.0.0.1", false);
    let server = Server::start(&a, &cert).await;
    let c = cert.client(&server.url);
    let Reply::Info(v) = c.request(&Request::Info {}).await.unwrap() else {
        panic!("info")
    };
    assert!(matches!(
        c.request(&Request::Node {
            volume: "a".repeat(32),
            node: ROOT.into()
        })
        .await
        .unwrap(),
        Reply::Error(_)
    ));
    assert_eq!(v.sequence, 0);
    let denied = WireClient::https(
        &server.url,
        Token::new("a".repeat(64)).unwrap(),
        Some(cert.cert.as_bytes()),
    )
    .unwrap()
    .request(&Request::Info {})
    .await
    .unwrap_err();
    assert_eq!(denied, WireError::HttpStatus(401));
    assert_redacted(&denied, &[SECRET, &server.url]);
    let wrong = Certificate::new("127.0.0.1", false);
    let trust = wrong
        .client(&server.url)
        .request(&Request::Info {})
        .await
        .unwrap_err();
    assert!(
        matches!(
            trust,
            WireError::Transport {
                phase: WirePhase::Connect,
                timed_out: false,
                body_frame_polled: false,
                ..
            }
        ),
        "{trust:?}"
    );
    assert_redacted(&trust, &[SECRET, &server.url, &wrong.cert]);
    assert!(
        WireClient::https(&server.url, Token::new(SECRET.into()).unwrap(), None)
            .unwrap()
            .request(&Request::Info {})
            .await
            .is_err()
    );
    // A slow unauthenticated TLS handshake cannot monopolize acceptance.
    let addr = url::Url::parse(&server.url).unwrap();
    let mut plain = TcpStream::connect(("127.0.0.1", addr.port().unwrap()))
        .await
        .unwrap();
    assert!(matches!(
        c.request(&Request::Info {}).await.unwrap(),
        Reply::Info(_)
    ));
    plain
        .write_all(b"POST /v1/namespace HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let mut bytes = [0; 64];
    let result = tokio::time::timeout(Duration::from_secs(2), plain.read(&mut bytes))
        .await
        .unwrap();
    if let Ok(length) = result {
        // Rustls may send a TLS alert for a malformed record. It must never
        // reply with a plaintext HTTP response from the application router.
        assert!(
            length == 0 || bytes[0] == 0x15,
            "TLS listener replied in plaintext"
        );
    }
    // Shutdown also wakes incomplete handshakes; no waiting for their timeout.
    let idle = TcpStream::connect(("127.0.0.1", addr.port().unwrap()))
        .await
        .unwrap();
    server.stop().await;
    drop(idle);
    for (name, expired) in [("different.example.test", false), ("127.0.0.1", true)] {
        let invalid = Certificate::new(name, expired);
        let s = Server::start(&a, &invalid).await;
        assert!(
            invalid
                .client(&s.url)
                .request(&Request::Info {})
                .await
                .is_err(),
            "invalid certificate accepted: {name}/{expired}"
        );
        s.stop().await;
    }
    a.shutdown().await.unwrap();
}

#[tokio::test]
async fn tls_competing_clients_and_unread_reply_reconcile_exact_operation() {
    let temp = tempfile::tempdir().unwrap();
    let a = Authority::open(temp.path().join("authority"))
        .await
        .unwrap();
    let cert = Certificate::new("127.0.0.1", false);
    let server = Server::start(&a, &cert).await;
    let first = cert.client(&server.url);
    let second = cert.client(&server.url);
    let Reply::Info(v) = first.request(&Request::Info {}).await.unwrap() else {
        panic!("info")
    };
    let create = |operation: u64, name: &str, sequence| Request::Apply {
        request: Apply {
            volume: v.id.clone(),
            operation: format!("{operation:032x}"),
            expected_sequence: sequence,
            mutation: Mutation::Create {
                parent: ROOT.into(),
                name: name.into(),
                node_kind: Kind::Directory,
            },
        },
    };
    let left = create(1, "one shared name 日本語 🦀", v.sequence);
    let right = create(2, "one shared name 日本語 🦀", v.sequence);
    let (l, r) = tokio::join!(first.request(&left), second.request(&right));
    let (l, r) = (l.unwrap(), r.unwrap());
    assert_eq!(
        usize::from(matches!(l, Reply::Applied(_))) + usize::from(matches!(r, Reply::Applied(_))),
        1
    );
    assert!(
        matches!(l, Reply::Error(Error::Conflict)) || matches!(r, Reply::Error(Error::Conflict))
    );
    let Reply::Info(current) = first.request(&Request::Info {}).await.unwrap() else {
        panic!("info")
    };
    let request = create(3, "unread response café", current.sequence);
    let body = serde_json::to_vec(&request).unwrap();
    let mut raw = cert.raw(&server.url).await;
    let header = format!(
        "POST /v1/namespace HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {SECRET}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    raw.write_all(header.as_bytes()).await.unwrap();
    raw.write_all(&body).await.unwrap();
    raw.flush().await.unwrap();
    let operation = format!("{:032x}", 3);
    let saved = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match second
                .request(&Request::Operation {
                    volume: v.id.clone(),
                    operation: operation.clone(),
                })
                .await
                .unwrap()
            {
                Reply::Operation(Some(result)) => break result.unwrap(),
                Reply::Operation(None) => tokio::time::sleep(Duration::from_millis(10)).await,
                _ => panic!("operation lookup reply"),
            }
        }
    })
    .await
    .expect("unread request did not commit");
    drop(raw); // The originating client never consumes its return value.
    let Reply::Applied(replayed) = first.request(&request).await.unwrap() else {
        panic!("exact replay")
    };
    assert_eq!(
        serde_json::to_value(&saved).unwrap(),
        serde_json::to_value(&replayed).unwrap()
    );
    let Reply::Info(after) = second.request(&Request::Info {}).await.unwrap() else {
        panic!("info")
    };
    assert_eq!(
        after.sequence,
        current.sequence + 1,
        "TLS replay repeated a mutation"
    );
    server.stop().await;
    a.shutdown().await.unwrap();
}

#[tokio::test]
async fn tls_header_and_body_deadlines_close_incomplete_requests() {
    let temp = tempfile::tempdir().unwrap();
    let a = Authority::open(temp.path().join("authority"))
        .await
        .unwrap();
    let cert = Certificate::new("127.0.0.1", false);
    let server = Server::start(&a, &cert).await;
    let mut header = cert.raw(&server.url).await;
    header
        .write_all(b"POST /v1/namespace HTTP/1.1\r\n")
        .await
        .unwrap();
    let mut body = cert.raw(&server.url).await;
    body.write_all(format!("POST /v1/namespace HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {SECRET}\r\nContent-Type: application/json\r\nContent-Length: 64\r\n\r\n{{").as_bytes()).await.unwrap();
    async fn closed(mut stream: tokio_rustls::client::TlsStream<TcpStream>, seconds: u64) {
        let mut bytes = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(seconds), stream.read_to_end(&mut bytes))
            .await
            .expect("incomplete request exceeded deadline");
        assert!(
            bytes.is_empty(),
            "incomplete request produced an application response"
        );
    }
    tokio::join!(closed(header, 7), closed(body, 12));
    assert!(matches!(
        cert.client(&server.url)
            .request(&Request::Info {})
            .await
            .unwrap(),
        Reply::Info(_)
    ));
    server.stop().await;
    a.shutdown().await.unwrap();
}

#[test]
fn tls_setup_and_urls_refuse_invalid_material_without_disclosing_it() {
    let cert = Certificate::new("127.0.0.1", false);
    let wrong = Certificate::new("127.0.0.1", false);
    for (c, k) in [
        (cert.cert.as_bytes(), wrong.key.as_bytes()),
        (b"private-marker".as_slice(), cert.key.as_bytes()),
    ] {
        let error = ServerTls::from_pem(c, k)
            .err()
            .expect("invalid TLS material accepted");
        assert!(!error.to_string().contains("private-marker"));
        assert!(!error.to_string().contains("PRIVATE KEY"));
    }
    for url in [
        "http://127.0.0.1/v1/namespace",
        "https://secret@example.test/v1/namespace",
        "https://example.test/wrong",
        "https://example.test/v1/namespace?secret=x",
        "https://example.test/v1/namespace#x",
    ] {
        assert!(
            WireClient::https(
                url,
                Token::new(SECRET.into()).unwrap(),
                Some(cert.cert.as_bytes())
            )
            .is_err()
        );
    }
    assert!(
        WireClient::https(
            "https://example.test/v1/namespace",
            Token::new(SECRET.into()).unwrap(),
            Some(b"not a certificate")
        )
        .is_err()
    );
}
