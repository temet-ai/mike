use axum::Router;
use axum::routing::get;

use super::{Coverage, RouteEntry, route};
use crate::api::changes;
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] =
    &[route("GET", "/api/changes", Coverage::Tool("what_changed"))];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router.route("/api/changes", get(changes::get_changes))
}
