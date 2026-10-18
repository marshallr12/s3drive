//! Actual production exchange/caller boundary with controlled shutdown, real
//! TLS/SQLite and exact durable operation receipts. No provider credentials.
use super::{owned::Stream, *};
use crate::{Apply, Authority, Kind, Mutation, ROOT};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
use tokio_rustls::{TlsAcceptor, TlsConnector};

#[derive(Clone, Copy, PartialEq)]
enum Close {
    Clean,
    Fail,
    Expire,
}
struct Gate {
    stream: Stream,
    entered: Option<oneshot::Sender<()>>,
    release: Option<oneshot::Receiver<()>>,
    fail: bool,
}
impl AsyncRead for Gate {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}
impl AsyncWrite for Gate {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        use std::future::Future;
        if let Some(entered) = self.entered.take() {
            let _ = entered.send(());
        }
        if let Some(release) = &mut self.release {
            if Pin::new(release).poll(cx).is_pending() {
                return Poll::Pending;
            }
            self.release = None;
        }
        if self.fail {
            return Poll::Ready(Err(io::Error::other("controlled shutdown failure")));
        }
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
async fn reap<T>(mut task: tokio::task::JoinHandle<T>) -> Result<T, &'static str> {
    match tokio::time::timeout(Duration::from_secs(5), &mut task).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(_)) => Err("fixture task failed"),
        Err(_) => {
            task.abort();
            let _ = task.await;
            Err("fixture timed out")
        }
    }
}
async fn receipt_with_gated_shutdown(mode: Close) {
    let tmp = tempfile::tempdir().unwrap();
    let authority = Authority::open(tmp.path().join("authority")).await.unwrap();
    let backend = authority.client();
    let Reply::Info(volume) = backend.request(Request::Info {}).await else {
        panic!("volume")
    };
    let operation = "5".repeat(32);
    let request = Request::Apply {
        request: Apply {
            volume: volume.id.clone(),
            operation: operation.clone(),
            expected_sequence: 0,
            mutation: Mutation::Create {
                parent: ROOT.into(),
                name: "retained receipt café 🦀".into(),
                node_kind: Kind::Directory,
            },
        },
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    let expected_bytes = bytes.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let endpoint = format!("https://{address}/v1/namespace");
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let server_config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![cert.der().clone()],
        rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
    )
    .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    let client = WireClient::https(
        &endpoint,
        Token::new("a".repeat(64)).unwrap(),
        Some(cert.pem().as_bytes()),
    )
    .unwrap();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut tls = acceptor.accept(tcp).await.unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            assert!(headers.len() < 8192);
            headers.push(tls.read_u8().await.unwrap());
        }
        let text = std::str::from_utf8(&headers).unwrap().to_ascii_lowercase();
        assert!(text.starts_with("post /v1/namespace http/1.1\r\n"));
        assert!(text.contains(&format!("\r\nhost: {address}\r\n")));
        assert!(text.contains(&format!("\r\nauthorization: bearer {}\r\n", "a".repeat(64))));
        assert!(text.contains("\r\nconnection: close\r\n"));
        assert!(!text.contains("transfer-encoding:"));
        let length: usize = text
            .lines()
            .find_map(|l| l.strip_prefix("content-length: "))
            .unwrap()
            .parse()
            .unwrap();
        let mut body = vec![0; length];
        tls.read_exact(&mut body).await.unwrap();
        assert_eq!(body, expected_bytes);
        let committed = backend
            .request(serde_json::from_slice(&body).unwrap())
            .await;
        assert!(matches!(committed, Reply::Applied(_)));
        if mode == Close::Expire {
            tokio::time::sleep(Duration::from_secs(6)).await;
        }
        let data = serde_json::to_vec(&committed).unwrap();
        tls.write_all(
            format!(
                "HTTP/1.1 201 Created\r\nContent-Length: {}\r\n\r\n",
                data.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
        tls.write_all(&data).await.unwrap();
        tls.flush().await.unwrap();
        let closed = tls.read(&mut [0; 1]).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "unexpected replay"
        );
        (committed, closed)
    });
    let started = tokio::time::Instant::now();
    let deadline = started + REQUEST_TIMEOUT;
    let watchdog = deadline + Duration::from_secs(4);
    let progress = Arc::new(Progress::default());
    let worker_progress = progress.clone();
    let (completed, reply) = oneshot::channel();
    let (entered, entering) = oneshot::channel();
    let (release, released) = oneshot::channel();
    let mut tasks = tokio::task::JoinSet::new();
    tasks.spawn(async move {
        let tcp = TcpStream::connect(address).await.unwrap();
        let tls = TlsConnector::from(client.tls.clone().unwrap())
            .connect("127.0.0.1".try_into().unwrap(), tcp)
            .await
            .unwrap();
        client
            .exchange_stream(
                Box::new(Gate {
                    stream: Box::new(tls),
                    entered: Some(entered),
                    release: Some(released),
                    fail: mode == Close::Fail,
                }),
                bytes,
                WireRequest::Apply,
                worker_progress,
                deadline,
                completed,
            )
            .await
    });
    let mut call = tokio::spawn(async move {
        finish_request(tasks, reply, deadline, WireRequest::Apply, &progress).await
    });
    let entered = tokio::time::timeout_at(watchdog, entering).await;
    let premature = tokio::time::timeout(Duration::from_millis(100), &mut call).await;
    let returned_early = premature.is_ok();
    if mode != Close::Expire {
        let _ = release.send(());
    }
    let outcome = match premature {
        Ok(result) => Some(result),
        Err(_) => match tokio::time::timeout_at(watchdog, &mut call).await {
            Ok(result) => Some(result),
            Err(_) => {
                call.abort();
                let _ = call.await;
                None
            }
        },
    };
    let elapsed = started.elapsed();
    let peer = reap(server).await;
    let recovered = authority
        .client()
        .request(Request::Operation {
            volume: volume.id.clone(),
            operation,
        })
        .await;
    let after = authority.client().request(Request::Info {}).await;
    authority.shutdown().await.unwrap();
    entered.unwrap().unwrap();
    assert!(!returned_early, "caller abandoned pending cleanup");
    let result = outcome
        .expect("cleanup exceeded original deadline")
        .unwrap()
        .unwrap();
    let (receipt, closed) = peer.unwrap();
    assert_eq!(result, receipt);
    let Reply::Applied(receipt) = receipt else {
        unreachable!()
    };
    assert_eq!(recovered, Reply::Operation(Some(Ok(receipt))));
    assert_eq!(
        after,
        Reply::Info(crate::Volume {
            sequence: 1,
            ..volume
        })
    );
    if mode == Close::Clean {
        assert_eq!(closed.unwrap(), 0, "missing TLS close_notify");
    } else {
        assert!(
            matches!(closed, Err(e) if matches!(e.kind(), io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset))
        );
    }
    if mode == Close::Expire {
        assert!(
            elapsed >= REQUEST_TIMEOUT && elapsed < REQUEST_TIMEOUT + Duration::from_secs(4),
            "{elapsed:?}"
        );
    }
}
#[tokio::test]
async fn production_complete_receipt_waits_for_clean_tls_shutdown() {
    receipt_with_gated_shutdown(Close::Clean).await;
}
#[tokio::test]
async fn production_complete_receipt_survives_shutdown_failure() {
    receipt_with_gated_shutdown(Close::Fail).await;
}
#[tokio::test]
async fn production_outer_deadline_retains_committed_receipt_during_shutdown() {
    receipt_with_gated_shutdown(Close::Expire).await;
}

#[tokio::test]
async fn expired_before_dispatch_and_invalid_requests_open_no_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/namespace", listener.local_addr().unwrap());
    let client = WireClient::loopback(&endpoint, Token::new("a".repeat(64)).unwrap()).unwrap();
    let progress = Arc::new(Progress::default());
    let (completed, mut reply) = oneshot::channel();
    let result = client
        .request_inner(
            serde_json::to_vec(&Request::Info {}).unwrap(),
            WireRequest::Info,
            progress,
            tokio::time::Instant::now() - Duration::from_secs(1),
            completed,
        )
        .await;
    assert!(matches!(
        result,
        Err(WireError::Transport {
            timed_out: true,
            connector_started: false,
            connector_completed: false,
            body_frame_polled: false,
            owned_connection: crate::OwnedConnection::Unopened,
            ..
        })
    ));
    assert_eq!(reply.try_recv(), Err(oneshot::error::TryRecvError::Closed));
    assert_eq!(
        client
            .request(&Request::Node {
                volume: "x".repeat(5000),
                node: ROOT.into()
            })
            .await,
        Err(WireError::InvalidRequest)
    );
    assert_eq!(
        client
            .request(&Request::Apply {
                request: Apply {
                    volume: "invalid".into(),
                    operation: "1".repeat(32),
                    expected_sequence: 0,
                    mutation: Mutation::Create {
                        parent: ROOT.into(),
                        name: "refused before I/O".into(),
                        node_kind: Kind::Directory
                    },
                }
            })
            .await,
        Err(WireError::InvalidRequest)
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[test]
fn owned_diagnostics_distinguish_unopened_and_omitted_without_private_addresses() {
    assert!(std::mem::size_of::<WireError>() <= 128);
    let progress = Progress::default();
    assert_eq!(
        progress.owned_connection(),
        crate::OwnedConnection::Unopened
    );
    progress.socket.set(None).unwrap();
    assert_eq!(progress.owned_connection(), crate::OwnedConnection::Omitted);
}

// Independent I/O faults distinguish a peer already at EOF before HTTP starts
// from an actual attempted first write. Neither establishes a remote outcome.
struct FirstIoFault {
    eof: bool,
}
impl AsyncRead for FirstIoFault {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.eof {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}
impl AsyncWrite for FirstIoFault {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, _: &[u8]) -> Poll<io::Result<usize>> {
        assert!(!self.eof, "must observe existing peer EOF before writing");
        Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
#[tokio::test]
async fn production_progress_distinguishes_first_read_eof_from_failed_first_write() {
    let client = WireClient::loopback(
        "http://127.0.0.1:1/v1/namespace",
        Token::new("a".repeat(64)).unwrap(),
    )
    .unwrap();
    for eof in [true, false] {
        let progress = Arc::new(Progress::default());
        let (completed, reply) = oneshot::channel();
        tokio::time::timeout(
            Duration::from_secs(2),
            client.exchange_stream(
                Box::new(FirstIoFault { eof }),
                serde_json::to_vec(&Request::Info {}).unwrap(),
                WireRequest::Info,
                progress.clone(),
                tokio::time::Instant::now() + Duration::from_secs(1),
                completed,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        let Err(WireError::Transport {
            exchange,
            body_frame_polled,
            io_kind,
            ..
        }) = reply.await.unwrap()
        else {
            panic!("expected controlled transport error")
        };
        assert!(exchange.http_ready_ms.is_some());
        assert!(exchange.request_queued_ms.is_some());
        assert_eq!(exchange.sender_ready_before_enqueue, Some(false));
        assert!(exchange.first_driver_poll_ms.is_some());
        assert!(exchange.driver_polls > 0 && exchange.read_polls > 0);
        assert_eq!(exchange.read_bytes, 0);
        assert_eq!(exchange.written_bytes, 0);
        if eof {
            assert!(!body_frame_polled);
            assert!(!exchange.body_end_stream_queried);
            assert!(!exchange.body_size_hint_queried);
            assert_eq!(exchange.write_polls, 0);
        } else {
            assert!(body_frame_polled);
            assert!(exchange.body_end_stream_queried);
            assert!(exchange.body_size_hint_queried);
            assert!(exchange.write_polls > 0);
            assert_eq!(io_kind, Some(io::ErrorKind::BrokenPipe));
        }
        // Error snapshot is captured before cleanup; the live observation
        // separately confirms that error cleanup still attempts shutdown.
        assert!(progress.exchange.snapshot().shutdown_polls > 0);
    }
}

/// Models read-side cooperative work that leaves no budget for HTTP's dispatch
/// channel. It is a mechanism control, not a reproduction of the Windows cause.
struct BudgetRead {
    stream: TcpStream,
    blocked: Arc<AtomicBool>,
}
impl AsyncRead for BudgetRead {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.blocked.load(Ordering::Acquire) {
            // Bound fixture work even if a caller accidentally runs it under
            // unconstrained budgeting; do not assume a particular budget size.
            for _ in 0..4096 {
                match tokio::task::coop::poll_proceed(cx) {
                    Poll::Ready(permit) => permit.made_progress(),
                    Poll::Pending => return Poll::Pending,
                }
            }
            Poll::Ready(Err(io::Error::other(
                "fixture requires constrained task budget",
            )))
        } else {
            Pin::new(&mut self.stream).poll_read(cx, buf)
        }
    }
}
impl AsyncWrite for BudgetRead {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }
    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write_vectored(cx, bufs)
    }
    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

async fn dispatch_budget_case(drain_budget: bool) {
    let tmp = tempfile::tempdir().unwrap();
    let authority = Authority::open(tmp.path().join("authority")).await.unwrap();
    let expected = authority.client().request(Request::Info {}).await;
    let response = serde_json::to_vec(&expected).unwrap();
    let request = serde_json::to_vec(&Request::Info {}).unwrap();
    let expected_request = request.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut tcp, _) = listener.accept().await.unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            assert!(headers.len() < 8192);
            headers.push(tcp.read_u8().await.unwrap());
        }
        let text = std::str::from_utf8(&headers).unwrap().to_ascii_lowercase();
        assert!(text.starts_with("post /v1/namespace http/1.1\r\n"));
        assert!(text.contains("\r\nconnection: close\r\n"));
        assert!(!text.contains("transfer-encoding:"));
        let length: usize = text
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(length, expected_request.len());
        let mut received = vec![0; length];
        tcp.read_exact(&mut received).await.unwrap();
        assert_eq!(received, expected_request);
        tcp.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
        tcp.write_all(&response).await.unwrap();
        tcp.flush().await.unwrap();
        assert_eq!(
            tcp.read(&mut [0; 1]).await.unwrap(),
            0,
            "extra request bytes"
        );
        listener
    });
    let tcp = TcpStream::connect(address).await.unwrap();
    let client = WireClient::loopback(
        &format!("http://{address}/v1/namespace"),
        Token::new("a".repeat(64)).unwrap(),
    )
    .unwrap();
    let blocked = Arc::new(AtomicBool::new(drain_budget));
    let progress = Arc::new(Progress::default());
    let worker_progress = progress.clone();
    let worker_gate = blocked.clone();
    let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
    let (completed, reply) = oneshot::channel();
    let mut tasks = tokio::task::JoinSet::new();
    tasks.spawn(async move {
        client
            .exchange_stream(
                Box::new(BudgetRead {
                    stream: tcp,
                    blocked: worker_gate,
                }),
                request,
                WireRequest::Info,
                worker_progress,
                deadline,
                completed,
            )
            .await
    });
    let caller_progress = progress.clone();
    let caller = tokio::spawn(async move {
        finish_request(tasks, reply, deadline, WireRequest::Info, &caller_progress).await
    });
    let stalled = if drain_budget {
        Some(
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let observed = progress.exchange.snapshot();
                    if observed.driver_polls >= 3 && observed.driver_budget_exhausted_after {
                        break (observed, progress.body_frame_polled.load(Ordering::Acquire));
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await,
        )
    } else {
        None
    };
    // Always release the fixture and reap workers before checking observations.
    blocked.store(false, Ordering::Release);
    let completed = reap(caller).await;
    let peer = reap(server).await;
    authority.shutdown().await.unwrap();
    if let Some(stalled) = stalled {
        let (stalled, frame_polled) = stalled.expect("no controlled dispatch-budget observation");
        assert_eq!(stalled.sender_ready_before_enqueue, Some(false));
        assert!(!stalled.driver_budget_exhausted_before);
        assert!(!stalled.body_end_stream_queried);
        assert!(!stalled.body_size_hint_queried);
        assert!(!frame_polled);
        assert_eq!(stalled.write_polls, 0);
        assert_eq!(stalled.written_bytes, 0);
        assert!(stalled.read_pending_polls >= 3 && stalled.flush_polls >= 3);
    }
    assert_eq!(completed.unwrap().unwrap(), expected);
    let listener = peer.unwrap();
    let mut cx = Context::from_waker(std::task::Waker::noop());
    assert!(
        listener.poll_accept(&mut cx).is_pending(),
        "extra connection"
    );
    let after = progress.exchange.snapshot();
    assert!(after.body_end_stream_queried && after.body_size_hint_queried);
    assert!(progress.body_frame_polled.load(Ordering::Acquire));
    assert!(after.write_polls > 0 && after.written_bytes > 0 && after.read_bytes > 0);
}

#[tokio::test]
async fn production_dispatch_budget_control_resumes_one_original_request_after_active_no_write_polls()
 {
    dispatch_budget_case(false).await;
    dispatch_budget_case(true).await;
}

#[tokio::test]
async fn observed_body_descriptors_preserve_empty_and_unicode_bytes_and_exact_framing_hints() {
    for bytes in [Vec::new(), "descriptor café 日本語 🦀".as_bytes().to_vec()] {
        let original = reqwest::Body::from(bytes.clone());
        let original_hint = original.size_hint();
        let original_end = original.is_end_stream();
        let progress = Arc::new(Progress::default());
        let mut observed = ObservedBody {
            inner: original,
            progress: progress.clone(),
        };
        assert!(!progress.exchange.snapshot().body_end_stream_queried);
        assert!(!progress.exchange.snapshot().body_size_hint_queried);
        let hint = observed.size_hint();
        assert_eq!(
            (hint.lower(), hint.upper()),
            (original_hint.lower(), original_hint.upper())
        );
        assert_eq!(observed.is_end_stream(), original_end);
        let mut received = Vec::new();
        while let Some(frame) =
            std::future::poll_fn(|cx| Pin::new(&mut observed).poll_frame(cx)).await
        {
            let frame = frame.unwrap().into_data().expect("unexpected trailers");
            received.extend_from_slice(&frame);
        }
        assert_eq!(received, bytes);
        assert!(observed.is_end_stream());
        let after = progress.exchange.snapshot();
        assert!(after.body_end_stream_queried && after.body_size_hint_queried);
        assert_eq!(after.write_polls, 0);
        assert_eq!(after.written_bytes, 0);
    }
}
