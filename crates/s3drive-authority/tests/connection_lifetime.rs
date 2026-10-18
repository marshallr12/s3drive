//! Observable transport lifetime, not a reproducer of the Windows failures.
//! The fixture allows keep-alive; each logical RPC must still use one transport.
use axum::{Json, Router, http::HeaderMap, routing::post, serve::ListenerExt};
use s3drive_authority::{Reply, Request, Token, Volume, WireClient};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::{net::TcpListener, sync::oneshot};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn metadata_calls_use_distinct_transports_even_when_the_peer_allows_keepalive() {
    let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/namespace", tcp.local_addr().unwrap());
    let client = WireClient::loopback(&endpoint, Token::new("a".repeat(64)).unwrap()).unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let count = accepted.clone();
    let listener = tcp.tap_io(move |_| {
        count.fetch_add(1, Ordering::SeqCst);
    });
    let closes = Arc::new(AtomicUsize::new(0));
    let close_count = closes.clone();
    let expected = Volume {
        id: "1".repeat(32),
        name_policy: "portable-collision-v1".into(),
        sequence: 0,
    };
    let returned = expected.clone();
    let app = Router::new().route(
        "/v1/namespace",
        post(move |headers: HeaderMap, Json(request): Json<Request>| {
            let volume = returned.clone();
            let close_count = close_count.clone();
            async move {
                assert!(matches!(request, Request::Info {}));
                assert!(headers.get("authorization").is_some());
                if headers
                    .get("connection")
                    .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"close"))
                {
                    close_count.fetch_add(1, Ordering::SeqCst);
                }
                Json(Reply::Info(volume))
            }
        }),
    );
    let (stop, stopped) = oneshot::channel();
    let mut server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
    });
    let replies = tokio::time::timeout(Duration::from_secs(20), async {
        let mut replies = Vec::new();
        for _ in 0..16 {
            let reply = client.request(&Request::Info {}).await;
            let failed = reply.is_err();
            replies.push(reply);
            if failed {
                break;
            }
        }
        replies
    })
    .await;
    // Close/reap the task before assertions, including a stalled/failed caller.
    let _ = stop.send(());
    let closed = tokio::time::timeout(Duration::from_secs(3), &mut server).await;
    if closed.is_err() {
        server.abort();
        let _ = server.await;
    }
    assert!(matches!(closed, Ok(Ok(Ok(())))), "fixture failed to stop");
    let replies = replies.expect("fixture RPC deadline");
    assert_eq!(replies.len(), 16);
    for reply in replies {
        assert_eq!(reply.unwrap(), Reply::Info(expected.clone()));
    }
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        16,
        "metadata client reused a physical transport"
    );
    assert_eq!(
        closes.load(Ordering::SeqCst),
        16,
        "metadata request did not declare its finite lifetime"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn metadata_client_closes_after_complete_reply_when_peer_omits_close() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/namespace", tcp.local_addr().unwrap());
    let client = WireClient::loopback(&endpoint, Token::new("a".repeat(64)).unwrap()).unwrap();
    let expected = Volume {
        id: "2".repeat(32),
        name_policy: "portable-collision-v1".into(),
        sequence: 9,
    };
    let reply = serde_json::to_vec(&Reply::Info(expected.clone())).unwrap();
    let mut server = tokio::spawn(async move {
        let exchange = async {
            let (mut stream, _) = tcp.accept().await.unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                assert!(header.len() < 8192, "bounded fixture request header");
                header.push(stream.read_u8().await.unwrap());
            }
            let text = std::str::from_utf8(&header).unwrap();
            let length = text
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap();
            assert!(length <= 4096);
            let mut body = vec![0; length];
            stream.read_exact(&mut body).await.unwrap();
            assert!(matches!(
                serde_json::from_slice::<Request>(&body).unwrap(),
                Request::Info {}
            ));
            // Deliberately omit Connection: close and leave the peer socket open.
            // The client's own lifetime decision must still close its transport.
            stream
                .write_all(
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", reply.len())
                        .as_bytes(),
                )
                .await
                .unwrap();
            stream.write_all(&reply).await.unwrap();
            stream.flush().await.unwrap();
            let mut byte = [0];
            assert_eq!(
                stream.read(&mut byte).await.unwrap(),
                0,
                "client sent another request instead of closing"
            );
        };
        tokio::time::timeout(Duration::from_secs(3), exchange)
            .await
            .expect("client retained its idle transport");
    });
    let observed = client.request(&Request::Info {}).await;
    let closed = tokio::time::timeout(Duration::from_secs(5), &mut server).await;
    if closed.is_err() {
        server.abort();
        let _ = server.await;
    }
    assert!(
        matches!(closed, Ok(Ok(()))),
        "fixture did not observe client-side close"
    );
    assert_eq!(observed.unwrap(), Reply::Info(expected));
}
