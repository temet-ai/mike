use axum::extract::{MatchedPath, Request};
use axum::http::HeaderMap;
use axum::http::header::USER_AGENT;
use axum::middleware::Next;
use axum::response::Response;

pub(crate) const COUNTER_INSPECT_REQUESTS: &str = "inspect_requests_total";
const CLIENT_CHARS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Labels {
    pub(crate) route: String,
    pub(crate) method: String,
    pub(crate) client: String,
}

pub(crate) fn labels(request: &Request) -> Labels {
    Labels {
        route: request
            .extensions()
            .get::<MatchedPath>()
            .map_or_else(|| "unmatched".to_owned(), |p| p.as_str().to_owned()),
        method: request.method().as_str().to_owned(),
        client: client_of(request.headers()),
    }
}

pub(crate) fn client_of(headers: &HeaderMap) -> String {
    headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .and_then(|agent| agent.split(['/', ' ']).next())
        .filter(|product| !product.is_empty())
        .map_or_else(
            || "unknown".to_owned(),
            |product| product.chars().take(CLIENT_CHARS).collect(),
        )
}

pub(crate) async fn count(request: Request, next: Next) -> Response {
    let Labels {
        route,
        method,
        client,
    } = labels(&request);
    metrics::counter!(
        COUNTER_INSPECT_REQUESTS,
        "route" => route,
        "method" => method,
        "client" => client,
    )
    .increment(1);
    next.run(request).await
}

#[cfg(test)]
#[path = "route_counter_tests.rs"]
mod tests;
