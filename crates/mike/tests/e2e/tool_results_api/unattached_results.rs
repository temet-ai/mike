use crate::db_helpers;
use serde_json::Value;

use super::support::{insert_emitting_request, start_upstream_stub};

#[tokio::test]
async fn get_request_tool_results_unknown_id_returns_404() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{inspect_url}/api/requests/999999999/tool-results"))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        404,
        "unknown request id must return 404 — the per-request route is id-keyed"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_request_tool_results_empty_when_no_results_attached() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-446655440004";
    let project_path = "test-project-empty";
    let request_id = insert_emitting_request(
        &pool,
        session_id,
        project_path,
        "toolu_no_result",
        "Bash",
        0,
    )
    .await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{inspect_url}/api/requests/{request_id}/tool-results"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    assert_eq!(body["request_id"].as_u64(), Some(request_id as u64));
    assert_eq!(body["total_count"].as_u64(), Some(0));
    assert!(
        body["results"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(false),
        "results must be an empty array when no tool_result blocks have been attached"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
