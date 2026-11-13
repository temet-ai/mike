use axum::Router;
use axum::routing::{get, post};

use crate::api::handlers;
use crate::state::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

const OPERATOR_PROMPTS: Coverage = Coverage::Exempt {
    reason: "operator-only prompt-class catalogue; SPA-driven, no MCP tool per issue temet-ai/mike-private#133 demand-driven policy",
};
const OPERATOR_PROMPT_DETAIL: Coverage = Coverage::Exempt {
    reason: "operator-only prompt-class detail; SPA-driven, no MCP tool per issue temet-ai/mike-private#133 demand-driven policy",
};
const OPERATOR_AGENTS: Coverage = Coverage::Exempt {
    reason: "operator-only agent registry; SPA-driven, not surfaced via MCP V1",
};
const OPERATOR_VOLATILE_LINES: Coverage = Coverage::Exempt {
    reason: "operator-only volatile-line registry; not surfaced via MCP",
};
const COMPRESSION_MEASUREMENT: Coverage = Coverage::Exempt {
    reason: "compression measurement is REST/SPA-only; no agent workflow uses it (2026-07-16 audit)",
};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/prompts/compositions",
        Coverage::Tool("list_prompt_compositions"),
    ),
    route(
        "POST",
        "/api/prompts/compositions",
        Coverage::Tool("create_prompt_composition"),
    ),
    route(
        "GET",
        "/api/prompts/compositions/{id}",
        Coverage::Tool("get_prompt_composition"),
    ),
    route(
        "GET",
        "/api/prompts/compositions/by-sha256/{prompt_sha256}",
        Coverage::Exempt {
            reason: "merged into the get_prompt_composition tool (sha256 arg); route kept for SPA/curl",
        },
    ),
    route(
        "GET",
        "/api/prompts/compositions/by-sha256/{prompt_sha256}/stats",
        Coverage::Exempt {
            reason: "operator-only usage rollup; SPA-driven, not surfaced via MCP V1",
        },
    ),
    route(
        "PATCH",
        "/api/prompts/compositions/{id}",
        Coverage::Tool("update_prompt_composition"),
    ),
    route(
        "DELETE",
        "/api/prompts/compositions/{id}",
        Coverage::Tool("delete_prompt_composition"),
    ),
    route(
        "GET",
        "/api/prompts/compositions/{id}/revisions",
        Coverage::Exempt {
            reason: "operator-only history view; not surfaced via MCP",
        },
    ),
    route(
        "GET",
        "/api/prompts/anomalies",
        Coverage::Tool("list_prompt_anomalies"),
    ),
    route("GET", "/api/prompts/classes", OPERATOR_PROMPTS),
    route("GET", "/api/prompts/classes/main", OPERATOR_PROMPT_DETAIL),
    route(
        "GET",
        "/api/prompts/classes/subagent/{persona}",
        OPERATOR_PROMPT_DETAIL,
    ),
    route(
        "GET",
        "/api/prompts/classes/main/versions",
        OPERATOR_PROMPT_DETAIL,
    ),
    route(
        "GET",
        "/api/prompts/classes/subagent/{persona}/versions",
        OPERATOR_PROMPT_DETAIL,
    ),
    route(
        "GET",
        "/api/prompts/classes/subagent/{persona}/instructions",
        OPERATOR_PROMPT_DETAIL,
    ),
    route(
        "POST",
        "/api/prompts/suggest-name",
        Coverage::Exempt {
            reason: "operator-only naming aid for the new-composition form; SPA-driven",
        },
    ),
    route("GET", "/api/agents", OPERATOR_AGENTS),
    route("GET", "/api/agents/{id}", OPERATOR_AGENTS),
    route("PATCH", "/api/agents/{id}", OPERATOR_AGENTS),
    route("DELETE", "/api/agents/{id}", OPERATOR_AGENTS),
    route("GET", "/api/volatile-lines", OPERATOR_VOLATILE_LINES),
    route("POST", "/api/volatile-lines", OPERATOR_VOLATILE_LINES),
    route("GET", "/api/volatile-lines/{id}", OPERATOR_VOLATILE_LINES),
    route("PATCH", "/api/volatile-lines/{id}", OPERATOR_VOLATILE_LINES),
    route(
        "DELETE",
        "/api/volatile-lines/{id}",
        OPERATOR_VOLATILE_LINES,
    ),
    route(
        "GET",
        "/api/prompts/compositions/export",
        Coverage::Exempt {
            reason: "operator-only bulk export; backup workflow is run from the SPA, not MCP",
        },
    ),
    route(
        "POST",
        "/api/prompts/compositions/import",
        Coverage::Exempt {
            reason: "operator-only bulk import; restore workflow is run from the SPA, not MCP",
        },
    ),
    route("POST", "/api/compress/estimate", COMPRESSION_MEASUREMENT),
    route("GET", "/api/compress/stats", COMPRESSION_MEASUREMENT),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/prompts/compositions",
            get(handlers::list_prompt_compositions).post(handlers::create_prompt_composition),
        )
        .route(
            "/api/prompts/compositions/export",
            get(handlers::export_prompt_compositions),
        )
        .route(
            "/api/prompts/compositions/import",
            post(handlers::import_prompt_compositions),
        )
        .route(
            "/api/prompts/compositions/{id}",
            get(handlers::get_prompt_composition)
                .patch(handlers::patch_prompt_composition)
                .delete(handlers::delete_prompt_composition),
        )
        .route(
            "/api/prompts/compositions/{id}/revisions",
            get(handlers::list_prompt_composition_revisions),
        )
        .route(
            "/api/prompts/compositions/by-sha256/{prompt_sha256}",
            get(handlers::get_prompt_composition_by_sha256),
        )
        .route(
            "/api/prompts/compositions/by-sha256/{prompt_sha256}/stats",
            get(handlers::get_composition_stats),
        )
        .route(
            "/api/prompts/anomalies",
            get(handlers::list_prompt_anomalies),
        )
        .route("/api/prompts/classes", get(handlers::list_prompt_classes))
        .route(
            "/api/prompts/classes/main",
            get(handlers::get_prompt_class_detail_main),
        )
        .route(
            "/api/prompts/classes/subagent/{persona}",
            get(handlers::get_prompt_class_detail_subagent),
        )
        .route(
            "/api/prompts/classes/main/versions",
            get(handlers::get_prompt_class_versions_main),
        )
        .route(
            "/api/prompts/classes/subagent/{persona}/versions",
            get(handlers::get_prompt_class_versions_subagent),
        )
        .route(
            "/api/prompts/classes/subagent/{persona}/instructions",
            get(handlers::get_prompt_class_instructions_subagent),
        )
        .route(
            "/api/prompts/suggest-name",
            post(handlers::suggest_prompt_name),
        )
        .route("/api/agents", get(handlers::list_agents))
        .route(
            "/api/agents/{id}",
            get(handlers::get_agent)
                .patch(handlers::patch_agent)
                .delete(handlers::delete_agent),
        )
        .route(
            "/api/volatile-lines",
            get(handlers::list_volatile_lines).post(handlers::create_volatile_line),
        )
        .route(
            "/api/volatile-lines/{id}",
            get(handlers::get_volatile_line)
                .patch(handlers::patch_volatile_line)
                .delete(handlers::delete_volatile_line),
        )
        .route("/api/compress/estimate", post(handlers::compress_estimate))
        .route("/api/compress/stats", get(handlers::compress_stats))
}
