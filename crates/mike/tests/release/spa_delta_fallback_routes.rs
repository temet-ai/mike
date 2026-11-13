use axum::body::Body;
use axum::http::{Request, StatusCode};
use mike::api::manifest::{Coverage, inspect_route_manifest, inspect_router};
use mike::config::ProxyConfig;
use mike::state::AppState;
use tower::ServiceExt;

const PATH: &str = "/api/diagnostics/spa_delta_fallback";

#[test]
fn spa_delta_fallback_manifest_has_one_exempt_post_entry() {
    let entries: Vec<_> = inspect_route_manifest()
        .into_iter()
        .filter(|entry| entry.path == PATH)
        .collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].method, "POST");
    assert!(matches!(entries[0].coverage, Coverage::Exempt { reason } if !reason.is_empty()));
}

#[tokio::test]
async fn spa_delta_fallback_registered_route_preserves_host_and_origin_guards() -> anyhow::Result<()>
{
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let router = inspect_router(state);
    for (method, host, origin, expected) in [
        (
            "POST",
            "127.0.0.1:9989",
            "http://localhost:9990",
            StatusCode::NO_CONTENT,
        ),
        (
            "GET",
            "127.0.0.1:9989",
            "http://localhost:9990",
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (
            "POST",
            "attacker.example",
            "http://localhost:9990",
            StatusCode::FORBIDDEN,
        ),
        (
            "POST",
            "127.0.0.1:9989",
            "https://attacker.example",
            StatusCode::FORBIDDEN,
        ),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(PATH)
                    .header("host", host)
                    .header("origin", origin)
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"reason":"compaction"}"#))?,
            )
            .await?;
        assert_eq!(response.status(), expected, "{method} {host} {origin}");
    }
    Ok(())
}
