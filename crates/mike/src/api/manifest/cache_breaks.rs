use axum::Router;
use axum::routing::{get, post};

use crate::api::cache_breaks;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/cache-breaks",
        Coverage::Exempt {
            reason: "the prompt-cache breaks behind the prompt_cache health row, read by the diagnostics page; the operator sees them without asking, a session has no use for the list",
        },
    ),
    route(
        "POST",
        "/api/cache-breaks/rescan",
        Coverage::Exempt {
            reason: "run-now for the prompt-cache scanner: moves its cursor back so the next pass rescans the last days; an operator control",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/cache-breaks",
        Coverage::Exempt {
            reason: "the session page's prompt-cache panel; a session investigating traffic reads the same rows through get_request",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/cache-breaks", get(cache_breaks::get_cache_breaks))
        .route(
            "/api/cache-breaks/rescan",
            post(cache_breaks::post_cache_breaks_rescan),
        )
        .route(
            "/api/sessions/{id}/cache-breaks",
            get(cache_breaks::get_session_cache_breaks),
        )
}
