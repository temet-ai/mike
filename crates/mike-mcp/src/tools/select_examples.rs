use std::sync::Arc;

use rmcp::model::Tool;
use serde_json::{Map, Value};

use super::schema::select_description;

const EXAMPLES: &[(&str, &str)] = &[
    ("define_project", "/knowledge/project_uuid"),
    ("register_project_location", "/knowledge/location_uuid"),
    (
        "list_project_locations",
        "/knowledge/projects/*/project_path",
    ),
    ("search_project_knowledge", "/knowledge/items/*/reference"),
    ("read_project_knowledge", "/knowledge/reference"),
    ("list_requests", "/requests/*/id"),
    ("get_request", "/request/id"),
    ("get_request_summary", "/summary/id"),
    ("get_request_body", "/body/byte_size"),
    ("get_request_sent_body", "/body/byte_size"),
    ("get_response_body", "/body/byte_size"),
    (
        "get_request_transformation_manifest",
        "/manifest/has_manifest",
    ),
    ("get_accumulated_text", "/accumulated_text/text"),
    (
        "get_request_stream_chunks",
        "/stream_chunks/stream_blocks/*/block_type",
    ),
    ("get_request_tool_calls", "/tool_calls/calls/*/name"),
    ("find_text", "/find/matches/*/request_id"),
    (
        "search_session_content",
        "/content_search/sessions/*/session_id",
    ),
    ("list_tool_errors", "/tool_errors/rows/*/tool_name"),
    ("list_sessions", "/sessions/*/id"),
    ("get_session", "/session/requests/*/id"),
    ("get_session_head", "/head/session_id"),
    ("get_session_threads", "/threads/threads/*/thread_uuid"),
    ("list_projects", "/projects/*/project_path"),
    ("list_session_tool_uses", "/tool_uses/rows/*/tool_name"),
    ("confirm_message_receipt", "/receipt/confidence"),
    ("list_file_writes", "/file_writes/files/*/file_path"),
    ("session_tool_use_stats", "/stats/by_tool/*/tool_name"),
    ("get_time_spent", "/time_spent/totals/wait_seconds"),
    ("set_session_goal", "/session/ok"),
    ("write_session_note", "/note/note_uuid"),
    ("list_session_notes", "/notes/*/note_uuid"),
    ("delete_session_note", "/result/deleted"),
    ("diagnostic_stats", "/stats/last_24h"),
    ("diagnostic_findings", "/findings/*/request_id"),
    ("diagnostic_memory", "/memory/entry_count"),
    ("get_stats", "/stats/runtime/request_count"),
    ("get_verifier_rate", "/verifier_rate/rows/*/project_path"),
    ("list_filter_rules", "/rules/*/id"),
    ("create_filter_rule", "/rule/rule/id"),
    ("update_filter_rule", "/rule/rule/id"),
    ("delete_filter_rule", "/result/deleted"),
    ("dry_run_filter_rules", "/dry_run/changed"),
    ("list_prompt_compositions", "/compositions/*/id"),
    ("get_prompt_composition", "/composition/id"),
    ("create_prompt_composition", "/composition/composition/id"),
    ("update_prompt_composition", "/composition/composition/id"),
    ("delete_prompt_composition", "/result/deleted"),
    ("list_prompt_anomalies", "/anomalies/*/signature_hash"),
    ("list_operator_turns", "/operator_turns/*/turn_id"),
    ("get_operator_turn", "/operator_turn/turn_id"),
    ("get_session_operator_turns", "/operator_turns/*/turn_id"),
    ("get_session_overview", "/session_overview/turns/*/turn_id"),
    ("list_corrected_turns", "/corrected_turns/*/turn_id"),
    ("list_outcome_summary", "/outcome_summary/counts/*/outcome"),
    ("list_agent_dispatches", "/agent_dispatches/*/subagent_type"),
    ("recall_project_context", "/project_context/project_path"),
    ("recall_similar_turns", "/similar_turns/turns/*/turn_id"),
    ("record_lesson", "/lesson/id"),
    ("list_lessons", "/lessons/*/id"),
    ("delete_lesson", "/result/deleted"),
    ("get_workspace_roster", "/roster/roster/*/session_id"),
    ("workspace_heartbeat", "/heartbeat/lease_expires_at"),
    ("register_workspace_session", "/registration/session_id"),
    ("end_workspace_registration", "/registration/ended"),
    ("get_fleet_graph", "/graph/nodes/*/session_id"),
    ("spawn_session", "/launch/session"),
    ("list_launches", "/launches/*/session_uuid"),
    ("kill_session", "/result/deleted"),
    ("define_fleet", "/fleet/state"),
    ("get_fleet", "/fleet/state"),
    ("list_fleet_changes", "/changes/changes"),
    ("list_fleet_results", "/results/results"),
    ("fleet_commit", "/commit/sha"),
    ("fleet_check", "/check/passed"),
    ("fleet_where", "/where/definitions"),
    ("fleet_propose", "/proposal/state"),
    ("fleet_split", "/split/new_task"),
    ("fleet_report", "/report/note"),
    ("get_fleet_record", "/record"),
    ("fleet_verdict", "/verdict/note"),
    ("fleet_decide", "/decision/problem_id"),
    ("what_changed", "/changes/uncommitted"),
    ("send_session_message", "/message/message_uuid"),
    ("get_session_messages", "/mailbox/inbox/*/message_uuid"),
    ("ack_session_message", "/ack/consumed_at"),
    ("verify_session_message", "/message/status"),
    ("verify_message_read", "/receipt/confidence"),
    ("list_config_changes", "/change_log/changes/*/subject_key"),
    ("get_config_snapshot", "/snapshot/entries/*/subject_key"),
    ("list_eval_bundles", "/bundles/bundles/*/id"),
    ("get_session_dossier", "/dossier/session_uuid"),
    ("get_turn_facts", "/turn_facts/turn_id"),
    ("relay_label", "/relay/label"),
    ("list_eval_labels", "/labels/labels/*/id"),
    ("get_turn_verdicts", "/turn_verdicts/verdicts"),
    ("list_turn_verdicts", "/turn_verdicts/items/*/id"),
    ("list_steerer_evaluations", "/evaluations/evaluations/*/id"),
    ("get_eval_rates", "/eval_rates/rows/*/metric"),
    ("list_eval_alarms", "/eval_alarms/alarms/*/kind"),
    ("list_eval_runs", "/eval/items/*/id"),
    ("get_eval_run", "/eval/status"),
    ("list_eval_run_items", "/eval/items/*/operator_turn_id"),
    ("list_eval_gates", "/eval/items/*/id"),
    ("get_eval_gate", "/eval/verdict"),
    ("get_semantic_status", "/get_semantic_status/backlog_turns"),
    ("get_semantic_neighbours", "/get_semantic_neighbours/total"),
    ("get_semantic_item", "/get_semantic_item/kind"),
    (
        "list_pattern_assignments",
        "/list_pattern_assignments/assignments/*/pattern",
    ),
    ("list_pattern_candidates", "/list_pattern_candidates/total"),
    ("get_embedding_drift", "/get_embedding_drift/days/*/day"),
    ("get_session_verdicts", "/session_verdicts/verdicts"),
    ("list_eval_session_end_items", "/session_verdicts/total"),
    (
        "get_steer_questions",
        "/steer_questions/questions/*/question",
    ),
    (
        "list_steer_question_events",
        "/steer_questions/events/*/scored_turn_id",
    ),
    ("ask", "/question/question_id"),
    ("answer", "/answer/question_id"),
    ("decline", "/decline/question_id"),
    ("read_thread", "/thread/messages/*/message_uuid"),
    ("resolve_thread", "/resolved/resolved_at"),
    ("work_thread_list", "/threads/threads/*/slug"),
    ("work_thread_get", "/thread/thread/slug"),
    ("work_thread_checkpoint", "/checkpoint/created"),
    ("work_thread_add_entry", "/entry/entry/ord"),
    (
        "get_cross_session_calibration",
        "/cross_session/snapshot/as_of",
    ),
    ("list_directive_proposals", "/cross_session/proposals/*/id"),
    ("get_improvement_effects", "/improvement/actions/*/kind"),
];

pub(super) fn select_example(tool: &str) -> Option<&'static str> {
    EXAMPLES
        .iter()
        .find(|(name, _)| *name == tool)
        .map(|(_, pointer)| *pointer)
}

pub(super) fn with_select_example(mut tool: Tool) -> Tool {
    let Some(example) = select_example(tool.name.as_ref()) else {
        return tool;
    };
    let mut schema = Map::clone(&tool.input_schema);
    let select = schema
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .and_then(|properties| properties.get_mut("select"))
        .and_then(Value::as_object_mut);
    if let Some(select) = select {
        select.insert(
            "description".to_string(),
            Value::String(select_description(Some(example))),
        );
    }
    tool.input_schema = Arc::new(schema);
    tool
}

#[cfg(test)]
#[path = "select_examples_tests.rs"]
mod select_examples_tests;
