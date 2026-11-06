use crate::common;

use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn mount_models_ok(anthropic: &MockServer, key: &str) {
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("x-api-key", key))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
        .mount(anthropic)
        .await;
}

async fn mount_models_rejected(anthropic: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .mount(anthropic)
        .await;
}

#[tokio::test]
async fn put_anthropic_key_empty_rejected_without_probing_upstream() -> anyhow::Result<()> {
    let anthropic = MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&anthropic.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .put(format!(
            "{inspect_url}/api/settings/assistant/anthropic/key"
        ))
        .json(&serde_json::json!({"key": "   "}))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    let body: serde_json::Value = resp.json().await?;
    assert_eq!(body["valid"], false);
    assert_eq!(body["persisted"], false);
    assert!(
        body["error"].as_str().is_some_and(|e| e.contains("empty")),
        "body: {body}"
    );
    Ok(())
}

#[tokio::test]
async fn put_anthropic_key_rejected_by_upstream_leaves_slot_untouched() -> anyhow::Result<()> {
    let anthropic = MockServer::start().await;
    mount_models_rejected(&anthropic).await;
    let (_proxy_url, inspect_url) = common::start_proxy(&anthropic.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .put(format!(
            "{inspect_url}/api/settings/assistant/anthropic/key"
        ))
        .json(&serde_json::json!({"key": "sk-ant-bad-key"}))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    let body: serde_json::Value = resp.json().await?;
    assert_eq!(body["valid"], false);
    assert_eq!(body["persisted"], false);
    assert!(body["error"].is_string(), "body: {body}");
    assert!(body.get("label").is_none(), "body: {body}");

    let status: serde_json::Value = client
        .get(format!("{inspect_url}/api/settings/assistant/anthropic"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(status["configured"], false);
    Ok(())
}

#[tokio::test]
async fn put_anthropic_key_valid_swaps_slot_persisted_false() -> anyhow::Result<()> {
    let anthropic = MockServer::start().await;
    mount_models_ok(&anthropic, "sk-ant-newkey-secret").await;
    let (_proxy_url, inspect_url) = common::start_proxy(&anthropic.uri()).await?;
    let client = reqwest::Client::new();

    let before: serde_json::Value = client
        .get(format!("{inspect_url}/api/settings/assistant/anthropic"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(before["configured"], false);
    assert_eq!(before["mode"], "claude_code_login");

    let put_resp = client
        .put(format!(
            "{inspect_url}/api/settings/assistant/anthropic/key"
        ))
        .json(&serde_json::json!({"key": "sk-ant-newkey-secret"}))
        .send()
        .await?;
    assert_eq!(put_resp.status(), reqwest::StatusCode::OK);
    let put_body: serde_json::Value = put_resp.json().await?;
    assert_eq!(put_body["valid"], true);
    assert_eq!(put_body["persisted"], false);
    let label = put_body["label"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("expected label in {put_body}"))?;
    assert!(
        !label.contains("newkey-secret"),
        "label must not contain the unmasked key: {label}"
    );
    assert!(label.contains("..."), "label must be masked: {label}");

    let after: serde_json::Value = client
        .get(format!("{inspect_url}/api/settings/assistant/anthropic"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(after["configured"], true);
    assert_eq!(after["masked_key"], put_body["label"]);
    Ok(())
}

#[tokio::test]
async fn delete_anthropic_key_clears_slot() -> anyhow::Result<()> {
    let anthropic = MockServer::start().await;
    mount_models_ok(&anthropic, "sk-ant-todelete").await;
    let (_proxy_url, inspect_url) = common::start_proxy(&anthropic.uri()).await?;
    let client = reqwest::Client::new();

    let put_resp = client
        .put(format!(
            "{inspect_url}/api/settings/assistant/anthropic/key"
        ))
        .json(&serde_json::json!({"key": "sk-ant-todelete"}))
        .send()
        .await?;
    assert_eq!(put_resp.status(), reqwest::StatusCode::OK);

    let delete_resp = client
        .delete(format!(
            "{inspect_url}/api/settings/assistant/anthropic/key"
        ))
        .send()
        .await?;
    assert_eq!(delete_resp.status(), reqwest::StatusCode::OK);
    let delete_body: serde_json::Value = delete_resp.json().await?;
    assert_eq!(delete_body["cleared"], true);

    let status: serde_json::Value = client
        .get(format!("{inspect_url}/api/settings/assistant/anthropic"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(status["configured"], false);
    assert!(status["masked_key"].is_null());
    Ok(())
}

#[tokio::test]
async fn test_connection_with_explicit_key_does_not_require_it_saved() -> anyhow::Result<()> {
    let anthropic = MockServer::start().await;
    mount_models_ok(&anthropic, "sk-ant-explicit-test-key").await;
    let (_proxy_url, inspect_url) = common::start_proxy(&anthropic.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!(
            "{inspect_url}/api/settings/assistant/anthropic/test"
        ))
        .json(&serde_json::json!({"key": "sk-ant-explicit-test-key"}))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await?;
    assert_eq!(body["ok"], true, "body: {body}");

    let status: serde_json::Value = client
        .get(format!("{inspect_url}/api/settings/assistant/anthropic"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        status["configured"], false,
        "a connection test must never save the key it validated"
    );
    Ok(())
}

#[tokio::test]
async fn test_connection_default_mode_is_claude_code_login_without_upstream_call()
-> anyhow::Result<()> {
    let anthropic = MockServer::start().await;
    let (_proxy_url, inspect_url) = common::start_proxy(&anthropic.uri()).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!(
            "{inspect_url}/api/settings/assistant/anthropic/test"
        ))
        .json(&serde_json::json!({}))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await?;
    assert_eq!(body["ok"], true, "body: {body}");
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|m| m.contains("Claude Code login")),
        "body: {body}"
    );
    Ok(())
}

#[tokio::test]
async fn credentials_route_returns_null_key_in_claude_code_login_mode_even_when_key_stored()
-> anyhow::Result<()> {
    let anthropic = MockServer::start().await;
    mount_models_ok(&anthropic, "sk-ant-stored-but-gated").await;
    let (inspect_url, service_token) =
        common::start_inspect_with_service_token(&anthropic.uri()).await?;
    let client = reqwest::Client::new();

    let put_resp = client
        .put(format!(
            "{inspect_url}/api/settings/assistant/anthropic/key"
        ))
        .json(&serde_json::json!({"key": "sk-ant-stored-but-gated"}))
        .send()
        .await?;
    assert_eq!(put_resp.status(), reqwest::StatusCode::OK);

    let creds: serde_json::Value = client
        .get(format!(
            "{inspect_url}/api/settings/assistant/anthropic/credentials"
        ))
        .header(mike::service_token::HEADER, &service_token)
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(creds["mode"], "claude_code_login");
    assert!(
        creds["api_key"].is_null(),
        "a key sitting in the slot must never leak while mode is claude_code_login: {creds}"
    );
    Ok(())
}

#[tokio::test]
async fn credentials_route_refuses_a_caller_without_the_service_token() -> anyhow::Result<()> {
    let anthropic = MockServer::start().await;
    let (inspect_url, service_token) =
        common::start_inspect_with_service_token(&anthropic.uri()).await?;
    let client = reqwest::Client::new();
    let url = format!("{inspect_url}/api/settings/assistant/anthropic/credentials");

    let missing = client.get(&url).send().await?;
    assert_eq!(missing.status(), reqwest::StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = missing.json().await?;
    assert!(body.get("api_key").is_none(), "body: {body}");
    assert!(body.get("mode").is_none(), "body: {body}");

    let wrong = client
        .get(&url)
        .header(mike::service_token::HEADER, "not-the-token")
        .send()
        .await?;
    assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);

    let right = client
        .get(&url)
        .header(mike::service_token::HEADER, &service_token)
        .send()
        .await?;
    assert_eq!(right.status(), reqwest::StatusCode::OK);
    Ok(())
}
