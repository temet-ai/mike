use std::time::{Duration, Instant};

use chrono::{DateTime, Datelike, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::store::persistence::PersistHandle;

const CACHE_TTL: Duration = Duration::from_secs(300);
const TOP_UP_URL: &str = "https://openrouter.ai/settings/credits";

#[derive(Debug, Deserialize)]
struct OpenRouterKeyEnvelope {
    data: OpenRouterKeyData,
}

#[derive(Debug, Deserialize)]
struct OpenRouterKeyData {
    label: String,
    #[serde(default)]
    limit: Option<f64>,
    #[serde(default)]
    limit_reset: Option<String>,
    #[serde(default)]
    limit_remaining: Option<f64>,
    usage: f64,
    #[serde(default)]
    usage_daily: Option<f64>,
    #[serde(default)]
    usage_weekly: Option<f64>,
    #[serde(default)]
    usage_monthly: Option<f64>,
    #[serde(default)]
    is_free_tier: bool,
}

#[derive(Debug, Deserialize)]
struct OpenRouterCreditsEnvelope {
    data: OpenRouterCreditsData,
}

#[derive(Debug, Deserialize)]
struct OpenRouterCreditsData {
    total_credits: f64,
    total_usage: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenRouterUsageResponse {
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_remaining: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_reset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_total: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_daily: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_weekly: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_monthly: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_total: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_used: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_remaining: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_free_tier: Option<bool>,
    #[serde(skip_serializing_if = "is_false", default)]
    pub credits_unavailable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_month_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unrecorded_month_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_daily_spend_7d_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_left: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_up_url: Option<&'static str>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl OpenRouterUsageResponse {
    fn unconfigured() -> Self {
        Self {
            configured: false,
            label: None,
            limit: None,
            limit_remaining: None,
            limit_reset: None,
            usage_total: None,
            usage_daily: None,
            usage_weekly: None,
            usage_monthly: None,
            credits_total: None,
            credits_used: None,
            credits_remaining: None,
            is_free_tier: None,
            credits_unavailable: false,
            recorded_month_usd: None,
            unrecorded_month_usd: None,
            avg_daily_spend_7d_usd: None,
            days_left: None,
            top_up_url: None,
        }
    }
}

pub fn month_start_utc(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .unwrap_or(now)
}

pub fn day_start_utc(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .unwrap_or(now)
}

pub fn compute_unrecorded_month(billed_month_usd: f64, recorded_month_usd: f64) -> f64 {
    (billed_month_usd - recorded_month_usd).max(0.0)
}

pub fn compute_avg_daily_spend_7d(total_cost_last_7_full_days_usd: f64) -> f64 {
    total_cost_last_7_full_days_usd / 7.0
}

pub fn compute_days_left(balance_usd: f64, avg_daily_spend_7d_usd: f64) -> Option<f64> {
    (avg_daily_spend_7d_usd > 0.0).then_some(balance_usd / avg_daily_spend_7d_usd)
}

struct CacheEntry {
    response: OpenRouterUsageResponse,
    fetched_at: Instant,
}

pub struct OpenRouterUsageCache {
    cache: RwLock<Option<CacheEntry>>,
}

impl OpenRouterUsageCache {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(None),
        }
    }

    pub async fn get(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: Option<&str>,
        persist: Option<&PersistHandle>,
    ) -> anyhow::Result<OpenRouterUsageResponse> {
        let Some(api_key) = api_key else {
            return Ok(OpenRouterUsageResponse::unconfigured());
        };

        if let Some(entry) = self.cache.read().await.as_ref()
            && entry.fetched_at.elapsed() < CACHE_TTL
        {
            return Ok(entry.response.clone());
        }

        let mut guard = self.cache.write().await;
        if let Some(entry) = guard.as_ref()
            && entry.fetched_at.elapsed() < CACHE_TTL
        {
            return Ok(entry.response.clone());
        }

        let response = fetch_from_openrouter(client, base_url, api_key, persist).await?;
        *guard = Some(CacheEntry {
            response: response.clone(),
            fetched_at: Instant::now(),
        });
        Ok(response)
    }

    pub async fn peek(&self) -> Option<OpenRouterUsageResponse> {
        let guard = self.cache.read().await;
        let entry = guard.as_ref()?;
        if entry.fetched_at.elapsed() < CACHE_TTL {
            Some(entry.response.clone())
        } else {
            None
        }
    }
}

async fn fetch_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
) -> anyhow::Result<T> {
    let resp = client
        .get(url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("OpenRouter request to {url} failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        return Err(anyhow::anyhow!(
            "OpenRouter {url} returned non-2xx status: {status}"
        ));
    }
    resp.json::<T>()
        .await
        .map_err(|e| anyhow::anyhow!("failed to deserialize OpenRouter response from {url}: {e}"))
}

enum CreditsOutcome {
    Data(OpenRouterCreditsData),
    Forbidden,
    Failed(anyhow::Error),
}

async fn fetch_credits(client: &reqwest::Client, url: &str, api_key: &str) -> CreditsOutcome {
    let resp = match client.get(url).bearer_auth(api_key).send().await {
        Ok(resp) => resp,
        Err(e) => {
            return CreditsOutcome::Failed(anyhow::anyhow!(
                "OpenRouter request to {url} failed: {e}"
            ));
        }
    };
    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        return CreditsOutcome::Forbidden;
    }
    if !resp.status().is_success() {
        let status = resp.status();
        return CreditsOutcome::Failed(anyhow::anyhow!(
            "OpenRouter {url} returned non-2xx status: {status}"
        ));
    }
    match resp.json::<OpenRouterCreditsEnvelope>().await {
        Ok(envelope) => CreditsOutcome::Data(envelope.data),
        Err(e) => CreditsOutcome::Failed(anyhow::anyhow!(
            "failed to deserialize OpenRouter response from {url}: {e}"
        )),
    }
}

async fn recorded_month_and_avg_7d(persist: Option<&PersistHandle>) -> (Option<f64>, Option<f64>) {
    let Some(persist) = persist else {
        return (None, None);
    };
    let now = Utc::now();
    let month_start = month_start_utc(now);
    let day_start = day_start_utc(now);
    let seven_days_start = day_start - chrono::Duration::days(7);

    let recorded_month = persist
        .sum_llm_spend_range(month_start, None)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "openrouter usage: recorded_month_usd query failed");
            0.0
        });
    let last_7_full_days = persist
        .sum_llm_spend_range(seven_days_start, Some(day_start))
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "openrouter usage: 7-day spend query failed");
            0.0
        });
    (
        Some(recorded_month),
        Some(compute_avg_daily_spend_7d(last_7_full_days)),
    )
}

async fn fetch_from_openrouter(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    persist: Option<&PersistHandle>,
) -> anyhow::Result<OpenRouterUsageResponse> {
    let key_url = format!("{base_url}/key");
    let credits_url = format!("{base_url}/credits");
    let (key_result, credits_outcome) = tokio::join!(
        fetch_json::<OpenRouterKeyEnvelope>(client, &key_url, api_key),
        fetch_credits(client, &credits_url, api_key),
    );
    let key = key_result?.data;

    let (credits, credits_unavailable) = match credits_outcome {
        CreditsOutcome::Data(data) => (Some(data), false),
        CreditsOutcome::Forbidden => (None, true),
        CreditsOutcome::Failed(e) => return Err(e),
    };

    let (recorded_month_usd, avg_daily_spend_7d_usd) = recorded_month_and_avg_7d(persist).await;
    let unrecorded_month_usd = match (key.usage_monthly, recorded_month_usd) {
        (Some(billed), Some(recorded)) => Some(compute_unrecorded_month(billed, recorded)),
        _ => None,
    };
    let balance_usd = credits.as_ref().map(|c| c.total_credits - c.total_usage);
    let days_left = match (balance_usd, avg_daily_spend_7d_usd) {
        (Some(balance), Some(avg)) => compute_days_left(balance, avg),
        _ => None,
    };

    Ok(OpenRouterUsageResponse {
        configured: true,
        label: Some(key.label),
        limit: key.limit,
        limit_remaining: key.limit_remaining,
        limit_reset: key.limit_reset,
        usage_total: Some(key.usage),
        usage_daily: key.usage_daily,
        usage_weekly: key.usage_weekly,
        usage_monthly: key.usage_monthly,
        credits_total: credits.as_ref().map(|c| c.total_credits),
        credits_used: credits.as_ref().map(|c| c.total_usage),
        credits_remaining: balance_usd,
        is_free_tier: Some(key.is_free_tier),
        credits_unavailable,
        recorded_month_usd,
        unrecorded_month_usd,
        avg_daily_spend_7d_usd,
        days_left,
        top_up_url: Some(TOP_UP_URL),
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn unconfigured_response_serializes_to_bare_configured_false() -> anyhow::Result<()> {
        let value = serde_json::to_value(OpenRouterUsageResponse::unconfigured())?;
        assert_eq!(value, serde_json::json!({"configured": false}));
        Ok(())
    }

    #[tokio::test]
    async fn fetch_from_openrouter_degrades_gracefully_on_credits_403() -> anyhow::Result<()> {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/key"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data": {
                        "label": "sk-or-v1-abc...xyz",
                        "usage": 1.5,
                        "is_free_tier": false
                    }
                })),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/credits"))
            .respond_with(
                wiremock::ResponseTemplate::new(403)
                    .set_body_string("Only management keys can fetch credits"),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let response = fetch_from_openrouter(&client, &server.uri(), "sk-test", None).await?;

        assert!(response.configured);
        assert_eq!(response.label.as_deref(), Some("sk-or-v1-abc...xyz"));
        assert!((response.usage_total.unwrap_or_default() - 1.5).abs() < 1e-9);
        assert!(response.credits_unavailable);
        assert!(response.credits_total.is_none());
        assert!(response.credits_used.is_none());
        assert!(response.credits_remaining.is_none());

        let value = serde_json::to_value(&response)?;
        assert_eq!(value["credits_unavailable"], true);
        assert!(value.get("credits_total").is_none());
        assert_eq!(value["top_up_url"], TOP_UP_URL);
        assert!(
            value.get("recorded_month_usd").is_none(),
            "no persist handle given, so the DB-derived fields must be omitted, not fabricated"
        );
        assert!(value.get("avg_daily_spend_7d_usd").is_none());
        assert!(value.get("days_left").is_none());
        Ok(())
    }

    #[tokio::test]
    async fn fetch_from_openrouter_still_fails_on_non_403_credits_error() -> anyhow::Result<()> {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/key"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data": {"label": "sk-or-v1-abc...xyz", "usage": 0.0, "is_free_tier": false}
                })),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/credits"))
            .respond_with(wiremock::ResponseTemplate::new(503).set_body_string("unavailable"))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = fetch_from_openrouter(&client, &server.uri(), "sk-test", None).await;
        assert!(result.is_err(), "expected an Err for a non-403 failure");
        Ok(())
    }

    #[test]
    fn month_start_utc_truncates_to_first_of_month() -> anyhow::Result<()> {
        let now = "2026-09-28T11:48:14Z".parse::<DateTime<Utc>>()?;
        assert_eq!(
            month_start_utc(now).to_rfc3339(),
            "2026-09-01T00:00:00+00:00"
        );
        Ok(())
    }

    #[test]
    fn day_start_utc_truncates_to_midnight() -> anyhow::Result<()> {
        let now = "2026-09-28T11:48:14Z".parse::<DateTime<Utc>>()?;
        assert_eq!(day_start_utc(now).to_rfc3339(), "2026-09-28T00:00:00+00:00");
        Ok(())
    }

    #[test]
    fn compute_unrecorded_month_floors_at_zero() {
        assert_eq!(compute_unrecorded_month(10.0, 15.0), 0.0);
        assert!((compute_unrecorded_month(15.0, 10.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn compute_avg_daily_spend_7d_divides_by_seven() {
        assert!((compute_avg_daily_spend_7d(7.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn compute_days_left_none_when_avg_zero_or_negative() {
        assert_eq!(compute_days_left(10.0, 0.0), None);
        assert_eq!(compute_days_left(10.0, -1.0), None);
    }

    #[test]
    fn compute_days_left_some_when_avg_positive() -> anyhow::Result<()> {
        let days =
            compute_days_left(10.0, 2.0).ok_or_else(|| anyhow::anyhow!("avg is positive"))?;
        assert!((days - 5.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn compute_days_left_negative_balance_yields_negative_days() -> anyhow::Result<()> {
        let balance = 80.0 - 80.19;
        let avg_daily = 36.12 / 30.0;
        let days = compute_days_left(balance, avg_daily)
            .ok_or_else(|| anyhow::anyhow!("avg is positive"))?;
        assert!(
            days < 0.0,
            "an already-negative balance must report negative runway, not None"
        );
        Ok(())
    }

    #[tokio::test]
    async fn recorded_month_and_avg_7d_none_without_persist() {
        let (recorded, avg) = recorded_month_and_avg_7d(None).await;
        assert_eq!(recorded, None);
        assert_eq!(avg, None);
    }
}
