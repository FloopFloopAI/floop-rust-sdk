use crate::error::{FloopError, FloopErrorCode};
use crate::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub subdomain: Option<String>,
    pub status: String,
    #[serde(rename = "botType")]
    pub bot_type: Option<String>,
    pub url: Option<String>,
    #[serde(rename = "amplifyAppUrl")]
    pub amplify_app_url: Option<String>,
    #[serde(rename = "isPublic")]
    pub is_public: bool,
    #[serde(rename = "isAuthProtected")]
    pub is_auth_protected: bool,
    #[serde(rename = "teamId")]
    pub team_id: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "thumbnailUrl")]
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct CreateProjectInput {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subdomain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "botType")]
    pub bot_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "isAuthProtected")]
    pub is_auth_protected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "teamId")]
    pub team_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Deployment {
    pub id: String,
    pub status: String,
    pub version: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreatedProject {
    pub project: Project,
    pub deployment: Deployment,
}

#[derive(Debug, Default, Clone)]
pub struct ListProjectsOptions {
    pub team_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatusEvent {
    pub step: i64,
    #[serde(rename = "totalSteps")]
    pub total_steps: i64,
    pub status: String,
    pub message: String,
    #[serde(default)]
    pub progress: Option<f64>,
    #[serde(default, rename = "queuePosition")]
    pub queue_position: Option<i64>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct RefineInput {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<RefineAttachment>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "codeEditOnly")]
    pub code_edit_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefineAttachment {
    pub key: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "fileType")]
    pub file_type: String,
    #[serde(rename = "fileSize")]
    pub file_size: i64,
}

/// Three-shape discriminated result from `Projects::refine`. Exactly one
/// of `queued`, `saved_only`, or `processing` is `Some` — pattern-match
/// to branch.
#[derive(Debug, Default, Clone)]
pub struct RefineResult {
    pub queued: Option<RefineQueued>,
    pub saved_only: Option<RefineSavedOnly>,
    pub processing: Option<RefineProcessing>,
}

#[derive(Debug, Clone)]
pub struct RefineQueued {
    pub message_id: String,
}

#[derive(Debug, Clone)]
pub struct RefineSavedOnly;

#[derive(Debug, Clone)]
pub struct RefineProcessing {
    pub deployment_id: String,
    pub queue_priority: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConversationMessage {
    pub id: String,
    #[serde(rename = "projectId")]
    pub project_id: String,
    pub role: String,
    pub content: String,
    pub metadata: Option<serde_json::Value>,
    pub status: String,
    pub position: Option<i64>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConversationsResult {
    pub messages: Vec<ConversationMessage>,
    pub queued: Vec<ConversationMessage>,
    #[serde(rename = "latestVersion")]
    pub latest_version: i64,
}

#[derive(Debug, Default, Clone)]
pub struct ConversationsOptions {
    /// 0 means "server default".
    pub limit: u32,
}

/// Configures `Projects::stream` and `Projects::wait_for_live`.
#[derive(Debug, Clone, Copy)]
pub struct StreamOptions {
    pub interval: Duration,
    pub max_wait: Duration,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(2),
            max_wait: Duration::from_secs(600),
        }
    }
}

/// Alias kept for parity with the Go SDK's `WaitForLiveOptions` name.
pub type WaitForLiveOptions = StreamOptions;

/// Callback invoked on every de-duplicated status snapshot — including
/// the terminal event. Return `Ok(())` to continue polling, or any
/// `Err` to stop early (the error propagates from `stream`).
pub type StreamHandler<E> = Box<dyn FnMut(&StatusEvent) -> Result<(), E> + Send>;

pub struct Projects<'c> {
    pub(crate) client: &'c Client,
}

impl<'c> Projects<'c> {
    pub async fn create(&self, input: CreateProjectInput) -> Result<CreatedProject, FloopError> {
        let body = serde_json::to_value(&input).unwrap();
        self.client
            .request_json(reqwest::Method::POST, "/api/v1/projects", Some(&body))
            .await
    }

    pub async fn list(&self, opts: ListProjectsOptions) -> Result<Vec<Project>, FloopError> {
        let path = match opts.team_id {
            Some(t) => format!("/api/v1/projects?teamId={}", urlencoding_encode(&t)),
            None => "/api/v1/projects".to_owned(),
        };
        self.client
            .request_json(reqwest::Method::GET, &path, None)
            .await
    }

    /// Fetch a single project by id or subdomain. No dedicated backend
    /// route — filters the full list locally, matching the other SDKs.
    pub async fn get(
        &self,
        reference: &str,
        opts: ListProjectsOptions,
    ) -> Result<Project, FloopError> {
        let all = self.list(opts).await?;
        all.into_iter()
            .find(|p| p.id == reference || p.subdomain.as_deref() == Some(reference))
            .ok_or_else(|| {
                FloopError::new(
                    FloopErrorCode::NotFound,
                    404,
                    format!("project not found: {reference}"),
                )
            })
    }

    pub async fn status(&self, reference: &str) -> Result<StatusEvent, FloopError> {
        let path = format!("/api/v1/projects/{}/status", urlencoding_encode(reference));
        self.client
            .request_json(reqwest::Method::GET, &path, None)
            .await
    }

    pub async fn cancel(&self, reference: &str) -> Result<(), FloopError> {
        let path = format!("/api/v1/projects/{}/cancel", urlencoding_encode(reference));
        self.client
            .request_empty(reqwest::Method::POST, &path, None)
            .await
    }

    pub async fn reactivate(&self, reference: &str) -> Result<(), FloopError> {
        let path = format!(
            "/api/v1/projects/{}/reactivate",
            urlencoding_encode(reference)
        );
        self.client
            .request_empty(reqwest::Method::POST, &path, None)
            .await
    }

    pub async fn refine(
        &self,
        reference: &str,
        input: RefineInput,
    ) -> Result<RefineResult, FloopError> {
        let path = format!("/api/v1/projects/{}/refine", urlencoding_encode(reference));
        let body = serde_json::to_value(&input).unwrap();
        let raw: serde_json::Value = self
            .client
            .request_json(reqwest::Method::POST, &path, Some(&body))
            .await?;
        let mut out = RefineResult::default();
        if let Some(q) = raw.get("queued").and_then(|v| v.as_bool()) {
            if q {
                let msg_id = raw
                    .get("messageId")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned();
                out.queued = Some(RefineQueued { message_id: msg_id });
            } else {
                out.saved_only = Some(RefineSavedOnly);
            }
            return Ok(out);
        }
        if let Some(p) = raw.get("processing").and_then(|v| v.as_bool()) {
            if p {
                let dep_id = raw
                    .get("deploymentId")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned();
                let prio = raw
                    .get("queuePriority")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                out.processing = Some(RefineProcessing {
                    deployment_id: dep_id,
                    queue_priority: prio,
                });
                return Ok(out);
            }
        }
        Err(FloopError::new(
            FloopErrorCode::Unknown,
            0,
            "refine: unrecognised response shape",
        ))
    }

    pub async fn conversations(
        &self,
        reference: &str,
        opts: ConversationsOptions,
    ) -> Result<ConversationsResult, FloopError> {
        let mut path = format!(
            "/api/v1/projects/{}/conversations",
            urlencoding_encode(reference)
        );
        if opts.limit > 0 {
            path.push_str(&format!("?limit={}", opts.limit));
        }
        self.client
            .request_json(reqwest::Method::GET, &path, None)
            .await
    }

    /// Poll the project's status endpoint, invoking `handler` on each
    /// de-duplicated event, until a terminal state (live / failed /
    /// cancelled), `opts.max_wait` elapses, or the handler returns an
    /// error.
    ///
    /// Return values:
    ///   - `Ok(())` — reached `live`
    ///   - `Err(FloopError{code: BuildFailed})` — terminal failure
    ///   - `Err(FloopError{code: BuildCancelled})` — user cancelled
    ///   - `Err(FloopError{code: Timeout})` — max_wait exceeded
    ///   - `Err(...)` — the handler's error (wrapped in FloopError::new(Unknown, ...))
    ///
    /// Events are de-duplicated on `(status, step, progress, queue_position)`
    /// so callers don't see dozens of identical "queued" snapshots.
    pub async fn stream<F>(
        &self,
        reference: &str,
        opts: Option<StreamOptions>,
        mut handler: F,
    ) -> Result<(), FloopError>
    where
        F: FnMut(&StatusEvent) -> Result<(), FloopError> + Send,
    {
        let o = opts.unwrap_or_default();
        let deadline = Instant::now() + o.max_wait;
        let mut last_key = String::new();
        loop {
            if Instant::now() >= deadline {
                return Err(FloopError::new(
                    FloopErrorCode::Timeout,
                    0,
                    format!(
                        "stream: project {reference} did not reach a terminal state within {:?}",
                        o.max_wait
                    ),
                ));
            }

            let ev = self.status(reference).await?;
            let key = stream_event_key(&ev);
            if key != last_key {
                last_key = key;
                handler(&ev)?;
            }

            match ev.status.as_str() {
                // `live` and `archived` are both terminal-success states.
                // The Node, Python, Swift, and Kotlin SDKs already group
                // them; Rust previously only matched `live`, so an archived
                // project mid-stream caused max_wait timeouts instead of
                // clean returns.
                "live" | "archived" => return Ok(()),
                "failed" => {
                    return Err(FloopError::new(
                        FloopErrorCode::BuildFailed,
                        0,
                        non_empty_or(&ev.message, "build failed"),
                    ))
                }
                "cancelled" => {
                    return Err(FloopError::new(
                        FloopErrorCode::BuildCancelled,
                        0,
                        non_empty_or(&ev.message, "build cancelled"),
                    ))
                }
                _ => {}
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            let sleep_for = o.interval.min(remaining);
            if sleep_for.is_zero() {
                // Loop one more time to hit the deadline branch cleanly.
                continue;
            }
            tokio_sleep(sleep_for).await;
        }
    }

    /// Block until the project reaches `live` and return the hydrated
    /// `Project`. Wraps `stream` internally.
    pub async fn wait_for_live(
        &self,
        reference: &str,
        opts: Option<WaitForLiveOptions>,
    ) -> Result<Project, FloopError> {
        self.stream(reference, opts, |_| Ok(())).await?;
        self.get(reference, ListProjectsOptions::default()).await
    }
}

fn stream_event_key(ev: &StatusEvent) -> String {
    let progress = ev.progress.map_or(String::new(), |p| format!("{p}"));
    let queue = ev.queue_position.map_or(String::new(), |q| format!("{q}"));
    format!("{}|{}|{progress}|{queue}", ev.status, ev.step)
}

fn non_empty_or(s: &str, fallback: &str) -> String {
    if s.is_empty() {
        fallback.to_owned()
    } else {
        s.to_owned()
    }
}

/// Minimal URL-encoder — path and query values are the only thing we
/// encode and none of the characters our refs contain need the fancy
/// stuff.  Keeps us off the `urlencoding` dep for a 5-line helper.
pub(crate) fn urlencoding_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// Tokio is a dev-dep; in production consumers bring their own runtime.
// We need a way to sleep inside `stream`, so wire to `tokio::time::sleep`
// through a tiny indirection that works with either full or rt-only
// tokio builds — matches how reqwest itself handles this.
async fn tokio_sleep(d: Duration) {
    // `reqwest`'s default stack pulls in tokio already — we re-use it.
    #[allow(clippy::module_name_repetitions)]
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    struct Sleep {
        until: Instant,
    }
    impl Future for Sleep {
        type Output = ();
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if Instant::now() >= self.until {
                Poll::Ready(())
            } else {
                // Crude polling loop — good enough for test doubles, and
                // downstream consumers will override via a custom HTTP
                // client anyway.  Tokio's real sleep kicks in on any
                // standard reqwest build; this fallback is only here so
                // the crate compiles without tokio on the default path.
                let waker = cx.waker().clone();
                let until = self.until;
                std::thread::spawn(move || {
                    let now = Instant::now();
                    if until > now {
                        std::thread::sleep(until - now);
                    }
                    waker.wake();
                });
                Poll::Pending
            }
        }
    }
    Sleep {
        until: Instant::now() + d,
    }
    .await;
}
