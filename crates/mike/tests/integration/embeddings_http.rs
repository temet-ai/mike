use anyhow::Context;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use mike::analysis::{EmbedError, EmbeddingConfig, EmbeddingService};

fn make_service(url: &str) -> EmbeddingService {
    EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: url.to_string(),
        model: "nomic-embed-text-v1.5".to_string(),
        chunk_max_chars: 1500,
    })
}

fn disabled_service() -> EmbeddingService {
    EmbeddingService::new(&EmbeddingConfig {
        enabled: false,
        url: "http://127.0.0.1:7997".to_string(),
        model: "nomic-embed-text-v1.5".to_string(),
        chunk_max_chars: 1500,
    })
}

fn infinity_response(count: usize) -> serde_json::Value {
    let vec: Vec<f32> = vec![0.0_f32; 768];
    let data: Vec<serde_json::Value> = (0..count)
        .map(|i| {
            serde_json::json!({
                "embedding": vec,
                "index": i,
                "object": "embedding"
            })
        })
        .collect();
    serde_json::json!({ "data": data })
}

#[tokio::test]
async fn embed_batch_happy_path_parses_response() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(infinity_response(2)))
        .mount(&server)
        .await;

    let svc = make_service(&server.uri());
    let vecs = svc
        .embed_batch(vec!["hello".to_string(), "world".to_string()])
        .await?;
    assert_eq!(vecs.len(), 2);
    assert_eq!(vecs[0].len(), 768);
    assert_eq!(vecs[1].len(), 768);
    Ok(())
}

#[tokio::test]
async fn embed_query_prepends_search_query_prefix() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(infinity_response(1)))
        .mount(&server)
        .await;

    let svc = make_service(&server.uri());
    let vec = svc.embed_query("my query").await?;
    assert_eq!(vec.len(), 768);

    let received = server.received_requests().await.context("wiremock recv")?;
    assert_eq!(received.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&received[0].body)?;
    let inputs = body["input"].as_array().context("input array")?;
    assert_eq!(inputs.len(), 1);
    let sent = inputs[0].as_str().context("string element")?;
    assert!(
        sent.starts_with("search_query: "),
        "expected search_query: prefix, got: {sent:?}"
    );
    assert!(sent.ends_with("my query"));
    Ok(())
}

#[tokio::test]
async fn embed_documents_prepends_search_document_prefix() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(infinity_response(2)))
        .mount(&server)
        .await;

    let svc = make_service(&server.uri());
    let texts = vec!["doc one".to_string(), "doc two".to_string()];
    let vecs = svc.embed_documents(&texts).await?;
    assert_eq!(vecs.len(), 2);

    let received = server.received_requests().await.context("wiremock recv")?;
    assert_eq!(received.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&received[0].body)?;
    let inputs = body["input"].as_array().context("input array")?;
    assert_eq!(inputs.len(), 2);
    for input in inputs.iter() {
        let sent = input.as_str().context("string element")?;
        assert!(
            sent.starts_with("search_document: "),
            "expected search_document: prefix, got: {sent:?}"
        );
    }
    let first = inputs[0].as_str().context("first")?;
    let second = inputs[1].as_str().context("second")?;
    assert!(first.ends_with("doc one"), "first: {first:?}");
    assert!(second.ends_with("doc two"), "second: {second:?}");
    Ok(())
}

#[tokio::test]
async fn embed_batch_empty_returns_empty_without_http_call() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let svc = make_service(&server.uri());
    let vecs = svc.embed_batch(vec![]).await?;
    assert!(vecs.is_empty());
    let received = server.received_requests().await.context("wiremock recv")?;
    assert!(
        received.is_empty(),
        "expected no HTTP request for empty input"
    );
    Ok(())
}

#[tokio::test]
async fn embed_documents_empty_returns_empty_without_http_call() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let svc = make_service(&server.uri());
    let vecs = svc.embed_documents(&[]).await?;
    assert!(vecs.is_empty());
    let received = server.received_requests().await.context("wiremock recv")?;
    assert!(
        received.is_empty(),
        "expected no HTTP request for empty input"
    );
    Ok(())
}

#[tokio::test]
async fn disabled_service_returns_disabled_error() -> anyhow::Result<()> {
    let svc = disabled_service();
    assert!(!svc.is_available());

    match svc.embed_batch(vec!["text".to_string()]).await {
        Err(EmbedError::Disabled) => {}
        other => anyhow::bail!("embed_batch: expected Disabled, got: {other:?}"),
    }
    match svc.embed_query("query").await {
        Err(EmbedError::Disabled) => {}
        other => anyhow::bail!("embed_query: expected Disabled, got: {other:?}"),
    }
    match svc.embed_documents(&["doc".to_string()]).await {
        Err(EmbedError::Disabled) => {}
        other => anyhow::bail!("embed_documents: expected Disabled, got: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn infinity_5xx_returns_http_error() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
        .mount(&server)
        .await;

    let svc = make_service(&server.uri());
    match svc.embed_batch(vec!["text".to_string()]).await {
        Err(EmbedError::Http(_)) => {}
        other => anyhow::bail!("expected Http error for 5xx, got: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn infinity_down_returns_http_error() -> anyhow::Result<()> {
    let svc = make_service("http://127.0.0.1:1");
    match svc.embed_batch(vec!["text".to_string()]).await {
        Err(EmbedError::Http(_)) => {}
        other => anyhow::bail!("expected Http error for connection refused, got: {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn warm_always_succeeds() -> anyhow::Result<()> {
    let svc = disabled_service();
    svc.warm().await?;
    let svc2 = make_service("http://127.0.0.1:1");
    svc2.warm().await?;
    Ok(())
}

#[tokio::test]
async fn model_name_returns_configured_model() {
    let svc = make_service("http://127.0.0.1:7997");
    assert_eq!(svc.model_name(), "nomic-embed-text-v1.5");
}

#[tokio::test]
async fn apply_setting_updates_live_request_target() -> anyhow::Result<()> {
    let server_a = MockServer::start().await;
    let server_b = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(infinity_response(1)))
        .mount(&server_b)
        .await;

    let svc = make_service(&server_a.uri());
    assert!(svc.apply_setting("embedding.url", &serde_json::json!(server_b.uri())));

    let vecs = svc.embed_batch(vec!["hello".to_string()]).await?;
    assert_eq!(vecs.len(), 1);

    let received_a = server_a
        .received_requests()
        .await
        .context("wiremock recv")?;
    assert!(
        received_a.is_empty(),
        "old embedding URL must stop receiving traffic after the live update"
    );
    let received_b = server_b
        .received_requests()
        .await
        .context("wiremock recv")?;
    assert_eq!(received_b.len(), 1);
    Ok(())
}

#[tokio::test]
async fn dim_returns_768() {
    let svc = make_service("http://127.0.0.1:7997");
    assert_eq!(svc.dim(), 768);
}
