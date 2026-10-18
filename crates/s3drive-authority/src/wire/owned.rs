//! One RPC owns the TCP/TLS stream, HTTP/1 driver and bounded body consumer.
use super::*;
use axum::http::{Request as HttpRequest, header};
use hyper_util::rt::TokioIo;
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    sync::oneshot,
    time::{Instant, timeout, timeout_at},
};
use tokio_rustls::TlsConnector;
use tower_service::Service;

pub(super) trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}
pub(super) type Stream = Box<dyn Io>;
const MAX_REPLY: usize = 8 * 1024 * 1024;

impl WireClient {
    pub(super) async fn request_inner(
        &self,
        bytes: Vec<u8>,
        kind: WireRequest,
        progress: Arc<Progress>,
        deadline: Instant,
        completed: oneshot::Sender<Result<Reply, WireError>>,
    ) -> Result<(), WireError> {
        if Instant::now() >= deadline {
            return Err(WireError::deadline(WirePhase::Request, kind, &progress));
        }
        let host = match self.url.host().ok_or(WireError::Configuration)? {
            url::Host::Domain(host) => host.to_owned(),
            url::Host::Ipv4(ip) => ip.to_string(),
            url::Host::Ipv6(ip) => ip.to_string(),
        };
        let uri: axum::http::Uri = self
            .url
            .as_str()
            .parse()
            .map_err(|_| WireError::Configuration)?;
        progress.connector.store(1, Ordering::Release);
        // Reuse the pinned legacy client's DNS/TCP connector and 300ms
        // alternate-family race, without its HTTP client, pool or driver task.
        // Only the winning TCP stream enters TLS; HTTP is never retried.
        let stream: Stream = timeout(CONNECT_TIMEOUT, async {
            let mut connector = hyper_util::client::legacy::connect::HttpConnector::new();
            connector.enforce_http(false);
            connector.set_connect_timeout(Some(CONNECT_TIMEOUT));
            connector.set_happy_eyeballs_timeout(Some(Duration::from_millis(300)));
            connector.set_nodelay(true);
            connector.set_keepalive(Some(Duration::from_secs(15)));
            connector.set_keepalive_interval(Some(Duration::from_secs(15)));
            connector.set_keepalive_retries(Some(3));
            std::future::poll_fn(|cx| connector.poll_ready(cx))
                .await
                .map_err(|e| WireError::transport(&e, WirePhase::Connect, kind, &progress))?;
            let tcp = connector
                .call(uri)
                .await
                .map_err(|e| WireError::transport(&e, WirePhase::Connect, kind, &progress))?
                .into_inner();
            let socket = tcp
                .local_addr()
                .ok()
                .zip(tcp.peer_addr().ok())
                .and_then(|(local, remote)| crate::LoopbackSocket::from_endpoints(local, remote));
            progress
                .socket
                .set(socket)
                .map_err(|_| WireError::Execution { request: kind })?;
            let stream: Stream = if let Some(config) = &self.tls {
                let name = rustls::pki_types::ServerName::try_from(host.clone())
                    .map_err(|_| WireError::Configuration)?;
                Box::new(
                    TlsConnector::from(config.clone())
                        .connect(name, tcp)
                        .await
                        .map_err(|e| {
                            WireError::transport(&e, WirePhase::Connect, kind, &progress)
                        })?,
                )
            } else {
                Box::new(tcp)
            };
            progress.candidate.observe(socket);
            progress.connector.store(2, Ordering::Release);
            Ok(stream)
        })
        .await
        .map_err(|_| WireError::deadline(WirePhase::Connect, kind, &progress))??;
        self.exchange_stream(stream, bytes, kind, progress, deadline, completed)
            .await
    }

    pub(super) async fn exchange_stream(
        &self,
        stream: Stream,
        bytes: Vec<u8>,
        kind: WireRequest,
        progress: Arc<Progress>,
        deadline: Instant,
        completed: oneshot::Sender<Result<Reply, WireError>>,
    ) -> Result<(), WireError> {
        let (mut sender, connection) = timeout_at(
            deadline,
            hyper::client::conn::http1::handshake(TokioIo::new(stream)),
        )
        .await
        .map_err(|_| WireError::deadline(WirePhase::Request, kind, &progress))?
        .map_err(|e| WireError::transport(&e, WirePhase::Request, kind, &progress))?;
        let authority = &self.url[url::Position::BeforeHost..url::Position::AfterPort];
        let request = HttpRequest::post(self.url.path())
            .header(header::HOST, authority)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token.0))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::CONNECTION, "close")
            .body(ObservedBody {
                inner: reqwest::Body::from(bytes),
                progress: progress.clone(),
            })
            .map_err(|_| WireError::Configuration)?;
        let observed = progress.clone();
        let work = async move {
            // The pinned channel permits the first request to be buffered
            // before its driver is polled. There is exactly one send_request.
            let response = sender
                .send_request(request)
                .await
                .map_err(|e| WireError::transport(&e, WirePhase::Request, kind, &observed))?;
            observed.response_headers.store(true, Ordering::Release);
            if !response.status().is_success() {
                return Err(WireError::HttpStatus(response.status().as_u16()));
            }
            if response
                .body()
                .size_hint()
                .upper()
                .is_some_and(|n| n > MAX_REPLY as u64)
            {
                return Err(WireError::ResponseTooLarge);
            }
            let mut body = response.into_body();
            let mut bytes = Vec::new();
            while let Some(frame) =
                std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await
            {
                let frame = frame.map_err(|e| {
                    WireError::transport(&e, WirePhase::ResponseBody, kind, &observed)
                })?;
                if let Ok(data) = frame.into_data() {
                    if data.len() > MAX_REPLY - bytes.len() {
                        return Err(WireError::ResponseTooLarge);
                    }
                    bytes.extend_from_slice(&data);
                }
            }
            serde_json::from_slice(&bytes).map_err(|_| WireError::InvalidReply)
        };
        let mut work = Box::pin(work);
        let mut connection = Box::pin(connection);
        let (result, driver_finished) = tokio::select! {
            biased;
            result = &mut work => (result, false),
            _ = &mut connection => (work.as_mut().await, true),
        };
        let success = result.is_ok();
        // Publish the exact parsed result before cleanup, including a known
        // protocol error. Caller timeout/drop still owns task cancellation.
        let _ = completed.send(result);
        drop(work);
        if success && !driver_finished {
            // Never repoll a completed connection; never reset the deadline.
            let _ = timeout_at(deadline, connection.as_mut()).await;
        } else if !success {
            // Stop HTTP/body processing, then attempt TLS close_notify on the
            // owned usable stream even for status, framing and JSON errors.
            let mut stream = (*Pin::into_inner(connection)).into_parts().io.into_inner();
            let _ = timeout_at(deadline, stream.shutdown()).await;
        }
        Ok(())
    }
}
