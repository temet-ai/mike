use crate::db_helpers;
use serde_json::Value;

use super::support::{ensure_parents, seed_request_with_body, start_upstream_stub};

#[tokio::test]
async fn get_session_head_returns_lean_fields_no_requests_array() -> anyhow::Result<()> {
    let upstream = start_upstream_stub("msg_head").await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping phase18_new_endpoints");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "bb000000-0000-4000-b000-000000000002";
    let project_path = "phase18-session-head-test";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    seed_request_with_body(&pool, session_id, project_id, b"{\"test\":true}", 0).await?;

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/sessions/{session_id}/head"))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "GET /api/sessions/{session_id}/head returned {}",
        resp.status()
    );
    let head: Value = resp.json().await?;

    anyhow::ensure!(
        head["session_id"].as_str().is_some(),
        "session_head missing session_id"
    );

    anyhow::ensure!(
        head.get("requests").is_none(),
        "session_head must not carry a requests array"
    );

    anyhow::ensure!(
        head["newest_request_id"].as_u64().is_some(),
        "session_head should have newest_request_id after seeding a request; got: {head}"
    );

    let not_found = reqwest::Client::new()
        .get(format!(
            "{inspect_url}/api/sessions/00000000-dead-beef-dead-000000000000/head"
        ))
        .send()
        .await?;
    anyhow::ensure!(
        not_found.status() == reqwest::StatusCode::NOT_FOUND,
        "unknown session should return 404, got {}",
        not_found.status()
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
