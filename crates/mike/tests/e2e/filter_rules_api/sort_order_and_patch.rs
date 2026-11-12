use crate::test_db;

use super::harness::{EnvGuard, acquire_env_lock, start_proxy_with_config};

use chrono::TimeZone;
use mike::app::inspect_router_on;
use mike::state::AppState;
use serde_json::{Value, json};
use test_db::TestDb;
use tokio::net::TcpListener;

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn get_rules_sorted_by_sort_order_asc_id_tiebreak() -> anyhow::Result<()> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let database_url = test_db.database_url.clone();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;

    let test_prefix = format!("tc-sort-{}", uuid::Uuid::new_v4());
    let t0 = chrono::Utc
        .with_ymd_and_hms(2026, 2, 1, 0, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("t0"))?;
    let t1 = t0 + chrono::Duration::seconds(1);
    let rows = [
        ("v-last", 30, t0),
        ("v-first", 10, t1),
        ("v-tied-a", 20, t1),
        ("v-tied-b", 20, t0),
    ];
    for (suffix, sort_order, ts) in &rows {
        sqlx::query(
            "INSERT INTO rules.rules
                (uuid, kind_id, value, action_id, text, enabled, source_id,
                 created_at, updated_at, sort_order)
             SELECT $1, rk.id, $2, ra.id, NULL, true, rs.id, $3, $3, $4
             FROM rules.kinds rk, rules.actions ra, rules.sources rs
             WHERE rk.code = 'xml_tag' AND ra.code = 'drop' AND rs.code = 'ui'",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(format!("{test_prefix}-{suffix}"))
        .bind(ts)
        .bind(sort_order)
        .execute(&pool)
        .await?;
    }

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
        test_db.database_url.as_str(),
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

    let listed: Vec<Value> = client
        .get(format!("{inspect_url}/api/filter/rules"))
        .send()
        .await?
        .json()
        .await?;
    let relevant: Vec<&Value> = listed
        .iter()
        .filter(|r| {
            r["value"]
                .as_str()
                .map(|s| s.starts_with(&test_prefix))
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(relevant.len(), 4, "expected 4 seeded rules: {relevant:?}");
    let values: Vec<String> = relevant
        .iter()
        .map(|r| r["value"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(
        values,
        vec![
            format!("{test_prefix}-v-first"),
            format!("{test_prefix}-v-tied-a"),
            format!("{test_prefix}-v-tied-b"),
            format!("{test_prefix}-v-last"),
        ],
        "sort order must be sort_order asc, id asc on ties (updated_at must not matter)"
    );
    let sort_orders: Vec<i64> = relevant
        .iter()
        .map(|r| r["sort_order"].as_i64().unwrap_or(-1))
        .collect();
    assert_eq!(sort_orders, vec![10, 20, 20, 30]);

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    pool.close().await;
    drop(serial);
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn create_assigns_incrementing_sort_order() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let mut sort_orders = Vec::new();
    for n in 0..2 {
        let envelope: Value = client
            .post(format!("{}/api/filter/rules", proxy.inspect_url))
            .json(&json!({
                "kind": "xml_tag",
                "value": format!("tc-sortinc-{n}-{}", uuid::Uuid::new_v4()),
                "action": "drop",
            }))
            .send()
            .await?
            .json()
            .await?;
        let so = envelope["rule"]["sort_order"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("create envelope must carry sort_order"))?;
        sort_orders.push(so);
    }
    assert!(
        sort_orders[0] >= 1,
        "sort_order starts at max+1 >= 1, got {sort_orders:?}"
    );
    assert_eq!(
        sort_orders[1],
        sort_orders[0] + 1,
        "each create appends at max(sort_order)+1: {sort_orders:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn patch_sort_order_reorders_rules() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let value_a = format!("tc-reorder-a-{}", uuid::Uuid::new_v4());
    let value_b = format!("tc-reorder-b-{}", uuid::Uuid::new_v4());
    let mut ids = Vec::new();
    let mut sort_orders = Vec::new();
    for value in [&value_a, &value_b] {
        let envelope: Value = client
            .post(format!("{}/api/filter/rules", proxy.inspect_url))
            .json(&json!({"kind": "xml_tag", "value": value, "action": "drop"}))
            .send()
            .await?
            .json()
            .await?;
        ids.push(
            envelope["rule"]["id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        );
        sort_orders.push(
            envelope["rule"]["sort_order"]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("create envelope must carry sort_order"))?,
        );
    }

    let list_values = |listed: Vec<Value>| -> Vec<String> {
        listed
            .iter()
            .filter(|r| {
                r["value"]
                    .as_str()
                    .map(|s| s.starts_with("tc-reorder-"))
                    .unwrap_or(false)
            })
            .map(|r| r["value"].as_str().unwrap_or("").to_string())
            .collect()
    };

    let listed: Vec<Value> = client
        .get(format!("{}/api/filter/rules", proxy.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(list_values(listed), vec![value_a.clone(), value_b.clone()]);

    let resp = client
        .patch(format!("{}/api/filter/rules/{}", proxy.inspect_url, ids[0]))
        .json(&json!({"sort_order": sort_orders[1] + 1}))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let envelope: Value = resp.json().await?;
    assert_eq!(
        envelope["rule"]["sort_order"].as_i64(),
        Some(sort_orders[1] + 1),
        "PATCH envelope must reflect the new sort_order"
    );

    let listed: Vec<Value> = client
        .get(format!("{}/api/filter/rules", proxy.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        list_values(listed),
        vec![value_b.clone(), value_a.clone()],
        "B must now precede A in engine order"
    );

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&proxy.test_db.database_url)
        .await?;
    let db_values: Vec<String> = sqlx::query_scalar(
        "SELECT value FROM rules.rules WHERE value LIKE 'tc-reorder-%'
         ORDER BY sort_order ASC, id ASC",
    )
    .fetch_all(&pool)
    .await?;
    pool.close().await;
    assert_eq!(db_values, vec![value_b, value_a]);
    Ok(())
}
