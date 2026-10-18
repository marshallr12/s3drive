//! Test-only transport study, not the production WireClient or a Windows fix.
//! Fixed ephemeral endpoints and generated explicit roots only. No S3/real keys.
use axum::http::{Request, header};
use hyper::body::{Body, Bytes, Frame, SizeHint};
use hyper_util::rt::TokioIo;
use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
use tokio_rustls::{TlsAcceptor, TlsConnector, rustls};
const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const MAX_REPLY: usize = 8 * 1024 * 1024;
const CONNECT: Duration = Duration::from_secs(2);
const WHOLE: Duration = Duration::from_secs(10);
trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}
type Stream = Box<dyn Io>;

// Only fixed categories survive; dependency errors, headers and bytes do not.
#[derive(Debug, PartialEq, Eq)]
enum Failure {
    Connect,
    ConnectDeadline,
    Deadline,
    Http,
    Body,
    Status(u16),
    TooLarge,
}
#[derive(Clone)]
struct Trust {
    config: Arc<rustls::ClientConfig>,
    name: &'static str,
}

struct TrackedBody {
    body: reqwest::Body,
    polled: Arc<AtomicBool>,
}
impl Body for TrackedBody {
    type Data = Bytes;
    type Error = reqwest::Error;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let frame = Pin::new(&mut self.body).poll_frame(cx);
        if matches!(&frame, Poll::Ready(Some(Ok(frame))) if frame.is_data()) {
            self.polled.store(true, Ordering::Release);
        }
        frame
    }
    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }
}

async fn exchange(
    address: SocketAddr,
    trust: Option<Trust>,
    payload: Vec<u8>,
    polled: Arc<AtomicBool>,
) -> Result<Vec<u8>, Failure> {
    let deadline = tokio::time::Instant::now() + WHOLE;
    let stream: Stream = tokio::time::timeout(CONNECT, async move {
        let stream = TcpStream::connect(address)
            .await
            .map_err(|_| Failure::Connect)?;
        stream.set_nodelay(true).map_err(|_| Failure::Connect)?;
        if let Some(trust) = trust {
            let name = rustls::pki_types::ServerName::try_from(trust.name)
                .map_err(|_| Failure::Connect)?;
            let stream = TlsConnector::from(trust.config)
                .connect(name, stream)
                .await
                .map_err(|_| Failure::Connect)?;
            Ok(Box::new(stream) as Stream)
        } else {
            Ok(Box::new(stream) as Stream)
        }
    })
    .await
    .map_err(|_| Failure::ConnectDeadline)??;
    exchange_stream(stream, address, payload, polled, deadline).await
}

// The caller owns the whole deadline, including graceful connection shutdown.
async fn exchange_stream(
    stream: Stream,
    address: SocketAddr,
    payload: Vec<u8>,
    polled: Arc<AtomicBool>,
    deadline: tokio::time::Instant,
) -> Result<Vec<u8>, Failure> {
    let (mut sender, connection) = tokio::time::timeout_at(
        deadline,
        hyper::client::conn::http1::handshake(TokioIo::new(stream)),
    )
    .await
    .map_err(|_| Failure::Deadline)?
    .map_err(|_| Failure::Http)?;
    let request = Request::post("/v1/namespace")
        .header(header::HOST, address.to_string())
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONNECTION, "close")
        .body(TrackedBody {
            body: reqwest::Body::from(payload),
            polled,
        })
        .map_err(|_| Failure::Http)?;
    let work = async move {
        // The first request is queued before the connection driver polls.
        // Resolved Hyper permits one buffered request without ready().
        let response = sender
            .send_request(request)
            .await
            .map_err(|_| Failure::Http)?;
        if response.status().as_u16() != 200 {
            return Err(Failure::Status(response.status().as_u16()));
        }
        if response
            .body()
            .size_hint()
            .upper()
            .is_some_and(|n| n > MAX_REPLY as u64)
        {
            return Err(Failure::TooLarge);
        }
        let mut body = response.into_body();
        let mut bytes = Vec::new();
        while let Some(frame) = std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await
        {
            let frame = frame.map_err(|_| Failure::Body)?;
            if let Ok(data) = frame.into_data() {
                if data.len() > MAX_REPLY - bytes.len() {
                    return Err(Failure::TooLarge);
                }
                bytes.extend_from_slice(&data);
            }
        }
        Ok(bytes)
    };
    let work = tokio::time::timeout_at(deadline, work);
    tokio::pin!(work, connection);
    // No pool, driver spawn or task that can outlive this exchange.
    // A finished driver may already have queued the complete response or
    // its body error; drain that work under the same absolute deadline.
    tokio::select! {
        biased;
        result = &mut work => {
            let result = result.map_err(|_| Failure::Deadline)?;
            if result.is_ok() {
                // Reading all response bytes does not imply the driver has
                // finished poll_shutdown (including TLS close_notify). Keep
                // driving under the caller's ORIGINAL whole deadline. A
                // cleanup error cannot invalidate an already framed reply.
                let _ = tokio::time::timeout_at(deadline, connection).await;
            }
            result
        },
        _ = &mut connection => work.await.map_err(|_| Failure::Deadline)?,
    }
}

fn tls(name: &str) -> (TlsAcceptor, Trust) {
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = rcgen::CertificateParams::new(vec![name.into()])
        .unwrap()
        .self_signed(&key)
        .unwrap();
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut server = rustls::ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
        )
        .unwrap();
    server.alpn_protocols = vec![b"http/1.1".to_vec()];
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert.der().clone()).unwrap();
    let mut client = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    client.alpn_protocols = vec![b"http/1.1".to_vec()];
    (
        TlsAcceptor::from(Arc::new(server)),
        Trust {
            config: Arc::new(client),
            name: "127.0.0.1",
        },
    )
}

fn payload() -> Vec<u8> {
    serde_json::to_vec(&s3drive_authority::Request::Apply {
        request: s3drive_authority::Apply {
            volume: "1".repeat(32),
            operation: "2".repeat(32),
            expected_sequence: 0,
            mutation: s3drive_authority::Mutation::Create {
                parent: s3drive_authority::ROOT.into(),
                name: "café 雪 🦀".into(),
                node_kind: s3drive_authority::Kind::File,
            },
        },
    })
    .unwrap()
}
async fn received(stream: &mut Stream, address: SocketAddr) -> Vec<u8> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        assert!(header.len() < 8192);
        header.push(stream.read_u8().await.unwrap());
    }
    let text = std::str::from_utf8(&header).unwrap();
    assert!(text.starts_with("POST /v1/namespace HTTP/1.1\r\n"));
    let lower = text.to_ascii_lowercase();
    assert!(lower.contains(&format!("\r\nhost: {address}\r\n")));
    assert!(text.contains(&format!("authorization: Bearer {TOKEN}\r\n")));
    assert!(lower.contains("\r\ncontent-type: application/json\r\n"));
    assert!(lower.contains("\r\nconnection: close\r\n"));
    assert!(!lower.contains("transfer-encoding:"));
    let length: usize = lower
        .lines()
        .find_map(|line| line.strip_prefix("content-length: "))
        .unwrap()
        .parse()
        .unwrap();
    assert!(length <= 4096);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.unwrap();
    body
}
#[derive(Clone, Copy)]
enum Response {
    Complete,
    Lost,
    Redirect,
    Truncated,
    Oversize,
    ChunkedExact,
    ChunkedOversize,
}
async fn peer(
    tcp: TcpListener,
    acceptor: Option<TlsAcceptor>,
    response: Response,
    count: usize,
) -> Vec<Vec<u8>> {
    let address = tcp.local_addr().unwrap();
    let mut requests = Vec::new();
    for _ in 0..count {
        let (stream, _) = tcp.accept().await.unwrap();
        let mut stream: Stream = if let Some(acceptor) = &acceptor {
            Box::new(acceptor.accept(stream).await.unwrap())
        } else {
            Box::new(stream)
        };
        requests.push(received(&mut stream, address).await);
        let reply = match response {
            Response::Complete => b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nreply".as_slice(),
            Response::Lost => b"",
            Response::Redirect => b"HTTP/1.1 307 Temporary Redirect\r\nLocation: http://127.0.0.1:1/never\r\nContent-Length: 0\r\n\r\n",
            Response::Truncated => b"HTTP/1.1 200 OK\r\nContent-Length: 99\r\n\r\nshort",
            Response::Oversize => b"HTTP/1.1 200 OK\r\nContent-Length: 8388609\r\n\r\n",
            Response::ChunkedExact | Response::ChunkedOversize => b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n",
        };
        if !reply.is_empty() {
            stream.write_all(reply).await.unwrap();
            stream.flush().await.unwrap();
        }
        if matches!(response, Response::ChunkedExact | Response::ChunkedOversize) {
            let chunk = vec![0x42; 1024 * 1024];
            for _ in 0..8 {
                stream.write_all(b"100000\r\n").await.unwrap();
                stream.write_all(&chunk).await.unwrap();
                stream.write_all(b"\r\n").await.unwrap();
            }
            if matches!(response, Response::ChunkedOversize) {
                // No final zero chunk: exceeding the cap must fail on bytes,
                // rather than waiting for the peer to finish an unlimited body.
                stream.write_all(b"1\r\nx\r\n").await.unwrap();
            } else {
                stream.write_all(b"0\r\n\r\n").await.unwrap();
            }
            stream.flush().await.unwrap();
        }
        // Omit response Connection: close. The caller still owns finite lifetime.
        if matches!(response, Response::Complete | Response::ChunkedExact) {
            // read_u8 maps both a clean zero-byte EOF and TLS's missing-alert
            // error to UnexpectedEof. Only read == Ok(0) proves clean TLS EOF.
            let closed = stream.read(&mut [0; 1]).await;
            assert_eq!(closed.unwrap(), 0, "extra data after the sole request");
        }
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(100), tcp.accept())
            .await
            .is_err(),
        "unexpected additional transport/replay"
    );
    requests
}
async fn finish<T>(mut task: tokio::task::JoinHandle<T>) -> T {
    match tokio::time::timeout(Duration::from_secs(5), &mut task).await {
        Ok(result) => result.unwrap(),
        Err(_) => {
            task.abort();
            let _ = task.await;
            panic!("owned fixture did not stop");
        }
    }
}
fn canceled_socket_closed(result: std::io::Result<u8>) -> std::io::ErrorKind {
    let kind = result
        .expect_err("owned cancellation left another byte")
        .kind();
    // A dropped socket need not perform a graceful TCP/TLS shutdown. The
    // failed initial control measured ConnectionReset; do not accept arbitrary
    // read errors, timeouts, or any extra byte as evidence of closure.
    assert!(
        matches!(
            kind,
            std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
        ),
        "cancellation closure: {kind:?}"
    );
    kind
}

// Gate outside TLS so close_notify cannot be emitted until shutdown is released.
// Normal reads/writes are untouched. This makes the post-response pending-driver
// boundary deterministic rather than depending on socket scheduler timing.
struct GatedShutdown {
    inner: Stream,
    release: Option<oneshot::Receiver<()>>,
    entered: Option<oneshot::Sender<()>>,
    complete: Arc<AtomicBool>,
}
impl AsyncRead for GatedShutdown {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}
impl AsyncWrite for GatedShutdown {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if let Some(entered) = self.entered.take() {
            let _ = entered.send(());
        }
        if let Some(release) = &mut self.release {
            if Pin::new(release).poll(cx).is_pending() {
                return Poll::Pending;
            }
            self.release = None;
        }
        let result = Pin::new(&mut self.inner).poll_shutdown(cx);
        if matches!(result, Poll::Ready(Ok(()))) {
            self.complete.store(true, Ordering::Release);
        }
        result
    }
}

#[tokio::test]
async fn completed_tls_reply_keeps_pending_driver_until_clean_shutdown() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (acceptor, trust) = tls("127.0.0.1");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream: Stream = Box::new(acceptor.accept(stream).await.unwrap());
        assert_eq!(received(&mut stream, address).await, payload());
        // Leave the write side open; the client must initiate TLS closure.
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nreply")
            .await
            .unwrap();
        stream.flush().await.unwrap();
        stream.read(&mut [0; 1]).await
    });
    let (release, released) = oneshot::channel();
    let (entered, entering) = oneshot::channel();
    let complete = Arc::new(AtomicBool::new(false));
    let observed = complete.clone();
    let mut client = tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + WHOLE;
        tokio::time::timeout(WHOLE + Duration::from_secs(5), async move {
            let tcp = TcpStream::connect(address).await.unwrap();
            let stream = TlsConnector::from(trust.config)
                .connect(trust.name.try_into().unwrap(), tcp)
                .await
                .unwrap();
            exchange_stream(
                Box::new(GatedShutdown {
                    inner: Box::new(stream),
                    release: Some(released),
                    entered: Some(entered),
                    complete,
                }),
                address,
                payload(),
                Arc::new(AtomicBool::new(false)),
                deadline,
            )
            .await
        })
        .await
    });
    let entered = tokio::time::timeout(Duration::from_secs(5), entering).await;
    let premature = tokio::time::timeout(Duration::from_millis(100), &mut client).await;
    let returned_early = premature.is_ok();
    let _ = release.send(());
    let result = match premature {
        Ok(result) => result.unwrap(),
        Err(_) => finish(client).await,
    };
    let closed = finish(server).await;
    entered.unwrap().unwrap();
    assert!(
        !returned_early,
        "reply returned while TLS shutdown was held"
    );
    assert_eq!(result.unwrap(), Ok(b"reply".to_vec()));
    assert!(observed.load(Ordering::Acquire));
    assert_eq!(closed.unwrap(), 0, "peer did not observe clean TLS EOF");
}

#[tokio::test]
async fn pending_shutdown_preserves_complete_reply_with_only_remaining_budget() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream: Stream = Box::new(stream);
        assert_eq!(received(&mut stream, address).await, payload());
        // Spend most of the original budget BEFORE the response. A new whole
        // timeout for cleanup would run beyond the independent 14s watchdog.
        tokio::time::sleep(Duration::from_secs(6)).await;
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nreply")
            .await
            .unwrap();
        stream.flush().await.unwrap();
        stream.read(&mut [0; 1]).await
    });
    let (_release, released) = oneshot::channel();
    let (entered, entering) = oneshot::channel();
    let complete = Arc::new(AtomicBool::new(false));
    let observed = complete.clone();
    let started = tokio::time::Instant::now();
    let deadline = started + WHOLE;
    let watchdog = deadline + Duration::from_secs(4);
    let client = tokio::spawn(async move {
        let stream = TcpStream::connect(address).await.unwrap();
        exchange_stream(
            Box::new(GatedShutdown {
                inner: Box::new(stream),
                release: Some(released),
                entered: Some(entered),
                complete,
            }),
            address,
            payload(),
            Arc::new(AtomicBool::new(false)),
            deadline,
        )
        .await
    });
    let entered = tokio::time::timeout_at(watchdog, entering).await;
    // Bound fixture lifetime even if the whole-deadline behavior regresses.
    let mut client = client;
    let result = tokio::time::timeout_at(watchdog, &mut client).await;
    let result = match result {
        Ok(result) => Some(result),
        Err(_) => {
            client.abort();
            let _ = client.await;
            None
        }
    };
    let elapsed = started.elapsed();
    let closed = finish(server).await;
    entered.unwrap().unwrap();
    assert_eq!(
        result
            .expect("shutdown outlived original whole deadline")
            .unwrap(),
        Ok(b"reply".to_vec()),
        "cleanup lost the complete response"
    );
    assert!(
        elapsed >= WHOLE && elapsed < WHOLE + Duration::from_secs(4),
        "shutdown remaining-budget observation: {elapsed:?}"
    );
    assert!(!observed.load(Ordering::Acquire));
    assert_eq!(closed.unwrap(), 0, "deadline did not drop the owned socket");
}

#[tokio::test]
async fn owned_driver_http_and_tls_exact_authenticated_requests_have_finite_lifetimes() {
    for encrypted in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (acceptor, trust) = tls("127.0.0.1");
        let server = tokio::spawn(peer(
            listener,
            encrypted.then_some(acceptor),
            Response::Complete,
            32,
        ));
        let mut results = Vec::new();
        for _ in 0..32 {
            let polled = Arc::new(AtomicBool::new(false));
            results.push((
                exchange(
                    address,
                    encrypted.then(|| trust.clone()),
                    payload(),
                    polled.clone(),
                )
                .await,
                polled,
            ));
        }
        let requests = finish(server).await;
        assert_eq!(requests, vec![payload(); 32]);
        for (result, polled) in results {
            assert_eq!(result, Ok(b"reply".to_vec()));
            assert!(polled.load(Ordering::Acquire));
        }
    }
}

#[tokio::test]
async fn owned_driver_lost_reply_redirect_and_bad_bodies_never_replay() {
    for encrypted in [false, true] {
        for response in [
            Response::Lost,
            Response::Redirect,
            Response::Truncated,
            Response::Oversize,
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (acceptor, trust) = tls("127.0.0.1");
            let server = tokio::spawn(peer(listener, encrypted.then_some(acceptor), response, 1));
            let polled = Arc::new(AtomicBool::new(false));
            let result = exchange(
                address,
                encrypted.then_some(trust),
                payload(),
                polled.clone(),
            )
            .await;
            let requests = finish(server).await;
            assert_eq!(requests, vec![payload()]);
            assert!(polled.load(Ordering::Acquire));
            assert_eq!(
                result,
                Err(match response {
                    Response::Lost => Failure::Http,
                    Response::Redirect => Failure::Status(307),
                    Response::Truncated => Failure::Body,
                    Response::Oversize => Failure::TooLarge,
                    _ => unreachable!(),
                })
            );
            assert!(!format!("{result:?}").contains(TOKEN));
        }
    }
}

#[tokio::test]
async fn owned_driver_requires_correct_name_and_explicit_root() {
    for wrong_root in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (acceptor, mut trust) = tls("127.0.0.1");
        if wrong_root {
            trust = tls("127.0.0.1").1;
        } else {
            trust.name = "wrong.invalid";
        }
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            assert!(acceptor.accept(stream).await.is_err());
        });
        let polled = Arc::new(AtomicBool::new(false));
        let result = exchange(address, Some(trust), payload(), polled.clone()).await;
        finish(server).await;
        assert_eq!(result, Err(Failure::Connect));
        assert!(!polled.load(Ordering::Acquire));
    }
}

#[tokio::test]
async fn owned_driver_connect_deadline_includes_stalled_tls() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (_acceptor, trust) = tls("127.0.0.1");
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let _ = stopped.await;
        drop(stream);
    });
    let polled = Arc::new(AtomicBool::new(false));
    let result = exchange(address, Some(trust), payload(), polled.clone()).await;
    let _ = stop.send(());
    finish(server).await;
    assert_eq!(result, Err(Failure::ConnectDeadline));
    assert!(!polled.load(Ordering::Acquire));
}

#[tokio::test]
async fn canceling_owned_driver_after_full_receipt_closes_socket_without_replay() {
    for encrypted in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (acceptor, trust) = tls("127.0.0.1");
        let (admitted, receipt) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream: Stream = if encrypted {
                Box::new(acceptor.accept(stream).await.unwrap())
            } else {
                Box::new(stream)
            };
            let bytes = received(&mut stream, address).await;
            let _ = admitted.send(());
            let closed = stream.read_u8().await;
            let kind = canceled_socket_closed(closed);
            eprintln!("owned cancellation TLS={encrypted}, observed socket closure={kind:?}");
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
            bytes
        });
        let polled = Arc::new(AtomicBool::new(false));
        let client = tokio::spawn(exchange(
            address,
            encrypted.then_some(trust),
            payload(),
            polled.clone(),
        ));
        let received = tokio::time::timeout(Duration::from_secs(5), receipt).await;
        client.abort();
        assert!(client.await.unwrap_err().is_cancelled());
        let bytes = finish(server).await;
        received.unwrap().unwrap();
        assert_eq!(bytes, payload());
        assert!(polled.load(Ordering::Acquire));
    }
}

#[tokio::test]
async fn owned_driver_caps_chunked_bodies_without_content_length() {
    for encrypted in [false, true] {
        for response in [Response::ChunkedExact, Response::ChunkedOversize] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (acceptor, trust) = tls("127.0.0.1");
            let server = tokio::spawn(peer(listener, encrypted.then_some(acceptor), response, 1));
            let result = exchange(
                address,
                encrypted.then_some(trust),
                payload(),
                Arc::new(AtomicBool::new(false)),
            )
            .await;
            let requests = finish(server).await;
            assert_eq!(requests, vec![payload()]);
            if matches!(response, Response::ChunkedOversize) {
                assert_eq!(result, Err(Failure::TooLarge));
            } else {
                let bytes = result.unwrap();
                assert_eq!(bytes.len(), MAX_REPLY);
                assert!(bytes.iter().all(|&b| b == 0x42));
            }
        }
    }
}

#[tokio::test]
async fn owned_driver_whole_deadline_drops_a_stalled_response_and_socket() {
    for encrypted in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (acceptor, trust) = tls("127.0.0.1");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream: Stream = if encrypted {
                Box::new(acceptor.accept(stream).await.unwrap())
            } else {
                Box::new(stream)
            };
            let bytes = received(&mut stream, address).await;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 99\r\n\r\npartial")
                .await
                .unwrap();
            stream.flush().await.unwrap();
            canceled_socket_closed(stream.read_u8().await);
            bytes
        });
        let polled = Arc::new(AtomicBool::new(false));
        let result = exchange(
            address,
            encrypted.then_some(trust),
            payload(),
            polled.clone(),
        )
        .await;
        let bytes = finish(server).await;
        assert_eq!(bytes, payload());
        assert!(polled.load(Ordering::Acquire));
        assert_eq!(result, Err(Failure::Deadline));
    }
}
