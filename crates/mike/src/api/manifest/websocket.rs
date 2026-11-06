use axum::Router;
use axum::routing::get;

use super::{Coverage, RouteEntry, route};
use crate::api::ws;
use crate::config::AppState;

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
