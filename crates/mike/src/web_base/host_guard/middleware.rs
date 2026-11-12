use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::LocalAccessPolicy;

pub(crate) async fn guard(
    State(policy): State<LocalAccessPolicy>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if let Err(message) = policy.check(req.headers(), req.uri(), req.version()) {
        tracing::warn!(path = %req.uri().path(), message, "rejecting local API request");
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": message })),
        )
            .into_response();
    }
    next.run(req).await
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
