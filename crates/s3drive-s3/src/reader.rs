use crate::{Error, ErrorKind, ObjectRevision, Result, error::sdk_error};
use aws_config::{
    default_provider::credentials::DefaultCredentialsChain, profile::ProfileFileCredentialsProvider,
};
use aws_sdk_s3::{
    Client,
    config::{
        Credentials, ProvideCredentials, Region, SharedCredentialsProvider, retry::RetryConfig,
        timeout::TimeoutConfig,
    },
    types::EncodingType,
};
use percent_encoding::percent_decode_str;
use s3drive_core::{ListUrlEncoding, Source};
use sha2::{Digest, Sha256};
use std::{fmt, future::Future, time::Duration};

pub const MAX_READ_BYTES: usize = 8 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct ListedObject {
    /// Original object key relative to the configured source prefix.
    pub key: String,
    pub size: u64,
    pub etag: Option<String>,
    pub modified_unix_seconds: Option<i64>,
}

#[derive(Debug)]
pub struct ListPage {
    pub objects: Vec<ListedObject>,
    /// Prefixes relative to the source root, retaining the trailing slash.
    pub directories: Vec<String>,
    /// Opaque token. Pass unchanged to the same source/directory request.
    pub next_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ObjectMetadata {
    pub key: String,
    pub size: u64,
    pub revision: ObjectRevision,
    pub modified_unix_seconds: Option<i64>,
}

#[derive(Clone)]
pub struct S3Reader {
    pub(crate) client: Client,
    pub(crate) credentials: SharedCredentialsProvider,
    pub(crate) source: Source,
    pub(crate) fingerprint: String,
    pub(crate) deadline: Duration,
}

/// An opaque binding of source configuration and one credential generation.
/// Available only after a successful, revision-bearing HEAD. This is private
/// local metadata, not a provider-global account ID or an authorization token.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheIdentity([u8; 32]);

impl CacheIdentity {
    /// Stable bytes for private local state. Never use this digest alone to
    /// authorize an open: obtain a fresh handle with an online HEAD first.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for CacheIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CacheIdentity([redacted])")
    }
}

/// A handle binds the reader, credentials, key, size and revision together.
/// Callers cannot substitute metadata or credentials after opening.
#[derive(Clone)]
pub struct ReadHandle {
    reader: S3Reader,
    cache_identity: CacheIdentity,
    metadata: ObjectMetadata,
    full_key: String,
}

impl S3Reader {
    pub async fn from_source(source: &Source) -> Result<Self> {
        source
            .validate()
            .map_err(|_| Error::new("configure", ErrorKind::InvalidInput))?;
        let region = Region::new(source.region.clone());
        let credentials = if let Some(profile) = &source.profile {
            // An explicit profile must not silently use unrelated ambient keys.
            SharedCredentialsProvider::new(
                ProfileFileCredentialsProvider::builder()
                    .profile_name(profile)
                    .build(),
            )
        } else {
            SharedCredentialsProvider::new(
                DefaultCredentialsChain::builder()
                    .region(region)
                    .build()
                    .await,
            )
        };
        Self::with_credentials(source.clone(), credentials, DEADLINE, 3)
    }

    pub(crate) fn with_credentials(
        source: Source,
        credentials: SharedCredentialsProvider,
        deadline: Duration,
        attempts: u32,
    ) -> Result<Self> {
        source
            .validate()
            .map_err(|_| Error::new("configure", ErrorKind::InvalidInput))?;
        // Do not copy SDK shared endpoint/region configuration into this client.
        // Each source is authoritative; credential discovery is separate.
        let mut builder = aws_sdk_s3::config::Builder::new()
            .behavior_version_latest()
            .region(Region::new(source.region.clone()))
            .credentials_provider(credentials.clone())
            .force_path_style(source.path_style)
            .use_fips(false)
            .use_dual_stack(false)
            .retry_config(RetryConfig::standard().with_max_attempts(attempts))
            .timeout_config(
                TimeoutConfig::builder()
                    .operation_timeout(deadline)
                    .connect_timeout(Duration::from_secs(5))
                    .build(),
            );
        builder.set_endpoint_url(source.endpoint.clone());
        let fingerprint = source_fingerprint(&source);
        Ok(Self {
            client: Client::from_conf(builder.build()),
            credentials,
            source,
            fingerprint,
            deadline,
        })
    }

    /// Configuration identity, NOT proof of the currently authenticated principal.
    /// Do not use alone as a persistent cross-session cache key.
    pub fn configuration_fingerprint(&self) -> &str {
        &self.fingerprint
    }
    /// Stable routing scope only: excludes local aliases, profiles, credentials
    /// and LIST/addressing dialect. Not an authorization or principal identity.
    pub fn storage_fingerprint(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(b"s3drive-storage-route-v1\0");
        let endpoint = self
            .source
            .endpoint
            .as_ref()
            .map(|s| url::Url::parse(s).expect("validated endpoint").to_string());
        hash.update([u8::from(endpoint.is_some())]);
        for value in [
            endpoint.as_deref().unwrap_or_default(),
            &self.source.bucket,
            &self.source.region,
            &self.source.prefix,
        ] {
            hash.update((value.len() as u64).to_be_bytes());
            hash.update(value.as_bytes());
        }
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }

    pub async fn list_page(
        &self,
        directory: &str,
        token: Option<&str>,
        max_keys: u16,
    ) -> Result<ListPage> {
        if !(1..=1000).contains(&max_keys) || token.is_some_and(str::is_empty) {
            return Err(Error::new("list", ErrorKind::InvalidInput));
        }
        if !directory.is_empty() {
            let base = directory
                .strip_suffix('/')
                .ok_or_else(|| Error::new("list", ErrorKind::InvalidInput))?;
            validate_key(base)?;
        }
        let prefix = format!("{}{directory}", self.source.prefix);
        if prefix.len() > 1024 {
            return Err(Error::new("list", ErrorKind::InvalidInput));
        }
        deadline(self.deadline, "list", async {
            let output = self
                .client
                .list_objects_v2()
                .bucket(&self.source.bucket)
                .prefix(&prefix)
                .delimiter("/")
                .encoding_type(EncodingType::Url)
                .max_keys(i32::from(max_keys))
                .set_continuation_token(token.map(str::to_owned))
                .send()
                .await
                .map_err(|e| sdk_error("list", e))?;
            if output.encoding_type() != Some(&EncodingType::Url)
                || output.contents().len() + output.common_prefixes().len() > usize::from(max_keys)
            {
                return Err(Error::new("list", ErrorKind::InvalidResponse));
            }
            let next_token = match output.is_truncated() {
                Some(true) => {
                    let next = output
                        .next_continuation_token()
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| Error::new("list", ErrorKind::InvalidResponse))?;
                    if Some(next) == token {
                        return Err(Error::new("list", ErrorKind::InvalidResponse));
                    }
                    Some(next.to_owned())
                }
                Some(false) if output.next_continuation_token().is_none() => None,
                _ => return Err(Error::new("list", ErrorKind::InvalidResponse)),
            };
            let mut objects = Vec::with_capacity(output.contents().len());
            for object in output.contents() {
                objects.push(ListedObject {
                    key: self.list_key(object.key(), &prefix)?,
                    size: size(object.size(), "list")?,
                    etag: object.e_tag().map(str::to_owned),
                    modified_unix_seconds: object.last_modified().map(|t| t.secs()),
                });
            }
            let mut directories = Vec::with_capacity(output.common_prefixes().len());
            for entry in output.common_prefixes() {
                let key = self.list_key(entry.prefix(), &prefix)?;
                if !key.ends_with('/') || key == directory {
                    return Err(Error::new("list", ErrorKind::InvalidResponse));
                }
                directories.push(key);
            }
            Ok(ListPage {
                objects,
                directories,
                next_token,
            })
        })
        .await
    }

    fn list_key(&self, encoded: Option<&str>, requested_prefix: &str) -> Result<String> {
        let encoded = encoded.ok_or_else(|| Error::new("list", ErrorKind::InvalidResponse))?;
        // percent_decode_str deliberately leaves malformed percent sequences
        // alone; reject those first to avoid ambiguous provider responses.
        let bytes = encoded.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'%'
                && (index + 2 >= bytes.len()
                    || !bytes[index + 1].is_ascii_hexdigit()
                    || !bytes[index + 2].is_ascii_hexdigit())
            {
                return Err(Error::new("list", ErrorKind::InvalidResponse));
            }
        }
        // Wasabi's observed EncodingType=url responses encode spaces as '+'.
        // Select the dialect explicitly: guessing would alias distinct keys on
        // providers that return unescaped literal '+'. Replace before decoding
        // so %2B remains a plus and %252B remains the literal text %2B.
        let form_encoded;
        let encoded = if self.source.list_url_encoding == ListUrlEncoding::Form {
            form_encoded = encoded.replace('+', "%20");
            form_encoded.as_str()
        } else {
            encoded
        };
        let key = percent_decode_str(encoded)
            .decode_utf8()
            .map_err(|_| Error::new("list", ErrorKind::InvalidResponse))?;
        if !key.starts_with(requested_prefix) || key.len() > 1024 {
            return Err(Error::new("list", ErrorKind::InvalidResponse));
        }
        key.strip_prefix(&self.source.prefix)
            .map(str::to_owned)
            .ok_or_else(|| Error::new("list", ErrorKind::InvalidResponse))
    }

    pub async fn head(&self, key: &str) -> Result<ObjectMetadata> {
        let full_key = self.full_key(key)?;
        deadline(self.deadline, "head", async {
            let output = self
                .client
                .head_object()
                .bucket(&self.source.bucket)
                .key(full_key)
                .send()
                .await
                .map_err(|e| sdk_error("head", e))?;
            Ok(ObjectMetadata {
                key: key.to_owned(),
                size: size(output.content_length(), "head")?,
                revision: ObjectRevision {
                    etag: output.e_tag().filter(|s| !s.is_empty()).map(str::to_owned),
                    // A null version can be replaced when versioning is suspended.
                    version_id: output
                        .version_id()
                        .filter(|s| !s.is_empty() && *s != "null")
                        .map(str::to_owned),
                },
                modified_unix_seconds: output.last_modified().map(|t| t.secs()),
            })
        })
        .await
    }

    pub async fn open(&self, key: &str) -> Result<ReadHandle> {
        let full_key = self.full_key(key)?;
        // One outer deadline includes credential discovery and HEAD. The SDK's
        // Config::credentials_provider getter is deprecated and always None;
        // retain our configured provider explicitly instead.
        deadline(self.deadline, "open", async {
            let credentials = self
                .credentials
                .provide_credentials()
                .await
                .map_err(|_| Error::new("open", ErrorKind::Configuration))?;
            let provider = SharedCredentialsProvider::new(credentials.clone());
            let reader = Self {
                client: Client::from_conf(
                    self.client
                        .config()
                        .to_builder()
                        .credentials_provider(provider.clone())
                        .build(),
                ),
                credentials: provider,
                ..self.clone()
            };
            // Keep the SDK HEAD future on the heap. Nesting its large inline
            // state in this second deadline overflows WinFsp callback stacks
            // in debug builds, even though ordinary Tokio tests pass.
            let metadata = Box::pin(reader.head(key)).await?;
            if metadata.revision.etag.is_none() && metadata.revision.version_id.is_none() {
                return Err(Error::new("open", ErrorKind::Unsupported));
            }
            Ok(ReadHandle {
                cache_identity: credential_binding(&self.fingerprint, &credentials),
                reader,
                full_key,
                metadata,
            })
        })
        .await
    }

    pub(crate) fn full_key(&self, key: &str) -> Result<String> {
        validate_key(key)?;
        let key = format!("{}{key}", self.source.prefix);
        if key.len() > 1024 {
            return Err(Error::new("key", ErrorKind::InvalidInput));
        }
        Ok(key)
    }
}

impl ReadHandle {
    /// Configuration of this pinned handle, including its exact prefix dialect.
    pub fn configuration_fingerprint(&self) -> &str {
        &self.reader.fingerprint
    }

    pub fn cache_identity(&self) -> CacheIdentity {
        self.cache_identity
    }

    pub fn metadata(&self) -> &ObjectMetadata {
        &self.metadata
    }

    /// Returns at most 8 MiB; requests crossing EOF are clipped. A zero-length
    /// or EOF read makes no network request. Dropping the future cancels the read.
    pub async fn read_range(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        if length > MAX_READ_BYTES {
            return Err(Error::new("read", ErrorKind::InvalidInput));
        }
        if length == 0 || offset >= self.metadata.size {
            return Ok(Vec::new());
        }
        let length = (length as u64).min(self.metadata.size - offset);
        let end = offset + length - 1;
        deadline(self.reader.deadline, "read", async {
            let mut output = self
                .reader
                .client
                .get_object()
                .bucket(&self.reader.source.bucket)
                .key(&self.full_key)
                .range(format!("bytes={offset}-{end}"))
                .set_if_match(self.metadata.revision.etag.clone())
                .set_version_id(self.metadata.revision.version_id.clone())
                .send()
                .await
                .map_err(|e| sdk_error("read", e))?;
            let expected = format!("bytes {offset}-{end}/{}", self.metadata.size);
            if output.content_range() != Some(expected.as_str())
                || size(output.content_length(), "read")? != length
            {
                return Err(Error::new("read", ErrorKind::InvalidResponse));
            }
            if self
                .metadata
                .revision
                .etag
                .as_deref()
                .is_some_and(|etag| output.e_tag() != Some(etag))
                || self
                    .metadata
                    .revision
                    .version_id
                    .as_deref()
                    .is_some_and(|v| output.version_id() != Some(v))
            {
                return Err(Error::new("read", ErrorKind::RevisionChanged));
            }
            let mut bytes = Vec::with_capacity(length as usize);
            while let Some(chunk) = output.body.next().await {
                let chunk = chunk.map_err(|_| Error::new("read", ErrorKind::Transport))?;
                if chunk.len() > length as usize - bytes.len() {
                    return Err(Error::new("read", ErrorKind::InvalidResponse));
                }
                bytes.extend_from_slice(&chunk);
            }
            if bytes.len() != length as usize {
                return Err(Error::new("read", ErrorKind::InvalidResponse));
            }
            Ok(bytes)
        })
        .await
    }
}

pub(crate) fn credential_binding(fingerprint: &str, credentials: &Credentials) -> CacheIdentity {
    let mut hash = Sha256::new();
    hash.update(b"s3drive-credential-binding-v1");
    for part in [
        fingerprint,
        credentials.access_key_id(),
        credentials.secret_access_key(),
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    let token = credentials.session_token();
    hash.update([u8::from(token.is_some())]);
    let bytes = token.unwrap_or_default().as_bytes();
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
    CacheIdentity(hash.finalize().into())
}

fn validate_key(key: &str) -> Result<()> {
    if key.contains('\\')
        || key.chars().any(char::is_control)
        || key
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == "..")
    {
        return Err(Error::new("key", ErrorKind::InvalidInput));
    }
    Ok(())
}

fn size(value: Option<i64>, operation: &'static str) -> Result<u64> {
    value
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| Error::new(operation, ErrorKind::InvalidResponse))
}

async fn deadline<T>(
    duration: Duration,
    operation: &'static str,
    future: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(duration, future)
        .await
        .map_err(|_| Error::new(operation, ErrorKind::Timeout))?
}

pub(crate) fn source_fingerprint(source: &Source) -> String {
    let mut hash = Sha256::new();
    hash.update(b"s3drive-source-config-v2");
    let endpoint = source
        .endpoint
        .as_ref()
        .map(|s| url::Url::parse(s).expect("validated endpoint").to_string());
    for part in [
        &source.alias,
        &source.bucket,
        &source.region,
        &source.prefix,
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    for part in [endpoint.as_deref(), source.profile.as_deref()] {
        hash.update([u8::from(part.is_some())]);
        let bytes = part.unwrap_or_default().as_bytes();
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    hash.update([
        u8::from(source.path_style),
        u8::from(source.allow_http_loopback),
        u8::from(source.list_url_encoding == ListUrlEncoding::Form),
    ]);
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests;
