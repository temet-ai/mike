use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::MockResponse;
use serde_json::Value;

use super::support::{PAGE_SIZE, ensure_parents, seed_request_with_body};

async fn scan_all_pages_accumulating_matches(
    client: &reqwest::Client,
    inspect_url: &str,
    needle: &str,
    session_id: &str,
) -> anyhow::Result<(Vec<Value>, u32)> {
    let mut page: Value = client
        .get(format!("{inspect_url}/api/body/find"))
        .query(&[
            ("needle", needle),
            ("session_id", session_id),
            ("source", "request"),
        ])
        .send()
        .await?
        .json()
        .await?;

    let mut accumulated_matches: Vec<Value> = Vec::new();
    let mut pages_scanned = 1;
    loop {
        let has_more = page["has_more"].as_bool();
        if has_more == Some(true) {
            anyhow::ensure!(
                page["full_match_count"].is_null(),
                "an incomplete session scan must report full_match_count:null, never a page-scoped zero: {page}"
            );
        }
        let page_matches = page["matches"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("matches must be an array: {page}"))?;
        accumulated_matches.extend(page_matches.iter().cloned());

        if has_more != Some(true) {
            break;
        }
        let cursor = page["next_cursor"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("has_more:true page missing next_cursor: {page}"))?;
        page = client
            .get(format!("{inspect_url}/api/body/find"))
            .query(&[
                ("needle", needle),
                ("session_id", session_id),
                ("source", "request"),
                ("cursor", &cursor.to_string()),
            ])
            .send()
            .await?
            .json()
            .await?;
        pages_scanned += 1;
        anyhow::ensure!(
            pages_scanned <= 5,
            "runaway pagination — did not converge: {page}"
        );
    }

    Ok((accumulated_matches, pages_scanned))
}

#[tokio::test]
async fn request_source_session_scope_finds_needle_earlier_request_scope_finds()
-> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new("msg_127").minimal().build()),
        )
        .mount(&upstream)
        .await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping request_source_session_scope_finds_needle_earlier_request_scope_finds"
        );
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "cc000000-0000-4000-c000-000000127001";
    let project_path = "find-text-127-request-scope-test";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    let needle = "UNIQUE_NEEDLE_ISSUE_127_NON_LATEST_REQUEST";
    let needle_body = format!(r#"{{"messages":[{{"role":"user","content":"{needle}"}}]}}"#);
    let plain_request_body = br#"{"messages":[]}"#;

    let needle_id =
        seed_request_with_body(&pool, session_id, project_id, needle_body.as_bytes(), 0).await?;
    for i in 1..=PAGE_SIZE {
        seed_request_with_body(&pool, session_id, project_id, plain_request_body, i).await?;
    }

    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/requests/{needle_id}/body/find"))
        .query(&[("needle", needle), ("source", "request")])
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "request-scope lookup returned {}",
        resp.status()
    );
    let request_scope: Value = resp.json().await?;
    anyhow::ensure!(
        request_scope["full_match_count"].as_u64() == Some(1),
        "control: request-scope must find the needle directly in request {needle_id}: {request_scope}"
    );

    let (accumulated_matches, pages_scanned) =
        scan_all_pages_accumulating_matches(&client, &inspect_url, needle, session_id).await?;

    anyhow::ensure!(
        accumulated_matches.len() == 1,
        "session-scope must find exactly the 1 needle across all pages — issues #100/#109/#127 \
         regression: session scope reported authoritative zero for a needle request-scope found. \
         accumulated matches: {accumulated_matches:?}"
    );
    anyhow::ensure!(
        accumulated_matches[0]["request_id"].as_u64() == Some(needle_id as u64),
        "the match must be attributed to the request that actually holds it ({needle_id}): \
         {accumulated_matches:?}"
    );
    anyhow::ensure!(
        pages_scanned >= 2,
        "expected the scan to span at least 2 pages (61 requests > {PAGE_SIZE}-request page size), \
         got {pages_scanned} — the test no longer exercises pagination"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
