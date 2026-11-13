use axum::Router;
use axum::routing::get;

use crate::api::ws;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[route(
    "GET",
    "/api/ws",
    Coverage::Exempt {
        reason: "WebSocket upgrade; mike-mcp transport is stdio JSON-RPC",
    },
)];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router.route("/api/ws", get(ws::ws_handler))
}
