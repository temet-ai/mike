use axum::Router;
use axum::routing::{get, patch, post};

use crate::api::wright::{checks, problems as api};
use crate::config::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/wright/problems",
        Coverage::Exempt {
            reason: "Wright's problem list for the Wright page and operators; no session workflow reads it yet, and every field is in the API answer",
        },
    ),
    route(
        "GET",
        "/api/wright/problems/{id}",
        Coverage::Exempt {
            reason: "One Wright problem with its episodes and evidence for the Wright page; no session workflow reads it yet",
        },
    ),
    route("GET", "/api/wright/checks", Coverage::Tool("wright_checks")),
    route(
        "POST",
        "/api/wright/checks",
        Coverage::AggregatedInto("wright_checks"),
    ),
    route(
        "PATCH",
        "/api/wright/checks/{name}",
        Coverage::AggregatedInto("wright_checks"),
    ),
    route(
        "DELETE",
        "/api/wright/checks/{name}",
        Coverage::AggregatedInto("wright_checks"),
    ),
    route(
        "POST",
        "/api/wright/client-errors",
        Coverage::Exempt {
            reason: "Written by the web page when a route fails to load or throws; sessions never report page errors",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/wright/problems", get(api::list_problems))
        .route("/api/wright/problems/{id}", get(api::get_problem))
        .route(
            "/api/wright/checks",
            get(checks::list_checks).post(checks::add_check),
        )
        .route(
            "/api/wright/checks/{name}",
            patch(checks::patch_check).delete(checks::delete_check),
        )
        .route("/api/wright/client-errors", post(api::report_client_error))
}
