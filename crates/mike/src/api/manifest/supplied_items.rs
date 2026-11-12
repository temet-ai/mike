use axum::Router;
use axum::routing::{get, post};

use crate::api::supplied_items;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/supplied-items",
        Coverage::Tool("list_supplied_items"),
    ),
    route(
        "GET",
        "/api/supplied-items/figures",
        Coverage::Exempt {
            reason: "weekly figures of supplied memory (used, corrected anyway, ignored, bytes sent for nothing) read by the learning observer and the audit over HTTP",
        },
    ),
    route(
        "GET",
        "/api/supplied-items/hand-check",
        Coverage::Exempt {
            reason: "the weekly hand-check sample of supply verdicts and its agreement rate, read by the learning observer and the person who checks it",
        },
    ),
    route(
        "POST",
        "/api/supplied-items/{id}/hand-check",
        Coverage::Exempt {
            reason: "a person records their own verdict on a sampled item; no session writes hand labels",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/supplied-items",
            get(supplied_items::list_supplied_items),
        )
        .route(
            "/api/supplied-items/figures",
            get(supplied_items::supply_figures),
        )
        .route(
            "/api/supplied-items/hand-check",
            get(supplied_items::hand_check_sample),
        )
        .route(
            "/api/supplied-items/{id}/hand-check",
            post(supplied_items::post_hand_check),
        )
}
