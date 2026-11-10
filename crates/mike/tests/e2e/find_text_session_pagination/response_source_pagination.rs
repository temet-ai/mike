use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::MockResponse;
use serde_json::Value;

use super::support::{PAGE_SIZE, ensure_parents, seed_request_with_body, seed_response_body};

fn assert_one_call_covers_the_session(answer: &Value, needle_id: i64) -> anyhow::Result<()> {
    anyhow::ensure!(
        answer["scan_complete"].as_bool() == Some(true),
        "a session scan of {} requests must cover the whole session in one call: {answer}",
        PAGE_SIZE + 2
    );
    anyhow::ensure!(
        answer["has_more"].as_bool() == Some(false) && answer["next_cursor"].is_null(),
        "a complete scan carries no continuation: {answer}"
    );
    anyhow::ensure!(
        answer["full_match_count"].as_u64() == Some(1),
        "a complete scan must report the true total: {answer}"
    );
    anyhow::ensure!(
        answer["requests_scanned"].as_u64() == Some(PAGE_SIZE as u64 + 2)
            && answer["requests_total"].as_u64() == Some(PAGE_SIZE as u64 + 2),
        "requests_scanned and requests_total must both be the session size: {answer}"
    );
    let matches = answer["matches"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("matches must be an array: {answer}"))?;
    anyhow::ensure!(
        matches.len() == 1 && matches[0]["request_id"].as_u64() == Some(needle_id as u64),
        "expected exactly the match in the last request ({needle_id}): {answer}"
    );
    Ok(())
}

#[tokio::test]
async fn response_source_session_scan_spanning_two_windows_completes_in_one_call()
-> anyhow::Result<()> {
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

    for i in 0..PAGE_SIZE {
        let id = seed_request_with_body(&pool, session_id, project_id, b"{}", i).await?;
        seed_response_body(&pool, id, plain_response_body).await?;
    }

    let response_with_needle = format!(r#"{{"content":[{{"type":"text","text":"{needle}"}}]}}"#);
    let plain_id = seed_request_with_body(&pool, session_id, project_id, b"{}", PAGE_SIZE).await?;
    seed_response_body(&pool, plain_id, plain_response_body).await?;
    let needle_id =
        seed_request_with_body(&pool, session_id, project_id, b"{}", PAGE_SIZE + 1).await?;
    seed_response_body(&pool, needle_id, response_with_needle.as_bytes()).await?;

    let resp = reqwest::Client::new()
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
        "call returned {}",
        resp.status()
    );
    let answer: Value = resp.json().await?;
    assert_one_call_covers_the_session(&answer, needle_id)?;

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
