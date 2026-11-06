use crate::db_helpers;
use db_helpers::start_proxy_with_db;
use serde_json::Value;

use super::support::{mount_upstream, send_request};

#[tokio::test]
async fn list_sessions_effective_pinned_cascade_from_project() -> anyhow::Result<()> {
    let mock_server = mount_upstream().await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let session = uuid::Uuid::new_v4().to_string();
    send_request(&client, &proxy_url, &session, "u").await?;

    let initial: Value = client
        .get(format!("{}/api/sessions?limit=200", &inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let initial_arr = initial
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    let target = initial_arr
        .iter()
        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(&session))
        .ok_or_else(|| anyhow::anyhow!("session not found in listing"))?;
    assert_eq!(
        target.get("pinned"),
        Some(&Value::Bool(false)),
        "initial pinned must be false"
    );

    let project_id_row: (i64,) = sqlx::query_as(
        "SELECT s.project_id FROM sessions.sessions s WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(&session)
    .fetch_one(&pool)
    .await?;
    let project_id = project_id_row.0;

    sqlx::query("UPDATE projects.projects SET pinned = TRUE WHERE id = $1")
        .bind(project_id)
        .execute(&pool)
        .await?;

    let after_pin: Value = client
        .get(format!("{}/api/sessions?limit=200", &inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let after_pin_arr = after_pin
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    let after_target = after_pin_arr
        .iter()
        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(&session))
        .ok_or_else(|| anyhow::anyhow!("session not found post-pin"))?;
    assert_eq!(
        after_target.get("pinned"),
        Some(&Value::Bool(true)),
        "project-level pin must cascade to the session row's effective_pinned"
    );

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
