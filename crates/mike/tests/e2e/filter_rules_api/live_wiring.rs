use crate::anthropic_fixtures;
use crate::test_db;

use super::harness::{EnvGuard, acquire_env_lock, remediation_helpers, start_proxy_with_config};

use anthropic_fixtures::{RequestBody, text_block};
use mike::app::inspect_router_on;
use mike::state::AppState;
use serde_json::{Value, json};
use test_db::TestDb;
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(filter_rules_db)]
async fn post_rule_active_on_proxy_path_without_restart() -> anyhow::Result<()> {
    let server = remediation_helpers::mount_ok_upstream().await;
    let Some(proxy) =
        remediation_helpers::start_proxy_with_upstream(b"persist_enabled = true\n", &server.uri())
            .await?
    else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let post = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({
            "kind": "xml_tag",
            "value": "system-reminder",
            "action": "drop",
        }))
        .send()
        .await?;
    assert_eq!(post.status(), 201, "POST must succeed");

    let claude_body = RequestBody::default()
        .max_tokens(100)
        .system(json!([text_block(
            "<system-reminder>noise-{{arcsw-t1}}</system-reminder>"
        )]))
        .content("ping")
        .build();
    let resp = client
        .post(format!("{}/v1/messages", proxy.proxy_url))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("x-claude-code-session-id", uuid::Uuid::new_v4().to_string())
        .json(&claude_body)
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let received = server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(
        received.len(),
        1,
        "expected exactly one upstream round-trip, got {}",
        received.len()
    );
    let sent: Value = serde_json::from_slice(&received[0].body)?;
    let system_arr = sent["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("upstream body missing system array: {sent}"))?;
    assert!(
        system_arr.is_empty(),
        "ArcSwap hot-path propagation broken: reminder block still upstream: {sent:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn post_rule_persists_to_database() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let database_url = proxy.test_db.database_url.clone();

    let client = reqwest::Client::new();

    let value = format!("tc-dbread-{}", uuid::Uuid::new_v4());
    let resp = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({"kind": "xml_tag", "value": value, "action": "drop"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 201);

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;
    let row: Option<(String, String, String, String, bool)> = sqlx::query_as(
        "SELECT r.id::text, k.code, r.value, a.code, r.enabled
         FROM rules.rules r
         JOIN rules.kinds   k ON k.id = r.kind_id
         JOIN rules.actions a ON a.id = r.action_id
         WHERE r.value = $1",
    )
    .bind(&value)
    .fetch_optional(&pool)
    .await?;
    let row = row.ok_or_else(|| anyhow::anyhow!("expected DB row for value={value} after POST"))?;
    assert_eq!(row.1, "xml_tag", "DB kind must be xml_tag");
    assert_eq!(row.2, value, "DB value round-trips");
    assert_eq!(row.3, "drop", "DB action must be drop");
    assert!(row.4, "DB enabled must default to true");
    pool.close().await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn restart_seeds_arcswap_from_db() -> anyhow::Result<()> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let database_url = test_db.database_url.clone();

    let value = format!("tc-restart-seed-{}", uuid::Uuid::new_v4());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;
    let db_id: i64 = sqlx::query_scalar(
        "INSERT INTO rules.rules
            (uuid, kind_id, value, action_id, text, trigger_token, enabled,
             source_id, created_at, updated_at, sort_order)
         SELECT $1, rk.id, $2, ra.id, 'redacted', NULL, true, rs.id, NOW(), NOW(), 5
         FROM rules.kinds rk, rules.actions ra, rules.sources rs
         WHERE rk.code = 'text_substring' AND ra.code = 'replace' AND rs.code = 'ui'
         RETURNING id",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(&value)
    .fetch_one(&pool)
    .await?;

    let tmp = tempfile::TempDir::new()?;
    let config_path = tmp.path().join("config.toml");
    std::fs::write(&config_path, b"persist_enabled = true\n")?;

    let serial = acquire_env_lock().await;
    let _env_guard = EnvGuard::set("MIKE_CONFIG_DIR", tmp.path());
    let _home_guard = EnvGuard::clear("HOME");
    let _xdg_guard = EnvGuard::clear("XDG_CONFIG_HOME");

    use clap::Parser;
    let config_arg = config_path.to_string_lossy().into_owned();
    let overrides = mike::config::CliOverrides::try_parse_from([
        "mike",
        "--config",
        config_arg.as_str(),
        "--upstream-url",
        "http://127.0.0.1:0",
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        &database_url,
    ])?;
    let config = mike::config::load(overrides)?;
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
    let rules: Vec<Value> = client
        .get(format!("{inspect_url}/api/filter/rules"))
        .send()
        .await?
        .json()
        .await?;
    let rule = rules
        .iter()
        .find(|r| r["value"] == value)
        .ok_or_else(|| anyhow::anyhow!("DB-seeded rule absent after restart: {rules:?}"))?;
    assert_eq!(
        rule["id"],
        Value::String(db_id.to_string()),
        "DTO id must be the stringified bigint rules.rules.id"
    );
    assert_eq!(rule["kind"], "text_substring", "kind_id code round-trips");
    assert_eq!(rule["action"], "replace", "action_id code round-trips");
    assert_eq!(rule["source"], "ui", "source_id code round-trips");
    assert_eq!(rule["text"], "redacted");
    assert_eq!(rule["enabled"], true);
    assert_eq!(rule["sort_order"].as_i64(), Some(5));

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    pool.close().await;
    drop(serial);
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn post_does_not_touch_config_toml() -> anyhow::Result<()> {
    let fixture = std::fs::read("crates/mike/tests/fixtures/config/with_comments.toml")
        .or_else(|_| std::fs::read("tests/fixtures/config/with_comments.toml"))?;
    let mut seeded = b"persist_enabled = true\n\n".to_vec();
    seeded.extend_from_slice(&fixture);
    let Some(proxy) = start_proxy_with_config(&seeded).await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let pre_body = std::fs::read(&proxy.config_path)?;

    let value = format!("tc-byte-{}", uuid::Uuid::new_v4());
    let resp = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({"kind": "xml_tag", "value": value, "action": "drop"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 201);

    let post_body = std::fs::read(&proxy.config_path)?;
    assert_eq!(
        pre_body, post_body,
        "filter rules are DB-only state — a POST must leave config.toml byte-identical"
    );
    Ok(())
}
