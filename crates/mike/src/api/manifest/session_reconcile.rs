use axum::Router;
use axum::routing::get;

use crate::api::session_reconcile;
use crate::config::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[route(
    "GET",
    "/api/sessions/{id}/reconcile",
    Coverage::Exempt {
        reason: "the SPA's session page reads tether's mod-step vs wire-request reconciliation through it so the browser never holds tether's operator token; a session investigating traffic reads the captured requests directly",
    },
)];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router.route(
        "/api/sessions/{id}/reconcile",
        get(session_reconcile::get_session_reconcile),
    )
}
