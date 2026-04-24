//! End-to-end tests using `wiremock` to stub the FloopFloop API.
//! Every test brings up a fresh MockServer, configures it for the
//! expected request(s), points the SDK at it, and asserts on the
//! resulting types.

use floopfloop::{
    Client, ConversationsOptions, CreateApiKeyInput, CreateProjectInput, CreateUploadInput,
    FloopErrorCode, LibraryListOptions, ListProjectsOptions, RefineInput, StreamOptions,
};
use serde_json::json;
use std::time::Duration;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_for(server: &MockServer) -> Client {
    Client::builder("flp_test")
        .base_url(server.uri())
        .timeout(Duration::from_secs(5))
        .build()
        .expect("test client")
}

// ── transport ───────────────────────────────────────────────────────

#[tokio::test]
async fn bearer_and_data_envelope_unwrap() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/user/me"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"id":"u_1","email":"p@x","name":"Pim","plan":"business"}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    let client = client_for(&server);
    let me = client.user().me().await.expect("me");
    assert_eq!(me.id, "u_1");
    assert_eq!(me.email.as_deref(), Some("p@x"));
    assert_eq!(me.plan.as_deref(), Some("business"));
}

#[tokio::test]
async fn error_envelope_becomes_typed_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/user/me"))
        .respond_with(
            ResponseTemplate::new(404)
                .insert_header("x-request-id", "req_1")
                .set_body_raw(
                    r#"{"error":{"code":"NOT_FOUND","message":"no such user"}}"#,
                    "application/json",
                ),
        )
        .mount(&server)
        .await;
    let client = client_for(&server);
    let err = client.user().me().await.expect_err("should fail");
    assert_eq!(err.code, FloopErrorCode::NotFound);
    assert_eq!(err.status, 404);
    assert_eq!(err.message, "no such user");
    assert_eq!(err.request_id.as_deref(), Some("req_1"));
}

#[tokio::test]
async fn retry_after_delta_seconds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/user/me"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "5")
                .set_body_raw(
                    r#"{"error":{"code":"RATE_LIMITED","message":"slow"}}"#,
                    "application/json",
                ),
        )
        .mount(&server)
        .await;
    let client = client_for(&server);
    let err = client.user().me().await.expect_err("should fail");
    assert_eq!(err.code, FloopErrorCode::RateLimited);
    assert_eq!(err.retry_after, Some(Duration::from_secs(5)));
}

#[tokio::test]
async fn non_json_5xx_falls_back_to_server_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/user/me"))
        .respond_with(ResponseTemplate::new(500).set_body_string("upstream crashed"))
        .mount(&server)
        .await;
    let client = client_for(&server);
    let err = client.user().me().await.expect_err("should fail");
    assert_eq!(err.code, FloopErrorCode::ServerError);
    assert_eq!(err.status, 500);
}

#[tokio::test]
async fn unknown_server_code_roundtrips() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/user/me"))
        .respond_with(ResponseTemplate::new(418).set_body_raw(
            r#"{"error":{"code":"TEAPOT_MODE","message":"short and stout"}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    let client = client_for(&server);
    let err = client.user().me().await.expect_err("should fail");
    // Unknown code passes through via Other variant.
    assert_eq!(err.code.as_str(), "TEAPOT_MODE");
}

// ── projects ────────────────────────────────────────────────────────

#[tokio::test]
async fn projects_create_list_get_status() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"project":{"id":"p_1","name":"Cat","subdomain":"cat","status":"queued","botType":null,"url":null,"amplifyAppUrl":null,"isPublic":false,"isAuthProtected":false,"teamId":null,"createdAt":"","updatedAt":""},"deployment":{"id":"d_1","status":"queued","version":1}}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":[{"id":"p_1","name":"Cat","subdomain":"cat","status":"live","botType":null,"url":"https://cat.floop.tech","amplifyAppUrl":null,"isPublic":true,"isAuthProtected":false,"teamId":null,"createdAt":"","updatedAt":""}]}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/projects/p_1/status"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"step":2,"totalSteps":5,"status":"generating","message":"working","progress":0.4}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let created = client
        .projects()
        .create(CreateProjectInput {
            prompt: "a cat".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(created.project.id, "p_1");

    let listed = client
        .projects()
        .list(ListProjectsOptions::default())
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);

    let got = client
        .projects()
        .get("cat", ListProjectsOptions::default())
        .await
        .unwrap();
    assert_eq!(got.id, "p_1");

    let st = client.projects().status("p_1").await.unwrap();
    assert_eq!(st.status, "generating");
    assert_eq!(st.progress, Some(0.4));
}

#[tokio::test]
async fn projects_cancel_and_reactivate() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/projects/p_1/cancel"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(r#"{"data":{}}"#, "application/json"))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/projects/p_1/reactivate"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(r#"{"data":{}}"#, "application/json"))
        .mount(&server)
        .await;
    let client = client_for(&server);
    client.projects().cancel("p_1").await.unwrap();
    client.projects().reactivate("p_1").await.unwrap();
}

#[tokio::test]
async fn projects_refine_queued_variant() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/projects/p_1/refine"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"queued":true,"messageId":"m_1"}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    let client = client_for(&server);
    let res = client
        .projects()
        .refine(
            "p_1",
            RefineInput {
                message: "x".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(res.queued.is_some());
    assert!(res.saved_only.is_none());
    assert!(res.processing.is_none());
    assert_eq!(res.queued.unwrap().message_id, "m_1");
}

#[tokio::test]
async fn projects_conversations_forwards_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/projects/p_1/conversations"))
        .and(query_param("limit", "10"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"messages":[{"id":"m_1","projectId":"p_1","role":"user","content":"hi","metadata":null,"status":"sent","position":1,"createdAt":""}],"queued":[],"latestVersion":3}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    let client = client_for(&server);
    let out = client
        .projects()
        .conversations("p_1", ConversationsOptions { limit: 10 })
        .await
        .unwrap();
    assert_eq!(out.messages.len(), 1);
    assert_eq!(out.latest_version, 3);
}

// ── stream ──────────────────────────────────────────────────────────

#[tokio::test]
async fn stream_yields_sequence_and_returns_ok_on_live() {
    let server = MockServer::start().await;
    // wiremock serves mocks in registration order when `up_to_n_times`
    // is used; register three responders with an .up_to_n_times(1) cap.
    Mock::given(method("GET"))
        .and(path("/api/v1/projects/p_1/status"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"step":1,"totalSteps":3,"status":"queued","message":""}}"#,
            "application/json",
        ))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/projects/p_1/status"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"step":2,"totalSteps":3,"status":"generating","message":"","progress":0.3}}"#,
            "application/json",
        ))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/projects/p_1/status"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"step":3,"totalSteps":3,"status":"live","message":""}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let mut seen: Vec<String> = Vec::new();
    let res = client
        .projects()
        .stream(
            "p_1",
            Some(StreamOptions {
                interval: Duration::from_millis(5),
                max_wait: Duration::from_secs(5),
            }),
            |ev| {
                seen.push(ev.status.clone());
                Ok(())
            },
        )
        .await;
    assert!(res.is_ok(), "Stream should succeed: {:?}", res);
    assert_eq!(seen, vec!["queued", "generating", "live"]);
}

#[tokio::test]
async fn stream_failed_returns_typed_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/projects/p_1/status"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"step":1,"totalSteps":1,"status":"failed","message":"typecheck failed"}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    let client = client_for(&server);
    let err = client
        .projects()
        .stream(
            "p_1",
            Some(StreamOptions {
                interval: Duration::from_millis(5),
                max_wait: Duration::from_secs(5),
            }),
            |_| Ok(()),
        )
        .await
        .expect_err("should fail");
    assert_eq!(err.code, FloopErrorCode::BuildFailed);
    assert_eq!(err.message, "typecheck failed");
}

// ── secrets ─────────────────────────────────────────────────────────

#[tokio::test]
async fn secrets_list_and_set_and_remove() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/projects/p_1/secrets"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"secrets":[{"name":"STRIPE_KEY"},{"name":"DB_URL"}]}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/projects/p_1/secrets"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(r#"{"data":{"success":true}}"#, "application/json"),
        )
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/projects/p_1/secrets/STRIPE_KEY"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(r#"{"data":{"success":true}}"#, "application/json"),
        )
        .mount(&server)
        .await;
    let client = client_for(&server);

    let list = client.secrets().list("p_1").await.unwrap();
    assert_eq!(list.len(), 2);

    client
        .secrets()
        .set("p_1", "STRIPE_KEY", "sk_xxx")
        .await
        .unwrap();
    client.secrets().remove("p_1", "STRIPE_KEY").await.unwrap();
}

// ── subdomains ──────────────────────────────────────────────────────

#[tokio::test]
async fn subdomains_check_and_suggest() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/subdomains/check"))
        .and(query_param("slug", "hello"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"slug":"hello","available":true}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/subdomains/suggest"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(r#"{"data":{"slug":"cat-cafe"}}"#, "application/json"),
        )
        .mount(&server)
        .await;
    let client = client_for(&server);

    let c = client.subdomains().check("hello").await.unwrap();
    assert!(c.available);
    assert_eq!(c.slug, "hello");

    let s = client
        .subdomains()
        .suggest("a cat cafe landing page")
        .await
        .unwrap();
    assert_eq!(s.slug, "cat-cafe");
}

// ── library ─────────────────────────────────────────────────────────

#[tokio::test]
async fn library_list_supports_both_response_shapes() {
    let server = MockServer::start().await;
    // First call: bare array
    Mock::given(method("GET"))
        .and(path("/api/v1/library"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":[{"id":"p_1","name":"A","description":null,"subdomain":"a","botType":"site","cloneCount":42,"createdAt":""}]}"#,
            "application/json",
        ))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    // Second call: {items: [...]} envelope (overrides after first).
    Mock::given(method("GET"))
        .and(path("/api/v1/library"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"items":[{"id":"p_2","name":"B","description":null,"subdomain":"b","botType":"app","cloneCount":7,"createdAt":""}]}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    let client = client_for(&server);

    let bare = client
        .library()
        .list(LibraryListOptions::default())
        .await
        .unwrap();
    assert_eq!(bare.len(), 1);
    assert_eq!(bare[0].id, "p_1");

    let wrapped = client
        .library()
        .list(LibraryListOptions::default())
        .await
        .unwrap();
    assert_eq!(wrapped.len(), 1);
    assert_eq!(wrapped[0].id, "p_2");
}

// ── usage ───────────────────────────────────────────────────────────

#[tokio::test]
async fn usage_summary() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/usage/summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "plan": {"name":"business","displayName":"Business","monthlyCredits":10000,"maxProjects":100,"maxStorageMb":5000,"maxBandwidthMb":10000},
                "credits": {"currentCredits":5000,"rolledOverCredits":500,"lifetimeCreditsUsed":25000,"rolloverExpiresAt":null},
                "currentPeriod": {"start":"2026-04-01","end":"2026-05-01","projectsCreated":3,"buildsUsed":12,"refinementsUsed":40,"storageUsedMb":200,"bandwidthUsedMb":50}
            }
        })))
        .mount(&server)
        .await;
    let client = client_for(&server);
    let out = client.usage().summary().await.unwrap();
    assert_eq!(out.plan.name, "business");
    assert_eq!(out.credits.current_credits, 5000);
    assert_eq!(out.current_period.builds_used, 12);
}

// ── api keys ────────────────────────────────────────────────────────

#[tokio::test]
async fn api_keys_list_create_remove_by_name() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/api-keys"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"keys":[{"id":"k_7","name":"my-script","keyPrefix":"flp_","scopes":null,"lastUsedAt":null,"createdAt":""}]}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/api-keys"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"id":"k_new","rawKey":"flp_secretsecret","keyPrefix":"flp_secre"}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/api-keys/k_7"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(r#"{"data":{"success":true}}"#, "application/json"),
        )
        .mount(&server)
        .await;
    let client = client_for(&server);

    let list = client.api_keys().list().await.unwrap();
    assert_eq!(list.len(), 1);

    let created = client
        .api_keys()
        .create(CreateApiKeyInput { name: "new".into() })
        .await
        .unwrap();
    assert_eq!(created.raw_key, "flp_secretsecret");

    // Remove by name — SDK resolves name → id via a preflight list.
    client.api_keys().remove("my-script").await.unwrap();

    // And NOT_FOUND when the name doesn't exist.
    let err = client
        .api_keys()
        .remove("ghost")
        .await
        .expect_err("should fail");
    assert_eq!(err.code, FloopErrorCode::NotFound);
}

// ── uploads ─────────────────────────────────────────────────────────

#[tokio::test]
async fn uploads_happy_path() {
    let server = MockServer::start().await;
    // The presign response's uploadUrl has to point somewhere — point
    // it back at the same mock server under /s3/put, which we also stub.
    let upload_url = format!("{}/s3/put", server.uri());
    Mock::given(method("POST"))
        .and(path("/api/v1/uploads"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"{{"data":{{"uploadUrl":"{upload_url}","key":"uploads/u_1/cat.png","fileId":"f_1"}}}}"#
            ),
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/s3/put"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let out = client
        .uploads()
        .create(CreateUploadInput {
            file_name: "cat.png".into(),
            bytes: bytes::Bytes::from_static(b"fake-png-bytes"),
            file_type: None,
        })
        .await
        .unwrap();
    assert_eq!(out.key, "uploads/u_1/cat.png");
    assert_eq!(out.file_type, "image/png");
    assert_eq!(out.file_size, 14);
}

#[tokio::test]
async fn uploads_validation_size_and_mime() {
    let client = Client::builder("flp_test")
        .base_url("http://unused")
        .build()
        .unwrap();

    let too_big = bytes::Bytes::from(vec![0u8; (floopfloop::MAX_UPLOAD_BYTES as usize) + 1]);
    let err = client
        .uploads()
        .create(CreateUploadInput {
            file_name: "big.png".into(),
            bytes: too_big,
            file_type: None,
        })
        .await
        .expect_err("too big");
    assert_eq!(err.code, FloopErrorCode::ValidationError);
    assert!(err.message.contains("upload limit"));

    let bad_ext = client
        .uploads()
        .create(CreateUploadInput {
            file_name: "archive.tar.gz".into(),
            bytes: bytes::Bytes::from_static(b"x"),
            file_type: None,
        })
        .await
        .expect_err("bad ext");
    assert_eq!(bad_ext.code, FloopErrorCode::ValidationError);
}
