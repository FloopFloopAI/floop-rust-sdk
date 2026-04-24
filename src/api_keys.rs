use crate::error::{FloopError, FloopErrorCode};
use crate::projects::urlencoding_encode;
use crate::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct ApiKeySummary {
    pub id: String,
    pub name: String,
    #[serde(rename = "keyPrefix")]
    pub key_prefix: String,
    /// Shape still evolving server-side — pass through as raw JSON.
    pub scopes: Option<serde_json::Value>,
    #[serde(default, rename = "lastUsedAt")]
    pub last_used_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

/// Returned ONCE on Create — the `raw_key` is the only time the full
/// secret ever leaves the server. Surface it to the user and do not
/// persist it.
#[derive(Debug, Clone, Deserialize)]
pub struct IssuedApiKey {
    pub id: String,
    #[serde(rename = "rawKey")]
    pub raw_key: String,
    #[serde(rename = "keyPrefix")]
    pub key_prefix: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateApiKeyInput {
    pub name: String,
}

#[derive(Deserialize)]
struct ListResponse {
    keys: Vec<ApiKeySummary>,
}

pub struct ApiKeys<'c> {
    pub(crate) client: &'c Client,
}

impl<'c> ApiKeys<'c> {
    pub async fn list(&self) -> Result<Vec<ApiKeySummary>, FloopError> {
        let resp: ListResponse = self
            .client
            .request_json(reqwest::Method::GET, "/api/v1/api-keys", None)
            .await?;
        Ok(resp.keys)
    }

    pub async fn create(&self, input: CreateApiKeyInput) -> Result<IssuedApiKey, FloopError> {
        let body = serde_json::to_value(&input).unwrap();
        self.client
            .request_json(reqwest::Method::POST, "/api/v1/api-keys", Some(&body))
            .await
    }

    /// Revoke by id or by human-readable name — does a preflight list
    /// to resolve the id, then DELETEs.
    pub async fn remove(&self, id_or_name: &str) -> Result<(), FloopError> {
        let all = self.list().await?;
        let matched = all
            .into_iter()
            .find(|k| k.id == id_or_name || k.name == id_or_name)
            .ok_or_else(|| {
                FloopError::new(
                    FloopErrorCode::NotFound,
                    404,
                    format!("API key not found: {id_or_name}"),
                )
            })?;
        let path = format!("/api/v1/api-keys/{}", urlencoding_encode(&matched.id));
        self.client
            .request_empty(reqwest::Method::DELETE, &path, None)
            .await
    }
}
