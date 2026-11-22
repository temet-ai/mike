use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::HeaderMap;
use axum::middleware::{Next, from_fn};
use axum::routing::get;
use tower::ServiceExt as _;

use super::{Labels, client_of, labels};

fn agent(value: &str) -> anyhow::Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(axum::http::header::USER_AGENT, value.parse()?);
    Ok(headers)
}

#[test]
fn client_of_names_the_product_of_the_user_agent() -> anyhow::Result<()> {
    assert_eq!(client_of(&agent("Python-urllib/3.13")?), "Python-urllib");
    assert_eq!(client_of(&agent("curl/8.11.0")?), "curl");
    assert_eq!(client_of(&agent("tether 0.4")?), "tether");
    Ok(())
}

#[test]
fn client_of_without_a_user_agent_is_unknown() {
    assert_eq!(client_of(&HeaderMap::new()), "unknown");
}

#[tokio::test]
async fn labels_in_a_route_layer_name_the_route_pattern_not_the_path() -> anyhow::Result<()> {
    let seen: Arc<Mutex<Vec<Labels>>> = Arc::default();
    let sink = seen.clone();
    let router = Router::new()
        .route("/api/fleets/{fleet}", get(|| async { "ok" }))
        .route_layer(from_fn(move |request: Request, next: Next| {
            let sink = sink.clone();
            async move {
                if let Ok(mut seen) = sink.lock() {
                    seen.push(labels(&request));
                }
                next.run(request).await
            }
        }));
    let request = Request::builder()
        .uri("/api/fleets/abc")
        .header("user-agent", "curl/8.11.0")
        .body(Body::empty())?;
    router.oneshot(request).await?;
    let seen = seen.lock().map_err(|_| anyhow::anyhow!("poisoned"))?;
    assert_eq!(
        seen.as_slice(),
        [Labels {
            route: "/api/fleets/{fleet}".to_owned(),
            method: "GET".to_owned(),
            client: "curl".to_owned(),
        }]
    );
    Ok(())
}
