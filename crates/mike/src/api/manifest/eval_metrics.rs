use axum::Router;
use axum::routing::{get, patch};

use crate::api::eval::metrics;
use crate::config::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route("GET", "/api/eval/rates", Coverage::Tool("get_eval_rates")),
    route(
        "GET",
        "/api/eval/alarms",
        Coverage::Tool("list_eval_alarms"),
    ),
    route(
        "PATCH",
        "/api/eval/alarms/{id}/acknowledge",
        Coverage::Exempt {
            reason: "operator acknowledgement of a drift alarm; SPA control only, agents must not silence alarms",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/eval/rates", get(metrics::get_eval_rates))
        .route("/api/eval/alarms", get(metrics::list_eval_alarms))
        .route(
            "/api/eval/alarms/{id}/acknowledge",
            patch(metrics::acknowledge_eval_alarm),
        )
}
