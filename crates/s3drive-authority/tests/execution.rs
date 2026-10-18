//! Ownership/cancellation at the boundary used by native callback threads.
use s3drive_authority::{
    Apply, Kind, Mutation, ROOT, Reply, Request, Token, Volume, WireClient, WireError, WireRequest,
};
use std::{
    future::Future,
    io::ErrorKind,
    task::{Context, Poll, Waker},
    time::Duration,
};

#[test]
fn canceled_queued_rpc_leaves_no_task_or_connection_and_valid_rpc_still_works() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/namespace", listener.local_addr().unwrap());
    let client = {
        let _entered = runtime.enter();
        WireClient::loopback(&endpoint, Token::new("a".repeat(64)).unwrap()).unwrap()
    };
    let mut context = Context::from_waker(Waker::noop());
    // Unsupported execution context must fail explicitly, without network I/O.
    let request = Request::Info {};
    let mut outside = Box::pin(client.request(&request));
    assert!(matches!(
        outside.as_mut().poll(&mut context),
        Poll::Ready(Err(WireError::Execution {
            request: WireRequest::Info
        }))
    ));
    drop(outside);
    assert_eq!(listener.accept().unwrap_err().kind(), ErrorKind::WouldBlock);

    let before = runtime.metrics().num_alive_tasks();
    let apply = Request::Apply {
        request: Apply {
            volume: "1".repeat(32),
            operation: "2".repeat(32),
            expected_sequence: 0,
            mutation: Mutation::Create {
                parent: ROOT.into(),
                name: "cancel before dispatch 🦀".into(),
                node_kind: Kind::Directory,
            },
        },
    };
    {
        // Entering supplies the handle, but this single-thread runtime is not
        // being driven: the new RPC job cannot have executed before cancellation.
        let _entered = runtime.enter();
        let mut call = Box::pin(client.request(&apply));
        assert!(call.as_mut().poll(&mut context).is_pending());
        assert!(runtime.metrics().num_alive_tasks() > before);
        drop(call);
    }
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(2), async {
            while runtime.metrics().num_alive_tasks() != before {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("canceled queued RPC left owned work alive");
    });
    assert_eq!(listener.accept().unwrap_err().kind(), ErrorKind::WouldBlock);

    // A real exchange afterward proves cancellation did not poison the client,
    // and that the no-connection result was not merely invalid configuration.
    runtime.block_on(async {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let expected = Volume {
            id: "1".repeat(32),
            name_policy: "exact-utf8-v1".into(),
            sequence: 0,
        };
        let body = serde_json::to_vec(&Reply::Info(expected.clone())).unwrap();
        let mut server = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(3), async {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut headers = Vec::new();
                while !headers.ends_with(b"\r\n\r\n") {
                    assert!(headers.len() < 8192);
                    headers.push(stream.read_u8().await.unwrap());
                }
                let length = std::str::from_utf8(&headers)
                    .unwrap()
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|n| n.parse::<usize>().ok())
                    })
                    .unwrap();
                assert!(length < 4096);
                let mut request = vec![0; length];
                stream.read_exact(&mut request).await.unwrap();
                assert!(matches!(
                    serde_json::from_slice::<Request>(&request).unwrap(),
                    Request::Info {}
                ));
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                stream.write_all(&body).await.unwrap();
                stream.flush().await.unwrap();
            })
            .await
            .unwrap();
        });
        let reply = client.request(&Request::Info {}).await;
        let stopped = tokio::time::timeout(Duration::from_secs(4), &mut server).await;
        if stopped.is_err() {
            server.abort();
            let _ = server.await;
        }
        assert!(
            matches!(stopped, Ok(Ok(()))),
            "positive fixture did not finish"
        );
        assert_eq!(reply.unwrap(), Reply::Info(expected));
    });
}
