use axum::Router;
use axum::routing::any;

use crate::config::AppState;
use crate::proxy::handler::proxy_handler;

pub fn proxy_router(state: AppState) -> Router {
    let policy = crate::api::host_guard::LocalAccessPolicy::proxy(&state.config);
    Router::new()
        .route("/v1/{*rest}", any(proxy_handler))
        .route("/v2/{*rest}", any(proxy_handler))
        .layer(axum::middleware::from_fn_with_state(
            policy,
            crate::api::host_guard::guard,
        ))
        .with_state(state)
}

pub fn inspect_router(state: AppState) -> Router {
    crate::api::manifest::inspect_router(state)
}

pub fn inspect_router_on(state: AppState, addr: std::net::SocketAddr) -> Router {
    let policy = crate::api::host_guard::LocalAccessPolicy::inspect_on(&state.config, addr);
    crate::api::manifest::local_router(state, policy)
}

pub fn spa_router_on(state: AppState, addr: std::net::SocketAddr) -> Router {
    let policy = crate::api::host_guard::LocalAccessPolicy::spa_on(&state.config, addr);
    guarded_spa_router(state, policy)
}

pub fn spa_router(state: AppState) -> Router {
    let policy = crate::api::host_guard::LocalAccessPolicy::spa(&state.config);
    guarded_spa_router(state, policy)
}

fn guarded_spa_router(
    state: AppState,
    policy: crate::api::host_guard::LocalAccessPolicy,
) -> Router {
    crate::api::manifest::local_router(state, policy.clone())
        .merge(crate::spa::spa_router())
        .layer(axum::middleware::from_fn_with_state(
            policy,
            crate::api::host_guard::guard,
        ))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt;

    use super::spa_router;
    use crate::config::ProxyConfig;

    #[tokio::test]
    async fn spa_router_api_health_returns_non_html() -> anyhow::Result<()> {
        let state = crate::config::AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        let app = spa_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .header(header::HOST, "127.0.0.1:9990")
                    .body(Body::empty())?,
            )
            .await?;
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            content_type.starts_with("application/json"),
            "expected application/json content-type on /api/health, got {content_type:?}"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        assert!(
            !body.starts_with(b"<!doctype"),
            "expected JSON body, got SPA fallback HTML: {:?}",
            String::from_utf8_lossy(&body)
        );
        Ok(())
    }

    #[tokio::test]
    async fn spa_router_unmatched_api_path_returns_404_not_html() -> anyhow::Result<()> {
        let state = crate::config::AppState::new(ProxyConfig::for_tests_in_memory()).await?;
        let app = spa_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/nonexistent-zzz")
                    .header(header::HOST, "127.0.0.1:9990")
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            !content_type.starts_with("text/html"),
            "expected a non-HTML 404, got content-type {content_type:?}"
        );
        Ok(())
    }
}
