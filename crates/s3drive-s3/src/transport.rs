//! Private single-attempt S3 transport shared by the scoped provider probe and
//! the quarantined journal executor. This module grants no upload authority.
use crate::{
    Error, ErrorKind, ObjectRevision, Result, S3Reader, error::sdk_error,
    reader::credential_binding,
};
use aws_sdk_s3::{
    Client,
    config::{
        ProvideCredentials, RequestChecksumCalculation, SharedCredentialsProvider,
        retry::RetryConfig,
    },
    primitives::ByteStream,
};
use sha2::{Digest, Sha256};
use std::{future::Future, sync::Arc};

pub(crate) const MAX_PUT: usize = 8 * 1024 * 1024;
#[derive(Clone)]
pub(crate) enum Condition {
    Missing,
    Match(String),
}
pub(crate) struct Intent {
    pub key: String,
    pub bytes: Arc<[u8]>,
    pub operation: String,
    pub checksum: String,
    pub condition: Condition,
}
impl Intent {
    pub fn validate(&self) -> Result<()> {
        if self.bytes.len() > MAX_PUT
            || !is_hex(&self.operation, 64)
            || self.checksum != hex(Sha256::digest(&self.bytes).as_slice())
        {
            return Err(Error::new("put", ErrorKind::InvalidInput));
        }
        if let Condition::Match(tag) = &self.condition
            && !etag(tag)
        {
            return Err(Error::new("put", ErrorKind::InvalidInput));
        }
        Ok(())
    }
}
pub(crate) enum Outcome {
    Applied(ObjectRevision),
    Conflict(Error),
    Unknown(Error),
}
impl Outcome {
    pub fn description(&self) -> String {
        match self {
            Self::Applied(_) => "success response".into(),
            Self::Conflict(error) => format!("conditional rejection: {error}"),
            Self::Unknown(error) => format!("uncertain outcome: {error}"),
        }
    }
    pub fn conflict(&self) -> bool {
        matches!(self,Self::Conflict(error) if error.http_status==Some(412))
    }
}
#[derive(Clone)]
pub(crate) struct Head {
    pub revision: ObjectRevision,
    pub size: u64,
    pub operation: Option<String>,
    pub checksum: Option<String>,
}
pub(crate) enum Observation {
    Matched(ObjectRevision),
    Absent,
    Other,
}
pub(crate) struct Session {
    pub reader: S3Reader,
    pub binding: String,
    pub binding_id: [u8; 32],
}
impl Session {
    pub async fn pin(reader: S3Reader) -> Result<Self> {
        let credentials =
            tokio::time::timeout(reader.deadline, reader.credentials.provide_credentials())
                .await
                .map_err(|_| Error::new("probe-credentials", ErrorKind::Timeout))?
                .map_err(|_| Error::new("probe-credentials", ErrorKind::Configuration))?;
        let binding_id = *credential_binding(&reader.fingerprint, &credentials).as_bytes();
        let binding = hex(&binding_id);
        let provider = SharedCredentialsProvider::new(credentials);
        let client = Client::from_conf(
            reader
                .client
                .config()
                .to_builder()
                .credentials_provider(provider.clone())
                .retry_config(RetryConfig::standard().with_max_attempts(1))
                .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
                .build(),
        );
        Ok(Self {
            reader: S3Reader {
                client,
                credentials: provider,
                ..reader
            },
            binding,
            binding_id,
        })
    }
    pub(crate) async fn bounded<T>(
        &self,
        operation: &'static str,
        future: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        tokio::time::timeout(self.reader.deadline, future)
            .await
            .map_err(|_| Error::new(operation, ErrorKind::Timeout))?
    }
    pub async fn put(&self, intent: &Intent) -> Result<Outcome> {
        intent.validate()?;
        let full_key = self.reader.full_key(&intent.key)?;
        let mut request = self
            .reader
            .client
            .put_object()
            .bucket(&self.reader.source.bucket)
            .key(full_key)
            .body(ByteStream::from(intent.bytes.to_vec()))
            .content_length(intent.bytes.len() as i64)
            .metadata("s3drive-operation", &intent.operation)
            .metadata("s3drive-sha256", &intent.checksum);
        request = match &intent.condition {
            Condition::Missing => request.if_none_match("*"),
            Condition::Match(tag) => request.if_match(tag),
        };
        // No SDK replay: a lost success must not become a second conditional PUT.
        // Cancellation may still leave an applied operation; the plan is durable.
        let result = self
            .bounded("put", async {
                Box::pin(request.send())
                    .await
                    .map_err(|e| sdk_error("put", e))
            })
            .await;
        Ok(match result {
            Ok(output) => match revision(output.e_tag(), output.version_id()) {
                Ok(revision) => Outcome::Applied(revision),
                Err(error) => Outcome::Unknown(error),
            },
            Err(error) if matches!(error.http_status, Some(409 | 412)) => Outcome::Conflict(error),
            Err(error) => Outcome::Unknown(error),
        })
    }
    pub async fn head(&self, key: &str) -> Result<Option<Head>> {
        let full_key = self.reader.full_key(key)?;
        let result = self
            .bounded("probe-head", async {
                Box::pin(
                    self.reader
                        .client
                        .head_object()
                        .bucket(&self.reader.source.bucket)
                        .key(full_key)
                        .send(),
                )
                .await
                .map_err(|e| sdk_error("probe-head", e))
            })
            .await;
        let output = match result {
            Err(error) if error.kind == ErrorKind::NotFound => return Ok(None),
            other => other?,
        };
        let revision = revision(output.e_tag(), output.version_id())?;
        let size = output
            .content_length()
            .and_then(|size| u64::try_from(size).ok())
            .ok_or_else(|| Error::new("probe-head", ErrorKind::InvalidResponse))?;
        Ok(Some(Head {
            revision,
            size,
            operation: output
                .metadata()
                .and_then(|m| m.get("s3drive-operation"))
                .cloned(),
            checksum: output
                .metadata()
                .and_then(|m| m.get("s3drive-sha256"))
                .cloned(),
        }))
    }
    /// Verify one revision's metadata and complete bounded body. An absent or
    /// rival object is an observation, never permission to replay a lost PUT.
    pub async fn verify(&self, intent: &Intent) -> Result<Option<ObjectRevision>> {
        Ok(match self.observe(intent).await? {
            Observation::Matched(revision) => Some(revision),
            Observation::Absent | Observation::Other => None,
        })
    }
    pub async fn observe(&self, intent: &Intent) -> Result<Observation> {
        intent.validate()?;
        let Some(head) = self.head(&intent.key).await? else {
            return Ok(Observation::Absent);
        };
        if head.size != intent.bytes.len() as u64
            || head.operation.as_deref() != Some(&intent.operation)
            || head.checksum.as_deref() != Some(&intent.checksum)
        {
            return Ok(Observation::Other);
        }
        self.bounded("probe-verify", async {
            let full_key = self.reader.full_key(&intent.key)?;
            let mut output = Box::pin(
                self.reader
                    .client
                    .get_object()
                    .bucket(&self.reader.source.bucket)
                    .key(full_key)
                    .set_if_match(head.revision.etag.clone())
                    .set_version_id(head.revision.version_id.clone())
                    .send(),
            )
            .await
            .map_err(|e| sdk_error("probe-verify", e))?;
            if output.content_length() != Some(intent.bytes.len() as i64)
                || output.e_tag() != head.revision.etag.as_deref()
                || output.version_id().filter(|v| *v != "null")
                    != head.revision.version_id.as_deref()
                || output
                    .metadata()
                    .and_then(|m| m.get("s3drive-operation"))
                    .map(String::as_str)
                    != Some(&intent.operation)
                || output
                    .metadata()
                    .and_then(|m| m.get("s3drive-sha256"))
                    .map(String::as_str)
                    != Some(&intent.checksum)
            {
                return Err(Error::new("probe-verify", ErrorKind::RevisionChanged));
            }
            let mut hash = Sha256::new();
            let mut length = 0;
            while let Some(chunk) = output.body.next().await {
                let chunk = chunk.map_err(|_| Error::new("probe-verify", ErrorKind::Transport))?;
                if chunk.len() > intent.bytes.len() - length {
                    return Err(Error::new("probe-verify", ErrorKind::InvalidResponse));
                }
                length += chunk.len();
                hash.update(chunk);
            }
            if length != intent.bytes.len() || hex(hash.finalize().as_slice()) != intent.checksum {
                return Err(Error::new("probe-verify", ErrorKind::InvalidResponse));
            }
            Ok(Observation::Matched(head.revision))
        })
        .await
    }
}
fn etag(tag: &str) -> bool {
    (3..=8192).contains(&tag.len())
        && tag.starts_with('"')
        && tag.ends_with('"')
        && tag[1..tag.len() - 1]
            .bytes()
            .all(|b| b.is_ascii() && !b.is_ascii_control() && b != b'"')
}
fn revision(tag: Option<&str>, version: Option<&str>) -> Result<ObjectRevision> {
    let version = version.filter(|value| *value != "null");
    if !tag.is_some_and(etag)
        || version
            .is_some_and(|v| v.is_empty() || v.len() > 8192 || v.chars().any(char::is_control))
    {
        return Err(Error::new("put-revision", ErrorKind::InvalidResponse));
    }
    Ok(ObjectRevision {
        etag: tag.map(Into::into),
        version_id: version.map(Into::into),
    })
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub(crate) fn is_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
