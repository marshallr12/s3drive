//! Managed namespace authority prototype. Loopback-only transport; no S3 writes,
//! remote deployment or Turso backend is enabled. Managed read-only mounts use
//! this development transport, including opt-in managed native editing.
mod wire;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State as HttpState},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use s3drive_state::{State, namespace};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc, thread};
use tokio::sync::{Semaphore, mpsc, oneshot};
pub use wire::{WireClient, WireError};

pub use namespace::{
    Apply, Artifact, ContentGuard, Error, Kind, Listing, Mutation, Node, Payload, ROOT, Receipt,
    Reservation, Store, Volume, payload_reference,
};
const BODY_LIMIT: usize = 4096;
const QUEUE: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Request {
    Features {},
    Info {},
    Store { volume: String, store: String },
    Reservation { volume: String, reservation: String },
    Node { volume: String, node: String },
    List { volume: String, parent: String },
    Operation { volume: String, operation: String },
    Apply { request: Apply },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "result",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum Reply {
    Features(Vec<String>),
    Info(Volume),
    Store(Store),
    Reservation(Reservation),
    Node(Node),
    List(Listing),
    Operation(Option<std::result::Result<Receipt, Error>>),
    Applied(Receipt),
    Error(Error),
}
impl Request {
    fn validate(&self) -> namespace::Result<()> {
        let data = serde_json::to_vec(self).map_err(|_| Error::Invalid)?;
        if data.len() > BODY_LIMIT {
            return Err(Error::Invalid);
        }
        if let Self::Apply { request } = self {
            request.validate()?;
        }
        Ok(())
    }
    fn execute(self, state: &mut State) -> Reply {
        let reply = match self {
            Self::Features {} => Ok(Reply::Features(vec![
                "content-cas-v1".into(),
                "namespace-create-v1".into(),
            ])),
            Self::Info {} => state.namespace_volume().map(Reply::Info),
            Self::Store { volume, store } => {
                state.namespace_store(&volume, &store).map(Reply::Store)
            }
            Self::Reservation {
                volume,
                reservation,
            } => state
                .namespace_reservation(&volume, &reservation)
                .map(Reply::Reservation),
            Self::Node { volume, node } => state.namespace_node(&volume, &node).map(Reply::Node),
            Self::List { volume, parent } => {
                state.namespace_list(&volume, &parent).map(Reply::List)
            }
            Self::Operation { volume, operation } => state
                .namespace_operation(&volume, &operation)
                .map(Reply::Operation),
            Self::Apply { request } => state.namespace_apply(&request).map(Reply::Applied),
        };
        reply.unwrap_or_else(Reply::Error)
    }
}
enum Command {
    Request(Request, oneshot::Sender<Reply>),
    Stop,
}
/// Shared, bounded admission. A lost reply after acceptance means UNKNOWN;
/// look up the operation ID or resubmit the exact original request.
#[derive(Clone)]
pub struct Client {
    sender: mpsc::Sender<Command>,
}
impl Client {
    pub async fn request(&self, request: Request) -> Reply {
        if let Err(e) = request.validate() {
            return Reply::Error(e);
        }
        let (send, receive) = oneshot::channel();
        if self
            .sender
            .try_send(Command::Request(request, send))
            .is_err()
        {
            return Reply::Error(Error::Unavailable);
        }
        receive.await.unwrap_or(Reply::Error(Error::Unavailable))
    }
}
/// Dedicated owner thread; SQLite never runs on a Tokio executor thread.
/// Separate its private directory from clients' cache and staged bytes.
pub struct Authority {
    client: Client,
    thread: thread::JoinHandle<()>,
}
impl Authority {
    pub async fn open(directory: PathBuf) -> namespace::Result<Self> {
        Self::open_with_policy(directory, s3drive_core::names::EXACT.into()).await
    }
    pub async fn open_with_policy(directory: PathBuf, policy: String) -> namespace::Result<Self> {
        let (sender, mut receiver) = mpsc::channel(QUEUE);
        let (ready, started) = oneshot::channel();
        let thread = thread::Builder::new()
            .name("s3drive-authority".into())
            .spawn(move || {
                let state = State::open_managed(&directory)
                    .map_err(|_| Error::Internal)
                    .and_then(|mut state| {
                        state.namespace_initialize_with_policy(&policy)?;
                        Ok(state)
                    });
                let mut state = match state {
                    Ok(state) => {
                        let _ = ready.send(Ok(()));
                        state
                    }
                    Err(e) => {
                        let _ = ready.send(Err(e));
                        return;
                    }
                };
                while let Some(command) = receiver.blocking_recv() {
                    match command {
                        Command::Request(request, send) => {
                            let _ = send.send(request.execute(&mut state));
                        }
                        Command::Stop => receiver.close(),
                    }
                }
            })
            .map_err(|_| Error::Unavailable)?;
        match started.await.unwrap_or(Err(Error::Unavailable)) {
            Ok(()) => Ok(Self {
                client: Client { sender },
                thread,
            }),
            Err(e) => {
                let _ = tokio::task::spawn_blocking(move || thread.join()).await;
                Err(e)
            }
        }
    }
    pub fn client(&self) -> Client {
        self.client.clone()
    }
    /// Stop new admission, drain accepted commands and release DB ownership.
    pub async fn shutdown(self) -> namespace::Result<()> {
        let _ = self.client.sender.send(Command::Stop).await;
        tokio::task::spawn_blocking(move || self.thread.join())
            .await
            .map_err(|_| Error::Unavailable)?
            .map_err(|_| Error::Unavailable)
    }
}
/// 256-bit caller-supplied bearer secret. Never log or serialize this type.
/// No service account or persisted credential is created by this library.
#[derive(Clone)]
pub struct Token(String);
impl Token {
    pub fn from_private_file(path: &std::path::Path) -> namespace::Result<Self> {
        let bytes = s3drive_state::read_private_secret(path, 66).map_err(|_| Error::Invalid)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| Error::Invalid)?;
        Self::new(text.trim_end_matches(['\r', '\n']).into())
    }
    pub fn new(value: String) -> namespace::Result<Self> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Invalid);
        }
        Ok(Self(value))
    }
    fn accepts(&self, headers: &HeaderMap) -> bool {
        let Some(value) = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
        else {
            return false;
        };
        // Compare all bytes of a correctly sized token. This prototype is not a
        // hardened public network authentication implementation.
        value.len() == self.0.len()
            && value
                .bytes()
                .zip(self.0.bytes())
                .fold(0u8, |a, (b, c)| a | (b ^ c))
                == 0
    }
}
#[derive(Clone)]
struct HttpContext {
    client: Client,
    token: Token,
    slots: Arc<Semaphore>,
}
async fn handle(
    HttpState(context): HttpState<HttpContext>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Response {
    if !context.token.accepts(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.split(';').next() == Some("application/json"))
    {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    let Ok(_permit) = context.slots.try_acquire() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let request = match serde_json::from_slice(&bytes) {
        Ok(request) => request,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(Reply::Error(Error::Invalid))).into_response();
        }
    };
    let reply = context.client.request(request).await;
    // Domain rejections use the versioned JSON envelope. Transport failures may
    // be non-200 and cannot by themselves establish a mutation's outcome.
    Json(reply).into_response()
}
/// Local contract-test/development server. Reject nonloopback listeners.
/// Remote deployment needs TLS, identity/volume ACLs, deadlines and operations
/// acceptance. Do not put this prototype on a public reverse proxy.
pub async fn serve_loopback<F>(
    listener: tokio::net::TcpListener,
    client: Client,
    token: Token,
    shutdown: F,
) -> std::io::Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if !listener.local_addr()?.ip().is_loopback() {
        return Err(std::io::Error::other(
            "authority prototype requires loopback",
        ));
    }
    let context = HttpContext {
        client,
        token,
        slots: Arc::new(Semaphore::new(QUEUE)),
    };
    let app = Router::new()
        .route("/v1/namespace", post(handle))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .with_state(context);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
}
