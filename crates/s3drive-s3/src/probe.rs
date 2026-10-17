//! Explicit disposable-key capability probes. Never run from a filesystem open
//! or mount, and never interpret compatibility marketing as a verified guard.
mod transport;
use crate::transport::{hex, is_hex};
use crate::{Error, ErrorKind, Result, S3Reader, reader::source_fingerprint};
use s3drive_core::Source;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use transport::{Condition, Intent, Outcome, Session};

const NAMES: [&str; 6] = [
    "create.bin",
    "replace.bin",
    "race-create.bin",
    "race-replace.bin",
    "nested/日本語 %2F + café.txt",
    "empty",
];
const MAX_PLAN_BYTES: usize = 32 * 1024;
pub(crate) const CHECK_NAMES: [&str; 10] = [
    "conditional-create",
    "existing-key-create-rejected",
    "conditional-replace",
    "stale-replace-rejected",
    "wrong-validator-rejected",
    "concurrent-create-one-winner",
    "concurrent-replace-one-winner",
    "unicode-literal-key-replace",
    "empty-object-round-trip",
    "receipt-independent-observation",
];

/// A concrete, source-bound reservation of exactly six disposable keys. The
/// serialized file contains no credentials. Execution regenerates every key
/// from the random ID and source, rather than trusting file-supplied targets.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    version: u32,
    run_id: String,
    created_unix_seconds: u64,
    source_fingerprint: String,
    endpoint: Option<String>,
    region: String,
    bucket: String,
    source_prefix: String,
    relative_prefix: String,
    max_objects: u32,
    max_object_bytes: u32,
    max_put_attempts: u32,
    keys: Vec<String>,
}
impl Plan {
    pub fn prepare(source: &Source) -> Result<Self> {
        source
            .validate()
            .map_err(|_| Error::new("probe-plan", ErrorKind::Configuration))?;
        let mut id = [0; 16];
        getrandom::fill(&mut id)
            .map_err(|_| Error::new("probe-random", ErrorKind::Configuration))?;
        Self::build(source, hex(&id), now()?)
    }
    fn build(source: &Source, run_id: String, created: u64) -> Result<Self> {
        source
            .validate()
            .map_err(|_| Error::new("probe-plan", ErrorKind::Configuration))?;
        if !is_hex(&run_id, 32) {
            return Err(Error::new("probe-plan", ErrorKind::InvalidInput));
        }
        let relative_prefix = format!("s3drive-conformance/{run_id}/");
        let keys = NAMES
            .iter()
            .map(|name| format!("{}{relative_prefix}{name}", source.prefix))
            .collect::<Vec<_>>();
        if keys.iter().any(|key| key.len() > 1024) {
            return Err(Error::new("probe-key-length", ErrorKind::InvalidInput));
        }
        Ok(Self {
            version: 1,
            run_id,
            created_unix_seconds: created,
            source_fingerprint: source_fingerprint(source),
            endpoint: source.endpoint.clone(),
            region: source.region.clone(),
            bucket: source.bucket.clone(),
            source_prefix: source.prefix.clone(),
            relative_prefix,
            max_objects: 6,
            max_object_bytes: 4096,
            max_put_attempts: 14,
            keys,
        })
    }
    pub fn from_toml(text: &str) -> Result<Self> {
        if text.len() > MAX_PLAN_BYTES {
            return Err(Error::new("probe-plan-size", ErrorKind::InvalidInput));
        }
        toml::from_str(text).map_err(|_| Error::new("probe-plan-format", ErrorKind::InvalidInput))
    }
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self)
            .map_err(|_| Error::new("probe-plan-format", ErrorKind::InvalidInput))
    }
    pub fn digest(&self) -> Result<String> {
        let canonical = toml::to_string(self)
            .map_err(|_| Error::new("probe-plan-format", ErrorKind::InvalidInput))?;
        Ok(hex(Sha256::digest(canonical).as_slice()))
    }
    pub fn keys(&self) -> &[String] {
        &self.keys
    }
    pub fn bucket(&self) -> &str {
        &self.bucket
    }
    pub fn full_prefix(&self) -> String {
        format!("{}{}", self.source_prefix, self.relative_prefix)
    }
    pub fn validate(&self, source: &Source, confirmation: &str) -> Result<()> {
        let expected = Self::build(source, self.run_id.clone(), self.created_unix_seconds)?;
        if self != &expected || confirmation != self.digest()? {
            return Err(Error::new(
                "probe-plan-confirmation",
                ErrorKind::InvalidInput,
            ));
        }
        Ok(())
    }
    fn relative_key(&self, index: usize) -> String {
        format!("{}{}", self.relative_prefix, NAMES[index])
    }
    fn intent(&self, index: usize, variant: u8, condition: Condition) -> Intent {
        let bytes: Arc<[u8]> = payload(index, variant).into();
        let operation = hex(Sha256::digest(format!(
            "s3drive-probe-operation-v1\0{}\0{index}\0{variant}",
            self.run_id
        ))
        .as_slice());
        let checksum = hex(Sha256::digest(&bytes).as_slice());
        Intent {
            key: self.relative_key(index),
            bytes,
            operation,
            checksum,
            condition,
        }
    }
    /// Run only after explicit authorization for the exact plan. No durable
    /// upload jobs, writable mount settings or bucket policies are changed.
    pub async fn execute(&self, source: &Source, confirmation: &str) -> Result<Report> {
        self.validate(source, confirmation)?;
        let session = Session::pin(S3Reader::from_source(source).await?).await?;
        self.run(session).await
    }
    async fn run(&self, session: Session) -> Result<Report> {
        session.preflight(self).await?; // Nothing mutates if this fails.
        let mut report = Report::new(self, session.binding.clone())?;
        if let Err(error) = self.checks(&session, &mut report.checks).await {
            report.errors.push(error.to_string());
        }
        self.cleanup_with(&session, &mut report).await;
        report.passed = report.errors.is_empty()
            && report.remaining_keys.is_empty()
            && report.checks.len() == 10
            && report.checks.iter().all(|c| c.passed);
        Ok(report)
    }
    /// Recover fixture disposal after an interrupted probe. Refuses objects
    /// without this plan's known marker and exact bytes. Never issues a PUT.
    pub async fn cleanup(&self, source: &Source, confirmation: &str) -> Result<Report> {
        self.validate(source, confirmation)?;
        let session = Session::pin(S3Reader::from_source(source).await?).await?;
        session.ensure_unversioned().await?;
        let mut report = Report::new(self, session.binding.clone())?;
        self.cleanup_with(&session, &mut report).await;
        report.passed = report.errors.is_empty() && report.remaining_keys.is_empty();
        Ok(report)
    }
    async fn cleanup_with(&self, session: &Session, report: &mut Report) {
        if let Err(error) = session.ensure_unversioned().await {
            report.errors.push(error.to_string());
            report.remaining_keys = self.keys.clone();
            return;
        }
        for index in 0..self.keys.len() {
            if let Err(error) = session.cleanup(self, index).await {
                report.errors.push(error.to_string());
                report.remaining_keys.push(self.keys[index].clone());
            }
        }
        match session
            .reader
            .list_page(&self.relative_prefix, None, 1000)
            .await
        {
            Ok(page)
                if page.objects.is_empty()
                    && page.directories.is_empty()
                    && page.next_token.is_none() => {}
            Ok(_) => report
                .errors
                .push("probe prefix is not empty after cleanup".into()),
            Err(error) => report.errors.push(error.to_string()),
        }
    }
    async fn checks(&self, session: &Session, checks: &mut Vec<Check>) -> Result<()> {
        let create = self.intent(0, 0, Condition::Missing);
        let result = session.put(&create).await?;
        check(
            checks,
            "conditional-create",
            matches!(result, Outcome::Applied(_)) && session.verify(&create).await?.is_some(),
            result.description(),
        );
        let duplicate = self.intent(0, 1, Condition::Missing);
        let result = session.put(&duplicate).await?;
        check(
            checks,
            "existing-key-create-rejected",
            result.conflict() && session.verify(&create).await?.is_some(),
            result.description(),
        );

        let initial = self.intent(1, 0, Condition::Missing);
        let result = session.put(&initial).await?;
        let Outcome::Applied(old) = result else {
            check(checks, "replace-setup", false, result.description());
            return Ok(());
        };
        let replace = self.intent(
            1,
            1,
            Condition::Match(
                old.etag
                    .clone()
                    .ok_or_else(|| Error::new("probe-revision", ErrorKind::InvalidResponse))?,
            ),
        );
        let result = session.put(&replace).await?;
        check(
            checks,
            "conditional-replace",
            matches!(result, Outcome::Applied(_)) && session.verify(&replace).await?.is_some(),
            result.description(),
        );
        let stale = self.intent(
            1,
            2,
            Condition::Match(
                old.etag
                    .ok_or_else(|| Error::new("probe-revision", ErrorKind::InvalidResponse))?,
            ),
        );
        let result = session.put(&stale).await?;
        check(
            checks,
            "stale-replace-rejected",
            result.conflict() && session.verify(&replace).await?.is_some(),
            result.description(),
        );
        let bogus = self.intent(
            1,
            3,
            Condition::Match(format!("\"s3drive-nonexistent-{}\"", self.run_id)),
        );
        let result = session.put(&bogus).await?;
        check(
            checks,
            "wrong-validator-rejected",
            result.conflict() && session.verify(&replace).await?.is_some(),
            result.description(),
        );

        let a = self.intent(2, 0, Condition::Missing);
        let b = self.intent(2, 1, Condition::Missing);
        let (ra, rb) = tokio::join!(session.put(&a), session.put(&b));
        let (ra, rb) = (ra?, rb?);
        let winner = match (&ra, &rb) {
            (Outcome::Applied(_), other) if other.conflict() => Some(&a),
            (other, Outcome::Applied(_)) if other.conflict() => Some(&b),
            _ => None,
        };
        let verified = match winner {
            Some(winner) => session.verify(winner).await?.is_some(),
            None => false,
        };
        check(
            checks,
            "concurrent-create-one-winner",
            verified,
            format!("{}; {}", ra.description(), rb.description()),
        );

        let seed = self.intent(3, 0, Condition::Missing);
        let result = session.put(&seed).await?;
        let Outcome::Applied(revision) = result else {
            check(checks, "replace-race-setup", false, result.description());
            return Ok(());
        };
        let condition = Condition::Match(
            revision
                .etag
                .ok_or_else(|| Error::new("probe-revision", ErrorKind::InvalidResponse))?,
        );
        let a = self.intent(3, 1, condition.clone());
        let b = self.intent(3, 2, condition);
        let (ra, rb) = tokio::join!(session.put(&a), session.put(&b));
        let (ra, rb) = (ra?, rb?);
        let winner = match (&ra, &rb) {
            (Outcome::Applied(_), other) if other.conflict() => Some(&a),
            (other, Outcome::Applied(_)) if other.conflict() => Some(&b),
            _ => None,
        };
        let verified = match winner {
            Some(winner) => session.verify(winner).await?.is_some(),
            None => false,
        };
        check(
            checks,
            "concurrent-replace-one-winner",
            verified,
            format!("{}; {}", ra.description(), rb.description()),
        );

        let unicode = self.intent(4, 0, Condition::Missing);
        let result = session.put(&unicode).await?;
        let Outcome::Applied(revision) = result else {
            check(checks, "unicode-setup", false, result.description());
            return Ok(());
        };
        let updated = self.intent(
            4,
            1,
            Condition::Match(
                revision
                    .etag
                    .ok_or_else(|| Error::new("probe-revision", ErrorKind::InvalidResponse))?,
            ),
        );
        let result = session.put(&updated).await?;
        check(
            checks,
            "unicode-literal-key-replace",
            matches!(result, Outcome::Applied(_)) && session.verify(&updated).await?.is_some(),
            result.description(),
        );

        let empty = self.intent(5, 0, Condition::Missing);
        let result = session.put(&empty).await?;
        check(
            checks,
            "empty-object-round-trip",
            matches!(result, Outcome::Applied(_)) && session.verify(&empty).await?.is_some(),
            result.description(),
        );
        // Deliberately forget the successful receipt, then identify the exact
        // operation from current metadata plus a complete conditional data GET.
        check(
            checks,
            "receipt-independent-observation",
            session.verify(&updated).await?.is_some(),
            "marker, length and full SHA-256 match".into(),
        );
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}
fn check(checks: &mut Vec<Check>, name: &str, passed: bool, detail: String) {
    checks.push(Check {
        name: name.into(),
        passed,
        detail,
    });
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    version: u32,
    probe_version: String,
    observed_unix_seconds: u64,
    plan_digest: String,
    source_fingerprint: String,
    credential_generation: String,
    pub passed: bool,
    pub checks: Vec<Check>,
    pub errors: Vec<String>,
    pub remaining_keys: Vec<String>,
}
impl Report {
    /// Strict, bounded local evidence parsing. Parsing does not authorize any
    /// S3 mutation or imply that the local file is authenticated.
    pub fn from_toml(text: &str) -> Result<Self> {
        if text.len() > MAX_PLAN_BYTES {
            return Err(Error::new("probe-report-size", ErrorKind::InvalidInput));
        }
        toml::from_str(text).map_err(|_| Error::new("probe-report-format", ErrorKind::InvalidInput))
    }
    pub(crate) fn acceptance(&self, plan: &Plan) -> Result<(String, u64)> {
        if self.version != 1
            || self.probe_version != "conditional-put-v1"
            || self.observed_unix_seconds < plan.created_unix_seconds
            || self.plan_digest != plan.digest()?
            || self.source_fingerprint != plan.source_fingerprint
            || !is_hex(&self.credential_generation, 64)
            || !self.passed
            || !self.errors.is_empty()
            || !self.remaining_keys.is_empty()
            || self.checks.len() != CHECK_NAMES.len()
            || self.checks.iter().zip(CHECK_NAMES).any(|(c, name)| {
                c.name != name || !c.passed || c.detail.is_empty() || c.detail.len() > 1024
            })
        {
            return Err(Error::new("probe-acceptance", ErrorKind::InvalidInput));
        }
        Ok((
            self.credential_generation.clone(),
            self.observed_unix_seconds,
        ))
    }
    fn new(plan: &Plan, binding: String) -> Result<Self> {
        Ok(Self {
            version: 1,
            probe_version: "conditional-put-v1".into(),
            observed_unix_seconds: now()?,
            plan_digest: plan.digest()?,
            source_fingerprint: plan.source_fingerprint.clone(),
            credential_generation: binding,
            passed: false,
            checks: vec![],
            errors: vec![],
            remaining_keys: vec![],
        })
    }
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self)
            .map_err(|_| Error::new("probe-report", ErrorKind::InvalidResponse))
    }
}
fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .map_err(|_| Error::new("probe-clock", ErrorKind::Configuration))
}
fn payload(index: usize, variant: u8) -> Vec<u8> {
    if index == 5 {
        return vec![];
    }
    (0..4096)
        .map(|i| ((i * 29 + index * 17 + usize::from(variant) * 73) % 251) as u8)
        .collect()
}

#[cfg(test)]
pub(crate) mod tests;
