use axum::Router;
use axum::routing::{get, post};

use crate::api::steer_questions;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/steering/questions",
        Coverage::Tool("get_steer_questions"),
    ),
    route(
        "GET",
        "/api/steering/questions/{steerer_id}/{question}/events",
        Coverage::Tool("list_steer_question_events"),
    ),
    route(
        "POST",
        "/api/steering/questions/{steerer_id}/{question}/promote",
        Coverage::Exempt {
            reason: "operator act: changes live traffic; agents must not promote",
        },
    ),
    route(
        "POST",
        "/api/steering/questions/{steerer_id}/{question}/demote",
        Coverage::Exempt {
            reason: "operator act on live traffic; automatic demotion covers the unattended case",
        },
    ),
    route(
        "PATCH",
        "/api/steering/questions/{steerer_id}/{question}",
        Coverage::Exempt {
            reason: "SPA-driven settings panel, not needed via MCP",
        },
    ),
    route(
        "POST",
        "/api/steering/evaluations/{uuid}/label",
        Coverage::Exempt {
            reason: "operator ground truth; agents must not write gold labels (D2)",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/steering/questions",
            get(steer_questions::get_steer_questions),
        )
        .route(
            "/api/steering/questions/{steerer_id}/{question}/events",
            get(steer_questions::list_question_events),
        )
        .route(
            "/api/steering/questions/{steerer_id}/{question}/promote",
            post(steer_questions::promote_question),
        )
        .route(
            "/api/steering/questions/{steerer_id}/{question}/demote",
            post(steer_questions::demote_question),
        )
        .route(
            "/api/steering/questions/{steerer_id}/{question}",
            axum::routing::patch(steer_questions::patch_question),
        )
        .route(
            "/api/steering/evaluations/{uuid}/label",
            post(steer_questions::label_evaluation),
        )
}
