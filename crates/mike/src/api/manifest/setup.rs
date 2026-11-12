use axum::Router;
use axum::routing::get;

use crate::api::setup_state;
use crate::config::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[route(
    "GET",
    "/api/setup/state",
    Coverage::Exempt {
        reason: "Settings setup page: the same checks as `mike doctor --json`, the Claude Code wiring read from the config file, and the feature presets; SPA-only, read-only, not needed via MCP",
    },
)];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router.route("/api/setup/state", get(setup_state::get_setup_state))
}
