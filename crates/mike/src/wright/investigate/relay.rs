use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tokio::task::JoinHandle;

pub(crate) const RUN_HEADER: &str = "x-mike-wright-run";
const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
const PROXY_PATHS: [&str; 2] = ["/v1/messages", "/v1/messages/count_tokens"];
const CLOSED_API_PREFIXES: [&str; 4] = ["/api/wright", "/api/settings", "/api/config", "/api/ws"];
const HOP_BY_HOP: [&str; 9] = [
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "host",
];
const INSPECT_CREDENTIALS: [&str; 3] = ["authorization", "cookie", "x-api-key"];

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

pub(crate) fn forward_headers(incoming: &HeaderMap, run_id: i64, inspect: bool) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (name, value) in incoming {
        let lower = name.as_str();
        let dropped = HOP_BY_HOP.contains(&lower)
            || lower == "content-length"
            || lower == RUN_HEADER
            || (inspect && INSPECT_CREDENTIALS.contains(&lower));
        if !dropped {
            out.append(name.clone(), value.clone());
        }
    }
    out.insert(
        HeaderName::from_static(RUN_HEADER),
        HeaderValue::from(run_id),
    );
    out
}

fn refused(method: &Method, path: &str, reason: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        axum::Json(json!({
            "error": "refused by Wright's sandbox relay",
            "method": method.as_str(),
            "path": path,
            "reason": reason,
        })),
    )
        .into_response()
}

fn failed(status: StatusCode, message: String) -> Response {
    (status, axum::Json(json!({"error": message}))).into_response()
}

fn reply(upstream: reqwest::Response) -> Response {
    let status = upstream.status();
    let mut headers = HeaderMap::new();
    for (name, value) in upstream.headers() {
        if !HOP_BY_HOP.contains(&name.as_str()) {
            headers.append(name.clone(), value.clone());
        }
    }
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

async fn relay(State(targets): State<Arc<RelayTargets>>, request: Request) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let decision = route(&method, &path);
    let base = match decision {
        Route::Proxy => &targets.proxy,
        Route::Inspect => &targets.inspect,
        Route::Refuse(reason) => return refused(&method, &path, reason),
    };
    let target = request
        .uri()
        .path_and_query()
        .map_or(path.clone(), |pq| pq.as_str().to_owned());
    let (parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, MAX_REQUEST_BYTES).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return failed(
                StatusCode::PAYLOAD_TOO_LARGE,
                format!(
                    "request body for {path} could not be read within {MAX_REQUEST_BYTES} bytes: {e}"
                ),
            );
        }
    };
    let headers = forward_headers(&parts.headers, targets.run_id, decision == Route::Inspect);
    match targets
        .client
        .request(method, format!("{base}{target}"))
        .headers(headers)
        .body(bytes)
        .send()
        .await
    {
        Ok(upstream) => reply(upstream),
        Err(e) => failed(
            StatusCode::BAD_GATEWAY,
            format!("{base} did not answer {path}: {e}"),
        ),
    }
}

pub(crate) fn router(targets: RelayTargets) -> Router {
    Router::new().fallback(relay).with_state(Arc::new(targets))
}

pub(crate) struct RelayServer {
    socket: PathBuf,
    task: JoinHandle<()>,
}

impl RelayServer {
    pub(crate) fn start(socket: &Path, targets: RelayTargets) -> Result<Self, String> {
        if socket.exists() {
            std::fs::remove_file(socket)
                .map_err(|e| format!("cannot remove the old socket {}: {e}", socket.display()))?;
        }
        let listener = tokio::net::UnixListener::bind(socket)
            .map_err(|e| format!("cannot listen on {}: {e}", socket.display()))?;
        std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("cannot restrict {}: {e}", socket.display()))?;
        let app = router(targets);
        let task = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                tracing::warn!(error = %e, "wright: the sandbox relay stopped");
            }
        });
        Ok(Self {
            socket: socket.to_path_buf(),
            task,
        })
    }

    pub(crate) fn stop(self) {
        self.task.abort();
        if let Err(e) = std::fs::remove_file(&self.socket)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(error = %e, socket = %self.socket.display(), "wright: the relay socket could not be removed");
        }
    }
}
