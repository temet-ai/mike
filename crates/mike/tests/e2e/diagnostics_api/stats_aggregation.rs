use crate::db_helpers;
use crate::upstream;

use db_helpers::start_proxy_with_db;
use serde_json::Value;

use super::support::{insert_diagnostic, send_request};

#[tokio::test]
async fn diagnostics_stats_aggregates_categories_and_analyzers() -> anyhow::Result<()> {
    let mock_server = upstream::canned_json_upstream("msg_a3_diagnostics").await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let session = uuid::Uuid::new_v4().to_string();
    send_request(&client, &proxy_url, &session, "u").await?;

    let row: (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT r.id, r.created_at FROM requests.requests r
          JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::uuid LIMIT 1",
    )
    .bind(&session)
    .fetch_one(&pool)
    .await?;
    let (req_id, _req_created_at) = row;

    let now = chrono::Utc::now();
    for i in 0..3 {
        let t = now - chrono::Duration::milliseconds(i);
        insert_diagnostic(&pool, req_id, "overselling", "weasel_overselling", t).await?;
    }
    insert_diagnostic(&pool, req_id, "avoiding_work", "weasel_avoiding", now).await?;
    insert_diagnostic(&pool, req_id, "avoiding_work", "weasel_other", now).await?;

    let actual_overselling: (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::bigint FROM requests.diagnostics
         WHERE category = 'overselling'",
    )
    .fetch_one(&pool)
    .await?;
    let actual_avoiding: (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::bigint FROM requests.diagnostics
         WHERE category = 'avoiding_work'",
    )
    .fetch_one(&pool)
    .await?;

    let stats: Value = client
        .get(format!("{inspect_url}/api/diagnostics/stats"))
        .send()
        .await?
        .json()
        .await?;
    let by_cat = stats
        .get("totals_by_category")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("missing totals_by_category"))?;
    let by_analyzer = stats
        .get("totals_by_analyzer")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("missing totals_by_analyzer"))?;

    let overselling_total = by_cat
        .get("overselling")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let avoiding_total = by_cat
        .get("avoiding_work")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(
        overselling_total as i64, actual_overselling.0,
        "overselling category total must match table contents"
    );
    assert_eq!(
        avoiding_total as i64, actual_avoiding.0,
        "avoiding_work category total must match table contents"
    );

    assert!(
        by_analyzer.get("weasel_overselling").is_some(),
        "weasel_overselling analyzer missing from response"
    );
    assert!(
        by_analyzer.get("weasel_avoiding").is_some(),
        "weasel_avoiding analyzer missing from response"
    );
    assert!(
        by_analyzer.get("weasel_other").is_some(),
        "weasel_other analyzer missing from response"
    );

    let last_24h = stats
        .get("last_24h")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("missing last_24h"))?;
    let total_actual: (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::bigint FROM requests.diagnostics
         WHERE created_at > NOW() - INTERVAL '24 hours'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        last_24h as i64, total_actual.0,
        "last_24h must equal the actual 24h-window count"
    );

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn diagnostics_stats_excludes_rows_older_than_24h() -> anyhow::Result<()> {
    let mock_server = upstream::canned_json_upstream("msg_a3_diagnostics").await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let session = uuid::Uuid::new_v4().to_string();
    send_request(&client, &proxy_url, &session, "u").await?;
    let row: (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT r.id, r.created_at FROM requests.requests r
          JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::uuid LIMIT 1",
    )
    .bind(&session)
    .fetch_one(&pool)
    .await?;
    let (req_id, _req_created_at) = row;

    let now = chrono::Utc::now();
    let two_days_ago = now - chrono::Duration::hours(48);

    insert_diagnostic(&pool, req_id, "overselling", "fresh_analyzer", now).await?;
    insert_diagnostic(&pool, req_id, "overselling", "stale_analyzer", two_days_ago).await?;

    let stats: Value = client
        .get(format!("{inspect_url}/api/diagnostics/stats"))
        .send()
        .await?
        .json()
        .await?;
    let by_analyzer = stats
        .get("totals_by_analyzer")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("missing totals_by_analyzer"))?;

    assert!(
        by_analyzer.get("fresh_analyzer").is_some(),
        "fresh_analyzer (within 24h) must appear in stats"
    );
    assert!(
        by_analyzer.get("stale_analyzer").is_none(),
        "stale_analyzer (>24h old) must be EXCLUDED — got {:?}",
        by_analyzer.get("stale_analyzer")
    );

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
