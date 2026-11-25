use axum::Router;
use axum::routing::{get, post};

use crate::api::changes;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route("GET", "/api/changes", Coverage::Tool("what_changed")),
    route(
        "GET",
        "/api/code-findings",
        Coverage::Exempt {
            reason: "mike-code scans and findings for the operator; a tool is added once a session workflow reads them",
        },
    ),
    route(
        "POST",
        "/api/code-findings/{id}/answer",
        Coverage::Exempt {
            reason: "a session answers a finding with `mike findings`, named in the note that tells it, so no tool is carried on every request",
        },
    ),
    route(
        "POST",
        "/api/code-findings/{id}/accept",
        Coverage::Exempt {
            reason: "the operator's ruling on the code findings page; never a session's",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/changes", get(changes::get_changes))
        .route("/api/code-findings", get(changes::code::get_code_findings))
        .route(
            "/api/code-findings/{id}/answer",
            post(changes::code::post_code_answer),
        )
        .route(
            "/api/code-findings/{id}/accept",
            post(changes::code::post_code_accept),
        )
}
