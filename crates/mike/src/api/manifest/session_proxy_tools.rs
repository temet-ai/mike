use axum::Router;
use axum::routing::{get, post};

use crate::api::session_proxy_tools;
use crate::config::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/sessions/{id}/proxy-tools",
        Coverage::Exempt {
            reason: "called by tether to fetch the tools the proxy would declare into the session's requests; the proxy then stops declaring them for that session",
        },
    ),
    route(
        "POST",
        "/api/sessions/{id}/proxy-tools/{name}",
        Coverage::Exempt {
            reason: "called by tether to answer a proxy-tool call the model made, with the handler the in-stream tool loop uses",
        },
    ),
    route(
        "POST",
        "/api/sessions/{id}/weasel-guard",
        Coverage::Exempt {
            reason: "called by tether to say it guards the session's stream, so the proxy's weasel intercept stands aside for that session",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/sessions/{id}/proxy-tools",
            get(session_proxy_tools::get_proxy_tools),
        )
        .route(
            "/api/sessions/{id}/proxy-tools/{name}",
            post(session_proxy_tools::post_proxy_tool),
        )
        .route(
            "/api/sessions/{id}/weasel-guard",
            post(session_proxy_tools::post_weasel_guard),
        )
}
