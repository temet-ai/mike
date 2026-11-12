use axum::Router;
use axum::routing::post;

use crate::api::session_mail;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[route(
    "POST",
    "/api/sessions/{id}/mail/claim",
    Coverage::Exempt {
        reason: "called by the tether mod from inside the session to take its pending mail and launch briefs; a session investigating traffic has no use for it",
    },
)];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router.route(
        "/api/sessions/{id}/mail/claim",
        post(session_mail::post_mail_claim),
    )
}
