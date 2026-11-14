use reqwest::Method;

pub(crate) use crate::api::wright::relay::RelayServer;

pub(crate) const RUN_HEADER: &str = "x-mike-wright-run";
const PROXY_PATHS: [&str; 2] = ["/v1/messages", "/v1/messages/count_tokens"];
const CLOSED_API_PREFIXES: [&str; 4] = ["/api/wright", "/api/settings", "/api/config", "/api/ws"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Route {
    Proxy,
    Inspect,
    Refuse(&'static str),
}

fn canonical(path: &str) -> bool {
    path.starts_with('/')
        && !path.contains('%')
        && !path.contains("//")
        && !path.contains("/.")
        && !path.contains('\\')
}

pub(crate) fn route(method: &Method, path: &str) -> Route {
    if !canonical(path) {
        return Route::Refuse("the path is not in canonical form");
    }
    let lower = path.to_ascii_lowercase();
    if *method == Method::POST && PROXY_PATHS.contains(&path) {
        return Route::Proxy;
    }
    if lower.starts_with("/v1/") {
        return Route::Refuse(
            "only POST /v1/messages and /v1/messages/count_tokens reach the proxy",
        );
    }
    if !lower.starts_with("/api/") {
        return Route::Refuse("only /v1/messages and GET /api/* are relayed");
    }
    if *method != Method::GET {
        return Route::Refuse("the inspect API is read-only from an engineer run");
    }
    if CLOSED_API_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return Route::Refuse(
            "Wright's own routes, settings, configuration and the event socket are closed to an engineer run",
        );
    }
    Route::Inspect
}

#[derive(Debug, Clone)]
pub(crate) struct RelayTargets {
    pub(crate) run_id: i64,
    pub(crate) proxy: String,
    pub(crate) inspect: String,
    pub(crate) client: reqwest::Client,
}
