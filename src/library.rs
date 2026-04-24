use crate::error::{FloopError, FloopErrorCode};
use crate::projects::urlencoding_encode;
use crate::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryProject {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub subdomain: Option<String>,
    #[serde(rename = "botType")]
    pub bot_type: Option<String>,
    #[serde(rename = "cloneCount")]
    pub clone_count: i64,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Debug, Default, Clone)]
pub struct LibraryListOptions {
    pub bot_type: Option<String>,
    pub search: Option<String>,
    /// "popular" or "newest" — any other string is forwarded verbatim.
    pub sort: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClonedProject {
    pub id: String,
    pub name: String,
    pub subdomain: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CloneLibraryProjectInput {
    pub subdomain: String,
}

pub struct Library<'c> {
    pub(crate) client: &'c Client,
}

impl<'c> Library<'c> {
    pub async fn list(&self, opts: LibraryListOptions) -> Result<Vec<LibraryProject>, FloopError> {
        let mut params: Vec<String> = Vec::new();
        if let Some(v) = opts.bot_type {
            params.push(format!("botType={}", urlencoding_encode(&v)));
        }
        if let Some(v) = opts.search {
            params.push(format!("search={}", urlencoding_encode(&v)));
        }
        if let Some(v) = opts.sort {
            params.push(format!("sort={}", urlencoding_encode(&v)));
        }
        if let Some(v) = opts.page {
            params.push(format!("page={v}"));
        }
        if let Some(v) = opts.limit {
            params.push(format!("limit={v}"));
        }
        let path = if params.is_empty() {
            "/api/v1/library".to_owned()
        } else {
            format!("/api/v1/library?{}", params.join("&"))
        };

        // Backend can emit either a bare array or a { items: [...] } envelope.
        let raw: serde_json::Value = self
            .client
            .request_json(reqwest::Method::GET, &path, None)
            .await?;
        if let Ok(arr) = serde_json::from_value::<Vec<LibraryProject>>(raw.clone()) {
            return Ok(arr);
        }
        if let Some(items) = raw.get("items") {
            if let Ok(arr) = serde_json::from_value::<Vec<LibraryProject>>(items.clone()) {
                return Ok(arr);
            }
        }
        Err(FloopError::new(
            FloopErrorCode::Unknown,
            0,
            "library list: unrecognised response shape",
        ))
    }

    pub async fn clone_project(
        &self,
        project_id: &str,
        input: CloneLibraryProjectInput,
    ) -> Result<ClonedProject, FloopError> {
        let path = format!("/api/v1/library/{}/clone", urlencoding_encode(project_id));
        let body = serde_json::to_value(&input).unwrap();
        self.client
            .request_json(reqwest::Method::POST, &path, Some(&body))
            .await
    }
}
