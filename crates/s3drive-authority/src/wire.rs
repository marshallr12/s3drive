//! Bounded loopback client for the prototype authority. No redirects, proxy or
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
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| WireError)?;
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
