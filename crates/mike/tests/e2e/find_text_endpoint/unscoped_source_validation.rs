use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::MockResponse;
use serde_json::Value;

#[tokio::test]
async fn find_text_unscoped_request_source_returns_400_with_recall_hint() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new("msg_ft3").minimal().build()),
        )
        .mount(&upstream)
        .await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping find_text_endpoint");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/body/find"))
        .query(&[("needle", "anything"), ("source", "request")])
        .send()
        .await?;

    anyhow::ensure!(
        resp.status() == reqwest::StatusCode::BAD_REQUEST,
        "expected 400 for unscoped request-source search, got {}",
        resp.status()
    );

    let body: Value = resp.json().await?;
    let error_msg = body["error"].as_str().unwrap_or("");
    anyhow::ensure!(
        error_msg.contains("recall_similar_turns"),
        "400 body should mention recall_similar_turns; got: {body}"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn find_text_unscoped_response_source_returns_200_with_window_note() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new("msg_ft4").minimal().build()),
        )
        .mount(&upstream)
        .await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping find_text_endpoint");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let resp = reqwest::Client::new()
        .get(format!("{inspect_url}/api/body/find"))
        .query(&[
            ("needle", "XNOMATCH_RESPONSE_WINDOW_TEST"),
            ("source", "response"),
        ])
        .send()
        .await?;

    anyhow::ensure!(
        resp.status().is_success(),
        "expected 200 for unscoped response-source search, got {}",
        resp.status()
    );

    let result: Value = resp.json().await?;

    anyhow::ensure!(
        result.get("window_note").is_some(),
        "expected window_note field on unscoped response search; got: {result}"
    );
    anyhow::ensure!(
        result["window_note"].as_str().is_some(),
        "window_note must be a string; got: {result}"
    );

    anyhow::ensure!(
        result.get("matches").is_some(),
        "response missing matches field: {result}"
    );
    anyhow::ensure!(
        result.get("full_match_count").is_some(),
        "response missing full_match_count field: {result}"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}
