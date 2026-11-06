use super::*;

#[test]
fn default_config_has_known_model_and_is_disabled() {
    let cfg = EmbeddingConfig::default();
    assert!(!cfg.enabled);
    assert_eq!(cfg.model, DEFAULT_MODEL);
    assert_eq!(cfg.url, DEFAULT_INFINITY_URL);
}

#[test]
fn default_constants_match_schema() {
    assert_eq!(DEFAULT_DIM, 768, "must match `vector(768)` column");
    assert_eq!(DEFAULT_MODEL, "nomic-embed-text-v1.5");
}

#[test]
fn service_dim_and_model_name_match_schema_constants() {
    let svc = EmbeddingService::new(&EmbeddingConfig::default());
    assert_eq!(svc.dim(), 768, "must match `vector(768)` column");
    assert_eq!(svc.model_name(), DEFAULT_MODEL);
}

#[test]
fn service_disabled_is_unavailable() {
    let svc = EmbeddingService::new(&EmbeddingConfig {
        enabled: false,
        ..Default::default()
    });
    assert!(!svc.is_available());
}

#[test]
fn service_enabled_is_available() {
    let svc = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        ..Default::default()
    });
    assert!(svc.is_available());
}

#[tokio::test]
async fn embed_returns_disabled_error_when_disabled() {
    let svc = EmbeddingService::new(&EmbeddingConfig {
        enabled: false,
        ..Default::default()
    });
    match svc.embed("hello").await {
        Err(EmbedError::Disabled) => {}
        other => panic!("expected EmbedError::Disabled, got {other:?}"),
    }
}

#[tokio::test]
async fn embed_query_returns_disabled_error_when_disabled() {
    let svc = EmbeddingService::new(&EmbeddingConfig {
        enabled: false,
        ..Default::default()
    });
    match svc.embed_query("hello").await {
        Err(EmbedError::Disabled) => {}
        other => panic!("expected EmbedError::Disabled, got {other:?}"),
    }
}

#[tokio::test]
async fn embed_documents_returns_disabled_error_when_disabled() {
    let svc = EmbeddingService::new(&EmbeddingConfig {
        enabled: false,
        ..Default::default()
    });
    match svc.embed_documents(&["hello".to_string()]).await {
        Err(EmbedError::Disabled) => {}
        other => panic!("expected EmbedError::Disabled, got {other:?}"),
    }
}

#[tokio::test]
async fn embed_batch_empty_input_is_ok() -> anyhow::Result<()> {
    let svc = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        ..Default::default()
    });
    let out = svc
        .embed_batch(Vec::new())
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert!(out.is_empty());
    Ok(())
}

#[tokio::test]
async fn embed_documents_empty_input_is_ok() -> anyhow::Result<()> {
    let svc = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        ..Default::default()
    });
    let out = svc
        .embed_documents(&[])
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert!(out.is_empty());
    Ok(())
}

#[tokio::test]
async fn warm_is_always_ok() -> anyhow::Result<()> {
    let svc = EmbeddingService::new(&EmbeddingConfig {
        enabled: false,
        ..Default::default()
    });
    svc.warm().await.map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(svc.attributed_rss_bytes(), 0);
    Ok(())
}

#[test]
fn apply_setting_updates_live_config_snapshot() {
    let svc = EmbeddingService::new(&EmbeddingConfig::default());

    assert!(svc.apply_setting("embedding.enabled", &serde_json::json!(false)));
    assert!(!svc.is_available());

    assert!(svc.apply_setting("embedding.url", &serde_json::json!("http://127.0.0.1:7998")));
    assert_eq!(svc.url(), "http://127.0.0.1:7998");

    assert!(svc.apply_setting("embedding.model", &serde_json::json!("custom-model")));
    assert_eq!(svc.model_name(), "custom-model");

    assert!(svc.apply_setting("embedding.chunk_max_chars", &serde_json::json!(2048)));
    assert_eq!(svc.chunk_max_chars(), 2048);
}

#[tokio::test]
async fn connection_refused_returns_http_error() {
    let svc = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: "http://127.0.0.1:1".to_string(),
        ..Default::default()
    });
    match svc.embed("hello").await {
        Err(EmbedError::Http(_)) => {}
        other => panic!("expected EmbedError::Http, got {other:?}"),
    }
}

fn checked_config(url: String) -> EmbeddingConfig {
    EmbeddingConfig {
        enabled: true,
        url,
        ..Default::default()
    }
}

#[tokio::test]
async fn embed_documents_checked_returns_the_snapshot_it_posted_with() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{"embedding": [0.1, 0.2]}]
        })))
        .mount(&server)
        .await;
    let svc = EmbeddingService::new(&checked_config(server.uri()));
    let (snapshot, vectors) = svc
        .embed_documents_checked(&["continue".to_owned()], |_| true)
        .await?;
    assert_eq!(snapshot.url, server.uri());
    assert_eq!(vectors, vec![vec![0.1, 0.2]]);
    let received = server.received_requests().await.unwrap_or_default();
    let body: serde_json::Value = serde_json::from_slice(&received[0].body)?;
    assert_eq!(body["input"][0], format!("{PREFIX_DOCUMENT}continue"));
    Ok(())
}

#[tokio::test]
async fn embed_documents_checked_refused_sends_no_request() -> anyhow::Result<()> {
    use wiremock::MockServer;
    let server = MockServer::start().await;
    let svc = EmbeddingService::new(&checked_config(server.uri()));
    let result = svc
        .embed_documents_checked(&["continue".to_owned()], |_| false)
        .await;
    assert!(matches!(result, Err(EmbedError::Refused(_))));
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}

fn ollama_reply(count: usize, width: usize) -> serde_json::Value {
    let data: Vec<serde_json::Value> = (0..count)
        .map(|index| {
            serde_json::json!({
                "object": "embedding",
                "embedding": vec![0.25_f32; width],
                "index": index
            })
        })
        .collect();
    serde_json::json!({
        "object": "list",
        "data": data,
        "model": "nomic-embed-text",
        "usage": {"prompt_tokens": 3, "total_tokens": 3}
    })
}

fn ollama_service(base: &str) -> EmbeddingService {
    EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: format!("{base}/v1"),
        model: "nomic-embed-text".to_string(),
        ..Default::default()
    })
}

#[tokio::test]
async fn embedding_works_against_ollamas_v1_embeddings_route_and_reply_shape() -> anyhow::Result<()>
{
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .and(body_partial_json(serde_json::json!({
            "model": "nomic-embed-text",
            "input": ["search_query: ping"]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ollama_reply(1, 768)))
        .mount(&server)
        .await;
    let vector = ollama_service(&server.uri()).embed_query("ping").await?;
    assert_eq!(vector.len(), 768);
    Ok(())
}

#[tokio::test]
async fn a_batch_of_documents_comes_back_in_order_from_ollama() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ollama_reply(3, 768)))
        .mount(&server)
        .await;
    let texts = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let vectors = ollama_service(&server.uri())
        .embed_documents(&texts)
        .await?;
    assert_eq!(vectors.len(), 3);
    Ok(())
}

#[tokio::test]
async fn a_model_that_ollama_has_not_pulled_is_reported_with_its_message() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": {
                "message": "model \"nomic-embed-text\" not found, try pulling it first",
                "type": "api_error"
            }
        })))
        .mount(&server)
        .await;
    let error = ollama_service(&server.uri())
        .embed_query("ping")
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected an error"))?;
    assert!(matches!(error, EmbedError::Http(_)), "{error}");
    assert!(
        error.to_string().contains("try pulling it first"),
        "{error}"
    );
    Ok(())
}

#[tokio::test]
async fn an_address_with_a_trailing_slash_posts_to_the_same_route() -> anyhow::Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ollama_reply(1, 768)))
        .mount(&server)
        .await;
    let service = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: format!("{}/v1/", server.uri()),
        model: "nomic-embed-text".to_string(),
        ..Default::default()
    });
    assert_eq!(service.embed("x").await?.len(), 768);
    Ok(())
}
