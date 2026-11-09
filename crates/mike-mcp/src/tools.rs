mod catalogue;
mod changes;
mod dispatch;
mod envelope;
mod schema;

pub mod mail;
mod select;
mod select_examples;
mod select_hint;

mod config_history;
mod cross_session;
mod diagnostics;
mod dossier;
mod eval_bundles;
mod eval_labels;
mod eval_metrics;
mod eval_runs;
mod filters;
mod fleet_check;
mod fleet_decide;
mod fleet_propose;
mod fleet_record;
mod fleet_report;
mod fleet_tree;
mod fleet_views;
mod fleets;
mod improvement;
mod knowledge;
mod launches;
mod memory;
mod prompts;
mod questions;
mod requests;
mod semantic;
mod session_activity;
mod session_notes;
mod session_verdicts;
mod sessions;
mod steer_questions;
mod steering_evaluations;
mod text_search;
mod threads;
mod time_spent;
mod turn_verdicts;
mod turns;
mod turns_summary;
mod work_threads;
mod workspace;

use envelope::{
    arguments_as_value, parse_args_default, parse_args_required, wrap_success, wrap_success_paged,
    wrap_success_with_note,
};
use schema::{
    IdStringArgs, UpdateByIdArgs, id_only_schema, id_only_string_schema, id_with_pointer_schema,
    object_schema, order_property,
};

pub use catalogue::{CORE_TOOL_NAMES, Profile, all_tools, tools_for_profile};
pub(crate) use dispatch::call_tool;

pub use config_history::{TOOL_GET_CONFIG_SNAPSHOT, TOOL_LIST_CONFIG_CHANGES};
pub use cross_session::{TOOL_GET_CROSS_SESSION_CALIBRATION, TOOL_LIST_DIRECTIVE_PROPOSALS};
pub use diagnostics::{
    TOOL_DIAGNOSTIC_FINDINGS, TOOL_DIAGNOSTIC_MEMORY, TOOL_DIAGNOSTIC_STATS, TOOL_GET_STATS,
    TOOL_GET_VERIFIER_RATE,
};
pub use dossier::{TOOL_GET_SESSION_DOSSIER, TOOL_GET_TURN_FACTS};
pub use eval_bundles::TOOL_LIST_EVAL_BUNDLES;
pub use eval_labels::{TOOL_LIST_EVAL_LABELS, TOOL_RELAY_LABEL};
pub use eval_metrics::{TOOL_GET_EVAL_RATES, TOOL_LIST_EVAL_ALARMS};
pub use eval_runs::{
    TOOL_GET_EVAL_GATE, TOOL_GET_EVAL_RUN, TOOL_LIST_EVAL_GATES, TOOL_LIST_EVAL_RUN_ITEMS,
    TOOL_LIST_EVAL_RUNS,
};
pub use filters::{
    TOOL_CREATE_FILTER_RULE, TOOL_DELETE_FILTER_RULE, TOOL_DRY_RUN_FILTER_RULES,
    TOOL_LIST_FILTER_RULES, TOOL_UPDATE_FILTER_RULE,
};
pub use fleets::{TOOL_DEFINE_FLEET, TOOL_GET_FLEET};
pub use improvement::TOOL_GET_IMPROVEMENT_EFFECTS;
pub use launches::{TOOL_KILL_SESSION, TOOL_LIST_LAUNCHES, TOOL_SPAWN_SESSION};
pub use mail::{
    TOOL_ACK_SESSION_MESSAGE, TOOL_GET_SESSION_MESSAGES, TOOL_SEND_SESSION_MESSAGE,
    TOOL_VERIFY_MESSAGE_READ, TOOL_VERIFY_SESSION_MESSAGE,
};
pub use memory::{
    TOOL_DELETE_LESSON, TOOL_LIST_AGENT_DISPATCHES, TOOL_LIST_LESSONS, TOOL_RECALL_PROJECT_CONTEXT,
    TOOL_RECALL_SIMILAR_TURNS, TOOL_RECORD_LESSON,
};
pub use prompts::{
    TOOL_CREATE_PROMPT_COMPOSITION, TOOL_DELETE_PROMPT_COMPOSITION, TOOL_GET_PROMPT_COMPOSITION,
    TOOL_LIST_PROMPT_ANOMALIES, TOOL_LIST_PROMPT_COMPOSITIONS, TOOL_UPDATE_PROMPT_COMPOSITION,
};
pub use questions::{TOOL_ANSWER, TOOL_ASK, TOOL_DECLINE};
pub use requests::{
    TOOL_GET_ACCUMULATED_TEXT, TOOL_GET_REQUEST, TOOL_GET_REQUEST_BODY, TOOL_GET_REQUEST_SENT_BODY,
    TOOL_GET_REQUEST_STREAM_CHUNKS, TOOL_GET_REQUEST_SUMMARY, TOOL_GET_REQUEST_TOOL_CALLS,
    TOOL_GET_REQUEST_TRANSFORMATION_MANIFEST, TOOL_GET_RESPONSE_BODY, TOOL_LIST_REQUESTS,
};
pub use semantic::{
    TOOL_GET_EMBEDDING_DRIFT, TOOL_GET_SEMANTIC_ITEM, TOOL_GET_SEMANTIC_NEIGHBOURS,
    TOOL_GET_SEMANTIC_STATUS, TOOL_LIST_PATTERN_ASSIGNMENTS, TOOL_LIST_PATTERN_CANDIDATES,
};
pub use session_activity::{
    TOOL_CONFIRM_MESSAGE_RECEIPT, TOOL_LIST_FILE_WRITES, TOOL_LIST_SESSION_TOOL_USES,
    TOOL_SESSION_TOOL_USE_STATS, TOOL_SET_SESSION_GOAL,
};
pub use session_notes::{
    TOOL_DELETE_SESSION_NOTE, TOOL_LIST_SESSION_NOTES, TOOL_WRITE_SESSION_NOTE,
};
pub use session_verdicts::{TOOL_GET_SESSION_VERDICTS, TOOL_LIST_EVAL_SESSION_END_ITEMS};
pub use sessions::{
    TOOL_GET_SESSION, TOOL_GET_SESSION_HEAD, TOOL_GET_SESSION_THREADS, TOOL_LIST_PROJECTS,
    TOOL_LIST_SESSIONS,
};
pub use steer_questions::{TOOL_GET_STEER_QUESTIONS, TOOL_LIST_STEER_QUESTION_EVENTS};
pub use steering_evaluations::TOOL_LIST_STEERER_EVALUATIONS;
pub use text_search::{TOOL_FIND_TEXT, TOOL_LIST_TOOL_ERRORS, TOOL_SEARCH_SESSION_CONTENT};
pub use threads::{TOOL_READ_THREAD, TOOL_RESOLVE_THREAD};
pub use time_spent::TOOL_GET_TIME_SPENT;
pub use turn_verdicts::{TOOL_GET_TURN_VERDICTS, TOOL_LIST_TURN_VERDICTS};
pub use turns::{
    TOOL_GET_OPERATOR_TURN, TOOL_GET_SESSION_OPERATOR_TURNS, TOOL_LIST_OPERATOR_TURNS,
};
pub use turns_summary::{
    TOOL_GET_SESSION_OVERVIEW, TOOL_LIST_CORRECTED_TURNS, TOOL_LIST_OUTCOME_SUMMARY,
};
pub use work_threads::{
    TOOL_ADD_THREAD_ENTRY, TOOL_CHECKPOINT_THREAD, TOOL_GET_THREAD, TOOL_LIST_THREADS,
};
pub use workspace::{
    TOOL_END_WORKSPACE_REGISTRATION, TOOL_GET_FLEET_GRAPH, TOOL_GET_WORKSPACE_ROSTER,
    TOOL_REGISTER_WORKSPACE_SESSION, TOOL_WORKSPACE_HEARTBEAT,
};
