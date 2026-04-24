//! Error types for the FloopFloop SDK.

use std::time::Duration;

/// Error codes returned by the FloopFloop API plus a few the SDK itself
/// produces for transport-level failures. Unknown server codes pass
/// through verbatim in `FloopError::code` so callers can handle new
/// codes without an SDK update.
///
/// Mirrors the Node / Python / Go SDK error taxonomy byte-for-byte.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloopErrorCode {
    Unauthorized,
    Forbidden,
    ValidationError,
    RateLimited,
    NotFound,
    Conflict,
    ServiceUnavailable,
    ServerError,
    NetworkError,
    Timeout,
    BuildFailed,
    BuildCancelled,
    Unknown,
    /// Any server code the SDK doesn't have a dedicated variant for.
    Other(String),
}

impl FloopErrorCode {
    /// The canonical uppercase string used on the wire and in the other
    /// SDKs.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden => "FORBIDDEN",
            Self::ValidationError => "VALIDATION_ERROR",
            Self::RateLimited => "RATE_LIMITED",
            Self::NotFound => "NOT_FOUND",
            Self::Conflict => "CONFLICT",
            Self::ServiceUnavailable => "SERVICE_UNAVAILABLE",
            Self::ServerError => "SERVER_ERROR",
            Self::NetworkError => "NETWORK_ERROR",
            Self::Timeout => "TIMEOUT",
            Self::BuildFailed => "BUILD_FAILED",
            Self::BuildCancelled => "BUILD_CANCELLED",
            Self::Unknown => "UNKNOWN",
            Self::Other(s) => s,
        }
    }

    pub(crate) fn from_wire(s: &str) -> Self {
        match s {
            "UNAUTHORIZED" => Self::Unauthorized,
            "FORBIDDEN" => Self::Forbidden,
            "VALIDATION_ERROR" => Self::ValidationError,
            "RATE_LIMITED" => Self::RateLimited,
            "NOT_FOUND" => Self::NotFound,
            "CONFLICT" => Self::Conflict,
            "SERVICE_UNAVAILABLE" => Self::ServiceUnavailable,
            "SERVER_ERROR" => Self::ServerError,
            "NETWORK_ERROR" => Self::NetworkError,
            "TIMEOUT" => Self::Timeout,
            "BUILD_FAILED" => Self::BuildFailed,
            "BUILD_CANCELLED" => Self::BuildCancelled,
            "UNKNOWN" => Self::Unknown,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// The one error type every SDK call returns on failure. Pattern-match
/// on `.code` to branch:
///
/// ```no_run
/// # use floopfloop::{Client, FloopErrorCode};
/// # async fn example(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
/// match client.projects().status("p_1").await {
///     Ok(ev) => println!("status: {}", ev.status),
///     Err(e) if e.code == FloopErrorCode::RateLimited => {
///         if let Some(d) = e.retry_after { tokio::time::sleep(d).await; }
///     }
///     Err(e) => return Err(Box::new(e)),
/// }
/// # Ok(()) }
/// ```
#[derive(Debug, thiserror::Error)]
pub struct FloopError {
    pub code: FloopErrorCode,
    pub status: u16,
    pub message: String,
    pub request_id: Option<String>,
    pub retry_after: Option<Duration>,
}

impl std::fmt::Display for FloopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "floop: [{}", self.code.as_str())?;
        if self.status != 0 {
            write!(f, " {}", self.status)?;
        }
        write!(f, "] {}", self.message)?;
        if let Some(ref id) = self.request_id {
            write!(f, " (request {id})")?;
        }
        Ok(())
    }
}

impl FloopError {
    pub(crate) fn new(code: FloopErrorCode, status: u16, message: impl Into<String>) -> Self {
        Self {
            code,
            status,
            message: message.into(),
            request_id: None,
            retry_after: None,
        }
    }
}

/// Parses a `Retry-After` header value per RFC 7231 — accepts either
/// `delta-seconds` or an HTTP-date.  Returns `None` if the header is
/// empty or unparseable, matching the other SDKs.
pub(crate) fn parse_retry_after(header: Option<&str>) -> Option<Duration> {
    let raw = header?;
    if let Ok(secs) = raw.parse::<f64>() {
        if secs < 0.0 {
            return None;
        }
        return Some(Duration::from_millis((secs * 1000.0) as u64));
    }
    if let Ok(when) = httpdate::parse_http_date(raw) {
        if let Ok(delta) = when.duration_since(std::time::SystemTime::now()) {
            return Some(delta);
        }
        return Some(Duration::ZERO);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_delta_seconds() {
        assert_eq!(parse_retry_after(Some("5")), Some(Duration::from_secs(5)),);
        assert_eq!(
            parse_retry_after(Some("1.5")),
            Some(Duration::from_millis(1500)),
        );
        assert_eq!(parse_retry_after(Some("-1")), None);
        assert_eq!(parse_retry_after(Some("")), None);
        assert_eq!(parse_retry_after(None), None);
    }

    #[test]
    fn retry_after_http_date() {
        // A fixed date in the past → returns Duration::ZERO.
        let past = "Wed, 21 Oct 2015 07:28:00 GMT";
        assert_eq!(parse_retry_after(Some(past)), Some(Duration::ZERO));
    }

    #[test]
    fn error_code_roundtrip() {
        for wire in ["RATE_LIMITED", "NETWORK_ERROR", "UNKNOWN", "WEIRD_NEW_CODE"] {
            let parsed = FloopErrorCode::from_wire(wire);
            assert_eq!(parsed.as_str(), wire);
        }
    }

    #[test]
    fn error_display_format() {
        let mut err = FloopError::new(FloopErrorCode::RateLimited, 429, "slow");
        err.request_id = Some("r1".into());
        assert_eq!(
            err.to_string(),
            "floop: [RATE_LIMITED 429] slow (request r1)"
        );

        let err = FloopError::new(FloopErrorCode::NetworkError, 0, "boom");
        assert_eq!(err.to_string(), "floop: [NETWORK_ERROR] boom");
    }
}
