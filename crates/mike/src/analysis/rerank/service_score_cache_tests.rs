use std::time::Duration;

use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::service_tests::{allow_all, enabled};
use super::*;

const LIFETIME: Duration = Duration::from_secs(60);

fn passages(range: std::ops::Range<usize>) -> Vec<String> {
    range
        .map(|i| format!("passage {}", "x".repeat(i)))
        .collect()
}

fn score_by_length(request: &Request) -> ResponseTemplate {
    let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
    let results: Vec<Value> = body["documents"]
        .as_array()
        .map(|documents| {
            documents
                .iter()
                .enumerate()
                .map(|(index, text)| {
                    let length = u32::try_from(text.as_str().map_or(0, str::len)).unwrap_or(0);
                    json!({"index": index, "relevance_score": f64::from(length) / 1000.0})
                })
                .collect()
        })
        .unwrap_or_default();
    ResponseTemplate::new(200).set_body_json(json!({"results": results}))
}

async fn reranker() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(score_by_length)
        .mount(&server)
        .await;
    server
}

async fn sent_documents(server: &MockServer) -> Vec<usize> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|request| serde_json::from_slice::<Value>(&request.body).ok())
        .filter_map(|body| body["documents"].as_array().map(Vec::len))
        .collect()
}

fn service(server: &MockServer) -> RerankService {
    RerankService::with_score_limits(&enabled(&server.uri()), 1_000, LIFETIME)
}

#[tokio::test]
async fn rerank_counted_repeated_call_makes_no_second_request_and_reports_every_document_cached()
-> anyhow::Result<()> {
    let server = reranker().await;
    let service = service(&server);
    let documents = passages(1..6);

    let first = service
        .rerank_counted("query", &documents, allow_all)
        .await?;
    let second = service
        .rerank_counted("query", &documents, allow_all)
        .await?;

    assert_eq!(first.cached, 0);
    assert_eq!(second.cached, 5);
    assert_eq!(second.scores, first.scores);
    assert_eq!(sent_documents(&server).await, vec![5]);
    Ok(())
}

#[tokio::test]
async fn rerank_counted_overlapping_call_sends_only_the_unscored_documents_and_keeps_the_shape()
-> anyhow::Result<()> {
    let server = reranker().await;
    let service = service(&server);
    service
        .rerank_counted("query", &passages(1..4), allow_all)
        .await?;

    let overlapping = service
        .rerank_counted("query", &passages(2..6), allow_all)
        .await?;

    assert_eq!(overlapping.cached, 2);
    assert_eq!(sent_documents(&server).await, vec![3, 2]);
    let whole = RerankService::new(&enabled(&reranker().await.uri()))
        .rerank("query", &passages(2..6), allow_all)
        .await?;
    assert_eq!(overlapping.scores, whole);
    Ok(())
}

#[tokio::test]
async fn rerank_counted_entry_older_than_the_lifetime_is_asked_again() -> anyhow::Result<()> {
    let server = reranker().await;
    let service = service(&server);
    let documents = passages(1..4);
    service
        .rerank_counted("query", &documents, allow_all)
        .await?;

    service.age_cached_scores(LIFETIME + Duration::from_secs(1));
    let again = service
        .rerank_counted("query", &documents, allow_all)
        .await?;

    assert_eq!(again.cached, 0);
    assert_eq!(sent_documents(&server).await, vec![3, 3]);
    Ok(())
}

#[tokio::test]
async fn rerank_counted_failed_call_stores_nothing() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;
    let service = service(&server);

    let outcome = service
        .rerank_counted("query", &passages(1..4), allow_all)
        .await;

    assert!(matches!(outcome, Err(RerankError::Http(_))), "{outcome:?}");
    assert_eq!(service.cached_score_count(), 0);
    Ok(())
}

#[tokio::test]
async fn rerank_counted_same_passages_under_another_query_are_asked_again() -> anyhow::Result<()> {
    let server = reranker().await;
    let service = service(&server);
    let documents = passages(1..4);
    service
        .rerank_counted("first question", &documents, allow_all)
        .await?;

    let other = service
        .rerank_counted("second question", &documents, allow_all)
        .await?;

    assert_eq!(other.cached, 0);
    assert_eq!(sent_documents(&server).await, vec![3, 3]);
    Ok(())
}

#[tokio::test]
async fn rerank_counted_full_hit_is_served_while_the_breaker_is_open_and_leaves_it_open()
-> anyhow::Result<()> {
    let server = reranker().await;
    let service = service(&server);
    let documents = passages(1..4);
    service
        .rerank_counted("query", &documents, allow_all)
        .await?;
    let failing = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&failing)
        .await;
    service.apply_setting("rerank.url", &json!(failing.uri()));
    for round in 0..3 {
        let outcome = service
            .rerank_counted("query", &passages(10 + round..11 + round), allow_all)
            .await;
        assert!(matches!(outcome, Err(RerankError::Http(_))), "{outcome:?}");
    }
    assert!(service.status().breaker_open);

    let served = service
        .rerank_counted("query", &documents, allow_all)
        .await?;

    assert_eq!(served.cached, 3);
    assert!(service.status().breaker_open);
    let uncached = service
        .rerank_counted("query", &passages(20..21), allow_all)
        .await;
    assert_eq!(uncached, Err(RerankError::BreakerOpen));
    Ok(())
}

#[tokio::test]
async fn rerank_counted_when_the_reranker_is_disabled_ignores_cached_scores() -> anyhow::Result<()>
{
    let server = reranker().await;
    let service = service(&server);
    let documents = passages(1..4);
    service
        .rerank_counted("query", &documents, allow_all)
        .await?;
    service.apply_setting("rerank.enabled", &json!(false));

    let outcome = service.rerank_counted("query", &documents, allow_all).await;

    assert_eq!(outcome, Err(RerankError::Disabled));
    Ok(())
}
