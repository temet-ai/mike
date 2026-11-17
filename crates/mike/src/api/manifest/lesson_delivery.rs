use axum::Router;
use axum::routing::post;

use crate::api::lesson_delivery;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[route(
    "POST",
    "/api/lessons/for-prompt",
    Coverage::Exempt {
        reason: "called by the mike mod and tether at prompt submit to fetch the lessons they add as hook context; a session investigating traffic has no use for it",
    },
)];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router.route(
        "/api/lessons/for-prompt",
        post(lesson_delivery::post_for_prompt),
    )
}
