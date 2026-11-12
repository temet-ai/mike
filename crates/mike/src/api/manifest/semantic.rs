use axum::Router;
use axum::routing::{get, post};

use crate::api::semantic::{self as h, patterns};
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

const DECISION_REASON: &str = "operator names or dismisses a failure pattern; SPA control only, agents must not create failure patterns";

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/semantic/status",
        Coverage::Tool("get_semantic_status"),
    ),
    route(
        "GET",
        "/api/semantic/turns/{turn_id}/neighbours",
        Coverage::Tool("get_semantic_neighbours"),
    ),
    route(
        "GET",
        "/api/semantic/items/{id}",
        Coverage::Tool("get_semantic_item"),
    ),
    route(
        "GET",
        "/api/semantic/patterns/assignments",
        Coverage::Tool("list_pattern_assignments"),
    ),
    route(
        "GET",
        "/api/semantic/patterns/candidates",
        Coverage::Tool("list_pattern_candidates"),
    ),
    route(
        "POST",
        "/api/semantic/patterns/candidates/{id}/name",
        Coverage::Exempt {
            reason: DECISION_REASON,
        },
    ),
    route(
        "POST",
        "/api/semantic/patterns/candidates/{id}/dismiss",
        Coverage::Exempt {
            reason: DECISION_REASON,
        },
    ),
    route(
        "GET",
        "/api/semantic/drift",
        Coverage::Tool("get_embedding_drift"),
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/semantic/status", get(h::get_semantic_status))
        .route(
            "/api/semantic/turns/{turn_id}/neighbours",
            get(h::get_semantic_neighbours),
        )
        .route("/api/semantic/items/{id}", get(h::get_semantic_item))
        .route(
            "/api/semantic/patterns/assignments",
            get(patterns::list_pattern_assignments),
        )
        .route(
            "/api/semantic/patterns/candidates",
            get(patterns::list_pattern_candidates),
        )
        .route(
            "/api/semantic/patterns/candidates/{id}/name",
            post(patterns::name_pattern_candidate),
        )
        .route(
            "/api/semantic/patterns/candidates/{id}/dismiss",
            post(patterns::dismiss_pattern_candidate),
        )
        .route("/api/semantic/drift", get(patterns::get_embedding_drift))
}
