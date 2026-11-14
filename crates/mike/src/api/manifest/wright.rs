use axum::Router;
use axum::routing::{get, patch, post};

use crate::api::wright::{checks, investigations, problems as api};
use crate::state::AppState;
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
    route(
        "GET",
        "/api/wright/investigations",
        Coverage::Exempt {
            reason: "Wright's engineer runs for the Wright page; closed to engineer runs by the sandbox relay and read by no session workflow",
        },
    ),
    route(
        "GET",
        "/api/wright/findings",
        Coverage::Exempt {
            reason: "Wright's findings for the Wright page; no session workflow reads them yet",
        },
    ),
    route(
        "GET",
        "/api/wright/findings/{id}",
        Coverage::Exempt {
            reason: "One Wright finding with its run for the Wright page; no session workflow reads it yet",
        },
    ),
    route(
        "GET",
        "/api/wright/engineer-limits",
        Coverage::Exempt {
            reason: "The engineer's limits for the Wright page and the operator; sessions never read them",
        },
    ),
    route(
        "PATCH",
        "/api/wright/engineer-limits",
        Coverage::Exempt {
            reason: "Operator-only control of the engineer's limits; no session may change Wright's limits",
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
        .route(
            "/api/wright/investigations",
            get(investigations::list_investigations),
        )
        .route("/api/wright/findings", get(investigations::list_findings))
        .route(
            "/api/wright/findings/{id}",
            get(investigations::get_finding),
        )
        .route(
            "/api/wright/engineer-limits",
            get(investigations::get_limits).patch(investigations::patch_limits),
        )
}
