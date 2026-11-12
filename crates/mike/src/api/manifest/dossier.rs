use axum::Router;
use axum::routing::get;

use crate::api::dossier::handlers;
use crate::config::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/sessions/{id}/dossier",
        Coverage::Tool("get_session_dossier"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/prompt-stream",
        Coverage::AggregatedInto("get_session_dossier"),
    ),
    route(
        "GET",
        "/api/operator-turns/{id}/facts",
        Coverage::Tool("get_turn_facts"),
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/sessions/{id}/dossier",
            get(handlers::get_session_dossier),
        )
        .route(
            "/api/sessions/{id}/prompt-stream",
            get(handlers::get_session_prompt_stream),
        )
        .route(
            "/api/operator-turns/{id}/facts",
            get(handlers::get_turn_facts),
        )
}
