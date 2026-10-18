//! Configuration and a bucket-alias namespace. No filesystem or network I/O.

pub mod manifest;
pub mod multipart;
pub mod names;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Component, PathBuf},
};
use url::Url;

/// Phase 0 deliberately accepts only the first planned mount mode.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub mode: Mode,
    pub sources: Vec<Source>,
    pub cache: Option<CacheConfig>,
    pub writes: Option<WritesConfig>,
    pub managed: Option<ManagedConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedConfig {
    /// Optional owner-private PEM roots for HTTPS; replaces platform trust.
    pub authority_ca_file: Option<PathBuf>,
    #[serde(rename = "windows-disposition")]
    pub windows_disposition: Option<WindowsDisposition>,
    pub default_store: Option<String>,
    pub authority: String,
    pub volume: String,
    pub token_file: PathBuf,
    pub stores: Vec<ManagedStore>,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WindowsDisposition {
    PosixV1,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedStore {
    pub id: String,
    pub source: String,
}
impl ManagedConfig {
    fn validate(&self, sources: &[Source]) -> Result<(), String> {
        let id = |v: &str| {
            v.len() == 32
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && v != "00000000000000000000000000000000"
        };
        let url = Url::parse(&self.authority).map_err(|_| "invalid managed authority URL")?;
        let ip = url
            .host_str()
            .and_then(|s| s.trim_matches(['[', ']']).parse::<std::net::IpAddr>().ok());
        if !(url.scheme() == "https" && url.host().is_some()
            || url.scheme() == "http" && ip.is_some_and(|a| a.is_loopback()))
            || url.path() != "/v1/namespace"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !id(&self.volume)
            || !absolute_local(&self.token_file)
            || self
                .authority_ca_file
                .as_ref()
                .is_some_and(|p| url.scheme() != "https" || !absolute_local(p))
            || self.stores.is_empty()
            || self.stores.len() > 16
        {
            return Err("managed mode requires HTTPS or numeric loopback HTTP, a volume ID, absolute private token/optional HTTPS CA files and 1..16 store mappings".into());
        }
        let mut ids = HashSet::new();
        for s in &self.stores {
            if !id(&s.id)
                || !ids.insert(&s.id)
                || !sources.iter().any(|source| source.alias == s.source)
            {
                return Err(
                    "managed stores require unique IDs and configured source aliases".into(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WritesConfig {
    pub directory: PathBuf,
    pub sources: Vec<WriteSource>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WriteSource {
    pub alias: String,
    pub policy: PathBuf,
    pub plan: PathBuf,
    pub report: PathBuf,
}
fn absolute_local(path: &std::path::Path) -> bool {
    path.is_absolute()
        && !path.components().any(|c| matches!(c, Component::ParentDir))
        && !path.as_os_str().as_encoded_bytes().contains(&0)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    /// Explicit private local directory outside the mounted tree.
    pub directory: PathBuf,
    #[serde(default = "default_cache_bytes")]
    pub max_bytes: u64,
    #[serde(default = "default_cache_file")]
    pub max_file_bytes: usize,
    #[serde(default = "default_cache_entries")]
    pub max_entries: usize,
}
fn default_cache_bytes() -> u64 {
    64 * 1024 * 1024
}
fn default_cache_file() -> usize {
    1024 * 1024
}
fn default_cache_entries() -> usize {
    4096
}
impl CacheConfig {
    fn validate(&self) -> Result<(), String> {
        if !self.directory.is_absolute()
            || self
                .directory
                .components()
                .any(|c| matches!(c, Component::ParentDir))
            || self.directory.as_os_str().as_encoded_bytes().contains(&0)
        {
            return Err(
                "cache directory must be an absolute local path without parent traversal or NUL"
                    .into(),
            );
        }
        if !(1..=1024 * 1024 * 1024).contains(&self.max_bytes)
            || !(1..=1024 * 1024).contains(&self.max_file_bytes)
            || self.max_file_bytes as u64 > self.max_bytes
            || !(1..=65_536).contains(&self.max_entries)
        {
            return Err("cache limits require 1..1073741824 payload bytes, 1..1048576 bytes/file within that budget, and 1..65536 entries".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
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
        if let Some(managed) = &self.managed {
            if !matches!(
                (&self.mode, self.version, &self.writes),
                (Mode::ReadOnly, 3, None) | (Mode::ReadWrite, 4..=7, Some(_))
            ) || self.cache.is_some()
            {
                return Err("managed mode requires version 3/read-only or versions 4/5/6/7 read-write with policies, without a separate cache".into());
            }
            managed.validate(&self.sources)?;
            if (self.version == 7) != managed.windows_disposition.is_some() {
                return Err("managed version 7 requires windows-disposition = 'posix-v1'; older versions must omit it".into());
            }
            if self.version == 3 && managed.default_store.is_some() {
                return Err(
                    "read-only managed settings do not select a default write store".into(),
                );
            }
            if matches!(self.version, 4..=7)
                && !managed.default_store.as_ref().is_some_and(|id| {
                    managed.stores.iter().any(|s| {
                        &s.id == id
                            && self
                                .writes
                                .as_ref()
                                .unwrap()
                                .sources
                                .iter()
                                .any(|w| w.alias == s.source)
                    })
                })
            {
                return Err("managed writes require a mapped policy-bound default_store for unpublished empty files".into());
            }
        }
        match (&self.mode, self.version, &self.writes) {
            (Mode::ReadOnly, 3, None) if self.managed.is_some() => {}
            (Mode::ReadOnly, 1, None) if self.managed.is_none() => {}
            (Mode::ReadWrite, version, Some(writes)) if (version==2 && self.managed.is_none()) || (matches!(version,4..=7) && self.managed.is_some()) => {
                if !absolute_local(&writes.directory)
                    || writes.sources.is_empty()
                    || self
                        .cache
                        .as_ref()
                        .is_some_and(|cache| cache.directory != writes.directory)
                {
                    return Err("read-write configuration requires one absolute private state directory, explicit source policies, and the same directory for optional cache".into());
                }
                let mut selected = HashSet::new();
                for source in &writes.sources {
                    if !selected.insert(&source.alias)
                        || !self.sources.iter().any(|s| s.alias == source.alias)
                        || [&source.policy, &source.plan, &source.report]
                            .iter()
                            .any(|p| !absolute_local(p))
                    {
                        return Err("write sources require unique configured aliases and absolute policy/plan/report paths".into());
                    }
                }
            }
            _ => return Err(
                "version 1 is read-only; version 2 requires writes; versions 3/4/5/6/7 require managed read-only/read-write settings"
                    .into(),
            ),
        }
        if self.sources.is_empty() {
            return Err("at least one source is required".into());
        }
        if let Some(cache) = &self.cache {
            cache.validate()?;
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
        if self.managed.is_some() {
            return Err("managed names resolve through the authority, not direct S3 keys".into());
        }
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
    fn writable_mode_requires_explicit_version_bound_sources_and_shared_local_state() {
        let root = std::env::temp_dir().join("s3drive-write-config");
        let base = EXAMPLE
            .replace("version = 1", "version = 2")
            .replace("mode = \"read-only\"", "mode = \"read-write\"");
        let input = format!(
            "{base}\n[writes]\ndirectory={:?}\n[[writes.sources]]\nalias='documents'\npolicy={:?}\nplan={:?}\nreport={:?}\n",
            root.to_str().unwrap(),
            root.join("policy").to_str().unwrap(),
            root.join("plan").to_str().unwrap(),
            root.join("report").to_str().unwrap()
        );
        let config = Config::parse(&input).unwrap();
        assert_eq!(config.mode, Mode::ReadWrite);
        assert!(Config::parse(&input.replace("version = 2", "version = 1")).is_err());
        assert!(Config::parse(&input.replace("alias='documents'", "alias='missing'")).is_err());
        assert!(Config::parse(&format!("{input}\n[[writes.sources]]\nalias='documents'\npolicy='/tmp/policy'\nplan='/tmp/plan'\nreport='/tmp/report'\n")).is_err());
        let separate = root.join("other");
        assert!(
            Config::parse(&format!(
                "{input}\n[cache]\ndirectory={:?}\n",
                separate.to_str().unwrap()
            ))
            .is_err()
        );
        assert!(
            Config::parse(&format!(
                "{EXAMPLE}\n[writes]\ndirectory={:?}\nsources=[]\n",
                root.to_str().unwrap()
            ))
            .is_err()
        );
    }

    #[test]
    fn managed_configuration_requires_explicit_read_only_authority_and_store_bindings() {
        let token = std::env::temp_dir().join("authority.token");
        let text = format!(
            "{}\n[managed]\nauthority='http://127.0.0.1:43128/v1/namespace'\nvolume='00000000000000000000000000000001'\ntoken_file={:?}\n[[managed.stores]]\nid='00000000000000000000000000000002'\nsource='documents'\n",
            EXAMPLE.replace("version = 1", "version = 3"),
            token.to_str().unwrap()
        );
        let writable = format!(
            "{}\n[writes]\ndirectory={:?}\n[[writes.sources]]\nalias='documents'\npolicy={:?}\nplan={:?}\nreport={:?}\n",
            text.replace("version = 3", "version = 4")
                .replace("read-only", "read-write")
                .replace(
                    "[managed]",
                    "[managed]\ndefault_store='00000000000000000000000000000002'"
                ),
            token.parent().unwrap().join("writes").to_str().unwrap(),
            token.parent().unwrap().join("policy").to_str().unwrap(),
            token.parent().unwrap().join("plan").to_str().unwrap(),
            token.parent().unwrap().join("report").to_str().unwrap()
        );
        assert!(Config::parse(&writable).is_ok());
        assert!(Config::parse(&writable.replace("version = 4", "version = 6")).is_ok());
        let lifecycle = writable.replace("version = 4", "version = 7");
        assert!(Config::parse(&lifecycle).is_err());
        let lifecycle = lifecycle.replace("[managed]", "[managed]\nwindows-disposition='posix-v1'");
        assert!(Config::parse(&lifecycle).is_ok());
        assert!(Config::parse(&lifecycle.replace("version = 7", "version = 6")).is_err());
        assert!(Config::parse(&lifecycle.replace("posix-v1", "guessed-v1")).is_err());
        for (from, to) in [
            ("default_store='00000000000000000000000000000002'", ""),
            (
                "default_store='00000000000000000000000000000002'",
                "default_store='00000000000000000000000000000003'",
            ),
            ("version = 4", "version = 3"),
            ("alias='documents'", "alias='photos'"),
        ] {
            assert!(Config::parse(&writable.replace(from, to)).is_err());
        }
        let config = Config::parse(&text).unwrap();
        assert!(config.resolve("documents/a.txt").is_err());
        let https = text.replace(
            "http://127.0.0.1:43128/v1/namespace",
            "https://metadata.example.test/v1/namespace",
        );
        assert!(Config::parse(&https).is_ok());
        let ca = token.parent().unwrap().join("authority-ca.pem");
        let with_ca = https.replace(
            "[managed]",
            &format!("[managed]\nauthority_ca_file={:?}", ca.to_str().unwrap()),
        );
        assert!(Config::parse(&with_ca).is_ok());
        assert!(
            Config::parse(&with_ca.replace("https://metadata.example.test", "http://127.0.0.1"))
                .is_err()
        );
        assert!(
            Config::parse(&https.replace("/v1/namespace", "/v1/namespace?secret=bad")).is_err()
        );
        assert!(Config::parse(&https.replace("https://", "https://secret@")).is_err());
        assert!(Config::parse(&https.replace("https://", "http://")).is_err());
        for (from, to) in [
            ("read-only", "read-write"),
            (
                "http://127.0.0.1:43128/v1/namespace",
                "http://localhost:43128/v1/namespace",
            ),
            (
                "http://127.0.0.1:43128/v1/namespace",
                "http://secret@127.0.0.1/v1/namespace",
            ),
            ("source='documents'", "source='absent'"),
            (
                "volume='00000000000000000000000000000001'",
                "volume='00000000000000000000000000000000'",
            ),
            ("version = 3", "version = 1"),
        ] {
            let error = Config::parse(&text.replace(from, to)).unwrap_err();
            assert!(!error.contains("secret"));
        }
        assert!(Config::parse(&format!("{text}\n[[managed.stores]]\nid='00000000000000000000000000000002'\nsource='photos'\n")).is_err());
        assert!(
            Config::parse(&format!(
                "{text}\n[cache]\ndirectory={:?}\n",
                token.parent().unwrap().to_str().unwrap()
            ))
            .is_err()
        );
        assert!(
            Config::parse(&text.replace("[managed]", "[managed]\nsecret='never-print-me'"))
                .unwrap_err()
                .contains("invalid configuration")
        );
    }

    #[test]
    fn optional_cache_has_bounded_defaults_and_requires_absolute_local_path() {
        assert!(Config::parse(EXAMPLE).unwrap().cache.is_none());
        let directory = std::env::temp_dir().join("s3drive-config-cache");
        let path = directory
            .to_str()
            .unwrap()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let input = format!("{EXAMPLE}\n[cache]\ndirectory=\"{path}\"\n");
        let cache = Config::parse(&input).unwrap().cache.unwrap();
        assert_eq!(cache.directory, directory);
        assert_eq!(
            (cache.max_bytes, cache.max_file_bytes, cache.max_entries),
            (64 * 1024 * 1024, 1024 * 1024, 4096)
        );
        for extra in [
            "max_bytes=0",
            "max_file_bytes=1048577",
            "max_entries=0",
            "max_entries=65537",
            "max_bytes=2\nmax_file_bytes=3",
            "unknown=true",
        ] {
            assert!(
                Config::parse(&format!("{input}{extra}\n")).is_err(),
                "{extra}"
            );
        }
        assert!(Config::parse(&format!("{EXAMPLE}\n[cache]\ndirectory='relative'\n")).is_err());
    }

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
