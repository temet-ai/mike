use crate::db_helpers;
use db_helpers::start_proxy_with_db;
use serde_json::Value;

use super::support::{mount_upstream, send_request};

#[tokio::test]
async fn list_sessions_project_filter_returns_full_row_shape() -> anyhow::Result<()> {
    let mock_server = mount_upstream().await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let in_beta = uuid::Uuid::new_v4().to_string();
    let in_sentinel = uuid::Uuid::new_v4().to_string();
    send_request(&client, &proxy_url, &in_beta, "beta session").await?;
    send_request(&client, &proxy_url, &in_sentinel, "sentinel session").await?;

    sqlx::query(
        "WITH proj AS (
             INSERT INTO projects.projects (project_path, display_name)
             VALUES ('proj-beta/filter-shape', 'proj-beta')
             RETURNING id
         )
         UPDATE sessions.sessions s
            SET project_id = (SELECT id FROM proj),
                overview = 'beta overview text'
          WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(&in_beta)
    .execute(&pool)
    .await?;

    for extra in ["", "&min_request_count=1"] {
        let resp = client
            .get(format!(
                "{}/api/sessions?project=proj-beta{extra}",
                &inspect_url
            ))
            .send()
            .await?;
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::OK,
            "project-filtered listing (extra={extra:?}) must not fail row decode"
        );
        let rows: Value = resp.json().await?;
        let arr = rows
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("not array"))?;
        assert_eq!(arr.len(), 1, "filter must isolate the beta project");
        let row = &arr[0];
        assert_eq!(
            row.get("id").and_then(|v| v.as_str()),
            Some(in_beta.as_str())
        );
        assert_eq!(
            row.get("narrative_overview").and_then(|v| v.as_str()),
            Some("beta overview text"),
            "overview column must survive the filtered projection"
        );
        assert_eq!(
            row.get("pending_inbox_count").and_then(Value::as_i64),
            Some(0),
            "pending_inbox_count must be a real count, never absent"
        );
        assert_eq!(row.get("request_count").and_then(Value::as_i64), Some(1));
    }

    let none: Value = client
        .get(format!(
            "{}/api/sessions?project=proj-beta&min_request_count=5",
            &inspect_url
        ))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        none.as_array().map(Vec::len),
        Some(0),
        "count threshold above actual must yield an empty page"
    );

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
