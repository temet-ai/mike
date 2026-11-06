use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::MockResponse;
use serde_json::Value;

use super::support::{SentPatchSeed, ensure_parents, seed_request_with_sent_patch};

#[tokio::test]
async fn find_text_request_sent_source_finds_needle_from_sent_patch_not_in_original()
-> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new("msg_ft6").minimal().build()),
        )
        .mount(&upstream)
        .await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping find_text_endpoint");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "f4000006-0000-4000-f000-000000000006";
    let project_path = "find-text-endpoint-test-6";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    let needle = "UNIQUE_FIND_TEXT_SENT_PATCH_NEEDLE";
    let placeholder = "ORIGINAL_UNMUTATED_PLACEHOLDER";
    let request_body = format!(r#"{{"messages":[{{"role":"user","content":"{placeholder}"}}]}}"#);
    let patch_start = request_body
        .find(placeholder)
        .ok_or_else(|| anyhow::anyhow!("placeholder must be present in the seeded body"))?
        as i32;
    let patch_end = patch_start + placeholder.len() as i32;

    let request_id = seed_request_with_sent_patch(
        &pool,
        SentPatchSeed {
            session_id,
            project_id,
            request_body: request_body.as_bytes(),
            patch_start,
            patch_end,
            patch_bytes: needle.as_bytes(),
            offset_ms: 0,
        },
    )
    .await?;

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/body/find"))
        .query(&[
            ("needle", needle),
            ("request_id", &request_id.to_string()),
            ("source", "request_sent"),
        ])
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "GET /api/body/find?source=request_sent returned {}",
        resp.status()
    );
    let result: Value = resp.json().await?;
    anyhow::ensure!(
        result["full_match_count"].as_u64().unwrap_or(0) >= 1,
        "issue #139 regression: source=request_sent must find a needle present only in the \
         reconstructed sent body; got: {result}"
    );

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/body/find"))
        .query(&[
            ("needle", needle),
            ("request_id", &request_id.to_string()),
            ("source", "request"),
        ])
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "GET /api/body/find?source=request returned {}",
        resp.status()
    );
    let result: Value = resp.json().await?;
    anyhow::ensure!(
        result["full_match_count"].as_u64().unwrap_or(0) == 0,
        "source=request must not find a needle that only exists in the sent-patched bytes; \
         got: {result}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn find_text_scoped_session_request_sent_source_finds_needle_from_sent_patch()
-> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new("msg_ft7").minimal().build()),
        )
        .mount(&upstream)
        .await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping find_text_endpoint");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "f4000007-0000-4000-a000-000000000007";
    let project_path = "find-text-endpoint-test-7";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    let needle = "UNIQUE_FIND_TEXT_SESSION_SENT_PATCH_NEEDLE";
    let placeholder = "ORIGINAL_UNMUTATED_SESSION_PLACEHOLDER";
    let request_body = format!(r#"{{"messages":[{{"role":"user","content":"{placeholder}"}}]}}"#);
    let patch_start = request_body
        .find(placeholder)
        .ok_or_else(|| anyhow::anyhow!("placeholder must be present in the seeded body"))?
        as i32;
    let patch_end = patch_start + placeholder.len() as i32;

    let request_id = seed_request_with_sent_patch(
        &pool,
        SentPatchSeed {
            session_id,
            project_id,
            request_body: request_body.as_bytes(),
            patch_start,
            patch_end,
            patch_bytes: needle.as_bytes(),
            offset_ms: 0,
        },
    )
    .await?;

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/body/find"))
        .query(&[
            ("needle", needle),
            ("session_id", session_id),
            ("source", "request_sent"),
        ])
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "GET /api/body/find?session_id&source=request_sent returned {}",
        resp.status()
    );
    let result: Value = resp.json().await?;
    let matches = result["matches"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("matches must be an array"))?;
    anyhow::ensure!(
        !matches.is_empty(),
        "session-scoped source=request_sent must find a needle present only in the \
         reconstructed sent body; got: {result}"
    );
    anyhow::ensure!(
        matches[0]["request_id"].as_u64() == Some(request_id as u64),
        "match must point at the seeded request: {result}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
