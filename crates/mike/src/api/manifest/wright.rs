use axum::Router;
use axum::routing::{get, patch, post};

use crate::api::wright::{checks, problems as api, remedies};
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
        "/api/wright/remedies",
        Coverage::Exempt {
            reason: "Wright's known fixes with their limits and pause state, for the Wright page and the operator; no session workflow reads them",
        },
    ),
    route(
        "PATCH",
        "/api/wright/remedies/{name}",
        Coverage::Exempt {
            reason: "The operator's switch and limits for one known fix; a session must never change Wright's limits, so no tool reaches it",
        },
    ),
    route(
        "GET",
        "/api/wright/actions",
        Coverage::Exempt {
            reason: "What Wright's known fixes did, with prior state, effect and undo, for the Wright page; no session workflow reads it",
        },
    ),
    route(
        "POST",
        "/api/wright/actions/{id}/undo",
        Coverage::Exempt {
            reason: "The operator undoes one of Wright's setting or filter rule changes from the Wright page; sessions never undo Wright's actions",
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
        .route("/api/wright/remedies", get(remedies::list_remedies))
        .route("/api/wright/remedies/{name}", patch(remedies::patch_remedy))
        .route("/api/wright/actions", get(remedies::list_actions))
        .route("/api/wright/actions/{id}/undo", post(remedies::undo_action))
}
