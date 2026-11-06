use crate::common;

use common::start_proxy_with_openrouter;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TEST_API_KEY: &str = "sk-test-usage-openrouter-secret";

#[tokio::test]
async fn openrouter_usage_maps_key_and_credits_when_configured() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/key"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({
                    "data": {
                        "label": "sk-or-v1-4d2...d99",
                        "limit": 20,
                        "limit_reset": "monthly",
                        "limit_remaining": 17.763_098_87,
                        "usage": 18.312_676_819,
                        "usage_daily": 0.047_733_22,
                        "usage_weekly": 0.297_165_27,
                        "usage_monthly": 2.236_901_13,
                        "is_free_tier": false
                    }
                })),
        )
        .expect(1)
        .mount(&or_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/credits"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({
                    "data": {
                        "total_credits": 20,
                        "total_usage": 18.312_676_819
                    }
                })),
        )
        .expect(1)
        .mount(&or_server)
        .await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), Some(TEST_API_KEY)).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{inspect_url}/api/usage/openrouter"))
        .send()
        .await?;

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let raw_text = resp.text().await?;
    assert!(
        !raw_text.contains(TEST_API_KEY),
        "response body must never contain the API key: {raw_text}"
    );

    let body: serde_json::Value = serde_json::from_str(&raw_text)?;
    assert_configured_key_and_credits_fields(&body)?;

    shutdown.cancel();
    Ok(())
}

fn field_f64(value: &serde_json::Value, field: &str) -> anyhow::Result<f64> {
    value[field]
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("{field} must be a number, got {:?}", value[field]))
}

fn assert_configured_key_and_credits_fields(body: &serde_json::Value) -> anyhow::Result<()> {
    assert_eq!(body["configured"], true);
    assert_eq!(body["label"], "sk-or-v1-4d2...d99");
    assert_eq!(body["limit"], 20.0);
    assert_eq!(body["limit_reset"], "monthly");
    assert_eq!(body["credits_total"], 20.0);
    assert_eq!(body["is_free_tier"], false);

    let limit_remaining = field_f64(body, "limit_remaining")?;
    assert!(
        (limit_remaining - 17.763_098_87).abs() < 1e-9,
        "expected limit_remaining=17.763_098_87, got {limit_remaining}"
    );
    let usage_total = field_f64(body, "usage_total")?;
    assert!(
        (usage_total - 18.312_676_819).abs() < 1e-9,
        "expected usage_total=18.312_676_819, got {usage_total}"
    );
    let usage_daily = field_f64(body, "usage_daily")?;
    assert!(
        (usage_daily - 0.047_733_22).abs() < 1e-9,
        "expected usage_daily=0.047_733_22, got {usage_daily}"
    );
    let usage_weekly = field_f64(body, "usage_weekly")?;
    assert!(
        (usage_weekly - 0.297_165_27).abs() < 1e-9,
        "expected usage_weekly=0.297_165_27, got {usage_weekly}"
    );
    let usage_monthly = field_f64(body, "usage_monthly")?;
    assert!(
        (usage_monthly - 2.236_901_13).abs() < 1e-9,
        "expected usage_monthly=2.236_901_13, got {usage_monthly}"
    );
    let credits_used = field_f64(body, "credits_used")?;
    assert!(
        (credits_used - 18.312_676_819).abs() < 1e-9,
        "expected credits_used=18.312_676_819, got {credits_used}"
    );
    let credits_remaining = field_f64(body, "credits_remaining")?;
    assert!(
        (credits_remaining - (20.0 - 18.312_676_819)).abs() < 1e-9,
        "expected credits_remaining={}, got {credits_remaining}",
        20.0 - 18.312_676_819
    );
    Ok(())
}

#[tokio::test]
#[serial_test::serial(openrouter_env)]
async fn openrouter_usage_returns_unconfigured_when_no_key() -> anyhow::Result<()> {
    let _guards = [
        EnvVarGuard::unset("MIKE_STEERER_OPENROUTER_API_KEY"),
        EnvVarGuard::unset("OPENROUTER_API_KEY"),
    ];

    let or_server = MockServer::start().await;
    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), None).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{inspect_url}/api/usage/openrouter"))
        .send()
        .await?;

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await?;
    assert_eq!(
        body,
        serde_json::json!({"configured": false}),
        "unconfigured response must contain only the configured flag"
    );

    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn openrouter_usage_returns_502_on_upstream_failure() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/key"))
        .respond_with(ResponseTemplate::new(503).set_body_string("service unavailable"))
        .mount(&or_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/credits"))
        .respond_with(ResponseTemplate::new(503).set_body_string("service unavailable"))
        .mount(&or_server)
        .await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), Some(TEST_API_KEY)).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{inspect_url}/api/usage/openrouter"))
        .send()
        .await?;

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_GATEWAY,
        "expected 502 when OpenRouter is down; got {}",
        resp.status()
    );
    let raw_text = resp.text().await?;
    assert!(
        !raw_text.contains(TEST_API_KEY),
        "error response body must never contain the API key: {raw_text}"
    );

    shutdown.cancel();
    Ok(())
}

struct EnvVarGuard {
    key: &'static str,
    prior: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn unset(key: &'static str) -> Self {
        let prior = std::env::var_os(key);
        unsafe { std::env::remove_var(key) };
        Self { key, prior }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}
