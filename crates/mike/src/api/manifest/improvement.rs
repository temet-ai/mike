use axum::Router;
use axum::routing::{get, post};

use super::{Coverage, RouteEntry, route};
use crate::api::improvement;
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/improvement/exposures",
        Coverage::Exempt {
            reason: "bounded weekly audit measurement; existing effects tool remains the discovery entry",
        },
    ),
    route(
        "GET",
        "/api/improvement/actions",
        Coverage::Tool("get_improvement_effects"),
    ),
    route(
        "GET",
        "/api/improvement/trend",
        Coverage::Exempt {
            reason: "chart data for the Improvement page; the actions feed carries the per-action effect a session would ask for",
        },
    ),
    route(
        "GET",
        "/api/improvement/summary",
        Coverage::Exempt {
            reason: "page-section counts for the Improvement page's own layout",
        },
    ),
    route(
        "GET",
        "/api/improvement/repeats",
        Coverage::Exempt {
            reason: "what the operator keeps saying, for the Improvement page; the learning observer reads it over HTTP",
        },
    ),
    route(
        "POST",
        "/api/improvement/rollup",
        Coverage::Exempt {
            reason: "operator-invoked recompute button; the operator triggers it from the page, not an agent",
        },
    ),
    route(
        "GET",
        "/api/improvement/lessons",
        Coverage::Exempt {
            reason: "operator review act on /sessions/improvement; agents must not decide lesson candidates",
        },
    ),
    route(
        "GET",
        "/api/improvement/lessons/{id}",
        Coverage::Exempt {
            reason: "operator review act on /sessions/improvement; agents must not decide lesson candidates",
        },
    ),
    route(
        "POST",
        "/api/improvement/lessons/{id}/approve",
        Coverage::Exempt {
            reason: "operator review act on /sessions/improvement; agents must not decide lesson candidates",
        },
    ),
    route(
        "POST",
        "/api/improvement/lessons/{id}/reject",
        Coverage::Exempt {
            reason: "operator review act on /sessions/improvement; agents must not decide lesson candidates",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/improvement/exposures",
            get(improvement::exposures::list_exposures),
        )
        .route(
            "/api/improvement/actions",
            get(improvement::actions::list_actions),
        )
        .route("/api/improvement/trend", get(improvement::trend::get_trend))
        .route(
            "/api/improvement/summary",
            get(improvement::summary::get_counts),
        )
        .route(
            "/api/improvement/repeats",
            get(improvement::repeats::get_repeats),
        )
        .route(
            "/api/improvement/rollup",
            post(improvement::rollup::run_rollup),
        )
        .route(
            "/api/improvement/lessons",
            get(improvement::lessons::list_candidates),
        )
        .route(
            "/api/improvement/lessons/{id}",
            get(improvement::lessons::get_candidate),
        )
        .route(
            "/api/improvement/lessons/{id}/approve",
            post(improvement::lessons::approve_candidate),
        )
        .route(
            "/api/improvement/lessons/{id}/reject",
            post(improvement::lessons::reject_candidate),
        )
}
