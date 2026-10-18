//! Bounded authenticated authority client. No redirects, proxy or
//! automatic retry. Transport failures leave mutation outcomes unknown.
use crate::{Reply, Request, Token};
use hyper::body::{Body as HttpBody, Bytes, Frame, SizeHint};
use std::{
    fmt, io,
    pin::Pin,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
mod owned;
mod progress;
pub use progress::ExchangeObservation;
#[cfg(test)]
mod tests;

/// Bounded sanitized diagnostics only. Never retains a URL, token, request,
/// response body, certificate, or the underlying dependency error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError {
    Configuration,
    InvalidRequest,
    Transport {
        request: WireRequest,
        phase: WirePhase,
        timed_out: bool,
        io_kind: Option<io::ErrorKind>,
        incomplete_message: bool,
        connection_closed: bool,
        /// The HTTP consumer polled a data frame. This does not prove a
        /// socket write, server receipt, authentication or database commit.
        body_frame_polled: bool,
        /// A connector call was made under this request's task scope.
        connector_started: bool,
        /// Such a connector returned a connection successfully. This does not
        /// identify the selected socket or establish request delivery/commit.
        connector_completed: bool,
        connector_candidate: crate::ConnectorCandidate,
        /// Legacy error-chain metadata. The direct transport leaves this
        /// Unobserved; it never fabricates a legacy connector error.
        error_connection: crate::ErrorConnection,
        /// Actual TCP connection owned by this RPC, including TLS rejection.
        /// No address other than canonical loopback ports is representable.
        owned_connection: crate::OwnedConnection,
        /// Local HTTP driver and plaintext I/O observations, not delivery or
        /// commit evidence. Timings are relative to this RPC's creation.
        exchange: ExchangeObservation,
    },
    HttpStatus(u16),
    ResponseTooLarge,
    InvalidReply,
    /// Local RPC task/runtime failure. Mutation outcome remains unknown.
    Execution {
        request: WireRequest,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WirePhase {
    Connect,
    Request,
    ResponseBody,
}
/// Only the fixed protocol variant; never request fields or identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireRequest {
    Features,
    Info,
    Store,
    Reservation,
    Node,
    PendingRemoval,
    List,
    Operation,
    Apply,
}
impl From<&Request> for WireRequest {
    fn from(request: &Request) -> Self {
        match request {
            Request::Features {} => Self::Features,
            Request::Info {} => Self::Info,
            Request::Store { .. } => Self::Store,
            Request::Reservation { .. } => Self::Reservation,
            Request::Node { .. } => Self::Node,
            Request::PendingRemoval { .. } => Self::PendingRemoval,
            Request::List { .. } => Self::List,
            Request::Operation { .. } => Self::Operation,
            Request::Apply { .. } => Self::Apply,
        }
    }
}
impl WireError {
    fn transport(
        error: &(dyn std::error::Error + 'static),
        phase: WirePhase,
        request: WireRequest,
        progress: &Progress,
    ) -> Self {
        let mut io_kind = None;
        let mut incomplete_message = false;
        let mut connection_closed = false;
        let mut timed_out = false;
        let mut source = Some(error);
        // Pinned public typed predicates only, including the root error.
        // Never dependency strings, URLs, certificates, headers or payloads.
        for _ in 0..16 {
            let Some(cause) = source else { break };
            if let Some(error) = cause.downcast_ref::<io::Error>() {
                io_kind = Some(error.kind());
                timed_out |= error.kind() == io::ErrorKind::TimedOut;
            }
            if let Some(error) = cause.downcast_ref::<hyper::Error>() {
                incomplete_message |= error.is_incomplete_message();
                connection_closed |= error.is_closed();
                timed_out |= error.is_timeout();
            }
            source = cause.source();
        }
        let (connector_started, connector_completed) = progress.connector_observation();
        Self::Transport {
            request,
            phase,
            timed_out,
            io_kind,
            incomplete_message,
            connection_closed,
            body_frame_polled: progress.body_frame_polled.load(Ordering::Acquire),
            connector_started,
            connector_completed,
            connector_candidate: progress.candidate.snapshot(),
            error_connection: crate::ErrorConnection::Unobserved,
            owned_connection: progress.owned_connection(),
            exchange: progress.exchange.snapshot(),
        }
    }
    fn deadline(phase: WirePhase, request: WireRequest, progress: &Progress) -> Self {
        let (connector_started, connector_completed) = progress.connector_observation();
        Self::Transport {
            request,
            phase,
            timed_out: true,
            io_kind: None,
            incomplete_message: false,
            connection_closed: false,
            body_frame_polled: progress.body_frame_polled.load(Ordering::Acquire),
            connector_started,
            connector_completed,
            connector_candidate: progress.candidate.snapshot(),
            error_connection: crate::ErrorConnection::Unobserved,
            owned_connection: progress.owned_connection(),
            exchange: progress.exchange.snapshot(),
        }
    }
}

// Preserve the exact validated serialized bytes and framing hints. Only retain
// a boolean when the HTTP consumer actually polls a data frame.
struct ObservedBody {
    inner: reqwest::Body,
    progress: Arc<Progress>,
}
#[derive(Default)]
struct Progress {
    exchange: progress::ExchangeProgress,
    body_frame_polled: AtomicBool,
    response_headers: AtomicBool,
    // Monotonic aggregate: 0=unobserved, 1=called, 2=at least one success.
    connector: AtomicU8,
    candidate: crate::connection_tag::Candidate,
    socket: OnceLock<Option<crate::LoopbackSocket>>,
}
impl Progress {
    fn owned_connection(&self) -> crate::OwnedConnection {
        match self.socket.get() {
            None => crate::OwnedConnection::Unopened,
            Some(None) => crate::OwnedConnection::Omitted,
            Some(Some(socket)) => crate::OwnedConnection::Loopback(*socket),
        }
    }
    fn connector_observation(&self) -> (bool, bool) {
        let observed = self.connector.load(Ordering::Acquire);
        (observed >= 1, observed == 2)
    }
}
impl HttpBody for ObservedBody {
    type Data = Bytes;
    type Error = reqwest::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let frame = Pin::new(&mut self.inner).poll_frame(cx);
        if matches!(&frame, Poll::Ready(Some(Ok(frame))) if frame.data_ref().is_some()) {
            self.progress
                .body_frame_polled
                .store(true, Ordering::Release);
        }
        frame
    }
    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }
}
impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "authority transport failed ({self:?}); inspect the durable operation before proceeding"
        )
    }
}
impl std::error::Error for WireError {}
#[derive(Clone)]
pub struct WireClient {
    tls: Option<Arc<rustls::ClientConfig>>,
    url: url::Url,
    token: Token,
}
impl WireClient {
    pub fn loopback(endpoint: &str, token: Token) -> Result<Self, WireError> {
        let url = url::Url::parse(endpoint).map_err(|_| WireError::Configuration)?;
        let address = url
            .host_str()
            .and_then(|s| s.trim_matches(['[', ']']).parse::<std::net::IpAddr>().ok());
        if url.scheme() != "http"
            || !address.is_some_and(|a| a.is_loopback())
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/v1/namespace"
        {
            return Err(WireError::Configuration);
        }
        Self::build(url, token, None)
    }
    /// HTTPS keeps hostname/certificate validation enabled. Supplying roots
    /// replaces platform trust instead of silently adding another authority.
    pub fn https(endpoint: &str, token: Token, roots: Option<&[u8]>) -> Result<Self, WireError> {
        let url = url::Url::parse(endpoint).map_err(|_| WireError::Configuration)?;
        if url.scheme() != "https"
            || url.host().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/v1/namespace"
        {
            return Err(WireError::Configuration);
        }
        Self::build(url, token, roots)
    }
    /// Mount factory: private files are bounded, link/ownership checked, and
    /// their contents never enter errors. Local HTTP cannot carry TLS options.
    pub fn from_config(
        config: &s3drive_core::ManagedConfig,
        token: Token,
    ) -> Result<Self, WireError> {
        if url::Url::parse(&config.authority)
            .map_err(|_| WireError::Configuration)?
            .scheme()
            == "https"
        {
            let roots = config
                .authority_ca_file
                .as_ref()
                .map(|p| {
                    s3drive_state::read_private_secret(p, 65_536)
                        .map_err(|_| WireError::Configuration)
                })
                .transpose()?;
            Self::https(&config.authority, token, roots.as_deref())
        } else if config.authority_ca_file.is_none() {
            Self::loopback(&config.authority, token)
        } else {
            Err(WireError::Configuration)
        }
    }
    fn build(url: url::Url, token: Token, roots: Option<&[u8]>) -> Result<Self, WireError> {
        use rustls::pki_types::pem::PemObject;
        use rustls_platform_verifier::BuilderVerifierExt;
        let tls = if url.scheme() == "https" {
            let builder = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::aws_lc_rs::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .map_err(|_| WireError::Configuration)?;
            let builder = if let Some(roots) = roots {
                if roots.is_empty() || roots.len() > 65_536 {
                    return Err(WireError::Configuration);
                }
                let mut store = rustls::RootCertStore::empty();
                for cert in rustls::pki_types::CertificateDer::pem_slice_iter(roots) {
                    store
                        .add(cert.map_err(|_| WireError::Configuration)?)
                        .map_err(|_| WireError::Configuration)?;
                }
                if store.is_empty() {
                    return Err(WireError::Configuration);
                }
                builder.with_root_certificates(store)
            } else {
                builder
                    .with_platform_verifier()
                    .map_err(|_| WireError::Configuration)?
            };
            let mut config = builder.with_no_client_auth();
            config.alpn_protocols = vec![b"http/1.1".to_vec()];
            // Rustls defaults: no key logger, no early data. Explicit roots
            // replace platform trust; HTTP never consults proxy configuration.
            Some(Arc::new(config))
        } else {
            None
        };
        Ok(Self { tls, url, token })
    }
    pub async fn request(&self, request: &Request) -> Result<Reply, WireError> {
        request.validate().map_err(|_| WireError::InvalidRequest)?;
        let bytes = serde_json::to_vec(request).map_err(|_| WireError::InvalidRequest)?;
        let kind = WireRequest::from(request);
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| WireError::Execution { request: kind })?;
        let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
        let progress = Arc::new(Progress::default());
        let observed = progress.clone();
        let client = self.clone();
        let (completed, reply) = tokio::sync::oneshot::channel();
        // Drop aborts the whole RPC task, socket and driver, but never promises
        // to roll back an already accepted remote operation.
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn_on(
            async move {
                client
                    .request_inner(bytes, kind, observed, deadline, completed)
                    .await
            },
            &runtime,
        );
        finish_request(tasks, reply, deadline, kind, &progress).await
    }
}

// Retain a known parsed reply/error while the same task attempts bounded
// shutdown. The caller's outer timer cannot erase that known result. Nothing
// private is placed in Progress or a diagnostic field.
async fn finish_request(
    mut tasks: tokio::task::JoinSet<Result<(), WireError>>,
    mut reply: tokio::sync::oneshot::Receiver<Result<Reply, WireError>>,
    deadline: tokio::time::Instant,
    kind: WireRequest,
    progress: &Progress,
) -> Result<Reply, WireError> {
    let joined = tokio::time::timeout_at(deadline, tasks.join_next()).await;
    if let Ok(result) = reply.try_recv() {
        return result;
    }
    match joined {
        Ok(Some(Ok(Err(error)))) => Err(error),
        Ok(_) => Err(WireError::Execution { request: kind }),
        Err(_) => Err(WireError::deadline(
            if progress.response_headers.load(Ordering::Acquire) {
                WirePhase::ResponseBody
            } else {
                WirePhase::Request
            },
            kind,
            progress,
        )),
    }
}
