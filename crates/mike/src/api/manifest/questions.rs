use axum::Router;
use axum::routing::post;

use super::{Coverage, RouteEntry, route};
use crate::api::questions;
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] = &[
    route("POST", "/api/questions", Coverage::Tool("ask")),
    route(
        "POST",
        "/api/questions/{question_uuid}/answer",
        Coverage::Tool("answer"),
    ),
    route(
        "POST",
        "/api/questions/{question_uuid}/decline",
        Coverage::Tool("decline"),
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/questions", post(questions::post_question))
        .route(
            "/api/questions/{question_uuid}/answer",
            post(questions::post_question_answer),
        )
        .route(
            "/api/questions/{question_uuid}/decline",
            post(questions::post_question_decline),
        )
}
