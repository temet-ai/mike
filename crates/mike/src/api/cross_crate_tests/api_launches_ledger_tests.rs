use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use uuid::Uuid;

use crate::config::ProxyConfig;
use crate::launch::cli::ledger_wire::LedgerHello;
use crate::state::AppState;
use crate::store::persistence::launch::NewLaunch;

async fn call(method: &str, uri: &str, body: &str) -> anyhow::Result<(StatusCode, String)> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let response = crate::app::inspect_router(state)
        .oneshot(
            Request::builder()
                .header("host", "127.0.0.1:9989")
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))?,
        )
        .await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
}

#[tokio::test]
async fn hello_without_a_database_says_so() -> anyhow::Result<()> {
    let (status, body) = call("GET", "/api/launch-ledger", "").await?;
    assert_eq!(status, StatusCode::OK);
    let hello: LedgerHello = serde_json::from_str(&body)?;
    assert!(!hello.database);
    assert!(hello.settings.is_empty());
    Ok(())
}

#[tokio::test]
async fn writes_without_a_database_are_refused_as_unavailable() -> anyhow::Result<()> {
    let new = serde_json::to_string(&NewLaunch::new("herdr", "/proj"))?;
    let (status, body) = call("POST", "/api/launch-ledger/launches", &new).await?;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(body.contains("ledger_unavailable"));
    let (status, _) = call("GET", "/api/launch-ledger/runs?run=r1", "").await?;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    Ok(())
}

#[tokio::test]
async fn a_tools_listed_report_is_accepted() -> anyhow::Result<()> {
    let session = Uuid::new_v4();
    let (status, _) = call("POST", &format!("/api/launches/{session}/mcp"), "").await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(crate::api::launches::mcp_seen::tools_listed(session));
    Ok(())
}
