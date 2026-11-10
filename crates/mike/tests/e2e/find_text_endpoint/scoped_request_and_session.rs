use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::MockResponse;
use serde_json::Value;

use super::support::{ensure_parents, seed_request_with_body};

#[tokio::test]
async fn find_text_scoped_by_request_id_returns_matches_with_both_ids() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new("msg_ft1").minimal().build()),
        )
        .mount(&upstream)
        .await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping find_text_endpoint");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "f4000001-0000-4000-a000-000000000001";
    let project_path = "find-text-endpoint-test-1";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    let needle = "UNIQUE_FIND_TEXT_NEEDLE_REQ_SCOPE";
    let body = format!(r#"{{"messages":[{{"role":"user","content":"{needle}"}}]}}"#);
    let request_id =
        seed_request_with_body(&pool, session_id, project_id, body.as_bytes(), 0).await?;

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/body/find"))
        .query(&[("needle", needle), ("request_id", &request_id.to_string())])
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "GET /api/body/find?request_id= returned {}",
        resp.status()
    );
    let result: Value = resp.json().await?;

    let matches = result["matches"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("matches must be an array"))?;

    anyhow::ensure!(
        !matches.is_empty(),
        "expected ≥1 match for needle in request {request_id}; got: {result}"
    );

    for m in matches {
        anyhow::ensure!(
            m["request_id"].as_u64().is_some(),
            "match missing request_id: {m}"
        );
        let sid = m["session_id"].as_str().unwrap_or("");
        anyhow::ensure!(
            !sid.is_empty(),
            "match has empty session_id — D2 honest-session fix may be broken: {m}"
        );
        anyhow::ensure!(
            m["request_id"].as_u64() == Some(request_id as u64),
            "match points to wrong request: {m}"
        );
    }

    anyhow::ensure!(
        result["full_match_count"].as_u64().unwrap_or(0) >= 1,
        "full_match_count should be ≥1: {result}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn find_text_scoped_by_session_id_returns_session_matches() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new("msg_ft2").minimal().build()),
        )
        .mount(&upstream)
        .await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping find_text_endpoint");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "f4000002-0000-4000-b000-000000000002";
    let project_path = "find-text-endpoint-test-2";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    let needle = "UNIQUE_FIND_TEXT_NEEDLE_SESSION_SCOPE";
    let body_with = format!(r#"{{"messages":[{{"role":"user","content":"{needle}"}}]}}"#);
    let body_without = r#"{"messages":[{"role":"user","content":"something else entirely"}]}"#;

    let _req_older_without_needle =
        seed_request_with_body(&pool, session_id, project_id, body_without.as_bytes(), 0).await?;
    let req_with =
        seed_request_with_body(&pool, session_id, project_id, body_with.as_bytes(), 10).await?;

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/body/find"))
        .query(&[
            ("needle", needle),
            ("session_id", session_id),
            ("whole_bodies", "true"),
        ])
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "GET /api/body/find?session_id= returned {}",
        resp.status()
    );
    let result: Value = resp.json().await?;

    let matches = result["matches"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("matches must be an array"))?;

    anyhow::ensure!(
        !matches.is_empty(),
        "expected ≥1 match for needle across session {session_id}"
    );

    for m in matches {
        anyhow::ensure!(
            m["request_id"].as_u64().is_some(),
            "match missing request_id: {m}"
        );
        let sid = m["session_id"].as_str().unwrap_or("");
        anyhow::ensure!(!sid.is_empty(), "match has empty session_id: {m}");
    }

    let matched_req_id = matches[0]["request_id"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("first match missing request_id"))?;
    anyhow::ensure!(
        matched_req_id == req_with as u64,
        "match points to wrong request: got {matched_req_id}, expected {req_with}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn find_text_scoped_session_request_source_finds_needle_in_any_request() -> anyhow::Result<()>
{
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new("msg_ft5").minimal().build()),
        )
        .mount(&upstream)
        .await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping find_text_endpoint");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "f4000005-0000-4000-e000-000000000005";
    let project_path = "find-text-endpoint-test-5";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    let old_needle = "UNIQUE_FIND_TEXT_OLD_REQUEST_ONLY";
    let old_body = format!(r#"{{"messages":[{{"role":"user","content":"{old_needle}"}}]}}"#);
    let old_request_id =
        seed_request_with_body(&pool, session_id, project_id, old_body.as_bytes(), 0).await?;

    let latest_needle = "UNIQUE_FIND_TEXT_LATEST_REQUEST_NEEDLE";
    let latest_body = format!(r#"{{"messages":[{{"role":"user","content":"{latest_needle}"}}]}}"#);
    let latest_request_id =
        seed_request_with_body(&pool, session_id, project_id, latest_body.as_bytes(), 1000).await?;

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/body/find"))
        .query(&[
            ("needle", old_needle),
            ("session_id", session_id),
            ("source", "request"),
            ("whole_bodies", "true"),
        ])
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "GET /api/body/find?session_id&source=request (old needle) returned {}",
        resp.status()
    );
    let result: Value = resp.json().await?;
    let matches = result["matches"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("matches must be an array"))?;
    anyhow::ensure!(
        !matches.is_empty(),
        "issues #100/#109/#127 regression: the older request's needle must surface via \
         session-scoped source=request — a session-wide scan must not be limited to the \
         latest request; got: {result}"
    );
    anyhow::ensure!(
        matches[0]["request_id"].as_u64() == Some(old_request_id as u64),
        "match must point at the request that actually holds the needle: {result}"
    );
    anyhow::ensure!(
        result["has_more"].as_bool() == Some(false),
        "only 2 requests exist — the scan must complete in one page; got: {result}"
    );
    anyhow::ensure!(
        result["full_match_count"].as_u64() == Some(1),
        "full_match_count must be an honest, non-null total once the scan completes; got: {result}"
    );

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/body/find"))
        .query(&[
            ("needle", latest_needle),
            ("session_id", session_id),
            ("source", "request"),
            ("whole_bodies", "true"),
        ])
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "GET /api/body/find?session_id&source=request (latest needle) returned {}",
        resp.status()
    );
    let result: Value = resp.json().await?;
    let matches = result["matches"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("matches must be an array"))?;
    anyhow::ensure!(
        !matches.is_empty(),
        "expected ≥1 match for the latest request's needle; got: {result}"
    );
    anyhow::ensure!(
        matches[0]["request_id"].as_u64() == Some(latest_request_id as u64),
        "match must point at the session's latest request: {result}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
