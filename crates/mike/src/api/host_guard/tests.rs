use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::routing::any;
use tower::ServiceExt;

use super::{LocalAccessPolicy, guard};

#[tokio::test]
async fn rejection_precedes_read_mutation_and_upgrade_handlers() -> anyhow::Result<()> {
    let hits = Arc::new(AtomicUsize::new(0));
    let observed = hits.clone();
    let app = Router::new()
        .route(
            "/probe",
            any(move || {
                let hits = observed.clone();
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    "synthetic-capture"
                }
            }),
        )
        .layer(axum::middleware::from_fn_with_state(
            LocalAccessPolicy::new("127.0.0.1:9989", &[], &[]),
            guard,
        ));
    for method in [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::PATCH,
        Method::DELETE,
        Method::OPTIONS,
    ] {
        let req = Request::builder()
            .uri("/probe")
            .method(method)
            .header(header::HOST, "localhost:9989")
            .header(header::ORIGIN, "http://unrelated.example")
            .header(header::UPGRADE, "websocket")
            .header(header::CONNECTION, "Upgrade")
            .body(Body::empty())?;
        let response = app.clone().oneshot(req).await?;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        assert!(
            !body
                .windows(b"synthetic-capture".len())
                .any(|part| part == b"synthetic-capture")
        );
    }
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    let req = Request::builder()
        .uri("/probe")
        .header(header::HOST, "localhost:9989")
        .body(Body::empty())?;
    assert_eq!(app.oneshot(req).await?.status(), StatusCode::OK);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    Ok(())
}
