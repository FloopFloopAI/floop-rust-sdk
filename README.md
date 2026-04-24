# floopfloop

[![crates.io](https://img.shields.io/crates/v/floopfloop?logo=rust)](https://crates.io/crates/floopfloop)
[![docs.rs](https://img.shields.io/docsrs/floopfloop?logo=docs.rs)](https://docs.rs/floopfloop)
[![CI](https://img.shields.io/github/actions/workflow/status/FloopFloopAI/floop-rust-sdk/ci.yml?branch=main&logo=github&label=ci)](https://github.com/FloopFloopAI/floop-rust-sdk/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/crates/l/floopfloop)](./LICENSE)
[![Rust edition](https://img.shields.io/badge/edition-2021-orange?logo=rust)](https://doc.rust-lang.org/edition-guide/)

Official Rust SDK for the [FloopFloop](https://www.floopfloop.com) API. Build a project, refine it, manage secrets and subdomains from any async-Rust codebase.

## Install

```bash
cargo add floopfloop
```

Or in `Cargo.toml`:

```toml
[dependencies]
floopfloop = "0.1.0-alpha.1"
# Runtime — the SDK doesn't bundle one; bring tokio yourself:
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

## Quickstart

Grab an API key: `floop keys create my-sdk` (via the [floop CLI](https://github.com/FloopFloopAI/floop-cli)) or the dashboard → Account → API Keys. Business plan required to mint new keys.

```rust
use floopfloop::{Client, CreateProjectInput};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(std::env::var("FLOOP_API_KEY")?)?;

    // Create a project and wait for it to go live.
    let created = client.projects().create(CreateProjectInput {
        prompt: "A landing page for a cat cafe with a sign-up form".into(),
        name: Some("Cat Cafe".into()),
        subdomain: Some("cat-cafe".into()),
        bot_type: Some("site".into()),
        ..Default::default()
    }).await?;

    let live = client.projects().wait_for_live(&created.project.id, None).await?;
    println!("Live at: {}", live.url.unwrap_or_default());
    Ok(())
}
```

## Streaming progress

```rust
use floopfloop::{FloopErrorCode, StreamOptions};
use std::time::Duration;

let res = client.projects().stream(
    &created.project.id,
    Some(StreamOptions { interval: Duration::from_secs(2), max_wait: Duration::from_secs(600) }),
    |ev| {
        println!("{} ({}/{}) — {}", ev.status, ev.step, ev.total_steps, ev.message);
        Ok(()) // return an Err to stop polling early
    },
).await;

match res {
    Ok(()) => println!("live!"),
    Err(e) if e.code == FloopErrorCode::BuildFailed => eprintln!("build failed: {}", e.message),
    Err(e) => return Err(e.into()),
}
```

`stream` de-duplicates identical consecutive snapshots (same status / step / progress / queue_position) so you don't see dozens of identical "queued" events while a build waits — matches the Node, Python, and Go SDKs.

## Error handling

Every call returns `Result<T, floopfloop::FloopError>`. `FloopError.code` is a `FloopErrorCode` enum with a catch-all `Other(String)` variant so unknown server codes round-trip without an SDK update.

```rust
use floopfloop::FloopErrorCode;

match client.projects().status("my-project").await {
    Ok(ev) => println!("status: {}", ev.status),
    Err(e) if e.code == FloopErrorCode::RateLimited => {
        if let Some(d) = e.retry_after { tokio::time::sleep(d).await; }
    }
    Err(e) if e.code == FloopErrorCode::Unauthorized => {
        eprintln!("Check your FLOOP_API_KEY.");
    }
    Err(e) => eprintln!("[{}] {} (request {:?})", e.code.as_str(), e.message, e.request_id),
}
```

Known codes: `Unauthorized`, `Forbidden`, `ValidationError`, `RateLimited`, `NotFound`, `Conflict`, `ServiceUnavailable`, `ServerError`, `NetworkError`, `Timeout`, `BuildFailed`, `BuildCancelled`, `Unknown`, plus `Other(String)` for pass-through.

## Resources

| Accessor             | Methods |
|---|---|
| `client.projects()`  | `create`, `list`, `get`, `status`, `cancel`, `reactivate`, `refine`, `conversations`, `stream`, `wait_for_live` |
| `client.subdomains()`| `check`, `suggest` |
| `client.secrets()`   | `list`, `set`, `remove` |
| `client.library()`   | `list`, `clone_project` |
| `client.usage()`     | `summary` |
| `client.api_keys()`  | `list`, `create`, `remove` (accepts id or name) |
| `client.uploads()`   | `create` (presign + direct S3 PUT, returns `UploadedAttachment` for `Projects.refine`) |
| `client.user()`      | `me` |

Method-for-method parity with `@floopfloop/sdk` (Node), `floopfloop` (Python), and `floop-go-sdk` (Go).

## Configuration

```rust
use floopfloop::Client;
use std::time::Duration;

let client = Client::builder(std::env::var("FLOOP_API_KEY")?)
    .base_url("https://staging.floopfloop.com") // default production URL otherwise
    .timeout(Duration::from_secs(60))           // default 30s
    .user_agent_suffix("myapp/1.2")             // appended after floopfloop-rust-sdk/<v>
    // .http_client(my_reqwest_client)           // bring your own reqwest::Client
    .build()?;
```

`Client` is cheap to clone (wraps an `Arc`) and all methods are `&self`, so share it across tasks without concern.

## TLS

Uses `rustls` by default (no system OpenSSL required on any platform). If you need native TLS instead, depend on `reqwest` directly in your app with `native-tls` and pass your own `http_client` into `Client::builder`.

## Versioning

Follows [Semantic Versioning](https://semver.org/). Breaking changes in `0.x` are called out in [CHANGELOG.md](./CHANGELOG.md) and a new tag is cut with `v<version>`. Tag push triggers the release workflow which publishes to crates.io.

## License

MIT
