use axum::Router;
use axum::routing::{get, patch, post};

use crate::api::wright::{checks, conversation, problems as api};
use crate::api::wright::{checks, investigations, problems as api};
use crate::api::wright::{checks, problems as api, remedies};
use crate::api::wright::{checks, problems as api, repairs};
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
    route(
        "GET",
        "/api/wright/repairs",
        Coverage::Exempt {
            reason: "Wright's code repairs and their states for the Wright page; no session workflow reads them yet",
        },
    ),
    route(
        "GET",
        "/api/wright/repairs/{id}",
        Coverage::Exempt {
            reason: "One Wright code repair with its patch, gate result and merge for the Wright page; no session workflow reads it yet",
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
        .route("/api/wright/remedies", get(remedies::list_remedies))
        .route("/api/wright/remedies/{name}", patch(remedies::patch_remedy))
        .route("/api/wright/actions", get(remedies::list_actions))
        .route("/api/wright/actions/{id}/undo", post(remedies::undo_action))
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
        .route("/api/wright/repairs", get(repairs::list_repairs))
        .route("/api/wright/repairs/{id}", get(repairs::get_repair))
        .route(
            "/api/wright/messages",
            get(conversation::list_messages).post(conversation::post_message),
        )
        .route(
            "/api/wright/steers/{id}/undo",
            post(conversation::undo_steer),
        )
}
