use axum::Router;
use axum::routing::post;

use crate::api::session_recall;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[route(
    "POST",
    "/api/sessions/{id}/recall",
    Coverage::Exempt {
        reason: "called by tether to take the recall text the proxy's recall injector would add for the topics a session names, after which the proxy stands aside for that session (temet-ai/mike-private#442); a session investigating traffic reads the injected reminder in the request itself",
    },
)];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router.route(
        "/api/sessions/{id}/recall",
        post(session_recall::post_session_recall),
    )
}
