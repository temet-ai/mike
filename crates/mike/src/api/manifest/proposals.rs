use axum::Router;
use axum::routing::{get, post};

use super::{Coverage, RouteEntry, route};
use crate::api::proposals;
use crate::config::AppState;

const PAGE: Coverage = Coverage::Exempt {
    reason: "page data for the Proposals page, where a user reviews lessons mike placed as a rule or an agent statement",
};

pub(super) const ROUTES: &[RouteEntry] = &[
    route("GET", "/api/proposals/projects", PAGE),
    route("GET", "/api/proposals", PAGE),
    route(
        "POST",
        "/api/proposals/{lesson_id}/decision",
        Coverage::Exempt {
            reason: "a user's decision on the Proposals page; agents must not accept or decline what goes into a project's rules or agents",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/proposals/projects", get(proposals::list_projects))
        .route("/api/proposals", get(proposals::list_proposals))
        .route(
            "/api/proposals/{lesson_id}/decision",
            post(proposals::decide),
        )
}
