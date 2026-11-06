use super::{Coverage, RouteEntry, route};
use crate::{api::compression_review, config::AppState};
use axum::{
    Router,
    routing::{get, post},
};

const COVERAGE: Coverage = Coverage::Exempt {
    reason: "SPA compression investigation queue; CLI provides agent workflow",
};
pub(super) const ROUTES: &[RouteEntry] = &[
    route("GET", "/api/compression-review", COVERAGE),
    route("POST", "/api/compression-review", COVERAGE),
    route("POST", "/api/compression-review/{id}/review", COVERAGE),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/compression-review",
            get(compression_review::list).post(compression_review::flag),
        )
        .route(
            "/api/compression-review/{id}/review",
            post(compression_review::review),
        )
}
