use axum::Router;
use axum::routing::{get, post};

use crate::api::{fleet, handlers, workspace};
use crate::config::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "POST",
        "/api/workspace/register",
        Coverage::Tool("register_workspace_session"),
    ),
    route(
        "POST",
        "/api/workspace/register/{id}/end",
        Coverage::Tool("end_workspace_registration"),
    ),
    route(
        "GET",
        "/api/workspace/projects/{project_path}/roster",
        Coverage::Tool("get_workspace_roster"),
    ),
    route(
        "POST",
        "/api/workspace/heartbeat",
        Coverage::Tool("workspace_heartbeat"),
    ),
    route(
        "POST",
        "/api/workspace/sessions/{id}/channel/claim",
        Coverage::Exempt {
            reason: "mike-mcp's own ~2s background poll for MCP-channel push-wake (issue temet-ai/mike-private#352); a bridge-internal delivery mechanism, not an agent-facing investigation surface, so it is never a callable tool",
        },
    ),
    route(
        "POST",
        "/api/workspace/sessions/{id}/channel/release",
        Coverage::Exempt {
            reason: "mike-mcp hands back channel-claimed mail it could not push (issue temet-ai/mike-private#352); bridge-internal, never a callable tool",
        },
    ),
    route(
        "GET",
        "/api/workspace/projects/{project_path}/activity",
        Coverage::Exempt {
            reason: "coordination_events ledger read (issue temet-ai/mike-private#217); SPA-driven project activity feed, no MCP tool until an agent workflow needs it",
        },
    ),
    route(
        "GET",
        "/api/workspace/projects/{project_path}/notes",
        Coverage::Tool("list_session_notes"),
    ),
    route(
        "GET",
        "/api/workspace/wake-pending",
        Coverage::Exempt {
            reason: "pending-wake probe for the sibling wake/channel server, not an agent-facing investigation surface; no MCP tool until one needs it",
        },
    ),
    route("GET", "/api/fleet/graph", Coverage::Tool("get_fleet_graph")),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/workspace/register", post(workspace::register_session))
        .route(
            "/api/workspace/register/{id}/end",
            post(workspace::end_registration),
        )
        .route(
            "/api/workspace/projects/{project_path}/roster",
            get(workspace::get_project_roster),
        )
        .route("/api/workspace/heartbeat", post(workspace::heartbeat))
        .route(
            "/api/workspace/sessions/{id}/channel/claim",
            post(workspace::claim_channel_messages),
        )
        .route(
            "/api/workspace/sessions/{id}/channel/release",
            post(workspace::release_channel_messages),
        )
        .route(
            "/api/workspace/projects/{project_path}/activity",
            get(workspace::get_project_activity),
        )
        .route(
            "/api/workspace/projects/{project_path}/notes",
            get(handlers::get_project_notes),
        )
        .route(
            "/api/workspace/wake-pending",
            get(workspace::get_wake_pending),
        )
        .route("/api/fleet/graph", get(fleet::get_fleet_graph))
}
