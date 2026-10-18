//! Independent byte peers for production HTTP/TLS parsing, trust and bounds.
use s3drive_authority::{Reply, Request, Token, Volume, WireClient, WireError, WirePhase};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpListener,
};
use tokio_rustls::{TlsAcceptor, rustls};
const SECRET: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const LIMIT: usize = 8 * 1024 * 1024;
trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}
#[derive(Clone, Copy)]
enum Finish {
    Clean,
    Drop,
    EarlyCap,
}
fn expected() -> Reply {
    Reply::Info(Volume {
        id: "1".repeat(32),
        name_policy: "exact-utf8-v1".into(),
        sequence: 7,
    })
}
fn framed(status: &str, body: &[u8]) -> Vec<u8> {
    let mut wire = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    wire.extend_from_slice(body);
    wire
}
async fn case(encrypted: bool, wire: Vec<u8>, finish: Finish) -> Result<Reply, WireError> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    // Resolve localhost for TLS, while preserving the DNS authority and SNI.
    let host = if encrypted { "localhost" } else { "127.0.0.1" };
    let authority = format!("{host}:{}", address.port());
    let endpoint = format!(
        "{}://{authority}/v1/namespace",
        if encrypted { "https" } else { "http" }
    );
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
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
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let client = if encrypted {
        WireClient::https(
            &endpoint,
            Token::new(SECRET.into()).unwrap(),
            Some(cert.pem().as_bytes()),
        )
    } else {
        WireClient::loopback(&endpoint, Token::new(SECRET.into()).unwrap())
    }
    .unwrap();
    let mut peer = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut stream: Box<dyn Io> = if encrypted {
            let tls = acceptor.accept(tcp).await.unwrap();
            assert_eq!(tls.get_ref().1.server_name(), Some("localhost"));
            Box::new(tls)
        } else {
            Box::new(tcp)
        };
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            assert!(headers.len() < 8192);
            headers.push(stream.read_u8().await.unwrap());
        }
        let text = std::str::from_utf8(&headers).unwrap().to_ascii_lowercase();
        assert!(text.starts_with("post /v1/namespace http/1.1\r\n"));
        assert!(text.contains(&format!("\r\nhost: {authority}\r\n")));
        assert!(text.contains(&format!("\r\nauthorization: bearer {SECRET}\r\n")));
        assert!(text.contains("\r\nconnection: close\r\n"));
        assert!(!text.contains("transfer-encoding:"));
        let length: usize = text
            .lines()
            .find_map(|l| l.strip_prefix("content-length: "))
            .unwrap()
            .parse()
            .unwrap();
        let mut bytes = vec![0; length];
        stream.read_exact(&mut bytes).await.unwrap();
        assert_eq!(bytes, serde_json::to_vec(&Request::Info {}).unwrap());
        let written = stream.write_all(&wire).await;
        if let Err(error) = written {
            assert!(
                matches!(finish, Finish::EarlyCap)
                    && matches!(
                        error.kind(),
                        std::io::ErrorKind::BrokenPipe
                            | std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::ConnectionAborted
                    ),
                "fixture write failed: {error:?}"
            );
        } else {
            let _ = stream.flush().await;
        }
        match finish {
            Finish::Clean => assert_eq!(
                stream.read(&mut [0; 1]).await.unwrap(),
                0,
                "unclean TLS closure or extra request"
            ),
            Finish::Drop | Finish::EarlyCap => {
                let _ = stream.shutdown().await;
            }
        }
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "redirect/replay created another transport"
        );
    });
    let result = client.request(&Request::Info {}).await;
    let stopped = tokio::time::timeout(Duration::from_secs(4), &mut peer).await;
    if stopped.is_err() {
        peer.abort();
        let _ = peer.await;
    }
    assert!(
        matches!(stopped, Ok(Ok(()))),
        "independent peer failed: {stopped:?}"
    );
    result
}
#[tokio::test]
async fn production_accepts_all_success_statuses_then_strictly_decodes_json() {
    let body = serde_json::to_vec(&expected()).unwrap();
    for encrypted in [false, true] {
        for status in ["200 OK", "201 Created", "299 Custom"] {
            assert_eq!(
                case(encrypted, framed(status, &body), Finish::Clean)
                    .await
                    .unwrap(),
                expected()
            );
        }
        for wire in [
            b"HTTP/1.1 204 No Content\r\n\r\n".to_vec(),
            framed(
                "200 OK",
                br#"{"result":"info","value":{},"unexpected":true}"#,
            ),
            framed(
                "201 Created",
                &[body.as_slice(), b" invalid suffix"].concat(),
            ),
        ] {
            assert_eq!(
                case(encrypted, wire, Finish::Clean).await,
                Err(WireError::InvalidReply)
            );
        }
    }
}
#[tokio::test]
async fn production_rejects_conflicting_lengths_and_incomplete_chunked_messages() {
    for encrypted in [false, true] {
        let error = case(
            encrypted,
            b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nx".to_vec(),
            Finish::Drop,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(
                error,
                WireError::Transport {
                    phase: WirePhase::Request,
                    ..
                }
            ),
            "{error:?}"
        );
        let error = case(
            encrypted,
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nbody\r\n".to_vec(),
            Finish::Drop,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(
                error,
                WireError::Transport {
                    phase: WirePhase::ResponseBody,
                    ..
                }
            ),
            "{error:?}"
        );
    }
}
#[tokio::test]
async fn production_chunk_extensions_trailers_and_exact_streamed_bound() {
    let mut body = serde_json::to_vec(&expected()).unwrap();
    for encrypted in [false, true] {
        let mut wire = format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTrailer: X-End\r\n\r\n{:x};fixture=yes\r\n", body.len()).into_bytes();
        wire.extend_from_slice(&body);
        wire.extend_from_slice(b"\r\n0\r\nX-End: finished\r\n\r\n");
        assert_eq!(
            case(encrypted, wire, Finish::Clean).await.unwrap(),
            expected()
        );
    }
    body.resize(LIMIT, b' ');
    for encrypted in [false, true] {
        for excess in [false, true] {
            let mut wire = format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n",
                LIMIT + usize::from(excess)
            )
            .into_bytes();
            wire.extend_from_slice(&body);
            if excess {
                wire.push(b' ');
            }
            wire.extend_from_slice(b"\r\n0\r\n\r\n");
            let result = case(
                encrypted,
                wire,
                if excess {
                    Finish::EarlyCap
                } else {
                    Finish::Clean
                },
            )
            .await;
            if excess {
                assert_eq!(result, Err(WireError::ResponseTooLarge));
            } else {
                assert_eq!(result.unwrap(), expected());
            }
        }
        assert_eq!(
            case(
                encrypted,
                b"HTTP/1.1 200 OK\r\nContent-Length: 8388609\r\n\r\n".to_vec(),
                Finish::Drop
            )
            .await,
            Err(WireError::ResponseTooLarge)
        );
    }
}
#[tokio::test]
async fn production_ipv6_host_brackets_and_port_are_preserved() {
    let listener = TcpListener::bind("[::1]:0").await.unwrap();
    let authority = listener.local_addr().unwrap().to_string();
    let client = WireClient::loopback(
        &format!("http://{authority}/v1/namespace"),
        Token::new(SECRET.into()).unwrap(),
    )
    .unwrap();
    let mut peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            assert!(headers.len() < 8192);
            headers.push(stream.read_u8().await.unwrap());
        }
        let text = std::str::from_utf8(&headers).unwrap().to_ascii_lowercase();
        assert!(text.contains(&format!("\r\nhost: {authority}\r\n")));
        let length: usize = text
            .lines()
            .find_map(|l| l.strip_prefix("content-length: "))
            .unwrap()
            .parse()
            .unwrap();
        let mut body = vec![0; length];
        stream.read_exact(&mut body).await.unwrap();
        assert!(matches!(
            serde_json::from_slice::<Request>(&body).unwrap(),
            Request::Info {}
        ));
        stream
            .write_all(&framed("200 OK", &serde_json::to_vec(&expected()).unwrap()))
            .await
            .unwrap();
        stream.flush().await.unwrap();
        assert_eq!(stream.read(&mut [0; 1]).await.unwrap(), 0);
    });
    let result = client.request(&Request::Info {}).await;
    let stopped = tokio::time::timeout(Duration::from_secs(4), &mut peer).await;
    if stopped.is_err() {
        peer.abort();
        let _ = peer.await;
    }
    assert!(matches!(stopped, Ok(Ok(()))));
    assert_eq!(result.unwrap(), expected());
}
