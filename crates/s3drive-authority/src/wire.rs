//! Bounded authenticated authority client. No redirects, proxy or
//! automatic retry. Transport failures leave mutation outcomes unknown.
use crate::{Reply, Request, Token};
use hyper::body::{Body as HttpBody, Bytes, Frame, SizeHint};
use std::{
    fmt, io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

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
    },
    HttpStatus(u16),
    ResponseTooLarge,
    InvalidReply,
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
        body_frame_polled: bool,
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
        Self::Transport {
            request,
            phase,
            timed_out: error.is_timeout(),
            io_kind,
            incomplete_message,
            connection_closed,
            body_frame_polled,
        }
    }
}

// Preserve reqwest's original serialized body, framing hints and errors. Only
// retain a boolean when its HTTP consumer actually polls a data frame.
struct ObservedBody {
    inner: reqwest::Body,
    polled: Arc<AtomicBool>,
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
            self.polled.store(true, Ordering::Release);
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
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(10));
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
        let polled = Arc::new(AtomicBool::new(false));
        let mut built = self
            .http
            .post(self.url.clone())
            // One RPC per transport matches the TLS listener's finite lifetime.
            // This also makes loopback HTTP independent of peer keep-alive.
            .header(reqwest::header::CONNECTION, "close")
            .bearer_auth(&self.token.0)
            .json(request)
            .build()
            .map_err(|e| WireError::transport(e, WirePhase::Request, kind, false))?;
        let inner = built.body_mut().take().ok_or(WireError::InvalidRequest)?;
        *built.body_mut() = Some(reqwest::Body::wrap(ObservedBody {
            inner,
            polled: polled.clone(),
        }));
        let mut response = self.http.execute(built).await.map_err(|e| {
            WireError::transport(e, WirePhase::Request, kind, polled.load(Ordering::Acquire))
        })?;
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
        while let Some(chunk) = response.chunk().await.map_err(|e| {
            WireError::transport(
                e,
                WirePhase::ResponseBody,
                kind,
                polled.load(Ordering::Acquire),
            )
        })? {
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err(WireError::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| WireError::InvalidReply)
    }
}
