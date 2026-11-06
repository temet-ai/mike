use axum::Router;
use axum::routing::{get, patch, post};

use super::{Coverage, RouteEntry, route};
use crate::api::handlers;
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/filter/rules",
        Coverage::Tool("list_filter_rules"),
    ),
    route(
        "POST",
        "/api/filter/rules",
        Coverage::Tool("create_filter_rule"),
    ),
    route(
        "PATCH",
        "/api/filter/rules/{id}",
        Coverage::Tool("update_filter_rule"),
    ),
    route(
        "DELETE",
        "/api/filter/rules/{id}",
        Coverage::Tool("delete_filter_rule"),
    ),
    route(
        "POST",
        "/api/filter/rules/dry-run",
        Coverage::Tool("dry_run_filter_rules"),
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/filter/rules",
            get(handlers::list_filter_rules).post(handlers::create_filter_rule),
        )
        .route(
            "/api/filter/rules/{id}",
            patch(handlers::patch_filter_rule).delete(handlers::delete_filter_rule),
        )
        .route(
            "/api/filter/rules/dry-run",
            post(handlers::dry_run_filter_rules),
        )
}
