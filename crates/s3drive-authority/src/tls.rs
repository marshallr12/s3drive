//! TLS terminates in the Rust service. Plaintext is never accepted on this
//! listener; pre-authentication sockets and work have explicit bounds.
use crate::{Client, Token};
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use std::{
    fmt,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{Semaphore, watch},
    task::JoinSet,
};
use tokio_rustls::TlsAcceptor;
mod progress;
use progress::{ObservedIo, Progress};

// Unauthenticated peers can trigger these paths. Emit at most sixteen fixed
// categories per listener lifetime, then one suppression notice. Never format
// dependency errors, peer addresses, headers, bodies or certificate material.
#[derive(Debug)]
enum ConnectionFailure {
    Capacity,
    HandshakeRejected,
    HandshakeTimeout,
    HttpTimeout,
    IncompleteHttp,
    HttpTransport,
    Lifetime,
}
#[derive(Default)]
struct Diagnostics(AtomicU8);
impl Diagnostics {
    fn record(&self, failure: ConnectionFailure) {
        self.record_progress(failure, None);
    }
    fn record_progress(&self, failure: ConnectionFailure, progress: Option<&Progress>) {
        let previous = self
            .0
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                (n < 17).then_some(n + 1)
            });
        use std::io::Write;
        match previous {
            Ok(16) => {
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "authority TLS: further connection diagnostics suppressed"
                );
            }
            Ok(_) => {
                let mut stderr = std::io::stderr().lock();
                if let Some(p) = progress {
                    let _ = writeln!(
                        stderr,
                        "authority TLS connection failure: {failure:?} connection={} http_ms={} read_bytes={} written_bytes={} flushes={} dispatched={}",
                        p.connection,
                        p.started.elapsed().as_millis(),
                        p.read.load(Ordering::Relaxed),
                        p.written.load(Ordering::Relaxed),
                        p.flushed.load(Ordering::Relaxed),
                        p.dispatched.load(Ordering::Relaxed)
                    );
                } else {
                    let _ = writeln!(stderr, "authority TLS connection failure: {failure:?}");
                }
            }
            Err(_) => {}
        }
    }
    fn http(&self, result: Result<(), hyper::Error>, progress: &Progress) {
        if let Err(error) = result {
            self.record_progress(
                if error.is_timeout() {
                    ConnectionFailure::HttpTimeout
                } else if error.is_incomplete_message() {
                    ConnectionFailure::IncompleteHttp
                } else {
                    ConnectionFailure::HttpTransport
                },
                Some(progress),
            );
        }
    }
}

#[derive(Debug)]
pub struct TlsError;
impl fmt::Display for TlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("authority TLS initialization failed; require matching owner-private PEM certificate/key files")
    }
}
impl std::error::Error for TlsError {}

#[derive(Clone)]
pub struct ServerTls(Arc<ServerConfig>);
impl ServerTls {
    pub fn from_private_files(cert: &Path, key: &Path) -> Result<Self, TlsError> {
        let cert = s3drive_state::read_private_secret(cert, 65_536).map_err(|_| TlsError)?;
        let key = zeroize::Zeroizing::new(
            s3drive_state::read_private_secret(key, 65_536).map_err(|_| TlsError)?,
        );
        Self::from_pem(&cert, &key)
    }
    pub fn from_pem(cert: &[u8], key: &[u8]) -> Result<Self, TlsError> {
        if cert.is_empty() || key.is_empty() || cert.len() > 65_536 || key.len() > 65_536 {
            return Err(TlsError);
        }
        let certs = CertificateDer::pem_slice_iter(cert)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| TlsError)?;
        if certs.is_empty() || certs.len() > 16 {
            return Err(TlsError);
        }
        let key = PrivateKeyDer::from_pem_slice(key).map_err(|_| TlsError)?;
        let mut config = ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .map_err(|_| TlsError)?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|_| TlsError)?;
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        Ok(Self(Arc::new(config)))
    }
}

/// One bearer token authorizes one SQLite volume and its namespace operations.
/// The caller owns the listener; no binding, firewall or service installation
/// occurs here. Shutdown drains requests before the caller closes the DB owner.
pub async fn serve_tls<F>(
    listener: TcpListener,
    client: Client,
    token: Token,
    tls: ServerTls,
    shutdown: F,
) -> std::io::Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let app = crate::router(client, token);
    let acceptor = TlsAcceptor::from(tls.0);
    let slots = Arc::new(Semaphore::new(32));
    let diagnostics = Arc::new(Diagnostics::default());
    let (stopping, stopped) = watch::channel(false);
    let mut tasks = JoinSet::new();
    tokio::pin!(shutdown);
    let mut failure = None;
    let mut connection_number = 0_u64;
    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => break,
            result = tasks.join_next(), if !tasks.is_empty() => {
                if result.is_some_and(|r| r.is_err()) {
                    failure = Some(std::io::Error::other("authority TLS connection task failed"));
                    break;
                }
            }
            accepted = listener.accept() => {
                let (stream, _) = match accepted {
                    Ok(value) => value,
                    Err(error) => { failure = Some(error); break; }
                };
                let Ok(slot) = slots.clone().try_acquire_owned() else {
                    diagnostics.record(ConnectionFailure::Capacity);
                    drop(stream);
                    continue;
                };
                let acceptor = acceptor.clone();
                connection_number = connection_number.saturating_add(1);
                let number = connection_number;
                let diagnostics = diagnostics.clone();
                let app = app.clone();
                let mut stopped = stopped.clone();
                tasks.spawn(async move {
                    let _slot = slot;
                    let stream = tokio::select! {
                        _ = stopped.changed() => return,
                        result = tokio::time::timeout(Duration::from_secs(5), acceptor.accept(stream)) => {
                            match result {
                                Ok(Ok(stream)) => stream,
                                Ok(Err(_)) => { diagnostics.record(ConnectionFailure::HandshakeRejected); return; },
                                Err(_) => { diagnostics.record(ConnectionFailure::HandshakeTimeout); return; },
                            }
                        }
                    };
                    let mut http = hyper::server::conn::http1::Builder::new();
                    http.timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(5))
                        .max_buf_size(16_384).keep_alive(false);
                    let (stream, progress) = ObservedIo::new(stream, number);
                    let observed = progress.clone();
                    let service = TowerToHyperService::new(app);
                    let service = hyper::service::service_fn(move |request| {
                        // HTTP headers were parsed; this does not mean that
                        // authentication, a full body or a DB commit succeeded.
                        observed.dispatched.store(true, Ordering::Relaxed);
                        hyper::service::Service::call(&service, request)
                    });
                    let connection = http.serve_connection(TokioIo::new(stream), service);
                    tokio::pin!(connection);
                    let deadline = tokio::time::sleep(Duration::from_secs(10));
                    tokio::pin!(deadline);
                    tokio::select! {
                        result = &mut connection => diagnostics.http(result, &progress),
                        _ = &mut deadline => diagnostics.record_progress(ConnectionFailure::Lifetime, Some(&progress)),
                        _ = stopped.changed() => {
                            connection.as_mut().graceful_shutdown();
                            tokio::select! {
                                result = &mut connection => diagnostics.http(result, &progress),
                                _ = &mut deadline => diagnostics.record_progress(ConnectionFailure::Lifetime, Some(&progress)),
                            }
                        }
                    }
                });
            }
        }
    }
    drop(listener);
    let _ = stopping.send(true);
    // Each connection already has a fixed lifetime. Accepted SQLite commands
    // remain owned by the DB worker even if a client/connection disappears.
    while let Some(result) = tasks.join_next().await {
        if result.is_err() {
            failure = Some(std::io::Error::other(
                "authority TLS connection task failed",
            ));
        }
    }
    failure.map_or(Ok(()), Err)
}
