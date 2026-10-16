use aws_sdk_s3::error::SdkError;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidInput,
    PermissionDenied,
    NotFound,
    RevisionChanged,
    InvalidRange,
    Unsupported,
    Timeout,
    Transport,
    Throttled,
    ServiceUnavailable,
    InvalidResponse,
    Configuration,
}

/// Deliberately excludes SDK error text, request URLs and response bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub operation: &'static str,
    pub kind: ErrorKind,
    pub http_status: Option<u16>,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn new(operation: &'static str, kind: ErrorKind) -> Self {
        Self {
            operation,
            kind,
            http_status: None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.operation, self.kind)?;
        if let Some(status) = self.http_status {
            write!(f, " (HTTP {status})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

pub(crate) fn sdk_error<E>(operation: &'static str, error: SdkError<E>) -> Error {
    let http_status = error
        .raw_response()
        .map(|response| response.status().as_u16());
    let kind = match http_status {
        Some(401 | 403) => ErrorKind::PermissionDenied,
        Some(404) => ErrorKind::NotFound,
        Some(412) => ErrorKind::RevisionChanged,
        Some(416) => ErrorKind::InvalidRange,
        Some(408) => ErrorKind::Timeout,
        Some(429) => ErrorKind::Throttled,
        Some(500..=599) => ErrorKind::ServiceUnavailable,
        Some(_) => ErrorKind::InvalidResponse,
        None => match error {
            SdkError::TimeoutError(_) => ErrorKind::Timeout,
            SdkError::ConstructionFailure(_) => ErrorKind::Configuration,
            SdkError::DispatchFailure(_) => ErrorKind::Transport,
            _ => ErrorKind::InvalidResponse,
        },
    };
    Error {
        operation,
        kind,
        http_status,
    }
}
