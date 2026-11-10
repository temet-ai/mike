use std::time::Duration;

use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

pub(super) fn enabled(url: &str) -> RerankConfig {
    RerankConfig {
        enabled: true,
        url: url.to_owned(),
        ..RerankConfig::default()
    }
}

pub(super) fn documents(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("passage {i}")).collect()
}

pub(super) fn tagged_documents(tag: &str, count: usize) -> Vec<String> {
    (0..count).map(|i| format!("{tag} passage {i}")).collect()
}

pub(super) fn allow_all(_: &RerankConfig) -> bool {
    true
}

fn loading_body() -> serde_json::Value {
    json!({"error": {"code": 503, "message": "Loading model", "type": "unavailable_error"}})
}

fn refused_url() -> anyhow::Result<String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    drop(listener);
    Ok(format!("http://{addr}"))
}

#[test]
fn default_config_is_disabled_with_the_planned_values() {
    let config = RerankConfig::default();
    assert!(!config.enabled);
    assert!(!config.tuning_enabled);
    assert_eq!(config.model, "bge-reranker-v2-m3");
    assert_eq!(config.timeout_ms, 800);
    assert_eq!(config.candidates, 50);
}

#[tokio::test]
async fn rerank_when_server_answers_returns_scores_best_first() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "bge-reranker-v2-m3",
            "results": [
                {"index": 0, "relevance_score": 0.1},
                {"index": 2, "relevance_score": 0.9},
                {"index": 1, "relevance_score": 0.4}
            ]
        })))
        .mount(&server)
        .await;
    let service = RerankService::new(&enabled(&server.uri()));

    let scores = service.rerank("query", &documents(3), allow_all).await?;

    let order: Vec<usize> = scores.iter().map(|s| s.index).collect();
    assert_eq!(order, vec![2, 1, 0]);
    assert_eq!(service.status().phase, RerankPhase::Ready);
    Ok(())
}

#[tokio::test]
async fn rerank_sends_model_query_and_documents_in_llama_server_format() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .and(body_json(json!({
            "model": "bge-reranker-v2-m3",
            "query": "which lesson",
            "documents": ["passage 0", "passage 1"]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"index": 1, "relevance_score": 2.5}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let service = RerankService::new(&enabled(&server.uri()));

    let scores = service
        .rerank("which lesson", &documents(2), allow_all)
        .await?;

    assert_eq!(scores.len(), 1);
    assert_eq!(scores[0].index, 1);
    Ok(())
}

#[tokio::test]
async fn rerank_when_disabled_returns_disabled_without_a_request() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let service = RerankService::new(&RerankConfig {
        url: server.uri(),
        ..RerankConfig::default()
    });

    let outcome = service.rerank("query", &documents(2), allow_all).await;

    assert_eq!(outcome, Err(RerankError::Disabled));
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn rerank_when_egress_is_refused_returns_refused_without_a_request() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let service = RerankService::new(&enabled(&server.uri()));

    let outcome = service.rerank("query", &documents(2), |_| false).await;

    assert_eq!(outcome, Err(RerankError::Refused(server.uri())));
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn rerank_with_no_documents_returns_empty_without_a_request() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let service = RerankService::new(&enabled(&server.uri()));

    let scores = service.rerank("query", &[], allow_all).await?;

    assert!(scores.is_empty());
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn rerank_when_server_is_slower_than_the_timeout_returns_timeout() -> anyhow::Result<()> {
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
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let service = RerankService::new(&RerankConfig {
        timeout_ms: 50,
        cold_timeout_ms: 50,
        ..enabled(&server.uri())
    });
    assert_eq!(service.health().await, ServerHealth::Ready);

    let outcome = service.rerank("query", &documents(2), allow_all).await;

    assert_eq!(outcome, Err(RerankError::Timeout(50)));
    assert_eq!(service.status().phase, RerankPhase::Unavailable);
    Ok(())
}

#[tokio::test]
async fn rerank_when_server_is_loading_returns_loading_and_keeps_the_breaker_closed()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(ResponseTemplate::new(503).set_body_json(loading_body()))
        .expect(5)
        .mount(&server)
        .await;
    let service = RerankService::new(&RerankConfig {
        cold_timeout_ms: 100,
        ..enabled(&server.uri())
    });

    for _ in 0..5 {
        let outcome = service.rerank("query", &documents(2), allow_all).await;
        assert_eq!(outcome, Err(RerankError::Loading));
    }

    let status = service.status();
    assert_eq!(status.phase, RerankPhase::Loading);
    assert!(!status.breaker_open);
    Ok(())
}

#[tokio::test]
async fn rerank_after_repeated_failures_opens_the_breaker_and_skips_the_server()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .expect(3)
        .mount(&server)
        .await;
    let service = RerankService::new(&enabled(&server.uri()));

    for _ in 0..3 {
        let outcome = service.rerank("query", &documents(2), allow_all).await;
        assert!(matches!(outcome, Err(RerankError::Http(_))), "{outcome:?}");
    }
    let skipped = service.rerank("query", &documents(2), allow_all).await;

    assert_eq!(skipped, Err(RerankError::BreakerOpen));
    let status = service.status();
    assert_eq!(status.phase, RerankPhase::Unavailable);
    assert!(status.breaker_open);
    assert!(
        status.last_error.is_some_and(|e| e.contains("boom")),
        "the last error names the server's answer"
    );
    Ok(())
}

#[tokio::test]
async fn rerank_when_a_result_index_is_out_of_range_returns_parse_error() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"index": 7, "relevance_score": 0.5}]
        })))
        .mount(&server)
        .await;
    let service = RerankService::new(&enabled(&server.uri()));

    let outcome = service.rerank("query", &documents(2), allow_all).await;

    assert!(
        matches!(&outcome, Err(RerankError::Parse(m)) if m.contains('7')),
        "{outcome:?}"
    );
    Ok(())
}

#[tokio::test]
async fn rerank_when_the_body_is_not_json_returns_parse_error() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;
    let service = RerankService::new(&enabled(&server.uri()));

    let outcome = service.rerank("query", &documents(2), allow_all).await;

    assert!(matches!(outcome, Err(RerankError::Parse(_))), "{outcome:?}");
    Ok(())
}

#[tokio::test]
async fn rerank_when_nothing_listens_returns_http_error_naming_the_url() -> anyhow::Result<()> {
    let url = refused_url()?;
    let service = RerankService::new(&enabled(&url));

    let outcome = service.rerank("query", &documents(2), allow_all).await;

    assert!(
        matches!(&outcome, Err(RerankError::Http(m)) if m.contains(&url)),
        "{outcome:?}"
    );
    Ok(())
}

#[tokio::test]
async fn health_when_server_answers_200_is_ready() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "ok"})))
        .mount(&server)
        .await;
    let service = RerankService::new(&enabled(&server.uri()));

    assert_eq!(service.health().await, ServerHealth::Ready);
    assert_eq!(service.status().phase, RerankPhase::Ready);
    Ok(())
}

#[tokio::test]
async fn health_when_server_answers_503_loading_model_is_loading() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(503).set_body_json(loading_body()))
        .mount(&server)
        .await;
    let service = RerankService::new(&enabled(&server.uri()));

    assert_eq!(service.health().await, ServerHealth::Loading);
    assert_eq!(service.status().phase, RerankPhase::Loading);
    Ok(())
}

#[tokio::test]
async fn health_when_server_answers_503_for_another_reason_is_unavailable() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(503).set_body_string("no slot available"))
        .mount(&server)
        .await;
    let service = RerankService::new(&enabled(&server.uri()));

    let health = service.health().await;

    assert!(
        matches!(&health, ServerHealth::Unavailable(m) if m.contains("no slot available")),
        "{health:?}"
    );
    assert_eq!(service.status().phase, RerankPhase::Unavailable);
    Ok(())
}

#[tokio::test]
async fn health_when_nothing_listens_is_unavailable_and_names_the_url() -> anyhow::Result<()> {
    let url = refused_url()?;
    let service = RerankService::new(&enabled(&url));

    let health = service.health().await;

    assert!(
        matches!(&health, ServerHealth::Unavailable(m) if m.contains(&url)),
        "{health:?}"
    );
    assert_eq!(service.status().phase, RerankPhase::Unavailable);
    Ok(())
}

#[tokio::test]
async fn health_when_disabled_is_off_and_sends_no_request() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let service = RerankService::new(&RerankConfig {
        url: server.uri(),
        ..RerankConfig::default()
    });

    assert_eq!(service.health().await, ServerHealth::Off);
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    assert_eq!(service.status().phase, RerankPhase::Off);
    Ok(())
}

#[tokio::test]
async fn health_after_the_breaker_opened_still_reaches_the_server_and_closes_it()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let service = RerankService::new(&enabled(&server.uri()));
    for _ in 0..3 {
        let _ = service.rerank("query", &documents(2), allow_all).await;
    }
    assert!(service.status().breaker_open);

    assert_eq!(service.health().await, ServerHealth::Ready);

    let status = service.status();
    assert!(!status.breaker_open);
    assert_eq!(status.phase, RerankPhase::Ready);
    Ok(())
}

#[test]
fn status_before_any_call_is_idle_and_when_disabled_is_off() {
    let idle = RerankService::new(&enabled("http://127.0.0.1:1"));
    assert_eq!(idle.status().phase, RerankPhase::Idle);
    let off = RerankService::new(&RerankConfig::default());
    assert_eq!(off.status().phase, RerankPhase::Off);
}
