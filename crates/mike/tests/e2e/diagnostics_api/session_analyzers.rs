use crate::db_helpers;
use crate::upstream;

use db_helpers::start_proxy_with_db;
use serde_json::{Value, json};

use super::support::{DiagInsert, insert_diagnostic_full, send_request};

#[tokio::test]
async fn session_analyzers_reports_real_hit_counts_and_intercept_row() -> anyhow::Result<()> {
    let mock_server = upstream::canned_json_upstream("msg_a3_session_analyzers").await;

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

    for i in 0..2 {
        insert_diagnostic_full(
            &pool,
            DiagInsert {
                request_id: req_id,
                category: "overselling",
                analyzer: "overselling_analyzer",
                advice: "advice",
                matched_text: "matched",
                diag_created_at: chrono::Utc::now() + chrono::Duration::milliseconds(i),
            },
        )
        .await?;
    }

    sqlx::query("UPDATE requests.requests SET intercepted = true WHERE id = $1")
        .bind(req_id)
        .execute(&pool)
        .await?;

    let resp = client
        .get(format!("{inspect_url}/api/sessions/{session}/analyzers"))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "expected 200 for session analyzers"
    );
    let body: Value = resp.json().await?;
    let arr = body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected a JSON array, got {body}"))?;

    assert_eq!(
        arr.len(),
        6,
        "expected 5 category analyzers + weasel_intercept row, got {arr:?}"
    );

    let overselling = arr
        .iter()
        .find(|r| r["analyzer_name"] == "overselling_analyzer")
        .ok_or_else(|| anyhow::anyhow!("missing overselling_analyzer row in {arr:?}"))?;
    assert_eq!(
        overselling["hit_count"], 2,
        "overselling_analyzer hit_count must match the 2 seeded diagnostics"
    );
    assert_eq!(overselling["is_intercept"], false);
    assert!(
        overselling["enabled"].as_bool().unwrap_or(false),
        "overselling_analyzer must be enabled by default (no-default-off)"
    );

    let weasel = arr
        .iter()
        .find(|r| r["analyzer_name"] == "weasel_intercept")
        .ok_or_else(|| anyhow::anyhow!("missing weasel_intercept row in {arr:?}"))?;
    assert_eq!(
        weasel["hit_count"], 1,
        "weasel_intercept hit_count must count the 1 intercepted request"
    );
    assert_eq!(weasel["is_intercept"], true);

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn patch_session_analyzer_sets_override_and_get_reflects_it() -> anyhow::Result<()> {
    let mock_server = upstream::canned_json_upstream("msg_a3_session_analyzers").await;

    let Some(proxy) = start_proxy_with_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let session = uuid::Uuid::new_v4().to_string();
    send_request(&client, &proxy_url, &session, "u").await?;

    let patch_resp = client
        .patch(format!(
            "{inspect_url}/api/sessions/{session}/analyzers/overselling_analyzer"
        ))
        .json(&json!({ "enabled": false }))
        .send()
        .await?;
    assert_eq!(
        patch_resp.status(),
        reqwest::StatusCode::OK,
        "expected 200 for a known analyzer name on a known session"
    );
    let patch_body: Value = patch_resp.json().await?;
    assert_eq!(patch_body["analyzer_name"], "overselling_analyzer");
    assert_eq!(patch_body["enabled"], false);

    let get_resp = client
        .get(format!("{inspect_url}/api/sessions/{session}/analyzers"))
        .send()
        .await?;
    let body: Value = get_resp.json().await?;
    let arr = body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected a JSON array, got {body}"))?;

    let overselling = arr
        .iter()
        .find(|r| r["analyzer_name"] == "overselling_analyzer")
        .ok_or_else(|| anyhow::anyhow!("missing overselling_analyzer row in {arr:?}"))?;
    assert_eq!(
        overselling["enabled"], false,
        "PATCH override must be reflected by GET"
    );

    let deflecting = arr
        .iter()
        .find(|r| r["analyzer_name"] == "deflecting_blame_analyzer")
        .ok_or_else(|| anyhow::anyhow!("missing deflecting_blame_analyzer row in {arr:?}"))?;
    assert_eq!(
        deflecting["enabled"], true,
        "un-patched analyzers must be unaffected by another analyzer's override"
    );

    let bad_patch = client
        .patch(format!(
            "{inspect_url}/api/sessions/{session}/analyzers/not_a_real_analyzer"
        ))
        .json(&json!({ "enabled": false }))
        .send()
        .await?;
    assert_eq!(bad_patch.status(), reqwest::StatusCode::NOT_FOUND);

    let unknown_session_patch = client
        .patch(format!(
            "{inspect_url}/api/sessions/{}/analyzers/overselling_analyzer",
            uuid::Uuid::new_v4()
        ))
        .json(&json!({ "enabled": false }))
        .send()
        .await?;
    assert_eq!(
        unknown_session_patch.status(),
        reqwest::StatusCode::NOT_FOUND
    );

    shutdown.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
