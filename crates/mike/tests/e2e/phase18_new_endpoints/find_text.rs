use crate::db_helpers;
use serde_json::Value;
use sqlx::PgPool;

use super::support::{ensure_parents, seed_request_with_body, start_upstream_stub};

async fn seed_needle_and_plain_requests(
    pool: &PgPool,
    session_id: &str,
    project_id: i64,
    needle: &str,
) -> anyhow::Result<i64> {
    let body_with_needle = format!(r#"{{"messages":[{{"role":"user","content":"{needle}"}}]}}"#);
    let body_without = r#"{"messages":[{"role":"user","content":"something else entirely"}]}"#;

    let req_with =
        seed_request_with_body(pool, session_id, project_id, body_with_needle.as_bytes(), 0)
            .await?;
    let _req_without =
        seed_request_with_body(pool, session_id, project_id, body_without.as_bytes(), 10).await?;
    Ok(req_with)
}

async fn assert_find_text_result(
    inspect_url: &str,
    session_id: &str,
    needle: &str,
    req_with: i64,
) -> anyhow::Result<()> {
    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/sessions/{session_id}/body/find"))
        .query(&[("needle", needle)])
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "find_text_in_session returned {}",
        resp.status()
    );
    let result: Value = resp.json().await?;

    anyhow::ensure!(
        result["full_match_count"].as_u64().is_some(),
        "response missing full_match_count"
    );
    anyhow::ensure!(
        result.get("matches_truncated").is_some(),
        "response missing matches_truncated"
    );

    let matches = result["matches"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("matches must be an array"))?;

    anyhow::ensure!(
        !matches.is_empty(),
        "expected at least one match for needle"
    );

    for m in matches {
        anyhow::ensure!(
            m["request_id"].as_u64().is_some(),
            "match missing request_id: {m}"
        );
    }

    let matched_req_id = matches[0]["request_id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("first match missing request_id"))?;
    anyhow::ensure!(
        matched_req_id == req_with as u64,
        "match points to wrong request: got {matched_req_id}, expected {req_with}"
    );
    Ok(())
}

async fn assert_absent_needle_returns_empty(
    inspect_url: &str,
    session_id: &str,
) -> anyhow::Result<()> {
    let resp_absent = reqwest::Client::new()
        .get(format!("{inspect_url}/api/sessions/{session_id}/body/find"))
        .query(&[("needle", "XNOMATCHXX")])
        .send()
        .await?;
    anyhow::ensure!(
        resp_absent.status().is_success(),
        "absent-needle call failed"
    );
    let result_absent: Value = resp_absent.json().await?;
    anyhow::ensure!(
        result_absent["full_match_count"].as_u64() == Some(0),
        "absent needle should give full_match_count=0"
    );
    Ok(())
}

async fn assert_find_text_404_unknown_session(inspect_url: &str) -> anyhow::Result<()> {
    let not_found = reqwest::Client::new()
        .get(format!(
            "{inspect_url}/api/sessions/00000000-dead-beef-dead-000000000000/body/find"
        ))
        .query(&[("needle", "x")])
        .send()
        .await?;
    anyhow::ensure!(
        not_found.status() == reqwest::StatusCode::NOT_FOUND,
        "unknown session should return 404, got {}",
        not_found.status()
    );
    Ok(())
}

#[tokio::test]
async fn find_text_in_session_locates_needle_across_requests() -> anyhow::Result<()> {
    let upstream = start_upstream_stub("msg_fts").await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping phase18_new_endpoints");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "cc000000-0000-4000-c000-000000000003";
    let project_path = "phase18-find-text-in-session-test";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    let needle = "UNIQUE_SEARCH_NEEDLE_P18";
    let req_with = seed_needle_and_plain_requests(&pool, session_id, project_id, needle).await?;

    assert_find_text_result(&inspect_url, session_id, needle, req_with).await?;
    assert_absent_needle_returns_empty(&inspect_url, session_id).await?;
    assert_find_text_404_unknown_session(&inspect_url).await?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
