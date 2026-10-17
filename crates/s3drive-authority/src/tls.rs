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
use std::{fmt, path::Path, sync::Arc, time::Duration};
use tokio::{
    net::TcpListener,
    sync::{Semaphore, watch},
    task::JoinSet,
};
use tokio_rustls::TlsAcceptor;

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
    let (stopping, stopped) = watch::channel(false);
    let mut tasks = JoinSet::new();
    tokio::pin!(shutdown);
    let mut failure = None;
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
                let Ok(slot) = slots.clone().try_acquire_owned() else { drop(stream); continue; };
                let acceptor = acceptor.clone();
                let app = app.clone();
                let mut stopped = stopped.clone();
                tasks.spawn(async move {
                    let _slot = slot;
                    let stream = tokio::select! {
                        _ = stopped.changed() => return,
                        result = tokio::time::timeout(Duration::from_secs(5), acceptor.accept(stream)) => {
                            let Ok(Ok(stream)) = result else { return; };
                            stream
                        }
                    };
                    let mut http = hyper::server::conn::http1::Builder::new();
                    http.timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(5))
                        .max_buf_size(16_384).keep_alive(false);
                    let connection = http.serve_connection(TokioIo::new(stream), TowerToHyperService::new(app));
                    tokio::pin!(connection);
                    let deadline = tokio::time::sleep(Duration::from_secs(10));
                    tokio::pin!(deadline);
                    tokio::select! {
                        _ = &mut connection => {},
                        _ = &mut deadline => {},
                        _ = stopped.changed() => {
                            connection.as_mut().graceful_shutdown();
                            tokio::select! { _ = &mut connection => {}, _ = &mut deadline => {} }
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
