use crate::error::FloopError;
use crate::projects::urlencoding_encode;
use crate::Client;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SubdomainCheckResult {
    pub slug: String,
    pub available: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubdomainSuggestResult {
    pub slug: String,
}

pub struct Subdomains<'c> {
    pub(crate) client: &'c Client,
}

impl<'c> Subdomains<'c> {
    pub async fn check(&self, slug: &str) -> Result<SubdomainCheckResult, FloopError> {
        let path = format!("/api/v1/subdomains/check?slug={}", urlencoding_encode(slug));
        self.client
            .request_json(reqwest::Method::GET, &path, None)
            .await
    }

    pub async fn suggest(&self, prompt: &str) -> Result<SubdomainSuggestResult, FloopError> {
        let path = format!(
            "/api/v1/subdomains/suggest?prompt={}",
            urlencoding_encode(prompt)
        );
        self.client
            .request_json(reqwest::Method::GET, &path, None)
            .await
    }
}
