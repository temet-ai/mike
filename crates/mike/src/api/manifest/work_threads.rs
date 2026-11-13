use axum::Router;
use axum::routing::{get, post};

use crate::api::work_threads as api;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/work-threads",
        Coverage::Tool("work_thread_list"),
    ),
    route(
        "POST",
        "/api/work-threads",
        Coverage::Tool("work_thread_checkpoint"),
    ),
    route(
        "GET",
        "/api/work-threads/{slug}",
        Coverage::Tool("work_thread_get"),
    ),
    route(
        "PATCH",
        "/api/work-threads/{slug}",
        Coverage::AggregatedInto("work_thread_checkpoint"),
    ),
    route(
        "POST",
        "/api/work-threads/{slug}/entries",
        Coverage::Tool("work_thread_add_entry"),
    ),
    route(
        "GET",
        "/api/work-threads/{slug}/export",
        Coverage::Exempt {
            reason: "Markdown rendering of one thread for people to read or hand on; a session reads the same content as structured fields through work_thread_get",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/work-threads",
            get(api::list_threads).post(api::checkpoint_thread),
        )
        .route(
            "/api/work-threads/{slug}",
            get(api::get_thread).patch(api::patch_thread),
        )
        .route("/api/work-threads/{slug}/entries", post(api::add_entry))
        .route("/api/work-threads/{slug}/export", get(api::export_thread))
}
