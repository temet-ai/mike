use axum::Router;
use axum::routing::post;

use crate::api::skill_outcomes;
use crate::config::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[route(
    "POST",
    "/api/learning/skill-outcomes",
    Coverage::Exempt {
        reason: "written by the learning promoter script to record how each shipped skill did in live use; read it through memory.skill_evidence_v, a session investigating traffic never posts to it",
    },
)];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router.route(
        "/api/learning/skill-outcomes",
        post(skill_outcomes::post_skill_outcomes),
    )
}
