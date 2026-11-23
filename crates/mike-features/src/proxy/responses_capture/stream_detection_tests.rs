use axum::body::{Body, Bytes};
use axum::http::{Request, header};
use futures::StreamExt;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

use super::relay_test_support::*;
use super::routes;
use crate::store::persistence::runtime_repo::RuntimePersist as _;

#[tokio::test]
async fn headerless_responses_stream_terminal_event_completes_before_http_eof() -> anyhow::Result<()>
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let mut h = harness(&format!("http://{address}/v1"), "http://127.0.0.1:1")
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture test requires the test database"))?;
    let server = tokio::spawn(async move {
        let app = axum::Router::new().route(
            "/v1/responses",
            axum::routing::post(|| async {
                let chunks = [
                    Ok::<_, std::io::Error>(Bytes::from_static(&STREAM[..5])),
                    Ok(Bytes::from_static(&STREAM[5..])),
                ];
                let bytes = futures::stream::iter(chunks).chain(futures::stream::pending());
                axum::response::Response::new(Body::from_stream(bytes))
            }),
        );
        axum::serve(listener, app).await
    });
    let response = routes(h.state.clone())
        .oneshot(Request::post("/codex/responses").body(Body::from(REQUEST))?)
        .await?;
    assert!(!response.headers().contains_key(header::CONTENT_TYPE));
    let mut response = response.into_body();
    let mut received = Vec::new();
    while received.len() < STREAM.len() {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(5), response.frame())
            .await?
            .ok_or_else(|| anyhow::anyhow!("stream ended before terminal event"))??;
        if let Ok(bytes) = frame.into_data() {
            received.extend_from_slice(&bytes);
        }
    }
    assert_eq!(received, STREAM);
    drop(response);
    let diagnostic = next_recorded(&mut h).await?;
    server.abort();
    let _ = server.await;
    let capture = captured(&h, diagnostic).await?;
    assert_eq!(capture.status, "complete");
    assert_eq!(capture.metadata["response_is_event_stream"], true);
    assert_eq!(capture.metadata["response_state"], "completed");
    assert_eq!(capture.metadata["stream_events"], 3);
    assert_eq!(capture.metadata["usage"]["total_tokens"], 9);
    assert_eq!(capture.metadata["error"], serde_json::Value::Null);
    let document = capture
        .response_document_id
        .ok_or_else(|| anyhow::anyhow!("response missing"))?;
    assert_eq!(body(&h, document).await?, STREAM);
    assert!(
        h.persist
            .runtime()
            .capture_document(document)
            .await?
            .is_some_and(|row| !row.complete)
    );
    Ok(())
}

#[tokio::test]
async fn headerless_json_and_other_paths_keep_their_original_classification() -> anyhow::Result<()>
{
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let json_body = br#"{"id":"resp_json","status":"completed","usage":{"total_tokens":4}}"#;
    let upstream = MockServer::start().await;
    for (route, bytes) in [
        ("/v1/responses", json_body.as_slice()),
        ("/v1/models", STREAM),
    ] {
        Mock::given(method("POST"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
            .mount(&upstream)
            .await;
    }
    let mut h = harness(&format!("{}/v1", upstream.uri()), "http://127.0.0.1:1")
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture test requires the test database"))?;
    for (route, expected) in [
        ("/codex/responses", json_body.as_slice()),
        ("/codex/models", STREAM),
    ] {
        let response = routes(h.state.clone())
            .oneshot(Request::post(route).body(Body::from(REQUEST))?)
            .await?;
        assert!(!response.headers().contains_key(header::CONTENT_TYPE));
        assert_eq!(
            response.into_body().collect().await?.to_bytes().as_ref(),
            expected
        );
        let diagnostic = next_recorded(&mut h).await?;
        let capture = captured(&h, diagnostic).await?;
        assert_eq!(capture.metadata["response_is_event_stream"], false);
        assert_eq!(capture.metadata["stream_events"], 0);
        if route == "/codex/responses" {
            assert_eq!(capture.metadata["response_state"], "completed");
            assert_eq!(capture.metadata["usage"], json!({"total_tokens":4}));
        }
    }
    Ok(())
}
