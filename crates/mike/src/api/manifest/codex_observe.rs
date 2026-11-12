use axum::Router;
use axum::routing::{get, post};

use super::{Coverage, RouteEntry, route};
use crate::api::codex_observe as api;
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "POST",
        "/api/codex/observe/hooks",
        Coverage::Exempt {
            reason: "called by scripts/codex-hooks/mike-codex-hook from Codex's own lifecycle and tool hooks; a session never posts to it",
        },
    ),
    route(
        "POST",
        "/api/codex/observe/otel/v1/{signal}",
        Coverage::Exempt {
            reason: "the OTLP/JSON endpoint Codex's otlp-http exporter posts logs, traces and metrics to; a session never posts to it",
        },
    ),
    route(
        "GET",
        "/api/codex/observe/sessions",
        Coverage::Exempt {
            reason: "observed Codex threads, turns and tools for the operator and docs; no session workflow reads it yet",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/codex/observe/hooks", post(api::post_hook))
        .route("/api/codex/observe/otel/v1/{signal}", post(api::post_otel))
        .route("/api/codex/observe/sessions", get(api::list_observed))
}
