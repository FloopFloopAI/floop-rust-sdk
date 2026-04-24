use crate::error::FloopError;
use crate::projects::urlencoding_encode;
use crate::Client;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SecretSummary {
    pub name: String,
    #[serde(default, rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(default, rename = "updatedAt")]
    pub updated_at: Option<String>,
}

#[derive(Deserialize)]
struct SecretsListResponse {
    secrets: Vec<SecretSummary>,
}

pub struct Secrets<'c> {
    pub(crate) client: &'c Client,
}

impl<'c> Secrets<'c> {
    pub async fn list(&self, reference: &str) -> Result<Vec<SecretSummary>, FloopError> {
        let path = format!("/api/v1/projects/{}/secrets", urlencoding_encode(reference));
        let resp: SecretsListResponse = self
            .client
            .request_json(reqwest::Method::GET, &path, None)
            .await?;
        Ok(resp.secrets)
    }

    pub async fn set(&self, reference: &str, name: &str, value: &str) -> Result<(), FloopError> {
        let path = format!("/api/v1/projects/{}/secrets", urlencoding_encode(reference));
        let body = serde_json::json!({ "name": name, "value": value });
        self.client
            .request_empty(reqwest::Method::POST, &path, Some(&body))
            .await
    }

    pub async fn remove(&self, reference: &str, name: &str) -> Result<(), FloopError> {
        let path = format!(
            "/api/v1/projects/{}/secrets/{}",
            urlencoding_encode(reference),
            urlencoding_encode(name),
        );
        self.client
            .request_empty(reqwest::Method::DELETE, &path, None)
            .await
    }
}
