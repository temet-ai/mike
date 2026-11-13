use axum::Router;
use axum::routing::{get, post};

use crate::api::{learning_loop, proposer_pass};
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/learning-loop",
        Coverage::Exempt {
            reason: "the web app's learning-loop panel: each stage's switch, last run, rows written in 24 hours and where the loop is broken, with each stage's run-now route",
        },
    ),
    route(
        "POST",
        "/api/lessons/proposer-pass",
        Coverage::Exempt {
            reason: "the operator runs one lesson proposer pass now, outside the quiescence worker's schedule, from the web app or curl",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/learning-loop", get(learning_loop::get_learning_loop))
        .route(
            "/api/lessons/proposer-pass",
            post(proposer_pass::post_proposer_pass),
        )
}
