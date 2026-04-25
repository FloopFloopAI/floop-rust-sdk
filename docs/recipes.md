# Cookbook

Concrete `floopfloop` (crates.io) patterns you can copy-paste. Every snippet uses only the SDK's public surface — no undocumented endpoints, no private helpers.

For the basics (install, client setup, resource tour) see the [README](../README.md). This file is the **"I know the basics, now how do I actually build X"** layer.

These recipes mirror the [Node](https://github.com/FloopFloopAI/floop-node-sdk/blob/main/docs/recipes.md), [Python](https://github.com/FloopFloopAI/floop-python-sdk/blob/main/docs/recipes.md), and [Go](https://github.com/FloopFloopAI/floop-go-sdk/blob/main/docs/recipes.md) cookbooks, translated to async-Rust idioms (tokio runtime, `match` on `FloopErrorCode`, callback-based `stream`, generic backoff helper).

All snippets assume:

```toml
# Cargo.toml
[dependencies]
floopfloop = "0.1"
tokio = { version = "1", features = ["full"] }
```

---

## 1. Ship a project from prompt to live URL

The canonical one-call flow: create, wait, done. `wait_for_live` returns `FloopError { code: FloopErrorCode::BuildFailed | BuildCancelled | Timeout, .. }` on non-success terminals, so plain `match` is enough.

```rust
use floopfloop::{Client, CreateProjectInput, FloopErrorCode, StreamOptions};
use std::error::Error;
use std::time::Duration;

async fn ship(client: &Client, prompt: &str, subdomain: &str) -> Result<String, Box<dyn Error>> {
    let created = client.projects().create(CreateProjectInput {
        prompt: prompt.into(),
        subdomain: Some(subdomain.into()),
        bot_type: Some("site".into()),
        ..Default::default()
    }).await?;

    // Polls status every 2s; bounds the total wait to 10 minutes so a
    // stuck build doesn't hang forever.
    let live = client.projects().wait_for_live(
        &created.project.id,
        Some(StreamOptions {
            interval: Duration::from_secs(2),
            max_wait: Duration::from_secs(10 * 60),
        }),
    ).await?;

    // FloopError's constructor is pub(crate), so application code uses
    // its own error type at the boundary. Box<dyn Error> + a string
    // literal is the lowest-overhead choice.
    live.url.ok_or_else(|| "project is live but has no URL yet".into())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(std::env::var("FLOOP_API_KEY")?)?;
    match ship(&client, "A single-page portfolio for a landscape photographer", "landscape-portfolio").await {
        Ok(url) => println!("Live at {url}"),
        Err(e) if matches!(e.code, FloopErrorCode::BuildFailed) => {
            eprintln!("build failed: {}", e.message);
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
```

**`#[non_exhaustive]` on `FloopErrorCode`** means you must include a `_` wildcard arm in any `match` against it — new variants can ship in a non-breaking minor release. Use `matches!` (or `if let`) when you only care about a specific variant.

**When to prefer `stream` over `wait_for_live`:** when you want to show progress to a user. `wait_for_live` only returns at the end — no visibility into what the build is doing.

---

## 2. Watch a build progress in real time

`projects().stream(reference, opts, handler)` calls `handler` for every unique status transition and returns when the project reaches a terminal state (live / failed / cancelled), `max_wait` elapses, or the handler returns `Err`. Events are de-duplicated on `(status, step, progress, queue_position)` so the handler doesn't fire on every poll.

```rust
use floopfloop::{Client, FloopError, FloopErrorCode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(std::env::var("FLOOP_API_KEY")?)?;

    let result = client.projects().stream("recipe-blog", None, |ev| {
        let progress = ev.progress
            .map(|p| format!(" {p:.0}%"))
            .unwrap_or_default();
        println!("[{}]{} step={}/{} — {}",
            ev.status, progress, ev.step, ev.total_steps, ev.message);
        Ok(())  // return Err to stop polling early
    }).await;

    match result {
        Ok(()) => {
            // Reached "live" cleanly — fetch the hydrated project.
            let done = client.projects().get("recipe-blog", Default::default()).await?;
            if let Some(url) = done.url {
                println!("Live at {url}");
            }
        }
        Err(e) if matches!(e.code, FloopErrorCode::BuildFailed) => {
            eprintln!("build failed: {}", e.message);
        }
        Err(e) if matches!(e.code, FloopErrorCode::Timeout) => {
            eprintln!("build stalled past max_wait");
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
```

**Early abort via shared state.** The handler returns `Result<(), FloopError>`, but the `FloopError` constructor is `pub(crate)` — only the SDK can build one. So sentinel-error returns from your handler aren't possible today. Use shared state captured in the closure to signal "stop" instead, and let the loop run to completion:

```rust
use std::cell::Cell;
let seen_enough = Cell::new(false);
let result = client.projects().stream("recipe-blog", None, |ev| {
    if let Some(p) = ev.progress {
        if p >= 50.0 {
            seen_enough.set(true);
        }
    }
    Ok(())
}).await;
if seen_enough.get() {
    // act on the early signal
}
```

This is a known papercut — there's an open question about exposing a public `FloopError::new` so handlers can short-circuit cleanly.

---

## 3. Refine a project, even when it's mid-build

`projects().refine` returns a `RefineResult` with three `Option` fields — exactly one is `Some`:

- `queued: Some(RefineQueued { message_id })` — project is currently deploying; your message is queued.
- `processing: Some(RefineProcessing { deployment_id, queue_priority })` — your message triggered a new build immediately.
- `saved_only: Some(RefineSavedOnly)` — saved as a conversation entry without triggering a build.

```rust
use floopfloop::{Client, RefineInput};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(std::env::var("FLOOP_API_KEY")?)?;

    let res = client.projects().refine("recipe-blog", RefineInput {
        message: "Add a search bar to the header".into(),
        ..Default::default()
    }).await?;

    if let Some(p) = res.processing {
        println!("Build started (deployment {})", p.deployment_id);
        client.projects().wait_for_live("recipe-blog", None).await?;
    } else if let Some(q) = res.queued {
        println!("Queued behind current build (message {})", q.message_id);
        // Poll once — when "live", your queued message has been processed.
        client.projects().wait_for_live("recipe-blog", None).await?;
    } else if res.saved_only.is_some() {
        println!("Saved as a chat message, no build triggered");
    }
    Ok(())
}
```

**Why three `Option` fields instead of an enum?** Rust's enum would be the natural fit, but `serde` deserialisation against the JSON shape is cleaner with the three-`Option` layout — the fields exactly mirror what's on the wire. The SDK guarantees exactly one is `Some` on success.

---

## 4. Upload an image and refine with it as context

Uploads are two-step: `uploads().create` presigns an S3 URL and does the direct PUT, returning an `UploadedAttachment`. **There's a type-shape gotcha:** `RefineInput.attachments` is `Vec<RefineAttachment>`, not `Vec<UploadedAttachment>`. The fields are the same but `file_size` is `u64` on one and `i64` on the other — needs a cast.

```rust
use bytes::Bytes;
use floopfloop::{Client, CreateUploadInput, RefineAttachment, RefineInput, UploadedAttachment};
use tokio::fs;

fn into_refine_attachment(up: UploadedAttachment) -> RefineAttachment {
    RefineAttachment {
        key: up.key,
        file_name: up.file_name,
        file_type: up.file_type,
        file_size: up.file_size as i64,  // u64 -> i64
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(std::env::var("FLOOP_API_KEY")?)?;

    let bytes = fs::read("./mockup.png").await?;
    let up = client.uploads().create(CreateUploadInput {
        file_name: "mockup.png".into(),
        bytes: Bytes::from(bytes),
        file_type: None,  // None = guess from extension
    }).await?;

    client.projects().refine("recipe-blog", RefineInput {
        message: "Make the homepage look like this mockup.".into(),
        attachments: Some(vec![into_refine_attachment(up)]),
        ..Default::default()
    }).await?;
    Ok(())
}
```

**Supported types:** `png`, `jpg/jpeg`, `gif`, `svg`, `webp`, `ico`, `pdf`, `txt`, `csv`, `doc`, `docx`. Max 5 MB per upload. The SDK validates client-side before hitting the network, so bad inputs return `FloopError { code: ValidationError, .. }` with no round-trip.

Attachments only flow through `refine` today — `create` doesn't accept them via the SDK. If you need to anchor a brand-new project against images, create with a prompt first, then refine with the attachments as a follow-up.

---

## 5. Rotate an API key from a CI job

Three-step rotation: create the new key, write it to your secret store, then revoke the old one. The order matters — you must revoke with a **different** key than the one making the call (the backend returns `400 ValidationError` if you try to revoke the key you're authenticated with).

```rust
use floopfloop::{Client, CreateApiKeyInput};

async fn rotate(victim_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Use a long-lived bootstrap key (stored as a CI secret) to do the
    // rotation. Don't use the key we're about to revoke — that hits the
    // self-revoke guard.
    let bootstrap = Client::new(std::env::var("FLOOP_BOOTSTRAP_KEY")?)?;

    // 1. Find the key we want to rotate by its name. (Each name is
    //    unique per account because the dashboard enforces it; matching
    //    by name is more reliable than matching the prefix substring.)
    let keys = bootstrap.api_keys().list().await?;
    let victim = keys.iter()
        .find(|k| k.name == victim_name)
        .ok_or_else(|| format!("key not found: {victim_name}"))?;

    // 2. Mint the replacement.
    let fresh = bootstrap.api_keys().create(CreateApiKeyInput {
        name: format!("{victim_name}-new"),
    }).await?;
    write_secret("FLOOP_API_KEY", &fresh.raw_key).await?;

    // 3. Revoke the old one. remove() accepts an id OR a name.
    bootstrap.api_keys().remove(&victim.id).await?;
    Ok(())
}

async fn write_secret(_name: &str, _value: &str) -> Result<(), Box<dyn std::error::Error>> {
    // wire into your CI secret store — AWS Secrets Manager, Vault,
    // GitHub Actions `gh secret set`, etc.
    Ok(())
}
```

**Can't I just reuse the bootstrap key forever?** Technically yes — if it's tightly scoped and audited. In practice, a single long-lived "rotator key" is a common compromise: it only has permission to mint/list/revoke keys, never appears in application traffic, and itself gets rotated manually on a rare cadence (annually, or on compromise).

The 5-keys-per-account cap applies to active keys, so make sure to revoke old rotations rather than accumulating them.

---

## 6. Retry with backoff on `RateLimited` and `NetworkError`

`FloopError` carries everything you need to implement backoff correctly:

- `retry_after: Option<Duration>` — populated from the `Retry-After` header on 429s (parsed from delta-seconds OR HTTP-date).
- `code: FloopErrorCode` — distinguishes retryable (`RateLimited`, `NetworkError`, `Timeout`, `ServiceUnavailable`, `ServerError`) from permanent (`Unauthorized`, `Forbidden`, `ValidationError`, `NotFound`, `Conflict`, `BuildFailed`, `BuildCancelled`).

```rust
use floopfloop::{FloopError, FloopErrorCode};
use std::future::Future;
use std::time::Duration;

fn is_retryable(code: &FloopErrorCode) -> bool {
    matches!(
        code,
        FloopErrorCode::RateLimited
            | FloopErrorCode::NetworkError
            | FloopErrorCode::Timeout
            | FloopErrorCode::ServiceUnavailable
            | FloopErrorCode::ServerError
    )
}

pub async fn with_retry<F, Fut, T>(max_attempts: u32, mut fn_: F) -> Result<T, FloopError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, FloopError>>,
{
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        match fn_().await {
            Ok(v) => return Ok(v),
            Err(e) if !is_retryable(&e.code) || attempt >= max_attempts => return Err(e),
            Err(e) => {
                // Prefer the server's hint; fall back to exponential backoff
                // with jitter capped at 30 s.
                let server_hint = e.retry_after;
                let expo = Duration::from_millis(
                    (250u64 << attempt.min(7)).min(30_000),
                );
                let jitter = Duration::from_millis(rand_jitter_ms());
                let wait = server_hint.unwrap_or(expo) + jitter;

                eprintln!(
                    "floop: {} (attempt {}/{}), retrying in {:?}{}",
                    e.code.as_str(),
                    attempt,
                    max_attempts,
                    wait,
                    e.request_id.as_deref().map(|r| format!(" — request {r}")).unwrap_or_default(),
                );
                tokio::time::sleep(wait).await;
            }
        }
    }
}

// Avoid a `rand` dep just for this — std-only nanos jitter is fine.
fn rand_jitter_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 % 250)
        .unwrap_or(50)
}

// Usage:
//   let projects = with_retry(5, || async {
//       client.projects().list(Default::default()).await
//   }).await?;
```

**Don't retry everything.** `ValidationError`, `Unauthorized`, and `Forbidden` are not going to fix themselves between attempts — retrying them just burns rate-limit budget and delays the real error reaching your logs.

**Cancellation.** If you need a hard ceiling on the whole retry loop, wrap the call in `tokio::time::timeout`:

```rust
let result = tokio::time::timeout(
    Duration::from_secs(60),
    with_retry(5, || async { client.projects().list(Default::default()).await }),
).await;
```

The outer `timeout` aborts the in-flight retry sleep cleanly because `tokio::time::sleep` is cancellation-safe.

---

## Got a pattern worth adding?

Open an issue at [FloopFloopAI/floop-rust-sdk/issues](https://github.com/FloopFloopAI/floop-rust-sdk/issues) describing the use case. Recipes live in this file, not in `src/`, so they're easy to update without an SDK release.
