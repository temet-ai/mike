use axum::Router;
use axum::routing::post;

use super::{Coverage, RouteEntry, route};
use crate::api::session_agent_context;
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] = &[route(
    "POST",
    "/api/sessions/{id}/agent-context",
    Coverage::Exempt {
        reason: "called by tether when a subagent starts, to take the agent-scoped lessons the AgentLessonsStage would inject for its Claude Code agent_type, after which the proxy stands aside for that persona in that session; a session investigating traffic has no use for it",
    },
)];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router.route(
        "/api/sessions/{id}/agent-context",
        post(session_agent_context::post_session_agent_context),
    )
}
