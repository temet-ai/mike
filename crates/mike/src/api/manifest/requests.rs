use axum::Router;
use axum::routing::{get, post};

use super::{Coverage, RouteEntry, route};
use crate::api::handlers;
use crate::config::AppState;

pub(super) const ROUTES: &[RouteEntry] = &[
    route("GET", "/api/health", Coverage::AggregatedInto("get_stats")),
    route(
        "GET",
        "/api/connection-info",
        Coverage::Exempt {
            reason: "Installed-user endpoint metadata for the local UI",
        },
    ),
    route(
        "GET",
        "/api/capabilities",
        Coverage::Exempt {
            reason: "AC6 (temet-ai/mike-private#52) capability panel; SPA-driven /settings honesty check, not needed via MCP",
        },
    ),
    route(
        "GET",
        "/api/scheduled-jobs",
        Coverage::Exempt {
            reason: "home-page dashboard of the proxy's scheduled jobs; SPA-driven, not needed via MCP",
        },
    ),
    route(
        "GET",
        "/api/db-cost",
        Coverage::Exempt {
            reason: "the running database cost report behind the db_cost health row; the operator and the SPA read it to find a regressed statement, a session investigating traffic has no use for it",
        },
    ),
    route(
        "POST",
        "/api/scheduled-jobs/{name}/run",
        Coverage::Exempt {
            reason: "the home page's Run now button; accepted only from the mike SPA origin, so an agent has no use for it",
        },
    ),
    route(
        "GET",
        "/api/file-writes",
        Coverage::Tool("list_file_writes"),
    ),
    route("GET", "/api/requests", Coverage::Tool("list_requests")),
    route("GET", "/api/requests/{id}", Coverage::Tool("get_request")),
    route(
        "GET",
        "/api/requests/{id}/summary",
        Coverage::Tool("get_request_summary"),
    ),
    route(
        "GET",
        "/api/requests/{id}/body",
        Coverage::Exempt {
            reason: "REST discoverability index; MCP already exposes explicit body tools.",
        },
    ),
    route(
        "GET",
        "/api/requests/{id}/body/request",
        Coverage::Tool("get_request_body"),
    ),
    route(
        "GET",
        "/api/tool-results/{tool_use_id}/original",
        Coverage::Tool("get_request_body"),
    ),
    route(
        "GET",
        "/api/requests/{id}/body/request_sent",
        Coverage::Tool("get_request_sent_body"),
    ),
    route(
        "GET",
        "/api/requests/{id}/body/response",
        Coverage::Tool("get_response_body"),
    ),
    route(
        "GET",
        "/api/requests/{id}/manifest",
        Coverage::Tool("get_request_transformation_manifest"),
    ),
    route(
        "GET",
        "/api/requests/{id}/accumulated_text",
        Coverage::Tool("get_accumulated_text"),
    ),
    route(
        "GET",
        "/api/requests/{id}/stream_chunks",
        Coverage::Tool("get_request_stream_chunks"),
    ),
    route(
        "GET",
        "/api/sessions/{session_id}/requests/{request_id}/deltas",
        Coverage::Exempt {
            reason: "REST endpoint backing the SPA turn-delta view; MCP tool removed (low agent usage; the delta is derivable from get_request_body)",
        },
    ),
    route(
        "GET",
        "/api/requests/{id}/body/find",
        Coverage::AggregatedInto("find_text"),
    ),
    route(
        "GET",
        "/api/requests/{id}/tool-results",
        Coverage::Exempt {
            reason: "results-only REST endpoint; MCP tool removed as redundant — get_request_tool_calls returns each call paired with its result",
        },
    ),
    route(
        "GET",
        "/api/requests/{id}/tool-calls",
        Coverage::Tool("get_request_tool_calls"),
    ),
    route(
        "GET",
        "/api/requests/{id}/steering",
        Coverage::Exempt {
            reason: "per-request steering attribution point-lookup; inspect-API read, not surfaced via MCP per the demand-driven policy",
        },
    ),
    route(
        "GET",
        "/api/tool-results/errors",
        Coverage::Tool("list_tool_errors"),
    ),
    route(
        "GET",
        "/api/tool-results/errors/facets",
        Coverage::Exempt {
            reason: "UI-only filter-dropdown helper; returns full distinct lists, not an investigation surface requiring an MCP tool",
        },
    ),
    route(
        "GET",
        "/api/tool-uses/facets",
        Coverage::Exempt {
            reason: "UI-only filter-dropdown helper for the tool-use feed; SPA-driven, no MCP tool needed",
        },
    ),
    route(
        "GET",
        "/api/tool-uses",
        Coverage::Exempt {
            reason: "homepage tools-zone cross-session feed; SPA-driven, not surfaced via MCP V1",
        },
    ),
    route("GET", "/api/stats", Coverage::AggregatedInto("get_stats")),
    route(
        "GET",
        "/api/sync/status",
        Coverage::Exempt {
            reason: "Phase 4 resync status: SPA sync panel surface; demand-driven MCP policy, no tool until an agent workflow needs it",
        },
    ),
    route(
        "GET",
        "/api/metrics",
        Coverage::Exempt {
            reason: "Prometheus scrape surface, not an agent tool; reachable via curl/SPA",
        },
    ),
    route(
        "GET",
        "/api/llm/spend",
        Coverage::Exempt {
            reason: "operator cost-visibility surface; demand-driven MCP policy, no tool until an agent workflow needs it",
        },
    ),
    route(
        "POST",
        "/api/diagnostics/spa_delta_fallback",
        Coverage::Exempt {
            reason: "SPA-only bounded fallback telemetry; not an investigation tool",
        },
    ),
    route(
        "GET",
        "/api/diagnostics/stats",
        Coverage::Tool("diagnostic_stats"),
    ),
    route(
        "GET",
        "/api/diagnostics/findings",
        Coverage::Tool("diagnostic_findings"),
    ),
    route(
        "GET",
        "/api/diagnostics/patterns",
        Coverage::Exempt {
            reason: "operator-facing pattern catalogue; demand-driven MCP policy, no tool until an agent workflow needs it",
        },
    ),
    route(
        "GET",
        "/api/diagnostics/memory",
        Coverage::Tool("diagnostic_memory"),
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/diagnostics/spa_delta_fallback",
            handlers::spa_delta_fallback_route(),
        )
        .route("/api/health", get(handlers::health))
        .route("/api/connection-info", get(handlers::connection_info))
        .route("/api/capabilities", get(handlers::capabilities))
        .route("/api/scheduled-jobs", get(handlers::scheduled_jobs))
        .route("/api/db-cost", get(crate::api::db_cost::get_db_cost))
        .route(
            "/api/scheduled-jobs/{name}/run",
            post(crate::api::scheduled_job_run::post_run_scheduled_job),
        )
        .route("/api/file-writes", get(handlers::list_file_writes))
        .route("/api/requests", get(handlers::list_requests))
        .route("/api/requests/{id}", get(handlers::get_request))
        .route(
            "/api/requests/{id}/summary",
            get(handlers::get_request_summary),
        )
        .route(
            "/api/requests/{id}/body",
            get(handlers::get_request_body_index),
        )
        .route(
            "/api/requests/{id}/body/request",
            get(handlers::get_request_body),
        )
        .route(
            "/api/tool-results/{tool_use_id}/original",
            get(handlers::get_tool_result_original),
        )
        .route(
            "/api/requests/{id}/body/request_sent",
            get(handlers::get_request_sent_body),
        )
        .route(
            "/api/requests/{id}/body/response",
            get(handlers::get_response_body),
        )
        .route(
            "/api/requests/{id}/manifest",
            get(handlers::get_request_transformation_manifest),
        )
        .route(
            "/api/requests/{id}/accumulated_text",
            get(handlers::get_accumulated_text),
        )
        .route(
            "/api/sessions/{session_id}/requests/{request_id}/deltas",
            get(handlers::get_request_deltas),
        )
        .route(
            "/api/requests/{id}/stream_chunks",
            get(handlers::get_stream_chunks),
        )
        .route(
            "/api/requests/{id}/body/find",
            get(handlers::find_text_in_body),
        )
        .route(
            "/api/requests/{id}/tool-results",
            get(handlers::get_request_tool_results),
        )
        .route(
            "/api/requests/{id}/tool-calls",
            get(handlers::get_request_tool_calls),
        )
        .route(
            "/api/requests/{id}/steering",
            get(handlers::get_request_steering),
        )
        .route("/api/tool-results/errors", get(handlers::list_tool_errors))
        .route(
            "/api/tool-results/errors/facets",
            get(handlers::list_tool_error_facets),
        )
        .route("/api/tool-uses/facets", get(handlers::list_tool_use_facets))
        .route("/api/tool-uses", get(handlers::list_tool_uses))
        .route("/api/stats", get(handlers::stats))
        .route("/api/sync/status", get(handlers::get_sync_status))
        .route("/api/metrics", get(handlers::metrics))
        .route("/api/llm/spend", get(handlers::get_llm_spend))
        .route(
            "/api/diagnostics/stats",
            get(handlers::get_diagnostic_stats),
        )
        .route(
            "/api/diagnostics/findings",
            get(handlers::get_diagnostic_findings),
        )
        .route(
            "/api/diagnostics/patterns",
            get(handlers::get_diagnostic_patterns),
        )
        .route(
            "/api/diagnostics/memory",
            get(handlers::get_diagnostic_memory),
        )
}
