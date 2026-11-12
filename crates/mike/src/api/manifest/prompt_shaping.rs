use axum::Router;
use axum::routing::get;

use crate::api::prompt_shaping;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[route(
    "GET",
    "/api/sessions/{id}/prompt-shaping",
    Coverage::Exempt {
        reason: "called by tether to take the filter rules, lazy sections and compositions the proxy would apply to the prompt, after which the proxy stands aside for that session (temet-ai/mike-private#442); a session investigating traffic has no use for it",
    },
)];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router.route(
        "/api/sessions/{id}/prompt-shaping",
        get(prompt_shaping::get_prompt_shaping),
    )
}
