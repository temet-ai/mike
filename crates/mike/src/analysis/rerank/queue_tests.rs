use std::time::{Duration, Instant};

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::service_tests::{allow_all, documents, enabled};
use super::*;

fn answer(delay: Duration) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(json!({"results": [{"index": 0, "relevance_score": 0.5}]}))
        .set_delay(delay)
}

fn loading() -> ResponseTemplate {
    ResponseTemplate::new(503).set_body_json(
        json!({"error": {"code": 503, "message": "Loading model", "type": "unavailable_error"}}),
    )
}

async fn mount_rerank(server: &MockServer, template: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(template)
        .mount(server)
        .await;
}

fn config(url: &str, timeout_ms: u64, cold_timeout_ms: u64) -> RerankConfig {
    RerankConfig {
        timeout_ms,
        cold_timeout_ms,
        ..enabled(url)
    }
}

async fn requests_seen(server: &MockServer) -> usize {
    server.received_requests().await.unwrap_or_default().len()
}

#[tokio::test]
async fn rerank_when_a_cold_answer_is_slower_than_the_timeout_but_inside_the_cold_timeout_returns_scores()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    mount_rerank(&server, answer(Duration::from_millis(400))).await;
    let service = RerankService::new(&config(&server.uri(), 100, 1_500));

    let scores = service.rerank("query", &documents(1), allow_all).await?;

    assert_eq!(scores.len(), 1);
    assert_eq!(service.status().phase, RerankPhase::Ready);
    Ok(())
}

#[tokio::test]
async fn rerank_when_the_model_is_loading_twice_then_answers_retries_and_returns_scores()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(loading())
        .up_to_n_times(2)
        .mount(&server)
        .await;
    mount_rerank(&server, answer(Duration::ZERO)).await;
    let service = RerankService::new(&config(&server.uri(), 800, 4_000));

    let scores = service.rerank("query", &documents(1), allow_all).await?;

    assert_eq!(scores.len(), 1);
    assert_eq!(requests_seen(&server).await, 3);
    Ok(())
}

#[tokio::test]
async fn rerank_when_the_model_keeps_loading_past_the_cold_deadline_returns_loading()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    mount_rerank(&server, loading()).await;
    let service = RerankService::new(&config(&server.uri(), 100, 1_200));

    let outcome = service.rerank("query", &documents(1), allow_all).await;

    assert_eq!(outcome, Err(RerankError::Loading));
    assert!(requests_seen(&server).await >= 2);
    assert_eq!(service.status().phase, RerankPhase::Loading);
    Ok(())
}

#[tokio::test]
async fn rerank_when_a_warm_call_times_out_retries_once_and_returns_scores() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    mount_rerank(&server, answer(Duration::ZERO)).await;
    let service = RerankService::new(&config(&server.uri(), 150, 1_500));
    service.rerank("query", &documents(1), allow_all).await?;
    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(answer(Duration::from_secs(1)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_rerank(&server, answer(Duration::ZERO)).await;

    let scores = service.rerank("query", &documents(1), allow_all).await?;

    assert_eq!(scores.len(), 1);
    assert_eq!(requests_seen(&server).await, 2);
    assert_eq!(service.status().phase, RerankPhase::Ready);
    assert!(!service.status().breaker_open);
    Ok(())
}

#[tokio::test]
async fn rerank_when_a_warm_call_and_its_retry_both_time_out_returns_timeout() -> anyhow::Result<()>
{
    let server = MockServer::start().await;
    mount_rerank(&server, answer(Duration::ZERO)).await;
    let service = RerankService::new(&config(&server.uri(), 100, 300));
    service.rerank("query", &documents(1), allow_all).await?;
    server.reset().await;
    mount_rerank(&server, answer(Duration::from_secs(2))).await;

    let outcome = service.rerank("query", &documents(1), allow_all).await;

    assert_eq!(outcome, Err(RerankError::Timeout(300)));
    assert_eq!(requests_seen(&server).await, 2);
    Ok(())
}

#[tokio::test]
async fn rerank_with_three_concurrent_calls_to_a_slow_server_returns_scores_to_all()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    mount_rerank(&server, answer(Duration::from_millis(500))).await;
    let service = RerankService::new(&config(&server.uri(), 800, 6_000));
    let docs = documents(1);

    let (a, b, c) = tokio::join!(
        service.rerank("query", &docs, allow_all),
        service.rerank("query", &docs, allow_all),
        service.rerank("query", &docs, allow_all),
    );

    assert_eq!(a?.len(), 1);
    assert_eq!(b?.len(), 1);
    assert_eq!(c?.len(), 1);
    assert_eq!(requests_seen(&server).await, 3);
    Ok(())
}

#[tokio::test]
async fn rerank_when_queued_behind_a_slow_call_stops_at_the_whole_call_bound() -> anyhow::Result<()>
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(answer(Duration::ZERO))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_rerank(&server, answer(Duration::from_millis(700))).await;
    let service = RerankService::new(&config(&server.uri(), 200, 1_000));
    service.rerank("query", &documents(1), allow_all).await?;
    let docs = documents(1);
    let first = tokio::spawn({
        let service = service.clone();
        let docs = docs.clone();
        async move { service.rerank("query", &docs, allow_all).await }
    });
    while requests_seen(&server).await < 2 {
        tokio::task::yield_now().await;
    }

    let started = Instant::now();
    let queued = service.rerank("query", &docs, allow_all).await;
    let waited = started.elapsed();

    assert!(matches!(queued, Err(RerankError::Timeout(_))), "{queued:?}");
    assert!(
        waited < Duration::from_millis(1_900),
        "waited {waited:?}, bound is 1200 ms"
    );
    assert_eq!(first.await?.map(|scores| scores.len())?, 1);
    Ok(())
}

#[tokio::test]
async fn rerank_when_the_turn_is_not_free_within_the_cold_timeout_returns_timeout()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(answer(Duration::ZERO))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_rerank(&server, answer(Duration::from_millis(1_400))).await;
    let service = RerankService::new(&config(&server.uri(), 1_500, 400));
    service.rerank("query", &documents(1), allow_all).await?;
    let docs = documents(1);
    let first = tokio::spawn({
        let service = service.clone();
        let docs = docs.clone();
        async move { service.rerank("query", &docs, allow_all).await }
    });
    while requests_seen(&server).await < 2 {
        tokio::task::yield_now().await;
    }

    let queued = service.rerank("query", &docs, allow_all).await;

    assert_eq!(queued, Err(RerankError::Timeout(400)));
    assert_eq!(first.await?.map(|scores| scores.len())?, 1);
    assert_eq!(requests_seen(&server).await, 2);
    Ok(())
}
