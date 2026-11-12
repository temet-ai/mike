use axum::Router;
use axum::routing::{get, post};

use crate::api::{cross_session, cross_session_approve};
use crate::config::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/cross-session/calibration",
        Coverage::Tool("get_cross_session_calibration"),
    ),
    route(
        "GET",
        "/api/cross-session/thresholds",
        Coverage::AggregatedInto("get_cross_session_calibration"),
    ),
    route(
        "GET",
        "/api/directive-proposals",
        Coverage::Tool("list_directive_proposals"),
    ),
    route(
        "GET",
        "/api/directive-proposals/{id}",
        Coverage::AggregatedInto("list_directive_proposals"),
    ),
    route(
        "POST",
        "/api/directive-proposals/{id}/reject",
        Coverage::Exempt {
            reason: "operator review act on /memory; agents must not decide directives",
        },
    ),
    route(
        "POST",
        "/api/directive-proposals/{id}/approve",
        Coverage::Exempt {
            reason: "operator review act on /memory; agents must not decide directives",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/cross-session/calibration",
            get(cross_session::get_calibration),
        )
        .route(
            "/api/cross-session/thresholds",
            get(cross_session::get_thresholds),
        )
        .route(
            "/api/directive-proposals",
            get(cross_session::list_proposals),
        )
        .route(
            "/api/directive-proposals/{id}",
            get(cross_session::get_proposal),
        )
        .route(
            "/api/directive-proposals/{id}/reject",
            post(cross_session::reject_proposal),
        )
        .route(
            "/api/directive-proposals/{id}/approve",
            post(cross_session_approve::approve_proposal),
        )
}
