use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::MockResponse;
use serde_json::Value;

use super::support::{PAGE_SIZE, ensure_parents, seed_request_with_body, seed_response_body};

fn assert_first_page_reports_null_not_zero(page1: &Value, last_id: i64) -> anyhow::Result<u64> {
    anyhow::ensure!(
        page1["matches"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(false),
        "first page must find 0 matches (needle is past the page boundary): {page1}"
    );
    anyhow::ensure!(
        page1["has_more"].as_bool() == Some(true),
        "first page must report has_more:true (2 requests remain unscanned): {page1}"
    );
    anyhow::ensure!(
        page1["full_match_count"].is_null(),
        "full_match_count must be null (NOT 0) while has_more is true — a 0 here reads as \
         false authoritative absence, the issue #175 bug: {page1}"
    );
    let cursor = page1["next_cursor"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("expected next_cursor on a has_more:true page: {page1}"))?;
    anyhow::ensure!(
        cursor as i64 == last_id,
        "next_cursor must resume after the last scanned id ({last_id}), got {cursor}"
    );
    Ok(cursor)
}

fn assert_final_page_completes_scan(page2: &Value) -> anyhow::Result<()> {
    anyhow::ensure!(
        page2["has_more"].as_bool() == Some(false),
        "second page must complete the scan (only 2 requests remained): {page2}"
    );
    anyhow::ensure!(
        page2["full_match_count"].as_u64() == Some(1),
        "second page must report an honest full_match_count once the scan completes: {page2}"
    );
    let matches2 = page2["matches"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("matches must be an array: {page2}"))?;
    anyhow::ensure!(
        matches2.len() == 1,
        "expected exactly 1 match on the second page: {page2}"
    );
    Ok(())
}

#[tokio::test]
async fn full_match_count_is_null_not_zero_while_session_has_unscanned_pages() -> anyhow::Result<()>
{
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new("msg_175").minimal().build()),
        )
        .mount(&upstream)
        .await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping find_text_session_pagination");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "cc000000-0000-4000-c000-000000175001";
    let project_path = "find-text-175-pagination-test";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    let needle = "UNIQUE_NEEDLE_ISSUE_175_BEYOND_PAGE_ONE";
    let plain_response_body = br#"{"content":[{"type":"text","text":"nothing interesting here"}]}"#;

    let mut last_id = 0i64;
    for i in 0..PAGE_SIZE {
        last_id = seed_request_with_body(&pool, session_id, project_id, b"{}", i).await?;
        seed_response_body(&pool, last_id, plain_response_body).await?;
    }

    let response_with_needle = format!(r#"{{"content":[{{"type":"text","text":"{needle}"}}]}}"#);
    let plain_id = seed_request_with_body(&pool, session_id, project_id, b"{}", PAGE_SIZE).await?;
    seed_response_body(&pool, plain_id, plain_response_body).await?;
    let needle_id =
        seed_request_with_body(&pool, session_id, project_id, b"{}", PAGE_SIZE + 1).await?;
    seed_response_body(&pool, needle_id, response_with_needle.as_bytes()).await?;

    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/body/find"))
        .query(&[
            ("needle", needle),
            ("session_id", session_id),
            ("source", "response"),
        ])
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "first page returned {}",
        resp.status()
    );
    let page1: Value = resp.json().await?;
    let cursor = assert_first_page_reports_null_not_zero(&page1, last_id)?;

    let resp2 = client
        .get(format!("{inspect_url}/api/body/find"))
        .query(&[
            ("needle", needle),
            ("session_id", session_id),
            ("source", "response"),
            ("cursor", &cursor.to_string()),
        ])
        .send()
        .await?;
    anyhow::ensure!(
        resp2.status().is_success(),
        "second page returned {}",
        resp2.status()
    );
    let page2: Value = resp2.json().await?;
    assert_final_page_completes_scan(&page2)?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
