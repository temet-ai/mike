use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::MockResponse;
use serde_json::Value;

use super::support::{PAGE_SIZE, ensure_parents, seed_request_with_body};

async fn scan_whole_bodies_once(
    client: &reqwest::Client,
    inspect_url: &str,
    needle: &str,
    session_id: &str,
) -> anyhow::Result<Value> {
    let resp = client
        .get(format!("{inspect_url}/api/body/find"))
        .query(&[
            ("needle", needle),
            ("session_id", session_id),
            ("source", "request"),
            ("whole_bodies", "true"),
        ])
        .send()
        .await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "call returned {}",
        resp.status()
    );
    Ok(resp.json().await?)
}

#[tokio::test]
async fn request_source_whole_bodies_scope_finds_needle_earlier_request_scope_finds()
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
            "MIKE_TEST_ADMIN_URL unset; skipping request_source_whole_bodies_scope_finds_needle_earlier_request_scope_finds"
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

    let answer = scan_whole_bodies_once(&client, &inspect_url, needle, session_id).await?;

    let matches = answer["matches"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("matches must be an array: {answer}"))?;
    anyhow::ensure!(
        matches.len() == 1 && matches[0]["request_id"].as_u64() == Some(needle_id as u64),
        "session scope must find the one needle in the request that holds it ({needle_id}) in a \
         single call, not report an absence for it: {answer}"
    );
    anyhow::ensure!(
        answer["scan_complete"].as_bool() == Some(true)
            && answer["full_match_count"].as_u64() == Some(1)
            && answer["requests_total"].as_u64() == Some(PAGE_SIZE as u64 + 1),
        "{} requests span more than one {PAGE_SIZE}-request window and must be covered in one \
         call: {answer}",
        PAGE_SIZE + 1
    );
    anyhow::ensure!(
        answer["positions"].as_str() == Some("body"),
        "whole_bodies positions are body offsets: {answer}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
