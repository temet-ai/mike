use crate::launch::state::LaunchState;
use axum::Router;
use axum::extract::FromRef;
use axum::routing::{get, post};

use crate::api::fleet;
use crate::base_state::BaseState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/fleets/by-session/{session}/status",
        Coverage::Exempt {
            reason: "SPA drill-in from a selected fleet session; agents already hold their fleet id",
        },
    ),
    route("POST", "/api/fleets", Coverage::Tool("define_fleet")),
    route(
        "GET",
        "/api/fleets",
        Coverage::Exempt {
            reason: "a listing for the operator's `mike fleet list` and the SPA; a session that \
                     defines a fleet already holds its id and reads it with get_fleet",
        },
    ),
    route("GET", "/api/fleets/{fleet}", Coverage::Tool("get_fleet")),
    route(
        "PATCH",
        "/api/fleets/{fleet}",
        Coverage::Tool("define_fleet"),
    ),
    route(
        "POST",
        "/api/fleets/{fleet}/start",
        Coverage::Exempt {
            reason: "the operator's go-ahead when the fleet_auto_start setting is off; a \
                     session never starts a fleet, mike starts it once the checks pass",
        },
    ),
    route(
        "POST",
        "/api/fleets/{fleet}/stop",
        Coverage::Exempt {
            reason: "the operator's `mike fleet stop`; a fleet also ends on its own once every \
                     seat's process has exited, so no session needs a tool for it",
        },
    ),
    route(
        "GET",
        "/api/fleets/{fleet}/changes",
        Coverage::Tool("list_fleet_changes"),
    ),
    route(
        "GET",
        "/api/fleets/{fleet}/results",
        Coverage::Tool("list_fleet_results"),
    ),
    route(
        "GET",
        "/api/fleets/{fleet}/record",
        Coverage::Tool("get_fleet_record"),
    ),
    route(
        "GET",
        "/api/fleets/{fleet}/reuse",
        Coverage::Exempt {
            reason: "asked by the launcher's check before a seat's slot command; a session \
                     asking for itself reads list_fleet_results",
        },
    ),
    route(
        "POST",
        "/api/fleets/{fleet}/commit",
        Coverage::Tool("fleet_commit"),
    ),
    route(
        "POST",
        "/api/fleets/{fleet}/integrate",
        Coverage::AggregatedInto("fleet_commit"),
    ),
    route(
        "POST",
        "/api/fleets/{fleet}/check",
        Coverage::Tool("fleet_check"),
    ),
    route(
        "POST",
        "/api/fleets/{fleet}/gate",
        Coverage::Exempt {
            reason: "the acceptance seat's `mike fleet gate` in its pane: one seat asks for it \
                     a few times a run, so it is a command and not a tool every session carries",
        },
    ),
    route(
        "GET",
        "/api/fleets/{fleet}/where",
        Coverage::Tool("fleet_where"),
    ),
    route(
        "POST",
        "/api/fleets/{fleet}/propose",
        Coverage::Tool("fleet_propose"),
    ),
    route(
        "POST",
        "/api/fleets/{fleet}/split",
        Coverage::Tool("fleet_split"),
    ),
    route(
        "POST",
        "/api/fleets/{fleet}/report",
        Coverage::Tool("fleet_report"),
    ),
    route(
        "POST",
        "/api/fleets/{fleet}/decide",
        Coverage::Tool("fleet_decide"),
    ),
    route(
        "POST",
        "/api/fleets/{fleet}/verdict",
        Coverage::Tool("fleet_verdict"),
    ),
    route(
        "GET",
        "/api/fleets/{fleet}/status",
        Coverage::Exempt {
            reason: "read by `mike fleet status` and the status pane; the definition and its \
                     missing items are already returned by get_fleet",
        },
    ),
    route(
        "POST",
        "/api/fleets/{fleet}/seats/{session}/stop-facts",
        Coverage::Exempt {
            reason: "fleet-comms mod only",
        },
    ),
];

pub fn register<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    BaseState: FromRef<S>,
    LaunchState: FromRef<S>,
{
    router
        .route(
            "/api/fleets/by-session/{session}/status",
            get(fleet::get_fleet_status_for_session),
        )
        .route(
            "/api/fleets",
            get(fleet::list_fleets).post(fleet::create_fleet),
        )
        .route(
            "/api/fleets/{fleet}",
            get(fleet::get_fleet).patch(fleet::patch_fleet),
        )
        .route("/api/fleets/{fleet}/start", post(fleet::start_fleet))
        .route("/api/fleets/{fleet}/stop", post(fleet::stop_fleet))
        .route("/api/fleets/{fleet}/status", get(fleet::get_fleet_status))
        .route("/api/fleets/{fleet}/changes", get(fleet::get_fleet_changes))
        .route("/api/fleets/{fleet}/results", get(fleet::get_fleet_results))
        .route("/api/fleets/{fleet}/record", get(fleet::get_fleet_record))
        .route("/api/fleets/{fleet}/reuse", get(fleet::get_fleet_reuse))
        .route(
            "/api/fleets/{fleet}/integrate",
            post(fleet::integrate_fleet),
        )
        .route("/api/fleets/{fleet}/where", get(fleet::where_in_fleet))
        .route(
            "/api/fleets/{fleet}/seats/{session}/stop-facts",
            post(fleet::post_stop_facts),
        )
        .route("/api/fleets/{fleet}/propose", post(fleet::propose_task))
        .route("/api/fleets/{fleet}/split", post(fleet::split_task))
        .route("/api/fleets/{fleet}/report", post(fleet::report_slice))
        .route("/api/fleets/{fleet}/decide", post(fleet::decide_problem))
        .route("/api/fleets/{fleet}/verdict", post(fleet::record_verdict))
}

pub fn register_waited<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    BaseState: FromRef<S>,
    LaunchState: FromRef<S>,
{
    router
        .route("/api/fleets/{fleet}/commit", post(fleet::commit_fleet))
        .route("/api/fleets/{fleet}/check", post(fleet::check_fleet))
        .route("/api/fleets/{fleet}/gate", post(fleet::gate_fleet))
}
