use crate::common;

use common::start_proxy_with_openrouter;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn or_assist_envelope(prompt: &str) -> Value {
    let inner = json!({
        "suggestion": {
            "prompt": prompt,
            "authority": "redirect",
            "mode": "drive",
            "config": {
                "confidence_threshold": 0.85,
                "debounce_turns": 2
            }
        },
        "rationale": {
            "prompt": "The anchor is a directive the runtime steerer evaluates turns against.",
            "authority": "redirect is appropriate for behavioral violations.",
            "mode": "drive ensures the correction is injected even if the session resists.",
            "confidence_threshold": "0.85 catches specific behavioral violations reliably.",
            "debounce_turns": "2 re-checks quickly when a behavioral pattern is detected."
        }
    });
    json!({
        "choices": [{
            "message": {
                "content": inner.to_string()
            }
        }]
    })
}

#[tokio::test]
async fn assist_returns_anchor_shaped_output_on_success() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;

    let anchor_prompt = "Hold the session to the originally requested deliverable. \
        The session MUST NOT dismiss unresolved errors as pre-existing or out of scope; \
        every error raised must be addressed before any completion claim.";

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(or_assist_envelope(anchor_prompt)),
        )
        .expect(1)
        .mount(&or_server)
        .await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), Some("sk-test-or-key"))
            .await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/steerers/assist"))
        .json(&json!({
            "session_id": "550e8400-e29b-41d4-a716-446655440000",
            "user_intent": "prevent the session from dismissing pre-existing errors"
        }))
        .send()
        .await?;

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "assist must return 200"
    );

    let body: Value = resp.json().await?;

    assert!(
        body.get("suggestion").is_some(),
        "response missing 'suggestion' field: {body}"
    );
    assert!(
        body.get("rationale").is_some(),
        "response missing 'rationale' field: {body}"
    );
    assert!(
        body.get("model_used").is_some(),
        "response missing 'model_used' field: {body}"
    );

    let suggestion = &body["suggestion"];
    let prompt = suggestion["prompt"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("suggestion.prompt must be a string: {suggestion}"))?;

    assert!(
        prompt.contains("MUST NOT") || prompt.contains("must not") || prompt.contains("Hold the"),
        "suggestion.prompt reads like a conversation reply rather than a steering anchor: {prompt:?}"
    );
    assert_eq!(
        prompt, anchor_prompt,
        "suggestion.prompt must be the anchor text the steer-author authored"
    );

    assert_eq!(suggestion["authority"], "redirect");
    assert_eq!(suggestion["mode"], "drive");
    let threshold = suggestion["config"]["confidence_threshold"]
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("confidence_threshold must be a number"))?;
    assert!(
        (threshold - 0.85).abs() < 1e-6,
        "unexpected threshold: {threshold}"
    );
    assert_eq!(suggestion["config"]["debounce_turns"], 2);

    or_server.verify().await;
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn assist_injects_anchor_findings_into_user_prompt() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;

    let anchor_prompt = "The session MUST NOT claim a deliverable works unless tests were run \
        and their output appears in the same turn. Premature success claims are a violation. \
        The session MUST NOT exhibit overselling behavior — claiming 'all tests pass' without evidence.";

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(or_assist_envelope(anchor_prompt)),
        )
        .expect(1)
        .mount(&or_server)
        .await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), Some("sk-test-or-key"))
            .await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/steerers/assist"))
        .json(&json!({
            "session_id": "550e8400-e29b-41d4-a716-446655440000",
            "user_intent": "prevent false completion claims",
            "anchor_request_id": 42,
            "anchor_findings": [
                {
                    "category": "overselling",
                    "advice": "The assistant claimed tests pass without running them.",
                    "matched_text": "all tests pass"
                }
            ]
        }))
        .send()
        .await?;

    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let body: Value = resp.json().await?;
    assert!(
        body.get("suggestion").is_some(),
        "missing 'suggestion': {body}"
    );
    assert_eq!(body["suggestion"]["prompt"], anchor_prompt);

    let received = or_server.received_requests().await;
    let reqs = received
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("mock received no requests"))?;
    assert_eq!(reqs.len(), 1, "expected exactly 1 LLM call");

    let llm_body: Value = serde_json::from_slice(&reqs[0].body)?;
    let messages = llm_body["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("LLM request missing messages array"))?;
    let user_content = messages
        .iter()
        .find(|m| m["role"] == "user")
        .and_then(|m| m["content"].as_str())
        .ok_or_else(|| anyhow::anyhow!("no user message in LLM request"))?;

    assert!(
        user_content.contains("overselling"),
        "user prompt must reference the finding category; got: {user_content:?}"
    );
    assert!(
        user_content.contains("all tests pass"),
        "user prompt must include the matched_text; got: {user_content:?}"
    );
    assert!(
        user_content.contains("42"),
        "user prompt must note the anchor_request_id; got: {user_content:?}"
    );

    or_server.verify().await;
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn assist_unavailable_when_api_key_missing() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
        .expect(1)
        .mount(&or_server)
        .await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), Some("sk-test-or-key"))
            .await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/steerers/assist"))
        .json(&json!({
            "session_id": "550e8400-e29b-41d4-a716-446655440000",
            "user_intent": "test intent"
        }))
        .send()
        .await?;

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "assist_unavailable must return HTTP 200, not the LLM error status"
    );

    let body: Value = resp.json().await?;
    assert_eq!(
        body["error"], "assist_unavailable",
        "expected error=assist_unavailable on LLM failure, got: {body}"
    );
    assert!(
        body.get("message").is_some(),
        "assist_unavailable must include a message field: {body}"
    );

    or_server.verify().await;
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn assist_unavailable_when_llm_call_fails() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(503).set_body_string("service unavailable"))
        .expect(1)
        .mount(&or_server)
        .await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), Some("sk-test-or-key"))
            .await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/steerers/assist"))
        .json(&json!({
            "session_id": "550e8400-e29b-41d4-a716-446655440000",
            "user_intent": "test intent"
        }))
        .send()
        .await?;

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "assist_unavailable must return HTTP 200 even on LLM failure"
    );

    let body: Value = resp.json().await?;
    assert_eq!(
        body["error"], "assist_unavailable",
        "expected error=assist_unavailable on LLM failure, got: {body}"
    );
    assert!(
        body.get("message").is_some(),
        "assist_unavailable must include a message field: {body}"
    );

    or_server.verify().await;
    shutdown.cancel();
    Ok(())
}

#[tokio::test]
#[serial_test::serial(openrouter_env)]
async fn assist_picks_up_key_rotated_via_put_endpoint_no_restart() -> anyhow::Result<()> {
    let _guards = [
        EnvVarGuard::unset("MIKE_STEERER_OPENROUTER_API_KEY"),
        EnvVarGuard::unset("OPENROUTER_API_KEY"),
    ];

    let anchor_prompt = "The session MUST NOT skip verification before claiming a fix works.";

    let or_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"label": "sk-or-v1-rotated...ret", "usage": 0.0, "is_free_tier": false}
        })))
        .expect(1)
        .mount(&or_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(or_assist_envelope(anchor_prompt)),
        )
        .expect(1)
        .mount(&or_server)
        .await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), None).await?;

    let client = reqwest::Client::new();
    let assist_body = json!({
        "session_id": "550e8400-e29b-41d4-a716-446655440000",
        "user_intent": "prevent unverified completion claims"
    });

    let before: Value = client
        .post(format!("{inspect_url}/api/steerers/assist"))
        .json(&assist_body)
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        before["error"], "assist_unavailable",
        "expected assist_unavailable before any key is configured: {before}"
    );

    let put_resp = client
        .put(format!("{inspect_url}/api/settings/steerer/openrouter/key"))
        .json(&json!({"key": "sk-or-v1-rotated-secret"}))
        .send()
        .await?;
    assert_eq!(put_resp.status(), reqwest::StatusCode::OK);
    let put_body: Value = put_resp.json().await?;
    assert_eq!(put_body["valid"], true, "PUT body: {put_body}");

    let after: Value = client
        .post(format!("{inspect_url}/api/steerers/assist"))
        .json(&assist_body)
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        after["suggestion"]["prompt"], anchor_prompt,
        "assist must succeed using the rotated key with no restart: {after}"
    );

    or_server.verify().await;
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
