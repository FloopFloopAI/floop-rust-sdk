use crate::error::FloopError;
use crate::Client;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct UsagePlan {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "monthlyCredits")]
    pub monthly_credits: i64,
    #[serde(rename = "maxProjects")]
    pub max_projects: i64,
    #[serde(rename = "maxStorageMb")]
    pub max_storage_mb: i64,
    #[serde(rename = "maxBandwidthMb")]
    pub max_bandwidth_mb: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsageCredits {
    #[serde(rename = "currentCredits")]
    pub current_credits: i64,
    #[serde(rename = "rolledOverCredits")]
    pub rolled_over_credits: i64,
    #[serde(rename = "lifetimeCreditsUsed")]
    pub lifetime_credits_used: i64,
    #[serde(default, rename = "rolloverExpiresAt")]
    pub rollover_expires_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsageCurrentPeriod {
    pub start: String,
    pub end: String,
    #[serde(rename = "projectsCreated")]
    pub projects_created: i64,
    #[serde(rename = "buildsUsed")]
    pub builds_used: i64,
    #[serde(rename = "refinementsUsed")]
    pub refinements_used: i64,
    #[serde(rename = "storageUsedMb")]
    pub storage_used_mb: i64,
    #[serde(rename = "bandwidthUsedMb")]
    pub bandwidth_used_mb: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsageSummary {
    pub plan: UsagePlan,
    pub credits: UsageCredits,
    #[serde(rename = "currentPeriod")]
    pub current_period: UsageCurrentPeriod,
}

pub struct Usage<'c> {
    pub(crate) client: &'c Client,
}

impl<'c> Usage<'c> {
    pub async fn summary(&self) -> Result<UsageSummary, FloopError> {
        self.client
            .request_json(reqwest::Method::GET, "/api/v1/usage/summary", None)
            .await
    }
}
