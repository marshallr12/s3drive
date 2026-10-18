//! Bounded authenticated authority client. No redirects, proxy or
//! automatic retry. Transport failures leave mutation outcomes unknown.
use crate::{Reply, Request, Token};
use hyper::body::{Body as HttpBody, Bytes, Frame, SizeHint};
use std::{
    fmt, io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
mod connector;

/// Low-cardinality diagnostics only. Never retains a URL, token, request,
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
        error: reqwest::Error,
        phase: WirePhase,
        request: WireRequest,
        progress: &Progress,
    ) -> Self {
        let phase = if error.is_connect() {
            WirePhase::Connect
        } else {
            phase
        };
        let mut io_kind = None;
        let mut incomplete_message = false;
        let mut connection_closed = false;
        let mut source = std::error::Error::source(&error);
        // Inspect types and stable predicates, never dependency error strings.
        // Bound traversal even if a future dependency provides a cyclic source.
        for _ in 0..16 {
            let Some(cause) = source else { break };
            if let Some(error) = cause.downcast_ref::<io::Error>() {
                io_kind = Some(error.kind());
            }
            if let Some(error) = cause.downcast_ref::<hyper::Error>() {
                incomplete_message |= error.is_incomplete_message();
                connection_closed |= error.is_closed();
            }
            source = cause.source();
        }
        let (connector_started, connector_completed) = progress.connector_observation();
        Self::Transport {
            request,
            phase,
            timed_out: error.is_timeout(),
            io_kind,
            incomplete_message,
            connection_closed,
            body_frame_polled: progress.body_frame_polled.load(Ordering::Acquire),
            connector_started,
            connector_completed,
        }
    }
}

// Preserve reqwest's original serialized body, framing hints and errors. Only
// retain a boolean when its HTTP consumer actually polls a data frame.
struct ObservedBody {
    inner: reqwest::Body,
    progress: Arc<Progress>,
}
#[derive(Default)]
struct Progress {
    body_frame_polled: AtomicBool,
    response_headers: AtomicBool,
    // Monotonic aggregate: 0=unobserved, 1=called, 2=at least one success.
    connector: AtomicU8,
}
impl Progress {
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
    http: reqwest::Client,
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
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .http1_only()
            .tls_sslkeylogfile(false)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connector_layer(connector::Observe)
            .connect_timeout(Duration::from_secs(2))
            .timeout(REQUEST_TIMEOUT);
        if let Some(roots) = roots {
            if roots.is_empty() || roots.len() > 65_536 {
                return Err(WireError::Configuration);
            }
            let certs = reqwest::Certificate::from_pem_bundle(roots)
                .map_err(|_| WireError::Configuration)?;
            if certs.is_empty() {
                return Err(WireError::Configuration);
            }
            builder = builder.tls_certs_only(certs);
        }
        if url.scheme() == "https" {
            builder = builder.https_only(true);
        }
        let http = builder.build().map_err(|_| WireError::Configuration)?;
        Ok(Self { http, url, token })
    }
    pub async fn request(&self, request: &Request) -> Result<Reply, WireError> {
        request.validate().map_err(|_| WireError::InvalidRequest)?;
        let kind = WireRequest::from(request);
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| WireError::Execution { request: kind })?;
        // Native callback threads wait on this future. Drive HTTP itself in an
        // owned Tokio task, with one deadline covering both queuing and I/O.
        let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
        let progress = Arc::new(Progress::default());
        let observed = progress.clone();
        let client = self.clone();
        let request = request.clone();
        // JoinSet aborts owned work on drop, including caller cancellation.
        // A canceled/lost reply never makes a mutation safe to replay.
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn_on(
            async move {
                connector::CURRENT
                    .scope(observed.clone(), client.request_inner(&request, observed))
                    .await
            },
            &runtime,
        );
        match tokio::time::timeout_at(deadline, tasks.join_next()).await {
            Ok(Some(Ok(result))) => result,
            Ok(Some(Err(_))) | Ok(None) => Err(WireError::Execution { request: kind }),
            Err(_) => {
                let (connector_started, connector_completed) = progress.connector_observation();
                Err(WireError::Transport {
                    request: kind,
                    phase: if progress.response_headers.load(Ordering::Acquire) {
                        WirePhase::ResponseBody
                    } else {
                        WirePhase::Request
                    },
                    timed_out: true,
                    io_kind: None,
                    incomplete_message: false,
                    connection_closed: false,
                    body_frame_polled: progress.body_frame_polled.load(Ordering::Acquire),
                    connector_started,
                    connector_completed,
                })
            }
        }
    }
    async fn request_inner(
        &self,
        request: &Request,
        progress: Arc<Progress>,
    ) -> Result<Reply, WireError> {
        let kind = WireRequest::from(request);
        let mut built = self
            .http
            .post(self.url.clone())
            // One RPC per transport matches the TLS listener's finite lifetime.
            // This also makes loopback HTTP independent of peer keep-alive.
            .header(reqwest::header::CONNECTION, "close")
            .bearer_auth(&self.token.0)
            .json(request)
            .build()
            .map_err(|e| WireError::transport(e, WirePhase::Request, kind, &progress))?;
        let inner = built.body_mut().take().ok_or(WireError::InvalidRequest)?;
        *built.body_mut() = Some(reqwest::Body::wrap(ObservedBody {
            inner,
            progress: progress.clone(),
        }));
        let mut response = self
            .http
            .execute(built)
            .await
            .map_err(|e| WireError::transport(e, WirePhase::Request, kind, &progress))?;
        progress.response_headers.store(true, Ordering::Release);
        if !response.status().is_success() {
            return Err(WireError::HttpStatus(response.status().as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|n| n > 8 * 1024 * 1024)
        {
            return Err(WireError::ResponseTooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| WireError::transport(e, WirePhase::ResponseBody, kind, &progress))?
        {
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err(WireError::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| WireError::InvalidReply)
    }
}
