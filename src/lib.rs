//! Official Rust SDK for the [FloopFloop](https://www.floopfloop.com) API.
//!
//! # Quickstart
//!
//! ```no_run
//! use floopfloop::{Client, CreateProjectInput};
//!
//! # async fn main_() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Client::new(std::env::var("FLOOP_API_KEY")?)?;
//!
//! let created = client.projects().create(CreateProjectInput {
//!     prompt: "A landing page for a cat cafe".into(),
//!     subdomain: Some("cat-cafe".into()),
//!     bot_type: Some("site".into()),
//!     ..Default::default()
//! }).await?;
//!
//! let live = client.projects().wait_for_live(&created.project.id, None).await?;
//! println!("Live at: {}", live.url.unwrap_or_default());
//! # Ok(()) }
//! ```
//!
//! # Resources
//!
//! * [`projects`](Projects) — create / list / get / status / refine / cancel / reactivate / conversations / stream / wait_for_live
//! * [`subdomains`](Subdomains) — check / suggest
//! * [`secrets`](Secrets) — list / set / remove
//! * [`library`](Library) — list / clone
//! * [`usage`](Usage) — summary
//! * [`api_keys`](ApiKeys) — list / create / remove (remove accepts id OR name)
//! * [`uploads`](Uploads) — create (S3 presign + direct PUT)
//! * [`user`](UserApi) — me
//!
//! Every resource method returns [`Result<T, FloopError>`] on failure.
//! `FloopError.code` is a [`FloopErrorCode`] enum with a catch-all `Other`
//! variant so unknown server codes still round-trip losslessly.

#![deny(unsafe_code)]

mod api_keys;
mod error;
mod library;
mod projects;
mod secrets;
mod subdomains;
mod subscriptions;
mod uploads;
mod usage;
mod user;

pub use api_keys::{ApiKeySummary, ApiKeys, CreateApiKeyInput, IssuedApiKey};
pub use error::{FloopError, FloopErrorCode};
pub use library::{
    CloneLibraryProjectInput, ClonedProject, Library, LibraryListOptions, LibraryProject,
};
pub use projects::{
    ConversationMessage, ConversationsOptions, ConversationsResult, CreateProjectInput,
    CreatedProject, Deployment, ListProjectsOptions, Project, Projects, RefineAttachment,
    RefineInput, RefineResult, StatusEvent, StreamHandler, StreamOptions, WaitForLiveOptions,
};
pub use secrets::{SecretSummary, Secrets};
pub use subdomains::{SubdomainCheckResult, SubdomainSuggestResult, Subdomains};
pub use subscriptions::{
    CurrentSubscription, SubscriptionCredits, SubscriptionPlan, Subscriptions,
};
pub use uploads::{CreateUploadInput, UploadedAttachment, Uploads, MAX_UPLOAD_BYTES};
pub use usage::{Usage, UsageSummary};
pub use user::{User, UserApi};

use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::Duration;

/// Library semver, kept in sync with the latest `v*` git tag.
pub const VERSION: &str = "0.1.0-alpha.4";

const DEFAULT_BASE_URL: &str = "https://www.floopfloop.com";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// The main entry point. Construct once with [`Client::new`] or
/// [`Client::builder`] and reuse across tasks; all methods are `&self`.
///
/// Cheap to clone — internally wraps an `Arc` so clones share the same
/// HTTP connection pool.
#[derive(Clone)]
pub struct Client {
    inner: Arc<ClientInner>,
}

struct ClientInner {
    api_key: String,
    base_url: String,
    http: reqwest::Client,
    user_agent: String,
}

/// Configuration for [`Client::builder`]. Defaults:
///
/// * `base_url` = `https://www.floopfloop.com`
/// * `timeout` = 30s
/// * `user_agent` = `floopfloop-rust-sdk/<version>`
#[must_use]
pub struct ClientBuilder {
    api_key: String,
    base_url: String,
    timeout: Duration,
    user_agent_suffix: Option<String>,
    http: Option<reqwest::Client>,
}

impl Client {
    /// Shortcut for `Client::builder(api_key).build()` — panics on bad
    /// input (empty key, http client init failure). Prefer `builder`
    /// for customised setups.
    pub fn new(api_key: impl Into<String>) -> Result<Self, FloopError> {
        Self::builder(api_key).build()
    }

    pub fn builder(api_key: impl Into<String>) -> ClientBuilder {
        ClientBuilder {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            timeout: DEFAULT_TIMEOUT,
            user_agent_suffix: None,
            http: None,
        }
    }

    // ── Resource accessors ──────────────────────────────────────────

    pub fn projects(&self) -> Projects<'_> {
        Projects { client: self }
    }
    pub fn subdomains(&self) -> Subdomains<'_> {
        Subdomains { client: self }
    }
    pub fn secrets(&self) -> Secrets<'_> {
        Secrets { client: self }
    }
    pub fn library(&self) -> Library<'_> {
        Library { client: self }
    }
    pub fn usage(&self) -> Usage<'_> {
        Usage { client: self }
    }
    pub fn subscriptions(&self) -> Subscriptions<'_> {
        Subscriptions { client: self }
    }
    pub fn api_keys(&self) -> ApiKeys<'_> {
        ApiKeys { client: self }
    }
    pub fn uploads(&self) -> Uploads<'_> {
        Uploads { client: self }
    }
    pub fn user(&self) -> UserApi<'_> {
        UserApi { client: self }
    }

    pub fn base_url(&self) -> &str {
        &self.inner.base_url
    }

    // ── Internal transport ──────────────────────────────────────────

    pub(crate) async fn request_json<O: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<O, FloopError> {
        let text = self.request_text(method, path, body).await?;
        serde_json::from_str(&text).map_err(|e| {
            FloopError::new(
                FloopErrorCode::Unknown,
                0,
                format!("failed to decode response: {e}"),
            )
        })
    }

    pub(crate) async fn request_empty(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<(), FloopError> {
        let _ = self.request_text(method, path, body).await?;
        Ok(())
    }

    /// Raw request helper used by upload's two-step flow (returns the
    /// underlying `http` client so callers can PUT binary bodies
    /// directly to S3).
    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.inner.http
    }

    async fn request_text(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<String, FloopError> {
        let url = format!("{}{}", self.inner.base_url, path);
        let mut req = self
            .inner
            .http
            .request(method, &url)
            .bearer_auth(&self.inner.api_key)
            .header(reqwest::header::USER_AGENT, &self.inner.user_agent)
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(b) = body {
            req = req.json(b);
        }

        let resp = req.send().await.map_err(|err| {
            let code = if err.is_timeout() {
                FloopErrorCode::Timeout
            } else {
                FloopErrorCode::NetworkError
            };
            let msg = if err.is_timeout() {
                "request timed out".to_owned()
            } else {
                format!("could not reach {} ({})", self.inner.base_url, err)
            };
            FloopError::new(code, 0, msg)
        })?;

        let status = resp.status();
        let request_id = resp
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| error::parse_retry_after(Some(s)));

        let raw = resp.text().await.map_err(|err| {
            FloopError::new(
                FloopErrorCode::NetworkError,
                status.as_u16(),
                format!("failed to read response: {err}"),
            )
        })?;

        if !status.is_success() {
            let (code, message) = parse_error_envelope(&raw, status);
            let mut fe = FloopError::new(code, status.as_u16(), message);
            fe.request_id = request_id;
            fe.retry_after = retry_after;
            return Err(fe);
        }

        // Unwrap the {data: ...} envelope when present so callers
        // deserialize the inner shape directly.
        let unwrapped = unwrap_data_envelope(&raw);
        Ok(unwrapped.unwrap_or(raw))
    }
}

impl ClientBuilder {
    /// Override the base URL (e.g. for staging or a local dev server).
    /// Trailing slashes are stripped.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        let mut s = url.into();
        while s.ends_with('/') {
            s.pop();
        }
        self.base_url = s;
        self
    }

    /// Set the per-request timeout. Ignored if `http_client` is also
    /// supplied (configure the timeout on your custom client yourself).
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    /// Append a suffix to the User-Agent header (after
    /// `floopfloop-rust-sdk/<version>`).
    pub fn user_agent_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.user_agent_suffix = Some(suffix.into());
        self
    }

    /// Supply a caller-built `reqwest::Client`. Overrides `.timeout()`
    /// — configure the timeout on the custom client yourself.
    pub fn http_client(mut self, http: reqwest::Client) -> Self {
        self.http = Some(http);
        self
    }

    pub fn build(self) -> Result<Client, FloopError> {
        if self.api_key.is_empty() {
            return Err(FloopError::new(
                FloopErrorCode::ValidationError,
                0,
                "api_key is required",
            ));
        }
        let http = match self.http {
            Some(h) => h,
            None => reqwest::Client::builder()
                .timeout(self.timeout)
                .build()
                .map_err(|e| {
                    FloopError::new(FloopErrorCode::Unknown, 0, format!("reqwest init: {e}"))
                })?,
        };
        let user_agent = match self.user_agent_suffix {
            Some(s) => format!("floopfloop-rust-sdk/{VERSION} {s}"),
            None => format!("floopfloop-rust-sdk/{VERSION}"),
        };
        Ok(Client {
            inner: Arc::new(ClientInner {
                api_key: self.api_key,
                base_url: self.base_url,
                http,
                user_agent,
            }),
        })
    }
}

fn default_code_for_status(status: StatusCode) -> FloopErrorCode {
    match status.as_u16() {
        401 => FloopErrorCode::Unauthorized,
        403 => FloopErrorCode::Forbidden,
        404 => FloopErrorCode::NotFound,
        409 => FloopErrorCode::Conflict,
        422 => FloopErrorCode::ValidationError,
        429 => FloopErrorCode::RateLimited,
        503 => FloopErrorCode::ServiceUnavailable,
        s if s >= 500 => FloopErrorCode::ServerError,
        _ => FloopErrorCode::Unknown,
    }
}

fn parse_error_envelope(raw: &str, status: StatusCode) -> (FloopErrorCode, String) {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(err) = v.get("error").and_then(|e| e.as_object()) {
            let code = err.get("code").and_then(|c| c.as_str()).map_or_else(
                || default_code_for_status(status),
                FloopErrorCode::from_wire,
            );
            let msg = err.get("message").and_then(|m| m.as_str()).map_or_else(
                || format!("request failed ({})", status.as_u16()),
                ToOwned::to_owned,
            );
            return (code, msg);
        }
    }
    (
        default_code_for_status(status),
        format!("request failed ({})", status.as_u16()),
    )
}

/// Unwrap `{"data": ...}` envelopes. Returns the inner JSON text as a
/// string, or `None` if the body doesn't match the envelope shape (in
/// which case the caller should use the raw body as-is).
fn unwrap_data_envelope(raw: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let inner = v.as_object()?.get("data")?;
    Some(inner.to_string())
}
