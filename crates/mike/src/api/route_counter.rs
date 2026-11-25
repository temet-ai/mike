use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, MatchedPath, Request};
use axum::http::HeaderMap;
use axum::http::header::USER_AGENT;
use axum::middleware::Next;
use axum::response::Response;

pub(crate) const COUNTER_INSPECT_REQUESTS: &str = "inspect_requests_total";
const CLIENT_CHARS: usize = 40;
const SEEN_FOR: Duration = Duration::from_secs(3600);
const SEEN_BOUND: usize = 4096;

type CallerKey = (String, String, u16);

static SEEN: LazyLock<Mutex<HashMap<CallerKey, Instant>>> = LazyLock::new(Mutex::default);

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

pub(crate) fn first_in_the_hour(
    seen: &mut HashMap<CallerKey, Instant>,
    key: CallerKey,
    now: Instant,
) -> bool {
    if seen
        .get(&key)
        .is_some_and(|at| now.duration_since(*at) < SEEN_FOR)
    {
        return false;
    }
    if seen.len() >= SEEN_BOUND {
        seen.retain(|_, at| now.duration_since(*at) < SEEN_FOR);
        if seen.len() >= SEEN_BOUND {
            seen.clear();
        }
    }
    seen.insert(key, now);
    true
}

fn log_caller(labels: &Labels, peer: SocketAddr) {
    let key = (labels.route.clone(), labels.method.clone(), peer.port());
    let first = first_in_the_hour(
        &mut SEEN.lock().unwrap_or_else(PoisonError::into_inner),
        key,
        Instant::now(),
    );
    if first {
        tracing::info!(
            route = %labels.route,
            method = %labels.method,
            peer_port = peer.port(),
            client = %labels.client,
            "inspect: first call to this route from this connection in the hour"
        );
    }
}

pub(crate) async fn count(request: Request, next: Next) -> Response {
    let labels = labels(&request);
    if let Some(ConnectInfo(peer)) = request.extensions().get::<ConnectInfo<SocketAddr>>() {
        log_caller(&labels, *peer);
    }
    let Labels {
        route,
        method,
        client,
    } = labels;
    let _running = mike_base::web_base::db_pressure::InFlight::start(&method, &route);
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
