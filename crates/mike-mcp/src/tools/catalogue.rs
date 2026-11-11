use rmcp::model::Tool;

pub use crate::profiles::Profile;

use super::{
    config_history, cross_session, diagnostics, dossier, eval_bundles, eval_labels, eval_metrics,
    eval_runs, filters, fleets, improvement, launches, mail, memory, prompts, requests, semantic,
    session_activity, session_notes, session_verdicts, sessions, steer_questions,
    steering_evaluations, text_search, time_spent, turn_verdicts, turns, turns_summary, workspace,
};

pub const CORE_TOOL_NAMES: &[&str] = &[
    "define_fleet",
    "get_fleet",
    "what_changed",
    "search_project_knowledge",
    "read_project_knowledge",
    "list_project_locations",
    super::TOOL_LIST_REQUESTS,
    super::TOOL_GET_REQUEST,
    super::TOOL_GET_REQUEST_SUMMARY,
    super::TOOL_GET_REQUEST_BODY,
    super::TOOL_GET_REQUEST_SENT_BODY,
    super::TOOL_GET_REQUEST_TRANSFORMATION_MANIFEST,
    super::TOOL_GET_RESPONSE_BODY,
    super::TOOL_GET_ACCUMULATED_TEXT,
    super::TOOL_GET_REQUEST_STREAM_CHUNKS,
    super::TOOL_LIST_SESSIONS,
    super::TOOL_GET_SESSION,
    super::TOOL_GET_SESSION_HEAD,
    super::TOOL_GET_SESSION_THREADS,
    super::TOOL_CONFIRM_MESSAGE_RECEIPT,
    super::TOOL_LIST_FILE_WRITES,
    super::TOOL_LIST_SESSION_TOOL_USES,
    super::TOOL_GET_TIME_SPENT,
    super::TOOL_FIND_TEXT,
    super::TOOL_SEARCH_SESSION_CONTENT,
    super::TOOL_LIST_PROJECTS,
    super::TOOL_GET_SESSION_OVERVIEW,
    super::TOOL_GET_SESSION_OPERATOR_TURNS,
    super::TOOL_LIST_OPERATOR_TURNS,
    super::TOOL_GET_OPERATOR_TURN,
    super::TOOL_GET_REQUEST_TOOL_CALLS,
    super::TOOL_LIST_TOOL_ERRORS,
    super::TOOL_LIST_AGENT_DISPATCHES,
    super::TOOL_LIST_CORRECTED_TURNS,
    super::TOOL_LIST_OUTCOME_SUMMARY,
    super::TOOL_GET_VERIFIER_RATE,
    super::TOOL_GET_STATS,
    super::TOOL_DIAGNOSTIC_STATS,
    super::TOOL_DIAGNOSTIC_FINDINGS,
    super::TOOL_RECALL_PROJECT_CONTEXT,
    super::TOOL_RECALL_SIMILAR_TURNS,
    super::TOOL_RECORD_LESSON,
    super::TOOL_LIST_LESSONS,
    super::TOOL_DELETE_LESSON,
    super::TOOL_LIST_FILTER_RULES,
    super::TOOL_CREATE_FILTER_RULE,
    super::TOOL_UPDATE_FILTER_RULE,
    super::TOOL_DELETE_FILTER_RULE,
    super::TOOL_DRY_RUN_FILTER_RULES,
    super::TOOL_LIST_PROMPT_COMPOSITIONS,
    super::TOOL_GET_PROMPT_COMPOSITION,
    super::TOOL_SET_SESSION_GOAL,
    super::TOOL_WRITE_SESSION_NOTE,
    super::TOOL_LIST_SESSION_NOTES,
    super::TOOL_LIST_THREADS,
    super::TOOL_GET_THREAD,
    super::TOOL_CHECKPOINT_THREAD,
    super::TOOL_GET_WORKSPACE_ROSTER,
    super::TOOL_WORKSPACE_HEARTBEAT,
    super::TOOL_REGISTER_WORKSPACE_SESSION,
    super::TOOL_END_WORKSPACE_REGISTRATION,
    super::TOOL_GET_FLEET_GRAPH,
    super::TOOL_SPAWN_SESSION,
    super::TOOL_LIST_LAUNCHES,
    super::TOOL_KILL_SESSION,
    super::TOOL_RELAY_LABEL,
    super::TOOL_GET_IMPROVEMENT_EFFECTS,
];

pub fn tools_for_profile(profile: Profile) -> Vec<Tool> {
    crate::profiles::tools_for(profile)
}

pub fn all_tools() -> Vec<Tool> {
    let mut tools = requests::requests_tools();
    tools.extend(super::knowledge::tools());
    tools.extend(text_search::text_search_tools());
    tools.extend(sessions::sessions_tools());
    tools.extend(session_activity::session_activity_tools());
    tools.extend(time_spent::time_spent_tools());
    tools.extend(session_notes::session_notes_tools());
    tools.extend(diagnostics::diagnostics_tools());
    tools.extend(filters::filters_tools());
    tools.extend(prompts::prompts_tools());
    tools.extend(turns::turns_tools());
    tools.extend(turns_summary::turns_summary_tools());
    tools.extend(memory::memory_tools());
    tools.extend(workspace::workspace_tools());
    tools.extend(launches::launch_tools());
    tools.extend(fleets::fleet_tools());
    tools.extend(super::fleet_tree::fleet_tree_tools());
    tools.extend(super::fleet_check::fleet_check_tools());
    tools.extend(super::fleet_record::fleet_record_tools());
    tools.extend(super::fleet_propose::fleet_propose_tools());
    tools.extend(super::fleet_report::fleet_report_tools());
    tools.extend(super::fleet_decide::fleet_decide_tools());
    tools.extend(super::changes::changes_tools());
    tools.extend(mail::mail_tools());
    tools.extend(config_history::config_history_tools());
    tools.extend(eval_bundles::eval_bundle_tools());
    tools.extend(dossier::dossier_tools());
    tools.extend(eval_labels::eval_label_tools());
    tools.extend(turn_verdicts::turn_verdict_tools());
    tools.extend(steering_evaluations::steering_evaluation_tools());
    tools.extend(eval_metrics::eval_metrics_tools());
    tools.extend(eval_runs::eval_run_tools());
    tools.extend(semantic::semantic_tools());
    tools.extend(session_verdicts::session_verdict_tools());
    tools.extend(steer_questions::steer_question_tools());
    tools.extend(super::questions::question_tools());
    tools.extend(super::threads::thread_tools());
    tools.extend(super::work_threads::work_threads_tools());
    tools.extend(cross_session::cross_session_tools());
    tools.extend(improvement::improvement_tools());
    tools.extend(super::supplied::supplied_tools());
    tools
        .into_iter()
        .map(super::select_examples::with_select_example)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const EXPECTED_ALL_TOOLS: &[&str] = &[
        "define_project",
        "register_project_location",
        "list_project_locations",
        "search_project_knowledge",
        "read_project_knowledge",
        super::super::TOOL_LIST_REQUESTS,
        super::super::TOOL_GET_REQUEST,
        super::super::TOOL_GET_REQUEST_SUMMARY,
        super::super::TOOL_GET_REQUEST_BODY,
        super::super::TOOL_GET_REQUEST_SENT_BODY,
        super::super::TOOL_GET_REQUEST_TRANSFORMATION_MANIFEST,
        super::super::TOOL_GET_RESPONSE_BODY,
        super::super::TOOL_GET_ACCUMULATED_TEXT,
        super::super::TOOL_GET_REQUEST_STREAM_CHUNKS,
        super::super::TOOL_GET_REQUEST_TOOL_CALLS,
        super::super::TOOL_FIND_TEXT,
        super::super::TOOL_SEARCH_SESSION_CONTENT,
        super::super::TOOL_LIST_TOOL_ERRORS,
        super::super::TOOL_LIST_SESSIONS,
        super::super::TOOL_GET_SESSION,
        super::super::TOOL_GET_SESSION_HEAD,
        super::super::TOOL_GET_SESSION_THREADS,
        super::super::TOOL_LIST_PROJECTS,
        super::super::TOOL_LIST_SESSION_TOOL_USES,
        super::super::TOOL_CONFIRM_MESSAGE_RECEIPT,
        super::super::TOOL_LIST_FILE_WRITES,
        super::super::TOOL_SESSION_TOOL_USE_STATS,
        super::super::TOOL_GET_TIME_SPENT,
        super::super::TOOL_SET_SESSION_GOAL,
        super::super::TOOL_WRITE_SESSION_NOTE,
        super::super::TOOL_LIST_SESSION_NOTES,
        super::super::TOOL_DELETE_SESSION_NOTE,
        super::super::TOOL_DIAGNOSTIC_STATS,
        super::super::TOOL_DIAGNOSTIC_FINDINGS,
        super::super::TOOL_DIAGNOSTIC_MEMORY,
        super::super::TOOL_GET_STATS,
        super::super::TOOL_GET_VERIFIER_RATE,
        super::super::TOOL_LIST_FILTER_RULES,
        super::super::TOOL_CREATE_FILTER_RULE,
        super::super::TOOL_UPDATE_FILTER_RULE,
        super::super::TOOL_DELETE_FILTER_RULE,
        super::super::TOOL_DRY_RUN_FILTER_RULES,
        super::super::TOOL_LIST_PROMPT_COMPOSITIONS,
        super::super::TOOL_GET_PROMPT_COMPOSITION,
        super::super::TOOL_CREATE_PROMPT_COMPOSITION,
        super::super::TOOL_UPDATE_PROMPT_COMPOSITION,
        super::super::TOOL_DELETE_PROMPT_COMPOSITION,
        super::super::TOOL_LIST_PROMPT_ANOMALIES,
        super::super::TOOL_LIST_OPERATOR_TURNS,
        super::super::TOOL_GET_OPERATOR_TURN,
        super::super::TOOL_GET_SESSION_OPERATOR_TURNS,
        super::super::TOOL_GET_SESSION_OVERVIEW,
        super::super::TOOL_LIST_CORRECTED_TURNS,
        super::super::TOOL_LIST_OUTCOME_SUMMARY,
        super::super::TOOL_LIST_AGENT_DISPATCHES,
        super::super::TOOL_RECALL_PROJECT_CONTEXT,
        super::super::TOOL_RECALL_SIMILAR_TURNS,
        super::super::TOOL_RECORD_LESSON,
        super::super::TOOL_LIST_LESSONS,
        super::super::TOOL_DELETE_LESSON,
        super::super::TOOL_GET_WORKSPACE_ROSTER,
        super::super::TOOL_WORKSPACE_HEARTBEAT,
        super::super::TOOL_REGISTER_WORKSPACE_SESSION,
        super::super::TOOL_END_WORKSPACE_REGISTRATION,
        super::super::TOOL_GET_FLEET_GRAPH,
        super::super::TOOL_SPAWN_SESSION,
        super::super::TOOL_LIST_LAUNCHES,
        super::super::TOOL_KILL_SESSION,
        super::super::TOOL_DEFINE_FLEET,
        super::super::TOOL_GET_FLEET,
        "list_fleet_changes",
        "list_fleet_results",
        "fleet_commit",
        "fleet_check",
        "fleet_where",
        "fleet_propose",
        "fleet_split",
        "fleet_report",
        "get_fleet_record",
        "fleet_verdict",
        "fleet_decide",
        "what_changed",
        super::super::TOOL_SEND_SESSION_MESSAGE,
        super::super::TOOL_GET_SESSION_MESSAGES,
        super::super::TOOL_ACK_SESSION_MESSAGE,
        super::super::TOOL_VERIFY_SESSION_MESSAGE,
        super::super::TOOL_VERIFY_MESSAGE_READ,
        super::super::TOOL_LIST_CONFIG_CHANGES,
        super::super::TOOL_GET_CONFIG_SNAPSHOT,
        super::super::TOOL_LIST_EVAL_BUNDLES,
        super::super::TOOL_GET_SESSION_DOSSIER,
        super::super::TOOL_GET_TURN_FACTS,
        super::super::TOOL_RELAY_LABEL,
        super::super::TOOL_LIST_EVAL_LABELS,
        super::super::TOOL_GET_TURN_VERDICTS,
        super::super::TOOL_LIST_TURN_VERDICTS,
        super::super::TOOL_LIST_STEERER_EVALUATIONS,
        super::super::TOOL_GET_EVAL_RATES,
        super::super::TOOL_LIST_EVAL_ALARMS,
        super::super::TOOL_LIST_EVAL_RUNS,
        super::super::TOOL_GET_EVAL_RUN,
        super::super::TOOL_LIST_EVAL_RUN_ITEMS,
        super::super::TOOL_LIST_EVAL_GATES,
        super::super::TOOL_GET_EVAL_GATE,
        super::super::TOOL_GET_SEMANTIC_STATUS,
        super::super::TOOL_GET_SEMANTIC_NEIGHBOURS,
        super::super::TOOL_GET_SEMANTIC_ITEM,
        super::super::TOOL_LIST_PATTERN_ASSIGNMENTS,
        super::super::TOOL_LIST_PATTERN_CANDIDATES,
        super::super::TOOL_GET_EMBEDDING_DRIFT,
        super::super::TOOL_GET_SESSION_VERDICTS,
        super::super::TOOL_LIST_EVAL_SESSION_END_ITEMS,
        super::super::TOOL_GET_STEER_QUESTIONS,
        super::super::TOOL_LIST_STEER_QUESTION_EVENTS,
        super::super::TOOL_ASK,
        super::super::TOOL_ANSWER,
        super::super::TOOL_DECLINE,
        super::super::TOOL_READ_THREAD,
        super::super::TOOL_RESOLVE_THREAD,
        super::super::TOOL_LIST_THREADS,
        super::super::TOOL_GET_THREAD,
        super::super::TOOL_CHECKPOINT_THREAD,
        super::super::TOOL_ADD_THREAD_ENTRY,
        super::super::TOOL_GET_CROSS_SESSION_CALIBRATION,
        super::super::TOOL_LIST_DIRECTIVE_PROPOSALS,
        super::super::TOOL_GET_IMPROVEMENT_EFFECTS,
    ];

    #[test]
    fn all_tools_returns_full_set() {
        let tools = all_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        let expected = EXPECTED_ALL_TOOLS;
        for tool in expected {
            assert!(names.contains(tool), "all_tools() is missing {tool}");
        }
        assert_eq!(
            tools.len(),
            expected.len(),
            "all_tools() exposes {} tools but the expected inventory lists {} — \
             add the new tool to this list (there is no count to bump). Got: {names:?}",
            tools.len(),
            expected.len(),
        );
    }

    #[test]
    fn all_tools_have_unique_names() {
        let tools = all_tools();
        let mut seen = HashSet::new();
        for t in &tools {
            assert!(
                seen.insert(t.name.as_ref()),
                "duplicate tool name: {}",
                t.name
            );
        }
    }

    #[test]
    fn core_tool_names_are_all_real_tools() {
        let all_names: HashSet<String> = all_tools()
            .iter()
            .map(|t| t.name.as_ref().to_string())
            .collect();
        for name in CORE_TOOL_NAMES {
            assert!(
                all_names.contains(*name),
                "CORE_TOOL_NAMES lists {name:?} which is not in all_tools()"
            );
        }
    }

    #[test]
    fn core_tool_names_have_no_duplicates() {
        let unique: HashSet<&&str> = CORE_TOOL_NAMES.iter().collect();
        assert_eq!(
            unique.len(),
            CORE_TOOL_NAMES.len(),
            "CORE_TOOL_NAMES contains a duplicate"
        );
    }

    #[test]
    fn core_profile_is_strict_subset_of_full() {
        let full: HashSet<String> = tools_for_profile(Profile::Full)
            .iter()
            .map(|t| t.name.as_ref().to_string())
            .collect();
        let core = tools_for_profile(Profile::Core);
        assert!(core.len() < full.len(), "core must be smaller than full");
        for t in &core {
            assert!(
                full.contains(t.name.as_ref()),
                "core tool {} missing from full",
                t.name
            );
        }
    }

    #[test]
    fn full_profile_matches_all_tools() {
        assert_eq!(tools_for_profile(Profile::Full).len(), all_tools().len());
    }

    #[test]
    fn core_profile_count_matches_core_names() {
        assert_eq!(
            tools_for_profile(Profile::Core).len(),
            CORE_TOOL_NAMES
                .iter()
                .chain(mail::MAIL_TOOL_NAMES.iter())
                .filter(|n| !crate::profiles::FULL_ONLY_TOOLS.contains(n))
                .count()
        );
    }

    #[test]
    fn core_profile_serves_the_mail_domain_but_the_full_only_tools() {
        let core: HashSet<String> = tools_for_profile(Profile::Core)
            .iter()
            .map(|t| t.name.as_ref().to_string())
            .collect();
        for tool in mail::MAIL_TOOL_NAMES {
            assert_eq!(
                core.contains(*tool),
                !crate::profiles::FULL_ONLY_TOOLS.contains(tool),
                "core profile mail tool {tool}"
            );
        }
    }

    #[test]
    fn core_profile_omits_known_deferred_tools() {
        let core: HashSet<String> = tools_for_profile(Profile::Core)
            .iter()
            .map(|t| t.name.as_ref().to_string())
            .collect();
        for deferred in [
            super::super::TOOL_DIAGNOSTIC_MEMORY,
            super::super::TOOL_CREATE_PROMPT_COMPOSITION,
            super::super::TOOL_UPDATE_PROMPT_COMPOSITION,
            super::super::TOOL_DELETE_PROMPT_COMPOSITION,
            super::super::TOOL_LIST_PROMPT_ANOMALIES,
            super::super::TOOL_SESSION_TOOL_USE_STATS,
            super::super::TOOL_DELETE_SESSION_NOTE,
            super::super::TOOL_LIST_CONFIG_CHANGES,
            super::super::TOOL_GET_CONFIG_SNAPSHOT,
            super::super::TOOL_LIST_EVAL_BUNDLES,
            super::super::TOOL_GET_SESSION_DOSSIER,
            super::super::TOOL_GET_TURN_FACTS,
            super::super::TOOL_LIST_EVAL_LABELS,
            super::super::TOOL_GET_TURN_VERDICTS,
            super::super::TOOL_LIST_TURN_VERDICTS,
            super::super::TOOL_LIST_STEERER_EVALUATIONS,
            super::super::TOOL_GET_EVAL_RATES,
            super::super::TOOL_LIST_EVAL_ALARMS,
            super::super::TOOL_LIST_EVAL_RUNS,
            super::super::TOOL_GET_EVAL_RUN,
            super::super::TOOL_LIST_EVAL_RUN_ITEMS,
            super::super::TOOL_LIST_EVAL_GATES,
            super::super::TOOL_GET_EVAL_GATE,
            super::super::TOOL_GET_SEMANTIC_STATUS,
            super::super::TOOL_GET_SEMANTIC_NEIGHBOURS,
            super::super::TOOL_GET_SEMANTIC_ITEM,
            super::super::TOOL_LIST_PATTERN_ASSIGNMENTS,
            super::super::TOOL_LIST_PATTERN_CANDIDATES,
            super::super::TOOL_GET_EMBEDDING_DRIFT,
            super::super::TOOL_GET_SESSION_VERDICTS,
            super::super::TOOL_LIST_EVAL_SESSION_END_ITEMS,
            super::super::TOOL_GET_STEER_QUESTIONS,
            super::super::TOOL_LIST_STEER_QUESTION_EVENTS,
            super::super::TOOL_GET_CROSS_SESSION_CALIBRATION,
            super::super::TOOL_LIST_DIRECTIVE_PROPOSALS,
        ] {
            assert!(
                !core.contains(deferred),
                "{deferred:?} should be Full-only but appears in Core"
            );
        }
    }

    #[test]
    fn core_profile_includes_full_investigation_surface() {
        let core: HashSet<String> = tools_for_profile(Profile::Core)
            .iter()
            .map(|t| t.name.as_ref().to_string())
            .collect();
        for investigative in [
            super::super::TOOL_GET_REQUEST_TOOL_CALLS,
            super::super::TOOL_LIST_TOOL_ERRORS,
            super::super::TOOL_LIST_AGENT_DISPATCHES,
            super::super::TOOL_GET_OPERATOR_TURN,
            super::super::TOOL_LIST_CORRECTED_TURNS,
            super::super::TOOL_LIST_OUTCOME_SUMMARY,
            super::super::TOOL_GET_REQUEST_STREAM_CHUNKS,
            super::super::TOOL_FIND_TEXT,
            super::super::TOOL_GET_SESSION_OPERATOR_TURNS,
            super::super::TOOL_GET_VERIFIER_RATE,
        ] {
            assert!(
                core.contains(investigative),
                "{investigative:?} is part of the investigation surface and must be in Core"
            );
        }
    }

    #[test]
    fn core_profile_preserves_all_tools_order() {
        let full_order: Vec<String> = all_tools()
            .iter()
            .map(|t| t.name.as_ref().to_string())
            .filter(|n| {
                (CORE_TOOL_NAMES.contains(&n.as_str())
                    || mail::MAIL_TOOL_NAMES.contains(&n.as_str()))
                    && !crate::profiles::FULL_ONLY_TOOLS.contains(&n.as_str())
            })
            .collect();
        let core_order: Vec<String> = tools_for_profile(Profile::Core)
            .iter()
            .map(|t| t.name.as_ref().to_string())
            .collect();
        assert_eq!(core_order, full_order, "core must keep all_tools() order");
    }

    #[test]
    fn relay_label_is_core_and_list_eval_labels_is_not() {
        let core: HashSet<String> = tools_for_profile(Profile::Core)
            .iter()
            .map(|t| t.name.as_ref().to_string())
            .collect();
        assert!(core.contains(super::super::TOOL_RELAY_LABEL));
        assert!(!core.contains(super::super::TOOL_LIST_EVAL_LABELS));
    }

    #[test]
    fn profile_parse_recognises_aliases_and_rejects_unknown() {
        assert_eq!(Profile::parse("core"), Some(Profile::Core));
        assert_eq!(Profile::parse("LEAN"), Some(Profile::Core));
        assert_eq!(Profile::parse(" default "), Some(Profile::Core));
        assert_eq!(Profile::parse("full"), Some(Profile::Full));
        assert_eq!(Profile::parse("ALL"), Some(Profile::Full));
        assert_eq!(Profile::parse("nonsense"), None);
        assert_eq!(Profile::default(), Profile::Core);
    }
}
