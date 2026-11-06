use axum::Router;
use axum::routing::get;

use super::{Coverage, RouteEntry, route};
use crate::api::config_history;
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/config/change-log",
        Coverage::Tool("list_config_changes"),
    ),
    route(
        "GET",
        "/api/config/snapshot",
        Coverage::Tool("get_config_snapshot"),
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/config/change-log",
            get(config_history::list_config_changes),
        )
        .route(
            "/api/config/snapshot",
            get(config_history::get_config_snapshot),
        )
}
