use axum::Router;
use axum::routing::{get, patch, post, put};

use crate::api::{
    embedding_graph, fleet, handlers, session_context, session_delivery, session_flow,
    session_verdicts, steering_facts, workspace,
};
use crate::config::AppState;
use crate::web_base::route_manifest::{Coverage, RouteEntry, route};

pub(super) const ROUTES: &[RouteEntry] = &[
    route("GET", "/api/sessions", Coverage::Tool("list_sessions")),
    route("GET", "/api/sessions/{id}", Coverage::Tool("get_session")),
    route(
        "PATCH",
        "/api/sessions/{id}/project",
        Coverage::Exempt {
            reason: "project resolution is an explicit SPA/operator action; automatic agents must not reassign sessions",
        },
    ),
    route(
        "PATCH",
        "/api/sessions/{id}/pin",
        Coverage::Exempt {
            reason: "pin/TTL lifecycle is SPA/operator-only; MCP tools removed after zero usage ever (temet-ai/mike-private#226)",
        },
    ),
    route(
        "PATCH",
        "/api/sessions/{id}/ttl",
        Coverage::Exempt {
            reason: "pin/TTL lifecycle is SPA/operator-only; MCP tools removed after zero usage ever (temet-ai/mike-private#226)",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/tool_uses",
        Coverage::Tool("list_session_tool_uses"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/tool_use_stats",
        Coverage::Tool("session_tool_use_stats"),
    ),
    route("GET", "/api/time_spent", Coverage::Tool("get_time_spent")),
    route(
        "GET",
        "/api/sessions/{id}/dedup-stats",
        Coverage::Exempt {
            reason: "shadow-mode measurement surface; demand-driven MCP exposure per issue temet-ai/mike-private#133 — no known agent workflow needs this yet",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/fleet",
        Coverage::Exempt {
            reason: "session-detail fleet-membership badge + /fleet deep-link; SPA-driven, no MCP tool until an agent workflow needs it",
        },
    ),
    route(
        "POST",
        "/api/sessions/{id}/message",
        Coverage::Tool("send_session_message"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/messages",
        Coverage::Tool("get_session_messages"),
    ),
    route(
        "POST",
        "/api/sessions/{id}/messages/{message_uuid}/ack",
        Coverage::Tool("ack_session_message"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/messages/{message_uuid}",
        Coverage::Tool("verify_session_message"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/messages/{message_uuid}/receipt",
        Coverage::Tool("verify_message_read"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/messages/{message_uuid}/receipt-evidence",
        Coverage::Tool("confirm_message_receipt"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/threads",
        Coverage::Tool("get_session_threads"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/team-comms",
        Coverage::Exempt {
            reason: "derived team-activity feed (AC18); SPA-driven — list_session_tool_uses/get_session_operator_turns already cover the underlying capture data for an investigating session",
        },
    ),
    route(
        "GET",
        "/api/threads",
        Coverage::Exempt {
            reason: "cross-session thread browsing; SPA-driven, no MCP tool until a workflow needs it beyond get_session_threads",
        },
    ),
    route(
        "POST",
        "/api/threads",
        Coverage::Exempt {
            reason: "open-thread creation (AC17); send_session_message(subject=..., to omitted) already covers the MCP open-create path",
        },
    ),
    route(
        "GET",
        "/api/threads/{thread_uuid}",
        Coverage::Tool("read_thread"),
    ),
    route(
        "POST",
        "/api/threads/{thread_uuid}/message",
        Coverage::Exempt {
            reason: "multi-recipient thread reply; send_session_message(thread_id=...) already covers the single-recipient MCP path",
        },
    ),
    route(
        "POST",
        "/api/threads/{thread_uuid}/resolve",
        Coverage::Tool("resolve_thread"),
    ),
    route(
        "GET",
        "/api/tags",
        Coverage::Exempt {
            reason: "tag vocabulary pick-list; SPA-driven, no MCP tool until a workflow needs it",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/requests",
        Coverage::Exempt {
            reason: "Session-scoped request list; MCP get_session already returns the session's requests array.",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/flow",
        Coverage::Exempt {
            reason: "cross-subsystem session flow projection (P12); SPA-driven visualization surface, no MCP tool until an agent workflow needs it",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/flow/export",
        Coverage::Exempt {
            reason: "Perfetto trace export of the session flow projection (P14); operator opens it in the Perfetto UI, no agent workflow consumes a trace file yet",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/notes",
        Coverage::Tool("list_session_notes"),
    ),
    route(
        "POST",
        "/api/sessions/{id}/notes",
        Coverage::Tool("write_session_note"),
    ),
    route(
        "PATCH",
        "/api/sessions/{id}/notes/{note_uuid}",
        Coverage::Tool("write_session_note"),
    ),
    route(
        "DELETE",
        "/api/sessions/{id}/notes/{note_uuid}",
        Coverage::Tool("delete_session_note"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/turns/{turn_id}/steering-facts",
        Coverage::Exempt {
            reason: "debug view of the guard's inputs; the dossier MCP tool (JC04) carries the persisted facts",
        },
    ),
    route(
        "GET",
        "/api/sessions/{id}/verdicts",
        Coverage::Tool("get_session_verdicts"),
    ),
    route(
        "GET",
        "/api/sessions/{id}/delivery",
        Coverage::Exempt {
            reason: "session-page panel joining the effective profile, delivered lessons, recalled topics and loaded skills (temet-ai/mike-private#601); SPA-driven, and the issue asks for no MCP tool — get_request_transformation_manifest and list_session_tool_uses already serve an investigating agent",
        },
    ),
    route(
        "POST",
        "/api/sessions/{id}/context",
        Coverage::Exempt {
            reason: "called by tether at a prompt to take the identity, briefing and change-tail blocks the proxy would add, after which the proxy stands aside for that session (temet-ai/mike-private#442); a session investigating traffic has no use for it",
        },
    ),
];

fn register_session_core_routes(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/api/sessions", get(handlers::list_sessions))
        .route("/api/sessions/{id}", get(handlers::get_session))
        .route(
            "/api/sessions/{id}/project",
            patch(workspace::assign_session_project),
        )
        .route("/api/sessions/{id}/pin", patch(handlers::patch_session_pin))
        .route("/api/sessions/{id}/ttl", patch(handlers::patch_session_ttl))
        .route(
            "/api/sessions/{id}/goal",
            put(handlers::put_session_goal).get(handlers::get_session_goal),
        )
        .route(
            "/api/sessions/{id}/tool_uses",
            get(handlers::list_session_tool_uses),
        )
        .route(
            "/api/sessions/{id}/tool_use_stats",
            get(handlers::session_tool_use_stats),
        )
        .route("/api/time_spent", get(handlers::get_time_spent))
        .route(
            "/api/sessions/{id}/dedup-stats",
            get(handlers::get_session_dedup_stats),
        )
        .route("/api/sessions/{id}/fleet", get(fleet::get_session_fleet))
}

fn register_messaging_and_thread_routes(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/sessions/{id}/message",
            post(handlers::post_session_message),
        )
        .route(
            "/api/sessions/{id}/messages",
            get(handlers::get_session_messages),
        )
        .route(
            "/api/sessions/{id}/messages/{message_uuid}",
            get(handlers::get_session_message),
        )
        .route(
            "/api/sessions/{id}/messages/{message_uuid}/ack",
            post(handlers::ack_session_message),
        )
        .route(
            "/api/sessions/{id}/messages/{message_uuid}/receipt",
            get(handlers::get_session_message_receipt),
        )
        .route(
            "/api/sessions/{id}/messages/{message_uuid}/receipt-evidence",
            get(handlers::confirm_message_receipt),
        )
        .route(
            "/api/sessions/{id}/threads",
            get(handlers::get_session_threads),
        )
        .route(
            "/api/sessions/{id}/team-comms",
            get(handlers::get_session_team_comms),
        )
        .route(
            "/api/threads",
            get(handlers::list_threads).post(handlers::post_thread),
        )
        .route("/api/threads/{thread_uuid}", get(handlers::get_thread))
        .route(
            "/api/threads/{thread_uuid}/message",
            post(handlers::post_thread_message),
        )
        .route(
            "/api/threads/{thread_uuid}/resolve",
            post(handlers::post_thread_resolve),
        )
        .route("/api/tags", get(handlers::get_tags))
}

fn register_session_turn_and_overview_routes(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/sessions/{id}/operator-turns",
            get(handlers::get_session_operator_turns),
        )
        .route(
            "/api/sessions/{id}/ask-ledger",
            get(handlers::get_session_ask_ledger),
        )
        .route(
            "/api/sessions/{id}/acceptance-criteria",
            get(handlers::get_session_acceptance_criteria),
        )
        .route(
            "/api/sessions/{id}/turns/{turn_id}/tool-calls",
            get(handlers::get_turn_tool_calls),
        )
        .route(
            "/api/sessions/{id}/turns/{turn_id}/tool-calls/{tool_id}/content",
            get(handlers::get_turn_tool_call_content),
        )
        .route(
            "/api/sessions/{id}/turns/{turn_id}/steering-facts",
            get(steering_facts::get_turn_steering_facts),
        )
        .route(
            "/api/sessions/{id}/verdicts",
            get(session_verdicts::get_session_verdicts),
        )
        .route(
            "/api/sessions/{id}/overview",
            get(handlers::get_session_overview),
        )
        .route(
            "/api/sessions/{id}/embedding-graph",
            get(embedding_graph::get_session_embedding_graph),
        )
        .route(
            "/api/sessions/{id}/narrative-overview",
            patch(handlers::patch_session_narrative_overview),
        )
        .route(
            "/api/sessions/{id}/narrative-overview/generate",
            post(handlers::generate_session_narrative_overview),
        )
        .route(
            "/api/sessions/{id}/workflow-runs",
            get(handlers::get_session_workflow_runs),
        )
        .route(
            "/api/sessions/{id}/flow",
            get(session_flow::get_session_flow),
        )
        .route(
            "/api/sessions/{id}/flow/export",
            get(session_flow::get_session_flow_export),
        )
        .route(
            "/api/sessions/{id}/delivery",
            get(session_delivery::get_session_delivery),
        )
        .route(
            "/api/sessions/{id}/context",
            post(session_context::post_session_context),
        )
}

fn register_session_notes_routes(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/api/sessions/{id}/notes",
            get(handlers::get_session_notes).post(handlers::post_session_note),
        )
        .route(
            "/api/sessions/{id}/notes/{note_uuid}",
            patch(handlers::patch_session_note).delete(handlers::delete_session_note),
        )
}

pub(super) fn register(router: Router<AppState>) -> Router<AppState> {
    let router = register_session_core_routes(router);
    let router = register_messaging_and_thread_routes(router);
    let router = register_session_turn_and_overview_routes(router);
    register_session_notes_routes(router)
}
