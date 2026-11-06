use crate::filter_proxy;
use crate::upstream;

use std::path::PathBuf;

use filter_proxy::start_proxy_with_rules;
use serde_json::{Value, json};

fn fixture_path() -> anyhow::Result<PathBuf> {
    for candidate in [
        "crates/mike/tests/fixtures/response-style-banned-terms.md",
        "tests/fixtures/response-style-banned-terms.md",
    ] {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path.canonicalize()?);
        }
    }
    anyhow::bail!("response-style-banned-terms.md fixture not found from either cwd convention")
}

struct StyleRulesPathGuard {
    prior: Option<std::ffi::OsString>,
}

impl StyleRulesPathGuard {
    fn set(path: &std::path::Path) -> Self {
        let prior = std::env::var_os("MIKE_STYLE_RULES_PATH");
        unsafe { std::env::set_var("MIKE_STYLE_RULES_PATH", path) };
        Self { prior }
    }
}

impl Drop for StyleRulesPathGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.prior {
                Some(v) => std::env::set_var("MIKE_STYLE_RULES_PATH", v),
                None => std::env::remove_var("MIKE_STYLE_RULES_PATH"),
            }
        }
    }
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_style_check)]
async fn create_rejects_text_containing_banned_vocabulary() -> anyhow::Result<()> {
    let _guard = StyleRulesPathGuard::set(&fixture_path()?);
    let Some(proxy) = start_proxy_with_rules("http://127.0.0.1:0", &[]).await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({
            "kind": "text_substring",
            "value": format!("tc-style-load-bearing-{}", uuid::Uuid::new_v4()),
            "action": "replace",
            "text": "while working, give brief updates when you find something load-bearing",
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 400, "banned vocabulary must reject with 400");
    let body: Value = resp.json().await?;
    let error = body["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("load-bearing"),
        "error must name the offending phrase: {body:?}"
    );
    assert!(
        error.contains("style_override"),
        "error must point at the override escape hatch: {body:?}"
    );

    proxy.cleanup().await
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_style_check)]
async fn create_with_style_override_true_saves_and_reports_override() -> anyhow::Result<()> {
    let _guard = StyleRulesPathGuard::set(&fixture_path()?);
    let Some(proxy) = start_proxy_with_rules("http://127.0.0.1:0", &[]).await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({
            "kind": "text_substring",
            "value": format!("tc-style-override-{}", uuid::Uuid::new_v4()),
            "action": "replace",
            "text": "while working, give brief updates when you find something load-bearing",
            "style_override": true,
        }))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        201,
        "style_override=true must accept a rule that would otherwise fail the check"
    );
    let envelope: Value = resp.json().await?;
    assert_eq!(
        envelope["rule"]["style_override"], true,
        "the override must round-trip on the created rule: {envelope:?}"
    );

    proxy.cleanup().await
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_style_check)]
async fn patch_rejects_banned_vocabulary_without_override() -> anyhow::Result<()> {
    let _guard = StyleRulesPathGuard::set(&fixture_path()?);
    let Some(proxy) = start_proxy_with_rules("http://127.0.0.1:0", &[]).await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let created: Value = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({
            "kind": "text_substring",
            "value": format!("tc-style-patch-{}", uuid::Uuid::new_v4()),
            "action": "replace",
            "text": "a plain sentence with nothing flagged at all",
        }))
        .send()
        .await?
        .json()
        .await?;
    let rule_id = created["rule"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("rule id must be a stringified bigint: {created:?}"))?;

    let resp = client
        .patch(format!("{}/api/filter/rules/{rule_id}", proxy.inspect_url))
        .json(&json!({
            "text": "not a flaky tool, not a red gate you did not cause",
        }))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        400,
        "patching in banned vocabulary without an override must 400"
    );
    let body: Value = resp.json().await?;
    let error = body["error"].as_str().unwrap_or_default();
    assert!(
        error.contains(rule_id),
        "error must name the rule id: {body:?}"
    );
    assert!(
        error.contains("gate"),
        "error must name the phrase: {body:?}"
    );

    proxy.cleanup().await
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_style_check)]
async fn dry_run_does_not_reject_draft_containing_banned_vocabulary() -> anyhow::Result<()> {
    let _guard = StyleRulesPathGuard::set(&fixture_path()?);
    let Some(proxy) = start_proxy_with_rules("http://127.0.0.1:0", &[]).await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/filter/rules/dry-run", proxy.inspect_url))
        .json(&json!({
            "request_id": 999_999_999_u64,
            "rules": [{
                "kind": "text_substring",
                "value": "anchor",
                "action": "replace",
                "text": "surface that instead of proceeding",
            }],
        }))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        404,
        "banned vocabulary must not block a dry run; it must reach the (unknown) \
         request-body lookup and 404 there instead of failing validation: {}",
        resp.status()
    );

    proxy.cleanup().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(filter_rules_style_check)]
async fn dry_run_against_real_request_ignores_banned_vocabulary() -> anyhow::Result<()> {
    let _guard = StyleRulesPathGuard::set(&fixture_path()?);
    let server = upstream::canned_json_upstream("msg_dry_run_style_check").await;
    let Some(proxy) = start_proxy_with_rules(&server.uri(), &[]).await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-claude-code-session-id", filter_proxy::session_id())
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hi"}],
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let requests: Value = client
        .get(format!("{}/api/requests", proxy.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let request_id = requests[0]["id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("request id must be a number, got: {requests}"))?;

    let dry_resp = client
        .post(format!("{}/api/filter/rules/dry-run", proxy.inspect_url))
        .json(&json!({
            "request_id": request_id,
            "rules": [{
                "kind": "text_substring",
                "value": "anchor",
                "action": "replace",
                "text": "surface that instead of proceeding",
            }],
        }))
        .send()
        .await?;
    assert_eq!(
        dry_resp.status(),
        200,
        "a dry run is a preview, not a write — the save-time vocabulary check must \
         not block it even when the draft text contains banned vocabulary: {}",
        dry_resp.text().await.unwrap_or_default()
    );

    proxy.cleanup().await
}
