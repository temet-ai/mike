use axum::Router;
use axum::routing::{get, patch, post};

use crate::api::wright::{checks, conversation, problems as api};
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
        "/api/wright/messages",
        Coverage::Exempt {
            reason: "The operator's conversation with Wright, read by the Wright page; sessions never read or join it, since the operator is the only voice Wright takes instructions from",
        },
    ),
    route(
        "POST",
        "/api/wright/messages",
        Coverage::Exempt {
            reason: "Only the paired operator posts to Wright, with the operator Bearer token; an MCP tool would let a session speak as the operator",
        },
    ),
    route(
        "POST",
        "/api/wright/steers/{id}/undo",
        Coverage::Exempt {
            reason: "Undoing a steer is the operator's act on the Wright page and needs the operator Bearer token; sessions never undo Wright's changes",
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
            "/api/wright/messages",
            get(conversation::list_messages).post(conversation::post_message),
        )
        .route(
            "/api/wright/steers/{id}/undo",
            post(conversation::undo_steer),
        )
}
