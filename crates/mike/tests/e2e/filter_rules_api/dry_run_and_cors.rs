use crate::anthropic_fixtures;

use super::harness::{remediation_helpers, start_proxy_with_config};

use anthropic_fixtures::{RequestBody, text_block};
use serde_json::{Value, json};

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn dry_run_unknown_request_id_returns_404() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let resp = reqwest::Client::new()
        .post(format!("{}/api/filter/rules/dry-run", proxy.inspect_url))
        .json(&json!({"request_id": 999_999_999_u64}))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        404,
        "unknown request id must 404 (not in memory, not in the DB)"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(filter_rules_db)]
async fn dry_run_without_rules_uses_active_ruleset() -> anyhow::Result<()> {
    let server = remediation_helpers::mount_ok_upstream().await;
    let Some(proxy) =
        remediation_helpers::start_proxy_with_upstream(b"persist_enabled = true\n", &server.uri())
            .await?
    else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let envelope: Value = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({
            "kind": "xml_tag",
            "value": "system-reminder",
            "action": "drop",
        }))
        .send()
        .await?
        .json()
        .await?;
    let rule_id: i64 = envelope["rule"]["id"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .map_err(|e| anyhow::anyhow!("rule id must be a stringified bigint: {e}"))?;

    let resp = client
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", uuid::Uuid::new_v4().to_string())
        .json(
            &RequestBody::default()
                .max_tokens(100)
                .system(json!([
                    text_block("Keep me."),
                    text_block("<system-reminder>drop me</system-reminder>"),
                ]))
                .content("hi")
                .build(),
        )
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
        .json(&json!({ "request_id": request_id }))
        .send()
        .await?;
    assert_eq!(dry_resp.status(), 200);
    let preview: Value = dry_resp.json().await?;

    assert_eq!(
        preview["changed"], true,
        "active xml_tag rule must fire on the captured ORIGINAL body: {preview}"
    );
    assert_eq!(
        preview["firing_rule_ids"],
        json!([rule_id]),
        "firing_rule_ids must carry the persisted rule's bigint id"
    );
    let sys = preview["filtered_system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("filtered_system must be the full system array"))?;
    assert_eq!(sys.len(), 1, "reminder dropped in active-ruleset preview");
    assert_eq!(sys[0]["text"], "Keep me.");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn cors_disallows_unknown_origin_on_mutating_route() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .request(
            reqwest::Method::OPTIONS,
            format!("{}/api/filter/rules", proxy.inspect_url),
        )
        .header("Origin", "http://evil.example.com")
        .header("Access-Control-Request-Method", "POST")
        .header("Access-Control-Request-Headers", "content-type")
        .send()
        .await?;

    let acao = resp
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        acao != "http://evil.example.com" && acao != "*",
        "disallowed origin leaked into ACAO header: {acao:?}"
    );
    Ok(())
}
