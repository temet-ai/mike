use axum::Router;
use axum::routing::get;

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
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/changes", get(changes::get_changes))
        .route("/api/code-findings", get(changes::code::get_code_findings))
}
