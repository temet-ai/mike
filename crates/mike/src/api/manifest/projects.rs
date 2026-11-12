use axum::Router;
use axum::routing::{get, patch, post};

use crate::api::{content_search, embedding_graph, handlers};
use crate::config::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route("GET", "/api/projects", Coverage::Tool("list_projects")),
    route(
        "GET",
        "/api/projects/{project_uuid}",
        Coverage::AggregatedInto("list_projects"),
    ),
    route(
        "GET",
        "/api/projects/by-path/{project_path}",
        Coverage::AggregatedInto("list_projects"),
    ),
    route(
        "GET",
        "/api/projects/{id}/embedding-graph",
        Coverage::Exempt {
            reason: "graph is a UI surface; no agent workflow needs it yet",
        },
    ),
    route(
        "PATCH",
        "/api/projects/{project_path}/pin",
        Coverage::Exempt {
            reason: "pin/TTL lifecycle is SPA/operator-only; MCP tools removed after zero usage ever (temet-ai/mike-private#226)",
        },
    ),
    route(
        "PATCH",
        "/api/projects/{project_path}/ttl",
        Coverage::Exempt {
            reason: "pin/TTL lifecycle is SPA/operator-only; MCP tools removed after zero usage ever (temet-ai/mike-private#226)",
        },
    ),
    route(
        "PATCH",
        "/api/projects/{project_path}/name",
        Coverage::Exempt {
            reason: "friendly-name lifecycle is SPA/operator-only, same as pin/TTL (temet-ai/mike-private#226)",
        },
    ),
    route(
        "POST",
        "/api/projects/{project_path}/merge",
        Coverage::Exempt {
            reason: "operator folds a wrongly split project into its real one from the SPA",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/head",
        Coverage::Tool("get_session_head"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/body/find",
        Coverage::AggregatedInto("find_text"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/ledger",
        Coverage::Exempt {
            reason: "work-item ledger v1; the MCP tool lands with the reconciler (work-registry spec 2.6)",
        },
    ),
    route(
        "POST",
        "/api/sessions/{id}/work-items/backfill",
        Coverage::Exempt {
            reason: "operator/maintenance backfill of work items from captured history",
        },
    ),
    route("GET", "/api/body/find", Coverage::Tool("find_text")),
    route(
        "GET",
        "/api/content/search",
        Coverage::Tool("search_session_content"),
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/projects", get(handlers::list_projects))
        .route("/api/projects/{project_uuid}", get(handlers::get_project))
        .route(
            "/api/projects/by-path/{project_path}",
            get(handlers::get_project_by_path),
        )
        .route(
            "/api/projects/{id}/embedding-graph",
            get(embedding_graph::get_project_embedding_graph),
        )
        .route(
            "/api/projects/{project_path}/pin",
            patch(handlers::patch_project_pin),
        )
        .route(
            "/api/projects/{project_path}/ttl",
            patch(handlers::patch_project_ttl),
        )
        .route(
            "/api/projects/{project_path}/name",
            patch(handlers::patch_project_name),
        )
        .route(
            "/api/projects/{project_path}/merge",
            post(handlers::post_project_merge),
        )
        .route("/api/sessions/{id}/head", get(handlers::get_session_head))
        .route(
            "/api/sessions/{id}/requests",
            get(handlers::get_session_requests),
        )
        .route(
            "/api/sessions/{id}/body/find",
            get(handlers::find_text_in_session),
        )
        .route(
            "/api/sessions/{id}/ledger",
            get(handlers::get_session_ledger),
        )
        .route(
            "/api/sessions/{id}/work-items/backfill",
            post(handlers::post_session_work_items_backfill),
        )
        .route("/api/body/find", get(handlers::find_text))
        .route("/api/content/search", get(content_search::search_content))
}
