use std::collections::HashSet;

use rmcp::model::Tool;

use crate::tools::{CORE_TOOL_NAMES, all_tools, mail};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Profile {
    #[default]
    Core,
    Full,
    Worker,
    Coordinator,
    Acceptance,
    Observer,
    Retrospective,
    Reference,
    ProblemSolver,
}

impl Profile {
    pub const NAMES: &'static str =
        "core|full|worker|coordinator|acceptance|observer|retrospective|reference|problem_solver";

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "core" | "lean" | "default" => Some(Self::Core),
            "full" | "all" => Some(Self::Full),
            "worker" => Some(Self::Worker),
            "coordinator" => Some(Self::Coordinator),
            "acceptance" => Some(Self::Acceptance),
            "observer" => Some(Self::Observer),
            "retrospective" => Some(Self::Retrospective),
            "reference" => Some(Self::Reference),
            "problem_solver" | "problem-solver" => Some(Self::ProblemSolver),
            _ => None,
        }
    }
}

pub const RETROSPECTIVE_TOOLS: &[&str] = &[
    "get_fleet",
    "list_session_notes",
    "get_session_overview",
    "list_fleet_results",
    crate::tools::TOOL_GET_REQUEST_SUMMARY,
    "get_request_body",
    "write_session_note",
    "end_workspace_registration",
];

pub const PROBLEM_SOLVER_TOOLS: &[&str] = &[
    "fleet_decide",
    "get_fleet",
    "list_fleet_results",
    "list_fleet_changes",
    "list_session_notes",
    "read_project_knowledge",
    "search_project_knowledge",
    "write_session_note",
    "send_session_message",
    "get_session_messages",
    "list_session_tool_uses",
];

pub const FULL_ONLY_TOOLS: &[&str] = &[
    "confirm_message_receipt",
    "register_workspace_session",
    "verify_message_read",
    "workspace_heartbeat",
];

pub const FLEET_SEAT_TOOLS: &[&str] = &[
    "get_fleet",
    "list_fleet_changes",
    "list_fleet_results",
    "fleet_commit",
    "fleet_check",
    "fleet_where",
    "what_changed",
    "fleet_propose",
    "fleet_split",
    "fleet_report",
];

pub const SEAT_MAIL_AND_NOTES: &[&str] = &[
    "send_session_message",
    "get_session_messages",
    "verify_session_message",
    "write_session_note",
    "list_session_notes",
    "get_workspace_roster",
    "set_session_goal",
    "ask",
    "answer",
    "decline",
    "read_thread",
    "resolve_thread",
];

pub const PEER_RUN_READING: &[&str] = &[
    "find_text",
    "get_request_tool_calls",
    "list_session_tool_uses",
    "list_tool_errors",
];

pub const PROJECT_MEMORY: &[&str] = &[
    "record_lesson",
    "recall_project_context",
    "search_project_knowledge",
    "read_project_knowledge",
];

pub const COORDINATION: &[&str] = &[
    "define_fleet",
    "spawn_session",
    "kill_session",
    "list_launches",
    "get_fleet_graph",
    "get_session_threads",
];

pub const SESSION_READING: &[&str] = &[
    "list_sessions",
    "get_session",
    "get_session_head",
    "get_session_overview",
    "get_session_operator_turns",
    "list_operator_turns",
    "get_operator_turn",
    "list_projects",
];

pub const REQUEST_READING: &[&str] = &[
    "list_requests",
    "get_request",
    "get_request_summary",
    "get_request_body",
    "get_request_sent_body",
    "get_request_transformation_manifest",
    "get_response_body",
    "get_accumulated_text",
    "get_request_stream_chunks",
];

pub const INVESTIGATION: &[&str] = &[
    "search_session_content",
    "list_file_writes",
    "list_agent_dispatches",
    "list_corrected_turns",
    "list_outcome_summary",
    "get_verifier_rate",
    "get_stats",
    "diagnostic_stats",
    "diagnostic_findings",
    "recall_similar_turns",
    "list_lessons",
    "get_improvement_effects",
    "list_filter_rules",
    "list_prompt_compositions",
    "get_prompt_composition",
];

pub const ONLY_IN_FULL: &[&str] = &[
    "define_project",
    "register_project_location",
    "session_tool_use_stats",
    "delete_session_note",
    "work_thread_add_entry",
    "diagnostic_memory",
    "create_prompt_composition",
    "update_prompt_composition",
    "delete_prompt_composition",
    "list_prompt_anomalies",
    "list_config_changes",
    "get_config_snapshot",
    "list_eval_bundles",
    "get_session_dossier",
    "get_turn_facts",
    "list_eval_labels",
    "get_turn_verdicts",
    "list_turn_verdicts",
    "list_steerer_evaluations",
    "get_eval_rates",
    "list_eval_alarms",
    "list_eval_runs",
    "get_eval_run",
    "list_eval_run_items",
    "list_eval_gates",
    "get_eval_gate",
    "get_semantic_status",
    "get_semantic_neighbours",
    "get_semantic_item",
    "list_pattern_assignments",
    "list_pattern_candidates",
    "get_embedding_drift",
    "get_session_verdicts",
    "list_eval_session_end_items",
    "get_steer_questions",
    "list_steer_question_events",
    "get_cross_session_calibration",
    "list_directive_proposals",
    "get_fleet_record",
    "list_supplied_items",
    "wright_checks",
];

fn concat(parts: &[&[&'static str]]) -> Vec<&'static str> {
    let mut seen = HashSet::new();
    parts
        .iter()
        .flat_map(|p| p.iter().copied())
        .filter(|n| seen.insert(*n))
        .collect()
}

fn seat_base() -> Vec<&'static str> {
    concat(&[
        FLEET_SEAT_TOOLS,
        SEAT_MAIL_AND_NOTES,
        PEER_RUN_READING,
        PROJECT_MEMORY,
    ])
}

pub fn served_names(profile: Profile) -> Vec<&'static str> {
    let base = seat_base();
    match profile {
        Profile::Full => Vec::new(),
        Profile::Core => CORE_TOOL_NAMES
            .iter()
            .chain(mail::MAIL_TOOL_NAMES.iter())
            .copied()
            .filter(|n| !FULL_ONLY_TOOLS.contains(n))
            .collect(),
        Profile::Worker => base,
        Profile::Coordinator => concat(&[&base, COORDINATION, SESSION_READING, &["get_stats"]]),
        Profile::Acceptance => {
            concat(&[&base, SESSION_READING, REQUEST_READING, &["fleet_verdict"]])
        }
        Profile::Observer => concat(&[
            &base,
            COORDINATION,
            SESSION_READING,
            REQUEST_READING,
            INVESTIGATION,
        ]),
        Profile::Retrospective => RETROSPECTIVE_TOOLS.to_vec(),
        Profile::ProblemSolver => PROBLEM_SOLVER_TOOLS.to_vec(),
        Profile::Reference => concat(&[
            &base,
            SESSION_READING,
            REQUEST_READING,
            &["search_session_content", "recall_similar_turns"],
        ]),
    }
}

pub fn tools_for(profile: Profile) -> Vec<Tool> {
    if profile == Profile::Full {
        return all_tools();
    }
    let served: HashSet<&str> = served_names(profile).into_iter().collect();
    all_tools()
        .into_iter()
        .filter(|t| served.contains(t.name.as_ref()))
        .collect()
}

pub fn placed_names() -> HashSet<&'static str> {
    [
        Profile::Core,
        Profile::Worker,
        Profile::Coordinator,
        Profile::Acceptance,
        Profile::Observer,
        Profile::Retrospective,
        Profile::Reference,
        Profile::ProblemSolver,
    ]
    .into_iter()
    .flat_map(served_names)
    .chain(FULL_ONLY_TOOLS.iter().copied())
    .chain(ONLY_IN_FULL.iter().copied())
    .collect()
}

#[cfg(test)]
#[path = "profiles_tests.rs"]
mod tests;
