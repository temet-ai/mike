use crate::db_helpers;

use super::fixtures::{ensure_parents, seed_request, start_upstream_stub};

use serde_json::Value;

#[tokio::test]
async fn declared_roundtrip_preserves_order_and_duplicates() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping request_tools_roundtrip");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-44665544aa01";
    let project_path = "test-project-rt-declared";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    let declared = ["Bash", "Read", "Bash"];
    let (request_id, _created_at) =
        seed_request(&pool, session_id, project_id, &declared, &[], 0).await?;

    let rows: Vec<(i32, String)> = sqlx::query_as(
        "SELECT ord, tool_name FROM requests.tools
          WHERE request_id = $1 ORDER BY ord",
    )
    .bind(request_id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        rows,
        vec![
            (0, "Bash".to_string()),
            (1, "Read".to_string()),
            (2, "Bash".to_string())
        ],
        "request_tools must preserve declaration order incl. duplicates"
    );

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/requests/{request_id}"))
        .send()
        .await?;
    let status = resp.status();
    let raw = resp.text().await?;
    anyhow::ensure!(
        status.is_success(),
        "GET /api/requests/{request_id} returned {status}; body={raw:?}"
    );
    let detail: Value = serde_json::from_str(&raw)?;
    let tool_names: Vec<String> = serde_json::from_value(
        detail
            .get("tool_names")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("RequestDetail.tool_names missing"))?,
    )?;
    assert_eq!(
        tool_names,
        vec!["Bash", "Read", "Bash"],
        "fetch_request_by_id must return declared tool_names in order with duplicates"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn empty_declared_yields_empty_vec() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping request_tools_roundtrip");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-44665544aa02";
    let project_path = "test-project-rt-empty";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    let (request_id, _created_at) =
        seed_request(&pool, session_id, project_id, &[], &[], 0).await?;

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM requests.tools WHERE request_id = $1")
        .bind(request_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(count.0, 0, "no declared tools => zero request_tools rows");

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/requests/{request_id}"))
        .send()
        .await?;
    let status = resp.status();
    let raw = resp.text().await?;
    anyhow::ensure!(
        status.is_success(),
        "GET /api/requests/{request_id} returned {status}; body={raw:?}"
    );
    let detail: Value = serde_json::from_str(&raw)?;
    let tool_names: Vec<String> = serde_json::from_value(
        detail
            .get("tool_names")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("RequestDetail.tool_names missing"))?,
    )?;
    assert!(
        tool_names.is_empty(),
        "fetch_request_by_id must yield an empty Vec, not an error, for no declared tools; got {tool_names:?}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
