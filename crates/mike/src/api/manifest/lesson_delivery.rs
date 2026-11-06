use axum::Router;
use axum::routing::{get, post};

use super::{Coverage, RouteEntry, route};
use crate::api::{lesson_delivery, lesson_delivery_comparison};
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "POST",
        "/api/lessons/for-prompt",
        Coverage::Exempt {
            reason: "called by the mike mod at prompt submit to fetch the lessons it adds as hook context; a session investigating traffic has no use for it",
        },
    ),
    route(
        "GET",
        "/api/lessons/delivery-comparison",
        Coverage::Exempt {
            reason: "bounded weekly audit measurement comparing the mod and control delivery arms; the audit reads it, a session investigating traffic does not",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/lessons/for-prompt",
            post(lesson_delivery::post_for_prompt),
        )
        .route(
            "/api/lessons/delivery-comparison",
            get(lesson_delivery_comparison::get_delivery_comparison),
        )
}
