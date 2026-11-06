use crate::db_helpers;
use serde_json::Value;
use sqlx::PgPool;

use super::support::{ensure_parents, seed_request_with_body, start_upstream_stub};

async fn seed_two_sibling_sessions(
    pool: &PgPool,
    session_a: &str,
    session_b: &str,
    project_path: &str,
) -> anyhow::Result<(i64, i64, i64)> {
    let project_id = ensure_parents(pool, session_a, project_path).await?;
    ensure_parents(pool, session_b, project_path).await?;

    let req_a1 = seed_request_with_body(pool, session_a, project_id, b"{\"a\":1}", 0).await?;
    let req_a2 = seed_request_with_body(pool, session_a, project_id, b"{\"a\":2}", 10).await?;
    let req_b = seed_request_with_body(pool, session_b, project_id, b"{\"b\":1}", 20).await?;
    Ok((req_a1, req_a2, req_b))
}

async fn assert_scoped_session_requests(
    inspect_url: &str,
    session_a: &str,
    req_a1: i64,
    req_a2: i64,
    req_b: i64,
) -> anyhow::Result<()> {
    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/sessions/{session_a}/requests"))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "GET /api/sessions/{session_a}/requests returned {}",
        resp.status()
    );
    let page: Value = resp.json().await?;

    anyhow::ensure!(
        page["session_id"].as_str() == Some(session_a),
        "page echoes wrong session_id: {}",
        page["session_id"]
    );
    anyhow::ensure!(
        page["total_requests"].as_u64() == Some(2),
        "expected total_requests=2 for session A, got {}",
        page["total_requests"]
    );

    let rows = page["requests"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("requests must be an array"))?;
    anyhow::ensure!(rows.len() == 2, "expected 2 rows, got {}", rows.len());

    let mut ids: Vec<u64> = Vec::new();
    for r in rows {
        anyhow::ensure!(
            r["session_id"].as_str() == Some(session_a),
            "scoped list leaked a row from another session: {r}"
        );
        ids.push(
            r["id"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("row missing numeric id: {r}"))?,
        );
    }
    ids.sort_unstable();
    let mut expected = vec![req_a1 as u64, req_a2 as u64];
    expected.sort_unstable();
    anyhow::ensure!(ids == expected, "ids {ids:?} != expected {expected:?}");
    anyhow::ensure!(
        !ids.contains(&(req_b as u64)),
        "decoy request {req_b} from session B leaked into session A's list"
    );
    Ok(())
}

async fn assert_session_requests_pagination(
    inspect_url: &str,
    session_a: &str,
) -> anyhow::Result<()> {
    let resp_paged = reqwest::Client::new()
        .get(format!("{inspect_url}/api/sessions/{session_a}/requests"))
        .query(&[("limit", "1")])
        .send()
        .await?;
    anyhow::ensure!(resp_paged.status().is_success(), "paged fetch failed");
    let paged: Value = resp_paged.json().await?;
    anyhow::ensure!(
        paged["requests"].as_array().map(|a| a.len()) == Some(1),
        "limit=1 should return exactly one row"
    );
    anyhow::ensure!(
        paged["total_requests"].as_u64() == Some(2),
        "limit=1 must not shrink total_requests denominator"
    );
    anyhow::ensure!(
        paged["limit"].as_u64() == Some(1),
        "page should echo the applied limit"
    );
    Ok(())
}

#[tokio::test]
async fn get_session_requests_returns_only_that_sessions_rows() -> anyhow::Result<()> {
    let upstream = start_upstream_stub("msg_sreq").await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping phase18_new_endpoints");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_a = "dd000000-0000-4000-d000-000000000004";
    let session_b = "dd000000-0000-4000-d000-000000000005";
    let project_path = "phase18-session-requests-test";
    let (req_a1, req_a2, req_b) =
        seed_two_sibling_sessions(&pool, session_a, session_b, project_path).await?;

    assert_scoped_session_requests(&inspect_url, session_a, req_a1, req_a2, req_b).await?;
    assert_session_requests_pagination(&inspect_url, session_a).await?;

    let not_found = reqwest::Client::new()
        .get(format!(
            "{inspect_url}/api/sessions/00000000-dead-beef-dead-000000000000/requests"
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
