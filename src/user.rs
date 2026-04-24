use crate::error::FloopError;
use crate::Client;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
}

/// Named `UserApi` rather than `User` because the `User` type already
/// owns the slot.  Accessed via `client.user()` — matches the Go SDK's
/// `client.User` convention.
pub struct UserApi<'c> {
    pub(crate) client: &'c Client,
}

impl<'c> UserApi<'c> {
    pub async fn me(&self) -> Result<User, FloopError> {
        self.client
            .request_json(reqwest::Method::GET, "/api/v1/user/me", None)
            .await
    }
}
