use crate::api::project_knowledge as api;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, patch, post},
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
    route(
        "GET",
        "/api/knowledge/kinds",
        Coverage::Exempt {
            reason: "settings page lists knowledge kinds with their embedding counts",
        },
    ),
    route(
        "GET",
        "/api/knowledge/kinds/tables",
        Coverage::Exempt {
            reason: "settings page lists the tables and columns a kind can read",
        },
    ),
    route(
        "POST",
        "/api/knowledge/kinds/preview",
        Coverage::Exempt {
            reason: "settings page previews one item of a kind before it is saved",
        },
    ),
    route(
        "POST",
        "/api/knowledge/kinds",
        Coverage::Exempt {
            reason: "operator defines a knowledge kind on the settings page",
        },
    ),
    route(
        "PATCH",
        "/api/knowledge/kinds/{kind}",
        Coverage::Exempt {
            reason: "operator switches a knowledge kind on or off or retunes it on the settings page",
        },
    ),
    route(
        "DELETE",
        "/api/knowledge/kinds/{kind}",
        Coverage::Exempt {
            reason: "operator deletes a knowledge kind on the settings page",
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
            "/api/knowledge/kinds",
            get(api::list_kinds).post(api::define_kind),
        )
        .route("/api/knowledge/kinds/tables", get(api::list_kind_tables))
        .route("/api/knowledge/kinds/preview", post(api::preview_kind))
        .route(
            "/api/knowledge/kinds/{kind}",
            patch(api::patch_kind).delete(api::delete_kind),
        )
        .route(
            "/api/knowledge/lessons",
            get(api::lesson_files).post(api::import_lesson),
        )
        .route("/api/knowledge/lessons/ack", post(api::acknowledge_lessons))
}
