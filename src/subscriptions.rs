use crate::error::FloopError;
use crate::Client;
use serde::Deserialize;
use serde_json::Value;

/// Plan + billing details for the authenticated user. Sourced from
/// `userSubscriptions` joined onto `subscriptionPlans` on the backend;
/// sensitive fields (Stripe customer / subscription IDs, invoice
/// metadata) are deliberately omitted from the wire shape.
#[derive(Debug, Clone, Deserialize)]
pub struct SubscriptionPlan {
    pub status: String,
    #[serde(default, rename = "billingPeriod")]
    pub billing_period: Option<String>,
    #[serde(rename = "currentPeriodStart")]
    pub current_period_start: String,
    #[serde(rename = "currentPeriodEnd")]
    pub current_period_end: String,
    #[serde(default, rename = "canceledAt")]
    pub canceled_at: Option<String>,
    #[serde(rename = "planName")]
    pub plan_name: String,
    #[serde(rename = "planDisplayName")]
    pub plan_display_name: String,
    #[serde(rename = "priceMonthly")]
    pub price_monthly: i64,
    #[serde(rename = "priceAnnual")]
    pub price_annual: i64,
    #[serde(rename = "monthlyCredits")]
    pub monthly_credits: i64,
    #[serde(rename = "maxProjects")]
    pub max_projects: i64,
    #[serde(rename = "maxStorageMb")]
    pub max_storage_mb: i64,
    #[serde(rename = "maxBandwidthMb")]
    pub max_bandwidth_mb: i64,
    #[serde(rename = "creditRolloverMonths")]
    pub credit_rollover_months: i64,
    /// Free-form feature-flag bag. Decoded as `serde_json::Value` so
    /// callers can inspect new flags without us cutting a release each
    /// time the backend grows a key.
    pub features: Value,
}

/// Credit-balance snapshot — the second half of the
/// `/api/v1/subscriptions/current` response.
#[derive(Debug, Clone, Deserialize)]
pub struct SubscriptionCredits {
    pub current: i64,
    #[serde(rename = "rolledOver")]
    pub rolled_over: i64,
    pub total: i64,
    #[serde(default, rename = "rolloverExpiresAt")]
    pub rollover_expires_at: Option<String>,
    #[serde(rename = "lifetimeUsed")]
    pub lifetime_used: i64,
}

/// Response envelope for [`Subscriptions::current`]. Both fields are
/// independently nullable: a user may exist without an active
/// subscription (mid-signup, cancelled with no grace credits remaining).
/// Treat `None` as "no active subscription data" rather than an error.
#[derive(Debug, Clone, Deserialize)]
pub struct CurrentSubscription {
    #[serde(default)]
    pub subscription: Option<SubscriptionPlan>,
    #[serde(default)]
    pub credits: Option<SubscriptionCredits>,
}

/// Resource namespace for plan + credit-balance.
///
/// Distinct from [`crate::Usage`] — `usage().summary()` returns
/// current-period consumption (credits remaining + builds used + storage),
/// while `subscriptions().current()` returns the plan tier itself
/// (price, billing period, cancel state). They overlap on
/// `monthly_credits` and `max_projects` but serve different audiences:
/// usage for "am I about to hit my limits?", current for "what plan am I
/// on, and when does it renew?".
pub struct Subscriptions<'c> {
    pub(crate) client: &'c Client,
}

impl<'c> Subscriptions<'c> {
    /// Fetch the authenticated user's current subscription + credit
    /// snapshot. Read-only; cheap to call.
    pub async fn current(&self) -> Result<CurrentSubscription, FloopError> {
        self.client
            .request_json(reqwest::Method::GET, "/api/v1/subscriptions/current", None)
            .await
    }
}
