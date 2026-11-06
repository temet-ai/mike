use axum::Router;
use axum::routing::get;

use super::{Coverage, RouteEntry, route};
use crate::api::prompt_shaping;
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] = &[route(
    "GET",
    "/api/sessions/{id}/prompt-shaping",
    Coverage::Exempt {
        reason: "called by tether to take the filter rules, lazy sections and compositions the proxy would apply to the prompt, after which the proxy stands aside for that session (#442); a session investigating traffic has no use for it",
    },
)];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router.route(
        "/api/sessions/{id}/prompt-shaping",
        get(prompt_shaping::get_prompt_shaping),
    )
}
