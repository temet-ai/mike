use crate::web_base::route_manifest::{Coverage, RouteEntry, route};
use crate::{api::project_knowledge as api, config::AppState};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "POST",
        "/api/knowledge/projects",
        Coverage::Tool("define_project"),
    ),
    route(
        "GET",
        "/api/knowledge/locations",
        Coverage::Tool("list_project_locations"),
    ),
    route(
        "POST",
        "/api/knowledge/locations",
        Coverage::Tool("register_project_location"),
    ),
    route(
        "POST",
        "/api/knowledge/sources",
        Coverage::Exempt {
            reason: "local collector registers explicitly configured roots",
        },
    ),
    route(
        "POST",
        "/api/knowledge/sources/{id}/scan",
        Coverage::Exempt {
            reason: "local collector submits complete source observations",
        },
    ),
    route(
        "GET",
        "/api/knowledge/search",
        Coverage::Tool("search_project_knowledge"),
    ),
    route(
        "GET",
        "/api/knowledge/lessons",
        Coverage::Exempt {
            reason: "collector lesson durability feed",
        },
    ),
    route(
        "POST",
        "/api/knowledge/lessons",
        Coverage::Exempt {
            reason: "collector imports portable lesson records",
        },
    ),
    route(
        "POST",
        "/api/knowledge/lessons/ack",
        Coverage::Exempt {
            reason: "collector acknowledges file delivery",
        },
    ),
    route(
        "GET",
        "/api/knowledge/read",
        Coverage::Tool("read_project_knowledge"),
    ),
    route(
        "POST",
        "/api/knowledge/rerank-pass",
        Coverage::Exempt {
            reason: "operator runs the overnight rerank pass now",
        },
    ),
];

pub(super) fn register_waited(router: Router<AppState>) -> Router<AppState> {
    router.route("/api/knowledge/rerank-pass", post(api::rerank_pass))
}

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/knowledge/projects", post(api::define_project))
        .route(
            "/api/knowledge/locations",
            get(api::locations).post(api::register_location),
        )
        .route("/api/knowledge/sources", post(api::register_source))
        .route(
            "/api/knowledge/sources/{id}/scan",
            post(api::scan).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/knowledge/search", get(api::search))
        .route("/api/knowledge/read", get(api::read))
        .route(
            "/api/knowledge/lessons",
            get(api::lesson_files).post(api::import_lesson),
        )
        .route("/api/knowledge/lessons/ack", post(api::acknowledge_lessons))
}
