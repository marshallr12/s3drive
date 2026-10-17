//! Local application intent and recorded provider evidence are separate inputs.
use super::{Failure, Guard};
use crate::{S3Reader, probe, transport};
use s3drive_core::Source;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    error::Error as StdError,
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_TEXT: usize = 32 * 1024;
const MAX_AGE: u64 = 30 * 24 * 60 * 60;

#[derive(Debug)]
pub enum SetupError {
    Policy,
    Evidence,
    SourceMismatch,
    CredentialMismatch,
    Target,
    Credentials(crate::Error),
}
impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Policy => "invalid or unsupported upload write policy",
            Self::Evidence => {
                "upload evidence is incomplete, inconsistent or not the selected report"
            }
            Self::SourceMismatch => "upload policy does not match the source configuration",
            Self::CredentialMismatch => {
                "upload evidence does not match the resolved credential generation"
            }
            Self::Target => "upload target is outside the selected policy or has an invalid key",
            Self::Credentials(_) => "could not pin upload credentials",
        })
    }
}
impl StdError for SetupError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Credentials(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    source_fingerprint: String,
    report_sha256: String,
    writable_prefix: String,
    max_object_bytes: usize,
    operations: Operations,
    object_metadata: String,
    evidence_max_age_seconds: u64,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Operations {
    CreateOnly,
    CreateAndReplace,
}

/// Explicit caller intent for one source, separate from a conformance result.
/// The report hash is over its exact UTF-8 bytes, not a rewritten TOML document.
/// Local files are trusted owner input, not signed provider attestations.
pub struct WritePolicy {
    document: Document,
    credential_generation: String,
    observed: u64,
    expires: u64,
}
impl fmt::Debug for WritePolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WritePolicy([source-bound local intent and evidence])")
    }
}
impl WritePolicy {
    /// Parse only local inputs; this does not execute a probe or contact S3.
    /// An expired policy can open an observation-only executor for recovery;
    /// it cannot grant a new send claim. Future-dated evidence is rejected.
    pub fn from_toml(
        source: &Source,
        policy_text: &str,
        plan_text: &str,
        report_text: &str,
    ) -> std::result::Result<Self, SetupError> {
        source.validate().map_err(|_| SetupError::SourceMismatch)?;
        if policy_text.len() > MAX_TEXT {
            return Err(SetupError::Policy);
        }
        let document: Document = toml::from_str(policy_text).map_err(|_| SetupError::Policy)?;
        if document.version != 1
            || document.object_metadata != "s3drive-markers-only"
            || !transport::is_hex(&document.source_fingerprint, 64)
            || !transport::is_hex(&document.report_sha256, 64)
            || !(1..=transport::MAX_PUT).contains(&document.max_object_bytes)
            || !(1..=MAX_AGE).contains(&document.evidence_max_age_seconds)
        {
            return Err(SetupError::Policy);
        }
        // Reuse the same exact path-component rules as source prefixes.
        let mut scoped = source.clone();
        scoped.prefix = document.writable_prefix.clone();
        scoped.validate().map_err(|_| SetupError::Policy)?;
        if source.prefix.len() + document.writable_prefix.len() >= 1024 {
            return Err(SetupError::Policy);
        }
        if document.source_fingerprint != source_fingerprint(source) {
            return Err(SetupError::SourceMismatch);
        }
        if report_text.len() > MAX_TEXT
            || document.report_sha256
                != transport::hex(Sha256::digest(report_text.as_bytes()).as_slice())
        {
            return Err(SetupError::Evidence);
        }
        let plan = probe::Plan::from_toml(plan_text).map_err(|_| SetupError::Evidence)?;
        plan.validate(source, &plan.digest().map_err(|_| SetupError::Evidence)?)
            .map_err(|_| SetupError::Evidence)?;
        let report = probe::Report::from_toml(report_text).map_err(|_| SetupError::Evidence)?;
        let (credential_generation, observed) =
            report.acceptance(&plan).map_err(|_| SetupError::Evidence)?;
        if observed > now().ok_or(SetupError::Evidence)? {
            return Err(SetupError::Evidence);
        }
        let expires = observed
            .checked_add(document.evidence_max_age_seconds)
            .ok_or(SetupError::Evidence)?;
        Ok(Self {
            document,
            credential_generation,
            observed,
            expires,
        })
    }

    pub(super) fn source_matches(&self, reader: &S3Reader) -> bool {
        self.document.source_fingerprint == reader.configuration_fingerprint()
    }
    pub(super) fn credentials_match(&self, binding: &str) -> bool {
        self.credential_generation == binding
    }
    pub(super) fn permits_key(&self, reader: &S3Reader, key: &str) -> bool {
        let Ok(full_key) = reader.full_key(key) else {
            return false;
        };
        // Disposable probe reservations must never become application targets.
        !key.starts_with("s3drive-conformance/")
            && !full_key.starts_with("s3drive-conformance/")
            && key.starts_with(&self.document.writable_prefix)
            && key.len() > self.document.writable_prefix.len()
    }
    pub(super) fn admission(
        &self,
        reader: &S3Reader,
        key: &str,
        size: u64,
    ) -> std::result::Result<(), Failure> {
        if !self.permits_key(reader, key) {
            return Err(Failure::PolicyScope);
        }
        if size > self.document.max_object_bytes as u64 {
            return Err(Failure::PolicySize);
        }
        match now() {
            Some(now) if self.observed <= now && now < self.expires => Ok(()),
            _ => Err(Failure::PolicyExpired),
        }
    }
    pub(super) fn guard(&self, guard: &Guard) -> std::result::Result<(), Failure> {
        if matches!(self.document.operations, Operations::CreateOnly)
            && matches!(guard, Guard::Match(_))
        {
            Err(Failure::PolicyOperation)
        } else {
            Ok(())
        }
    }
}
pub(super) fn source_fingerprint(source: &Source) -> String {
    crate::reader::source_fingerprint(source)
}
fn now() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|v| v.as_secs())
}

#[cfg(test)]
pub(super) mod tests;
