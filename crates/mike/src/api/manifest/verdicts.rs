use axum::Router;
use axum::routing::get;

use super::{Coverage, RouteEntry, route};
use crate::api::turn_verdicts;
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/operator-turns/{id}/verdicts",
        Coverage::Tool("get_turn_verdicts"),
    ),
    route(
        "GET",
        "/api/turn-verdicts",
        Coverage::Tool("list_turn_verdicts"),
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/operator-turns/{id}/verdicts",
            get(turn_verdicts::get_turn_verdicts),
        )
        .route("/api/turn-verdicts", get(turn_verdicts::list_turn_verdicts))
}
