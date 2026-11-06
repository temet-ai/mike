use crate::common;

use common::start_proxy_with_openrouter;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

#[tokio::test]
#[serial_test::serial(openrouter_env)]
async fn put_openrouter_key_valid_swaps_slot_persisted_false_without_db() -> anyhow::Result<()> {
    let _guards = [
        EnvVarGuard::unset("MIKE_STEERER_OPENROUTER_API_KEY"),
        EnvVarGuard::unset("OPENROUTER_API_KEY"),
    ];
    let or_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"label": "sk-or-v1-newkey...new", "usage": 0.0, "is_free_tier": false}
        })))
        .expect(2)
        .mount(&or_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/credits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"total_credits": 10.0, "total_usage": 0.0}
        })))
        .mount(&or_server)
        .await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), None).await?;

    let client = reqwest::Client::new();

    let before: serde_json::Value = client
        .get(format!("{inspect_url}/api/usage/openrouter"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(before, serde_json::json!({"configured": false}));

    let put_resp = client
        .put(format!("{inspect_url}/api/settings/steerer/openrouter/key"))
        .json(&serde_json::json!({"key": "sk-or-v1-newkey-secret"}))
        .send()
        .await?;
    assert_eq!(put_resp.status(), reqwest::StatusCode::OK);
    let put_body: serde_json::Value = put_resp.json().await?;
    assert_eq!(put_body["valid"], true);
    assert_eq!(
        put_body["persisted"], false,
        "persistence is disabled — the write must be reported as non-durable: {put_body}"
    );
    let label = put_body["label"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("expected label in {put_body}"))?;
    assert!(
        !label.contains("newkey-secret"),
        "label must not contain the unmasked key: {label}"
    );
    assert!(label.contains("..."), "label must be masked: {label}");
    assert!(put_body.get("error").is_none());

    let after: serde_json::Value = client
        .get(format!("{inspect_url}/api/usage/openrouter"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(after["configured"], true);
    assert_eq!(after["label"], "sk-or-v1-newkey...new");

    or_server.verify().await;
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
#[serial_test::serial(openrouter_env)]
async fn put_openrouter_key_invalid_leaves_slot_untouched() -> anyhow::Result<()> {
    let _guards = [
        EnvVarGuard::unset("MIKE_STEERER_OPENROUTER_API_KEY"),
        EnvVarGuard::unset("OPENROUTER_API_KEY"),
    ];
    let or_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/key"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .expect(1)
        .mount(&or_server)
        .await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), None).await?;

    let client = reqwest::Client::new();
    let put_resp = client
        .put(format!("{inspect_url}/api/settings/steerer/openrouter/key"))
        .json(&serde_json::json!({"key": "sk-or-v1-bad-key"}))
        .send()
        .await?;
    assert_eq!(put_resp.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    let put_body: serde_json::Value = put_resp.json().await?;
    assert_eq!(put_body["valid"], false);
    assert_eq!(put_body["persisted"], false);
    assert!(put_body["error"].is_string(), "body: {put_body}");
    assert!(put_body.get("label").is_none(), "body: {put_body}");
    let raw_text = serde_json::to_string(&put_body)?;
    assert!(
        !raw_text.contains("bad-key"),
        "error response must never echo the key: {raw_text}"
    );

    let after: serde_json::Value = client
        .get(format!("{inspect_url}/api/usage/openrouter"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(after, serde_json::json!({"configured": false}));

    or_server.verify().await;
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
#[serial_test::serial(openrouter_env)]
async fn put_openrouter_key_invalid_rejected_despite_models_endpoint_accepting_it()
-> anyhow::Result<()> {
    let _guards = [
        EnvVarGuard::unset("MIKE_STEERER_OPENROUTER_API_KEY"),
        EnvVarGuard::unset("OPENROUTER_API_KEY"),
    ];
    let or_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
        .mount(&or_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/key"))
        .and(header("authorization", "Bearer sk-or-v1-real-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"label": "sk-or-v1-real...key", "usage": 0.0, "is_free_tier": false}
        })))
        .mount(&or_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/key"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .expect(1)
        .mount(&or_server)
        .await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), None).await?;

    let client = reqwest::Client::new();
    let put_resp = client
        .put(format!("{inspect_url}/api/settings/steerer/openrouter/key"))
        .json(&serde_json::json!({"key": "sk-or-v1-definitely-not-a-real-key-000"}))
        .send()
        .await?;
    assert_eq!(
        put_resp.status(),
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "a key /models would accept must still be rejected once /key says no"
    );
    let put_body: serde_json::Value = put_resp.json().await?;
    assert_eq!(put_body["valid"], false, "body: {put_body}");
    assert_eq!(put_body["persisted"], false, "body: {put_body}");
    assert!(put_body.get("label").is_none(), "body: {put_body}");

    let after: serde_json::Value = client
        .get(format!("{inspect_url}/api/usage/openrouter"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(after, serde_json::json!({"configured": false}));

    or_server.verify().await;
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn put_openrouter_key_empty_rejected_without_probing_upstream() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;
    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), None).await?;

    let client = reqwest::Client::new();
    let put_resp = client
        .put(format!("{inspect_url}/api/settings/steerer/openrouter/key"))
        .json(&serde_json::json!({"key": "   "}))
        .send()
        .await?;
    assert_eq!(put_resp.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    let put_body: serde_json::Value = put_resp.json().await?;
    assert_eq!(put_body["valid"], false);
    assert_eq!(put_body["persisted"], false);
    assert!(
        put_body["error"]
            .as_str()
            .is_some_and(|e| e.contains("empty")),
        "body: {put_body}"
    );

    shutdown.cancel();
    Ok(())
}
