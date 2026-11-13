use axum::body::{Body, to_bytes};
use axum::http::Request;
use mike::config::ProxyConfig;
use mike::state::AppState;
use tower::ServiceExt;

#[tokio::test]
async fn effective_connection_metadata_is_same_origin_safe_and_non_secret() -> anyhow::Result<()> {
    let mut config = ProxyConfig::for_tests_in_memory();
    config.proxy_listen_addr = "0.0.0.0:19088".into();
    config.inspect_listen_addr = "127.0.0.1:19089".into();
    config.spa_bind = "[::]:19090".into();
    let state = AppState::new(config).await?;
    let app = mike::app::inspect_router(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/connection-info")
                .header("host", "127.0.0.1:19089")
                .body(Body::empty())?,
        )
        .await?;
    assert!(response.status().is_success());
    let body = to_bytes(response.into_body(), 4096).await?;
    let value: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(value["proxy_url"], "http://127.0.0.1:19088");
    assert_eq!(value["inspect_url"], "http://127.0.0.1:19089");
    assert_eq!(value["browser_url"], "http://[::1]:19090");
    let fields = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("metadata is not an object"))?;
    assert_eq!(fields.len(), 3);
    Ok(())
}
