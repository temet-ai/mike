use axum::Router;
use axum::routing::get;

use crate::api::changes;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] =
    &[route("GET", "/api/changes", Coverage::Tool("what_changed"))];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router.route("/api/changes", get(changes::get_changes))
}
