use std::time::Duration;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::service_tests::{allow_all, documents, enabled};
use super::*;

#[tokio::test]
async fn rerank_when_the_first_call_times_out_reports_loading_and_keeps_the_breaker_closed()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"results": []}))
                .set_delay(Duration::from_secs(5)),
        )
        .mount(&server)
        .await;
    let service = RerankService::new(&RerankConfig {
        timeout_ms: 50,
        ..enabled(&server.uri())
    });

    for _ in 0..4 {
        let outcome = service.rerank("query", &documents(2), allow_all).await;
        assert_eq!(outcome, Err(RerankError::Loading));
    }

    let status = service.status();
    assert_eq!(status.phase, RerankPhase::Loading);
    assert!(!status.breaker_open);
    Ok(())
}

#[tokio::test]
async fn rerank_truncates_each_document_and_the_query_before_sending() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": []})))
        .mount(&server)
        .await;
    let service = RerankService::new(&enabled(&server.uri()));
    let long_document = "é".repeat(MAX_DOCUMENT_CHARS + 500);
    let long_query = "q".repeat(MAX_QUERY_CHARS + 500);

    service
        .rerank(&long_query, &[long_document, "short".to_owned()], allow_all)
        .await?;

    let requests = server.received_requests().await.unwrap_or_default();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body)?;
    assert_eq!(
        body["documents"][0].as_str().map(|d| d.chars().count()),
        Some(MAX_DOCUMENT_CHARS)
    );
    assert_eq!(body["documents"][1], json!("short"));
    assert_eq!(
        body["query"].as_str().map(|q| q.chars().count()),
        Some(MAX_QUERY_CHARS)
    );
    Ok(())
}

#[test]
fn truncate_chars_keeps_short_text_and_cuts_on_a_character_boundary() {
    assert_eq!(service::truncate_chars("abc", 5), "abc");
    assert_eq!(service::truncate_chars("abc", 3), "abc");
    assert_eq!(service::truncate_chars("héllo", 2), "hé");
    assert_eq!(service::truncate_chars("", 0), "");
}

#[test]
fn cold_start_is_expected_when_unknown_loading_or_the_last_success_is_old() {
    let now = chrono::Utc::now();
    let mut runtime = runtime::Runtime::new();
    assert!(runtime.expects_cold_start_at(now));

    runtime.record_ok();
    assert!(!runtime.expects_cold_start_at(now));
    let after_idle_exit = now + chrono::Duration::seconds(COLD_AFTER_SECS + 5);
    assert!(runtime.expects_cold_start_at(after_idle_exit));

    runtime.record_failure("refused".to_owned());
    assert!(!runtime.expects_cold_start_at(after_idle_exit));

    runtime.record_loading();
    assert!(runtime.expects_cold_start_at(now));
}

#[tokio::test]
async fn health_when_the_connection_is_held_open_during_a_cold_start_is_loading()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(10)))
        .mount(&server)
        .await;
    let service =
        RerankService::with_health_timeout(&enabled(&server.uri()), Duration::from_millis(50));

    assert_eq!(service.health().await, ServerHealth::Loading);
    assert_eq!(service.status().phase, RerankPhase::Loading);
    Ok(())
}
