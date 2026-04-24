use crate::error::{FloopError, FloopErrorCode};
use crate::Client;
use serde::{Deserialize, Serialize};

/// Per-file ceiling enforced by the backend. Matches the Node / Python /
/// Go SDKs and the CLI.
pub const MAX_UPLOAD_BYTES: u64 = 5 * 1024 * 1024;

const EXT_TO_MIME: &[(&str, &str)] = &[
    (".png", "image/png"),
    (".jpg", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".gif", "image/gif"),
    (".svg", "image/svg+xml"),
    (".webp", "image/webp"),
    (".ico", "image/x-icon"),
    (".pdf", "application/pdf"),
    (".txt", "text/plain"),
    (".csv", "text/csv"),
    (".doc", "application/msword"),
    (
        ".docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
];

/// Guess a MIME type from a filename's extension. Returns `None` if the
/// extension isn't on the backend allowlist.
pub fn guess_mime_type(file_name: &str) -> Option<&'static str> {
    let lower = file_name.to_ascii_lowercase();
    let dot = lower.rfind('.')?;
    let ext = &lower[dot..];
    EXT_TO_MIME
        .iter()
        .find_map(|(e, mime)| if *e == ext { Some(*mime) } else { None })
}

fn is_allowed_mime(mime: &str) -> bool {
    EXT_TO_MIME.iter().any(|(_, m)| *m == mime)
}

#[derive(Debug, Clone)]
pub struct UploadedAttachment {
    pub key: String,
    pub file_name: String,
    pub file_type: String,
    pub file_size: u64,
}

/// Single file to upload.  Explicit `file_type` overrides the extension
/// guess.  The payload is held as `Bytes` — streaming reqwest `Body`
/// support can be layered on later if needed.
#[derive(Debug, Clone)]
pub struct CreateUploadInput {
    pub file_name: String,
    pub bytes: bytes::Bytes,
    pub file_type: Option<String>,
}

#[derive(Serialize)]
struct PresignRequest<'a> {
    #[serde(rename = "fileName")]
    file_name: &'a str,
    #[serde(rename = "fileType")]
    file_type: &'a str,
    #[serde(rename = "fileSize")]
    file_size: u64,
}

#[derive(Deserialize)]
struct PresignResponse {
    #[serde(rename = "uploadUrl")]
    upload_url: String,
    key: String,
    #[allow(dead_code)]
    #[serde(rename = "fileId")]
    file_id: String,
}

pub struct Uploads<'c> {
    pub(crate) client: &'c Client,
}

impl<'c> Uploads<'c> {
    pub async fn create(&self, input: CreateUploadInput) -> Result<UploadedAttachment, FloopError> {
        if input.file_name.is_empty() {
            return Err(FloopError::new(
                FloopErrorCode::ValidationError,
                0,
                "uploads: file_name is required",
            ));
        }
        let resolved_type = input
            .file_type
            .clone()
            .or_else(|| guess_mime_type(&input.file_name).map(str::to_owned));
        let Some(file_type) = resolved_type else {
            return Err(FloopError::new(
                FloopErrorCode::ValidationError,
                0,
                format!(
                    "uploads: unsupported file type for {}. Allowed: png, jpg, gif, svg, webp, ico, pdf, txt, csv, doc, docx.",
                    input.file_name
                ),
            ));
        };
        if !is_allowed_mime(&file_type) {
            return Err(FloopError::new(
                FloopErrorCode::ValidationError,
                0,
                format!("uploads: file type {file_type} is not on the backend allowlist"),
            ));
        }
        let size = input.bytes.len() as u64;
        if size > MAX_UPLOAD_BYTES {
            return Err(FloopError::new(
                FloopErrorCode::ValidationError,
                0,
                format!(
                    "uploads: {} is {:.1} MB — the upload limit is {} MB.",
                    input.file_name,
                    size as f64 / (1024.0 * 1024.0),
                    MAX_UPLOAD_BYTES / (1024 * 1024)
                ),
            ));
        }

        // 1. Presign
        let body = serde_json::to_value(PresignRequest {
            file_name: &input.file_name,
            file_type: &file_type,
            file_size: size,
        })
        .unwrap();
        let presign: PresignResponse = self
            .client
            .request_json(reqwest::Method::POST, "/api/v1/uploads", Some(&body))
            .await?;

        // 2. Direct PUT to S3.  No bearer auth — the presigned URL
        //    carries its own signature.
        let resp = self
            .client
            .http()
            .put(&presign.upload_url)
            .header(reqwest::header::CONTENT_TYPE, file_type.as_str())
            .body(input.bytes.clone())
            .send()
            .await
            .map_err(|err| {
                FloopError::new(
                    FloopErrorCode::NetworkError,
                    0,
                    format!("uploads: S3 PUT failed — {err}"),
                )
            })?;

        let status = resp.status();
        if !status.is_success() {
            let raw = resp.text().await.unwrap_or_default();
            let trimmed: String = raw.chars().take(512).collect();
            return Err(FloopError::new(
                FloopErrorCode::Unknown,
                status.as_u16(),
                format!(
                    "uploads: S3 rejected PUT ({}): {}",
                    status.as_u16(),
                    trimmed
                ),
            ));
        }

        Ok(UploadedAttachment {
            key: presign.key,
            file_name: input.file_name,
            file_type,
            file_size: size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_guesses() {
        assert_eq!(guess_mime_type("cat.PNG"), Some("image/png"));
        assert_eq!(
            guess_mime_type("resume.DOCX"),
            Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document")
        );
        assert_eq!(guess_mime_type("archive.tar.gz"), None);
        assert_eq!(guess_mime_type("noext"), None);
    }
}
