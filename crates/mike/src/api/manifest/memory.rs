use axum::Router;
use axum::routing::{delete, get, post};

use crate::api::{handlers, memory_audit};
use crate::config::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route(
        "GET",
        "/api/operator-turns",
        Coverage::Tool("list_operator_turns"),
    ),
    route(
        "GET",
        "/api/operator-turns/by-outcome",
        Coverage::Tool("list_corrected_turns"),
    ),
    route(
        "GET",
        "/api/operator-turns/outcome-summary",
        Coverage::Tool("list_outcome_summary"),
    ),
    route(
        "GET",
        "/api/operator-turns/{id}",
        Coverage::Tool("get_operator_turn"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/operator-turns",
        Coverage::Tool("get_session_operator_turns"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/ask-ledger",
        Coverage::Exempt {
            reason: "Ask-ledger read route (temet-ai/mike-private#113); demand-driven MCP policy — no tool until a named agent workflow needs it.",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/acceptance-criteria",
        Coverage::Exempt {
            reason: "Acceptance-criteria read route (temet-ai/mike-private#152); demand-driven MCP policy — no tool until a named agent workflow needs it.",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/turns/{turn_id}/tool-calls",
        Coverage::Exempt {
            reason: "UI-only turn tool-calls drawer; agent investigation uses the existing get_request_tool_calls tool per request",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/turns/{turn_id}/tool-calls/{tool_id}/content",
        Coverage::Exempt {
            reason: "UI-only lazy per-call content fetch for the turn tool-calls list; agent investigation uses get_request_tool_calls per request",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/overview",
        Coverage::Tool("get_session_overview"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/embedding-graph",
        Coverage::Exempt {
            reason: "graph is a UI surface; no agent workflow needs it yet",
        },
    ),
    route(
        "PATCH",
        "/api/sessions/{id}/narrative-overview",
        Coverage::Exempt {
            reason: "operator-authored override of the narrative overview; UI-driven, no named agent workflow needs write access yet",
        },
    ),
    route(
        "POST",
        "/api/sessions/{id}/narrative-overview/generate",
        Coverage::Exempt {
            reason: "on-demand LLM generation of the narrative overview; UI-driven, no named agent workflow needs it yet",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/workflow-runs",
        Coverage::Exempt {
            reason: "workflow-run grouping is UI-only; agent investigation uses operator-turns + tool-call drill-downs",
        },
    ),
    route(
        "GET",
        "/api/agent-dispatches",
        Coverage::Tool("list_agent_dispatches"),
    ),
    route(
        "GET",
        "/api/recall/project-context",
        Coverage::Tool("recall_project_context"),
    ),
    route(
        "GET",
        "/api/recall/file-history",
        Coverage::Exempt {
            reason: "file-scoped recall REST endpoint; MCP tool removed (zero agent usage; superseded by recall_similar_turns + find_text)",
        },
    ),
    route(
        "POST",
        "/api/recall/similar-turns",
        Coverage::Tool("recall_similar_turns"),
    ),
    route("POST", "/api/lessons", Coverage::Tool("record_lesson")),
    route("GET", "/api/lessons", Coverage::Tool("list_lessons")),
    route(
        "DELETE",
        "/api/lessons/{id}",
        Coverage::Tool("delete_lesson"),
    ),
    route(
        "GET",
        "/api/memory/consolidation/runs",
        Coverage::Exempt {
            reason: "consolidation-audit read surface; no agent workflow needs it yet (temet-ai/mike-private#110)",
        },
    ),
    route(
        "GET",
        "/api/memory/consolidation/runs/{id}",
        Coverage::Exempt {
            reason: "consolidation-audit read surface; no agent workflow needs it yet (temet-ai/mike-private#110)",
        },
    ),
    route(
        "GET",
        "/api/memory/decisions",
        Coverage::Exempt {
            reason: "consolidation-audit read surface; no agent workflow needs it yet (temet-ai/mike-private#110)",
        },
    ),
    route(
        "GET",
        "/api/memory/lesson-outcomes",
        Coverage::Exempt {
            reason: "observational lesson-outcome read surface; no agent workflow needs it yet, and MCP tools are demand-driven not parity-driven (temet-ai/mike-private#110)",
        },
    ),
    route(
        "GET",
        "/api/lessons/{id}/history",
        Coverage::Exempt {
            reason: "consolidation-audit read surface; no agent workflow needs it yet (temet-ai/mike-private#110)",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/memory-actions",
        Coverage::Exempt {
            reason: "consolidation-audit read surface; no agent workflow needs it yet (temet-ai/mike-private#110)",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/lessons",
        Coverage::Exempt {
            reason: "session-page read surface (which lessons were shown/recorded); no agent workflow needs it yet, and MCP tools are demand-driven not parity-driven (temet-ai/mike-private#110)",
        },
    ),
    route(
        "POST",
        "/api/lessons/{id}/restore",
        Coverage::Exempt {
            reason: "operator-only un-archive action; out of scope for MCP per temet-ai/mike-private#110",
        },
    ),
    route(
        "GET",
        "/api/memory/relations",
        Coverage::Exempt {
            reason: "lesson-relations read surface; no agent workflow needs it yet (temet-ai/mike-private#110)",
        },
    ),
    route(
        "POST",
        "/api/memory/relations/{id}/confirm",
        Coverage::Exempt {
            reason: "operator-only relation decision; out of scope for MCP per temet-ai/mike-private#110",
        },
    ),
    route(
        "POST",
        "/api/memory/relations/{id}/reject",
        Coverage::Exempt {
            reason: "operator-only relation decision; out of scope for MCP per temet-ai/mike-private#110",
        },
    ),
    route(
        "GET",
        "/api/lessons/{id}/relations",
        Coverage::Exempt {
            reason: "lesson-relations read surface; no agent workflow needs it yet (temet-ai/mike-private#110)",
        },
    ),
    route(
        "GET",
        "/api/lessons/archived",
        Coverage::Exempt {
            reason: "consolidation-audit read surface; no agent workflow needs it yet (temet-ai/mike-private#110)",
        },
    ),
];

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/operator-turns", get(handlers::list_operator_turns))
        .route(
            "/api/operator-turns/by-outcome",
            get(handlers::list_operator_turns_by_outcome),
        )
        .route(
            "/api/operator-turns/outcome-summary",
            get(handlers::get_operator_turns_outcome_summary),
        )
        .route("/api/operator-turns/{id}", get(handlers::get_operator_turn))
        .route(
            "/api/agent-dispatches",
            get(handlers::list_agent_dispatches),
        )
        .route(
            "/api/recall/project-context",
            get(handlers::recall_project_context),
        )
        .route(
            "/api/recall/file-history",
            get(handlers::recall_file_history),
        )
        .route(
            "/api/recall/similar-turns",
            post(handlers::recall_similar_turns),
        )
        .route(
            "/api/lessons",
            get(handlers::list_lessons).post(handlers::create_lesson),
        )
        .route(
            "/api/lessons/archived",
            get(memory_audit::list_archived_lessons),
        )
        .route("/api/lessons/{id}", delete(handlers::delete_lesson))
        .route(
            "/api/memory/consolidation/runs",
            get(memory_audit::list_consolidation_runs),
        )
        .route(
            "/api/memory/consolidation/runs/{id}",
            get(memory_audit::get_consolidation_run),
        )
        .route(
            "/api/memory/decisions",
            get(memory_audit::list_consolidation_decisions),
        )
        .route(
            "/api/memory/lesson-outcomes",
            get(memory_audit::list_lesson_outcomes),
        )
        .route(
            "/api/lessons/{id}/history",
            get(memory_audit::get_lesson_history),
        )
        .route(
            "/api/sessions/{id}/memory-actions",
            get(memory_audit::list_session_memory_actions),
        )
        .route(
            "/api/sessions/{id}/lessons",
            get(memory_audit::list_session_lessons),
        )
        .route(
            "/api/lessons/{id}/restore",
            post(memory_audit::restore_lesson),
        )
        .route(
            "/api/memory/relations",
            get(memory_audit::list_lesson_relations),
        )
        .route(
            "/api/memory/relations/{id}/confirm",
            post(memory_audit::confirm_lesson_relation),
        )
        .route(
            "/api/memory/relations/{id}/reject",
            post(memory_audit::reject_lesson_relation),
        )
        .route(
            "/api/lessons/{id}/relations",
            get(memory_audit::list_lesson_relations_for_lesson),
        )
}
