use super::harness::start_proxy_with_config;

use mike::app::inspect_router_on;
use mike::state::AppState;
use serde_json::{Value, json};
use tokio::net::TcpListener;

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn rejects_drop_with_text() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({
            "kind": "xml_tag",
            "value": format!("tc-drop-text-{}", uuid::Uuid::new_v4()),
            "action": "drop",
            "text": "forbidden",
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await?;
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("must not have a `text`"),
        "error should cite the invariant: {body:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn rejects_replace_with_empty_text() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({
            "kind": "text_substring",
            "value": format!("tc-replace-empty-{}", uuid::Uuid::new_v4()),
            "action": "replace",
            "text": "",
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn rejects_append_without_text() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({
            "kind": "text_substring",
            "value": format!("tc-append-none-{}", uuid::Uuid::new_v4()),
            "action": "append",
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn patch_rule_updates_action_and_text_round_trip() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let value = format!("tc-patch-rt-{}", uuid::Uuid::new_v4());
    let envelope: Value = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({"kind": "xml_tag", "value": value, "action": "drop"}))
        .send()
        .await?
        .json()
        .await?;
    let id = envelope["rule"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("create id missing: {envelope:?}"))?
        .to_string();

    let patch = client
        .patch(format!("{}/api/filter/rules/{id}", proxy.inspect_url))
        .json(&json!({"action": "replace", "text": "redacted"}))
        .send()
        .await?;
    assert_eq!(patch.status(), 200);
    let patched: Value = patch.json().await?;
    assert_eq!(patched["rule"]["action"], "replace");
    assert_eq!(patched["rule"]["text"], "redacted");

    let list: Vec<Value> = client
        .get(format!("{}/api/filter/rules", proxy.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let entry = list
        .iter()
        .find(|r| r["id"] == Value::String(id.clone()))
        .ok_or_else(|| anyhow::anyhow!("entry missing after PATCH"))?;
    assert_eq!(entry["action"], "replace");
    assert_eq!(entry["text"], "redacted");

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&proxy.test_db.database_url)
        .await?;
    let db_row: (String, Option<String>) = sqlx::query_as(
        "SELECT a.code, r.text
         FROM rules.rules r
         JOIN rules.actions a ON a.id = r.action_id
         WHERE r.value = $1",
    )
    .bind(&value)
    .fetch_one(&pool)
    .await?;
    pool.close().await;
    assert_eq!(db_row.0, "replace", "DB action must reflect the PATCH");
    assert_eq!(db_row.1.as_deref(), Some("redacted"));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn patch_rule_with_invalid_action_text_combo_returns_400() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let value = format!("tc-patch-bad-{}", uuid::Uuid::new_v4());
    let envelope: Value = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({"kind": "xml_tag", "value": value, "action": "drop"}))
        .send()
        .await?
        .json()
        .await?;
    let id = envelope["rule"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("create id missing: {envelope:?}"))?
        .to_string();

    let resp = client
        .patch(format!("{}/api/filter/rules/{id}", proxy.inspect_url))
        .json(&json!({"text": "forbidden"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 400, "drop + text must fail validation");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn mutation_returns_503_when_persistence_disabled() -> anyhow::Result<()> {
    let config = mike::config::ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        "http://127.0.0.1:0",
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "0",
    ])?;
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state, inspect_listener.local_addr()?);
    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await
            .ok();
    });
    let inspect_url = format!("http://{inspect_addr}");

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/filter/rules"))
        .json(&json!({
            "kind": "xml_tag",
            "value": format!("tc-503-{}", uuid::Uuid::new_v4()),
            "action": "drop",
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 503);
    let body: Value = resp.json().await?;
    let msg = body["error"].as_str().unwrap_or_default();
    assert!(
        msg.contains("persistence is disabled"),
        "503 body must say persistence is the missing dependency: {msg}"
    );

    shutdown.cancel();
    Ok(())
}
