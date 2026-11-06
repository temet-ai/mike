use crate::db_helpers;
use crate::upstream;

use db_helpers::start_proxy_with_db;
use serde_json::Value;

use super::support::{insert_diagnostic, send_request};

async fn seed_two_project_sessions(
    client: &reqwest::Client,
    proxy_url: &str,
    pool: &sqlx::PgPool,
) -> anyhow::Result<()> {
    let session_beta = uuid::Uuid::new_v4().to_string();
    let session_sentinel = uuid::Uuid::new_v4().to_string();
    send_request(client, proxy_url, &session_beta, "beta session").await?;
    send_request(client, proxy_url, &session_sentinel, "sentinel session").await?;

    let req_id_beta: (i64,) = sqlx::query_as(
        "SELECT r.id FROM requests.requests r
          JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::uuid LIMIT 1",
    )
    .bind(&session_beta)
    .fetch_one(pool)
    .await?;
    let req_id_sentinel: (i64,) = sqlx::query_as(
        "SELECT r.id FROM requests.requests r
          JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::uuid LIMIT 1",
    )
    .bind(&session_sentinel)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "WITH proj AS (
             INSERT INTO projects.projects (project_path, display_name)
             VALUES ('proj-beta/diagnostics-filter', 'proj-beta')
             RETURNING id
         )
         UPDATE sessions.sessions s
            SET project_id = (SELECT id FROM proj)
          WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(&session_beta)
    .execute(pool)
    .await?;

    let now = chrono::Utc::now();
    insert_diagnostic(pool, req_id_beta.0, "overselling", "beta_analyzer", now).await?;
    insert_diagnostic(
        pool,
        req_id_sentinel.0,
        "avoiding_work",
        "sentinel_analyzer",
        now,
    )
    .await?;
    Ok(())
}

async fn assert_stats_project_filter_isolates_beta(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let stats_filtered: Value = client
        .get(format!(
            "{inspect_url}/api/diagnostics/stats?project=proj-beta"
        ))
        .send()
        .await?
        .json()
        .await?;
    let by_analyzer_filtered = stats_filtered
        .get("totals_by_analyzer")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("missing totals_by_analyzer"))?;
    assert!(
        by_analyzer_filtered.get("beta_analyzer").is_some(),
        "beta_analyzer must appear under ?project=proj-beta"
    );
    assert!(
        by_analyzer_filtered.get("sentinel_analyzer").is_none(),
        "sentinel_analyzer must be EXCLUDED under ?project=proj-beta, got {:?}",
        by_analyzer_filtered.get("sentinel_analyzer")
    );
    let by_cat_filtered = stats_filtered
        .get("totals_by_category")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("missing totals_by_category"))?;
    assert_eq!(
        by_cat_filtered.get("overselling").and_then(Value::as_u64),
        Some(1),
        "totals must be true totals for the filtered project, not just a presence flag"
    );
    assert!(
        by_cat_filtered.get("avoiding_work").is_none(),
        "avoiding_work belongs to the sentinel project and must not leak into the filtered totals"
    );
    Ok(())
}

async fn assert_stats_unfiltered_includes_both(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let stats_unfiltered: Value = client
        .get(format!("{inspect_url}/api/diagnostics/stats"))
        .send()
        .await?
        .json()
        .await?;
    let by_analyzer_unfiltered = stats_unfiltered
        .get("totals_by_analyzer")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("missing totals_by_analyzer"))?;
    assert!(
        by_analyzer_unfiltered.get("beta_analyzer").is_some(),
        "unfiltered stats must still include beta_analyzer"
    );
    assert!(
        by_analyzer_unfiltered.get("sentinel_analyzer").is_some(),
        "unfiltered stats must still include sentinel_analyzer"
    );
    Ok(())
}

async fn assert_findings_project_filter_isolates_beta(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let findings_filtered: Value = client
        .get(format!(
            "{inspect_url}/api/diagnostics/findings?project=proj-beta&limit=100"
        ))
        .send()
        .await?
        .json()
        .await?;
    let findings_filtered_arr = findings_filtered
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    assert_eq!(
        findings_filtered_arr.len(),
        1,
        "project filter must isolate the beta project's single finding"
    );
    assert_eq!(
        findings_filtered_arr[0]
            .get("analyzer")
            .and_then(|v| v.as_str()),
        Some("beta_analyzer")
    );
    Ok(())
}

async fn assert_findings_unfiltered_and_total(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let findings_unfiltered: Value = client
        .get(format!("{inspect_url}/api/diagnostics/findings?limit=100"))
        .send()
        .await?
        .json()
        .await?;
    let findings_unfiltered_arr = findings_unfiltered
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("not array"))?;
    assert!(
        findings_unfiltered_arr.len() >= 2,
        "unfiltered findings must still include both projects, got {}",
        findings_unfiltered_arr.len()
    );

    let findings_total: Value = client
        .get(format!(
            "{inspect_url}/api/diagnostics/findings?project=proj-beta&total=true"
        ))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        findings_total.get("total").and_then(Value::as_u64),
        Some(1),
        "?total=true must report the filtered-set total, not the unfiltered total"
    );
    Ok(())
}

#[tokio::test]
async fn diagnostics_stats_and_findings_project_filter_isolates_target_project()
-> anyhow::Result<()> {
    let mock_server = upstream::canned_json_upstream("msg_a3_diagnostics").await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    seed_two_project_sessions(&client, &proxy_url, &pool).await?;

    assert_stats_project_filter_isolates_beta(&client, &inspect_url).await?;
    assert_stats_unfiltered_includes_both(&client, &inspect_url).await?;
    assert_findings_project_filter_isolates_beta(&client, &inspect_url).await?;
    assert_findings_unfiltered_and_total(&client, &inspect_url).await?;

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
