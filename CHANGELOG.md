# Changelog

All notable changes to `floopfloop` (Rust SDK) are documented in this file.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This crate follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0-alpha.3] — 2026-04-26

### Fixed
- **`Projects::stream` and `Projects::wait_for_live` looped until
  `max_wait` when a project entered the `archived` state mid-stream.**
  The `match ev.status.as_str()` only handled `"live"` / `"failed"` /
  `"cancelled"`; `"archived"` fell into the default arm and polling
  continued. Now `"live" | "archived" => return Ok(())` — matches
  Node, Python, Swift, and Kotlin which already treat archived as a
  non-error terminal. Same drift fixed in alpha.5 of floop-go-sdk,
  alpha.2 of floop-ruby-sdk, and alpha.3 of floop-php-sdk on the same
  day.
- New `stream_archived_terminates_cleanly_like_live` integration test
  using `wiremock` to lock in the regression.

### Changed
- `Cargo.toml` `version` bumped to `0.1.0-alpha.3`.

## [0.1.0-alpha.2] — 2026-04-25

### Added
- `FloopError::new(code, status, message)` is now `pub` (was `pub(crate)`).
  Callers can construct sentinel errors to short-circuit
  `projects().stream()` handlers — return `Err(FloopError::new(...))` from
  the closure to stop polling early without the previous `Cell<T>`
  workaround the cookbook had to recommend.
- `FloopError::with_request_id(id)` and `FloopError::with_retry_after(d)`
  builder-style setters for fully populating an externally-constructed
  `FloopError` (e.g. when forwarding upstream-service errors).
- `docs/recipes.md` cookbook — six end-to-end Rust recipes that mirror
  the same set of patterns shipped in the Node, Python, Go, Ruby, and
  PHP SDKs.

### Changed
- Cookbook recipe 2 ("Watch a build progress in real time") now uses
  the supported `Err(FloopError::new(...))` pattern for early-abort
  instead of the `Cell<bool>` shared-state workaround that was needed
  while `new` was crate-private.

## [0.1.0-alpha.1] — 2026-04-24

### Added
- `floopfloop::Client` with bearer auth, a builder-style configuration
  (`base_url`, `timeout`, `user_agent_suffix`, `http_client`) and full
  `Arc`-based `Clone` so one instance can be shared across tasks.
- `FloopError` + `FloopErrorCode` — 13 named variants plus an
  `Other(String)` catch-all for server codes the SDK doesn't yet
  recognise. `retry_after: Option<Duration>` parses both delta-seconds
  and RFC 7231 HTTP-date `Retry-After` headers, matching the
  Node / Python / Go SDKs.
- Eight resources, method-for-method parity with every other FloopFloop SDK:
  - `projects`: `create`, `list`, `get`, `status`, `cancel`, `reactivate`,
    `refine`, `conversations`, `stream`, `wait_for_live`.
  - `subdomains`: `check`, `suggest`.
  - `secrets`: `list`, `set`, `remove`.
  - `library`: `list` (tolerates both bare-array and `{items:[]}`
    response shapes), `clone_project`.
  - `usage`: `summary`.
  - `api_keys`: `list`, `create`, `remove` (accepts id **or** name and
    resolves via a preflight list).
  - `uploads`: `create` (presign + direct S3 PUT; 5 MB cap; extension
    allowlist matches Node / Python / Go).
  - `user`: `me`.
- `Projects::stream` is a callback-based polling iterator that
  de-duplicates identical consecutive snapshots (`status / step /
  progress / queue_position`) — same event sequence the Node and
  Python SDKs yield. `Projects::wait_for_live` is a thin wrapper that
  invokes `stream` with a no-op handler and returns the hydrated
  `Project` on success.
- `rustls`-only TLS stack by default (no OpenSSL / native-TLS needed on
  any platform). Override by passing a custom `reqwest::Client` via
  `builder.http_client(...)`.
- 25 tests pass: 5 unit (error formatting, retry-after parsing, mime
  guesses), 18 integration via `wiremock`, 2 doc-tests.
