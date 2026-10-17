//! Bounded authenticated authority client. No redirects, proxy or
//! automatic retry. Transport failures leave mutation outcomes unknown.
use crate::{Reply, Request, Token};
use std::{fmt, time::Duration};
#[derive(Debug)]
pub struct WireError;
impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("authority transport failed; inspect the durable operation before proceeding")
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
        let url = url::Url::parse(endpoint).map_err(|_| WireError)?;
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
            return Err(WireError);
        }
        Self::build(url, token, None)
    }
    /// HTTPS keeps hostname/certificate validation enabled. Supplying roots
    /// replaces platform trust instead of silently adding another authority.
    pub fn https(endpoint: &str, token: Token, roots: Option<&[u8]>) -> Result<Self, WireError> {
        let url = url::Url::parse(endpoint).map_err(|_| WireError)?;
        if url.scheme() != "https"
            || url.host().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/v1/namespace"
        {
            return Err(WireError);
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
            .map_err(|_| WireError)?
            .scheme()
            == "https"
        {
            let roots = config
                .authority_ca_file
                .as_ref()
                .map(|p| s3drive_state::read_private_secret(p, 65_536).map_err(|_| WireError))
                .transpose()?;
            Self::https(&config.authority, token, roots.as_deref())
        } else if config.authority_ca_file.is_none() {
            Self::loopback(&config.authority, token)
        } else {
            Err(WireError)
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
                return Err(WireError);
            }
            let certs = reqwest::Certificate::from_pem_bundle(roots).map_err(|_| WireError)?;
            if certs.is_empty() {
                return Err(WireError);
            }
            builder = builder.tls_certs_only(certs);
        }
        if url.scheme() == "https" {
            builder = builder.https_only(true);
        }
        let http = builder.build().map_err(|_| WireError)?;
        Ok(Self { http, url, token })
    }
    pub async fn request(&self, request: &Request) -> Result<Reply, WireError> {
        request.validate().map_err(|_| WireError)?;
        let mut response = self
            .http
            .post(self.url.clone())
            .bearer_auth(&self.token.0)
            .json(request)
            .send()
            .await
            .map_err(|_| WireError)?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|n| n > 8 * 1024 * 1024)
        {
            return Err(WireError);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| WireError)? {
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err(WireError);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| WireError)
    }
}
