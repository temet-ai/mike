use axum::Router;
use axum::extract::FromRef;
use axum::routing::{delete, get};

use crate::api::conditions;
use crate::base_state::BaseState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub const ROUTES: &[RouteEntry] = &[
    route("POST", "/api/conditions", Coverage::Tool("wait_for")),
    route("GET", "/api/conditions", Coverage::Tool("list_conditions")),
    route(
        "DELETE",
        "/api/conditions/{condition}",
        Coverage::Tool("cancel_condition"),
    ),
];

pub fn register<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    BaseState: FromRef<S>,
{
    router
        .route(
            "/api/conditions",
            get(conditions::list_conditions).post(conditions::post_condition),
        )
        .route(
            "/api/conditions/{condition}",
            delete(conditions::cancel_condition),
        )
}
