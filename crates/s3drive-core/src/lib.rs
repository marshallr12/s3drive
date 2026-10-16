//! Configuration and a bucket-alias namespace. No filesystem or network I/O.

use serde::Deserialize;
use std::collections::HashSet;
use url::Url;

/// Phase 0 deliberately accepts only the first planned mount mode.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    ReadOnly,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub mode: Mode,
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub alias: String,
    pub bucket: String,
    pub region: String,
    #[serde(default)]
    pub prefix: String,
    pub endpoint: Option<String>,
    #[serde(default)]
    pub path_style: bool,
    /// Opt in to plaintext only for numeric loopback endpoints in local tests.
    #[serde(default)]
    pub allow_http_loopback: bool,
    /// An AWS shared-config profile name, never a credential value.
    pub profile: Option<String>,
    /// How this provider encodes spaces in URL-encoded LIST response keys.
    #[serde(default)]
    pub list_url_encoding: ListUrlEncoding,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ListUrlEncoding {
    /// Percent decoding only: an unescaped plus remains a literal plus.
    #[default]
    Percent,
    /// Form decoding: an unescaped plus is a space; %2B is a literal plus.
    Form,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ObjectLocation<'a> {
    pub alias: &'a str,
    pub bucket: &'a str,
    pub key: String,
}

impl Config {
    pub fn parse(input: &str) -> Result<Self, String> {
        // TOML's Display includes source excerpts, which could contain secrets.
        let config: Self = toml::from_str(input).map_err(|e| {
            format!("invalid configuration syntax or fields at byte range {:?}; see examples/s3drive.toml", e.span())
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("only configuration version 1 is supported".into());
        }
        if self.sources.is_empty() {
            return Err("at least one source is required".into());
        }
        let mut aliases = HashSet::new();
        for (index, source) in self.sources.iter().enumerate() {
            source
                .validate()
                .map_err(|e| format!("source {index}: {e}"))?;
            if !aliases.insert(source.alias.to_ascii_lowercase()) {
                return Err("source aliases must be unique ignoring ASCII case".into());
            }
        }
        Ok(())
    }

    /// Resolve a slash-separated file path such as `photos/2026/image.jpg`.
    /// This is a planning utility, not OS filename projection or URL decoding.
    pub fn resolve(&self, path: &str) -> Result<ObjectLocation<'_>, String> {
        self.validate()?;
        validate_relative(path)?;
        let (alias, suffix) = path.split_once('/').ok_or("expected alias/file-path")?;
        let source = self
            .sources
            .iter()
            .find(|s| s.alias == alias)
            .ok_or("unknown source alias (resolution is case-sensitive)")?;
        let key = format!("{}{suffix}", source.prefix);
        if key.len() > 1024 {
            return Err("resolved object key exceeds 1024 UTF-8 bytes".into());
        }
        Ok(ObjectLocation {
            alias: &source.alias,
            bucket: &source.bucket,
            key,
        })
    }
}

impl Source {
    pub fn validate(&self) -> Result<(), String> {
        if self.alias.is_empty()
            || self.alias.len() > 63
            || !self
                .alias
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(
                "alias must contain 1..63 ASCII letters, digits, hyphens or underscores".into(),
            );
        }
        let upper = self.alias.to_ascii_uppercase();
        if ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&upper.as_str())
            || (upper.len() == 4
                && (upper.starts_with("COM") || upper.starts_with("LPT"))
                && matches!(upper.as_bytes()[3], b'1'..=b'9'))
        {
            return Err("alias is a reserved Windows device name".into());
        }
        if self.bucket.is_empty()
            || self.bucket.len() > 255
            || !self
                .bucket
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
        {
            return Err("bucket must be a simple bucket name, not a URL, path or ARN".into());
        }
        if self.region.is_empty()
            || !self
                .region
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("region must contain ASCII letters, digits or hyphens".into());
        }
        if !self.prefix.is_empty() {
            let base = self
                .prefix
                .strip_suffix('/')
                .ok_or("nonempty prefix must end in /")?;
            validate_relative(base)?;
            if self.prefix.len() >= 1024 {
                return Err("prefix must leave space for a file key within 1024 bytes".into());
            }
        }
        if let Some(endpoint) = &self.endpoint {
            let url = Url::parse(endpoint).map_err(|_| "endpoint must be an absolute HTTPS URL")?;
            let loopback = match url.host() {
                Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                _ => false,
            };
            let permitted_http =
                url.scheme() == "http" && self.allow_http_loopback && loopback && self.path_style;
            if (url.scheme() != "https" && !permitted_http)
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || url.path() != "/"
            {
                return Err("endpoint must be HTTPS (or opted-in numeric loopback HTTP with path_style), with no credentials, path, query or fragment".into());
            }
        }
        if self
            .profile
            .as_ref()
            .is_some_and(|p| p.trim().is_empty() || p.chars().any(char::is_control))
        {
            return Err("profile must be nonempty and contain no control characters".into());
        }
        Ok(())
    }
}

fn validate_relative(path: &str) -> Result<(), String> {
    if path.contains('\\')
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(
            "expected a relative path without empty, dot, parent, backslash or control components"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const EXAMPLE: &str = include_str!("../../../examples/s3drive.toml");

    #[test]
    fn routes_sources_and_preserves_literal_key_bytes() {
        let config = Config::parse(EXAMPLE).unwrap();
        let docs = config
            .resolve("documents/reports/a%2Fb + café.txt")
            .unwrap();
        assert_eq!(docs.bucket, "replace-me-documents");
        assert_eq!(docs.key, "desktop/reports/a%2Fb + café.txt");
        assert_eq!(
            config.resolve("photos/2026/a.jpg").unwrap().key,
            "2026/a.jpg"
        );
    }

    #[test]
    fn rejects_namespace_escape_and_ambiguous_paths() {
        let config = Config::parse(EXAMPLE).unwrap();
        for path in [
            "/documents/a",
            "documents/../a",
            "documents/./a",
            "documents//a",
            "documents/a/",
            "documents/a\\b",
            "documents/\0",
            "missing/a",
            "documents",
            "",
        ] {
            assert!(config.resolve(path).is_err(), "accepted {path:?}");
        }
    }

    #[test]
    fn checks_key_length_in_bytes_including_prefix() {
        let config = Config::parse(EXAMPLE).unwrap();
        assert!(
            config
                .resolve(&format!("documents/{}", "a".repeat(1016)))
                .is_ok()
        );
        assert!(
            config
                .resolve(&format!("documents/{}", "a".repeat(1017)))
                .is_err()
        );
        assert!(
            config
                .resolve(&format!("photos/{}", "é".repeat(513)))
                .is_err()
        );
    }

    #[test]
    fn rejects_case_collisions_and_reserved_aliases() {
        let mut config = Config::parse(EXAMPLE).unwrap();
        config.sources[1].alias = "DOCUMENTS".into();
        assert!(config.validate().is_err());
        config.sources[1].alias = "COM1".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_unsafe_endpoints_and_prefixes_without_echoing_them() {
        let mut config = Config::parse(EXAMPLE).unwrap();
        for endpoint in [
            "http://host",
            "https://user:secret@host",
            "https://host/?token=secret",
            "https://host/path",
            "https://host/#secret",
        ] {
            config.sources[0].endpoint = Some(endpoint.into());
            let error = config.validate().unwrap_err();
            assert!(!error.contains("secret"));
        }
        config.sources[0].endpoint = None;
        for prefix in ["/", "unsafe", "../", "a//", "a/../"] {
            config.sources[0].prefix = prefix.into();
            assert!(config.validate().is_err());
        }
    }

    #[test]
    fn plaintext_is_opt_in_and_restricted_to_numeric_loopback() {
        let mut config = Config::parse(EXAMPLE).unwrap();
        config.sources[0].endpoint = Some("http://127.0.0.1:9000".into());
        assert!(config.validate().is_err());
        config.sources[0].allow_http_loopback = true;
        assert!(config.validate().is_err());
        config.sources[0].path_style = true;
        assert!(config.validate().is_ok());
        for endpoint in ["http://[::1]:9000", "http://127.0.0.2:9000"] {
            config.sources[0].endpoint = Some(endpoint.into());
            assert!(config.validate().is_ok());
        }
        for endpoint in [
            "http://localhost:9000",
            "http://10.0.0.1:9000",
            "http://example.com:9000",
            "http://127.0.0.1:9000/secret",
        ] {
            config.sources[0].endpoint = Some(endpoint.into());
            assert!(config.validate().is_err());
        }
    }

    #[test]
    fn rejects_unimplemented_modes_unknown_fields_and_empty_sources() {
        assert!(Config::parse(&EXAMPLE.replace("read-only", "write-back")).is_err());
        let error =
            Config::parse(&format!("secret_key = 'never-print-me'\n{EXAMPLE}")).unwrap_err();
        assert!(!error.contains("never-print-me"));
        assert!(Config::parse("version = 1\nmode = 'read-only'\nsources = []").is_err());
        assert!(Config::parse(&EXAMPLE.replace("version = 1", "version = 2")).is_err());
    }
}
