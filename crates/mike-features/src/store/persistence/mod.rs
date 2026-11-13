//! Facade: `store::persistence` as mike_features sees it (crate split, ADR 0002).


pub use mike_base::store::persistence::*;
pub mod acceptance_criteria;
pub mod ask_ledger_backfill;
pub mod change_record;
pub mod compositions;
pub mod consolidation_audit;
pub mod consolidation_reads;
pub mod cross_session;
pub mod db_cost_reads;
pub mod db_maintenance_reads;
pub mod dossier_entries;
pub mod dossier_parity_reads;
pub mod dossier_repo;
pub mod dossier_view;
pub mod eval_bundles;
pub mod eval_labels;
pub mod eval_metrics;
pub mod eval_pointer;
pub mod eval_rejudge;
pub mod eval_repo;
pub mod eval_runs;
pub mod filter_rules;
pub mod fleet;
pub mod fleet_correlation;
pub mod fleet_repo;
pub mod improvement;
pub mod judge;
pub mod judge_digest;
pub mod knowledge_repo;
pub mod launch;
pub mod launch_repo;
pub mod lesson_injections;
pub mod lesson_pair_candidates;
pub mod lesson_relations;
pub mod lessons;
pub mod lessons_repo;
pub mod listings;
pub mod mailbox;
pub mod memory_ask_ledger;
#[cfg(test)]
pub mod opened_by_readers_tests;
pub mod project_knowledge;
pub mod project_pin_ttl;
pub mod prompt_stream;
pub mod proxy_repo;
pub mod questions;
pub mod recall_embedding;
#[cfg(test)]
pub mod recall_embedding_tests;
pub mod recall_reads;
pub mod remote_control;
pub mod response_intercepts;
pub mod runtime;
pub mod runtime_repo;
pub mod search_events;
pub mod search_grades;
pub mod seat_memory;
pub mod semantic;
pub mod session_action_list;
pub mod session_embeddings;
pub mod session_end;
pub mod session_goals;
pub mod session_notes;
pub mod state_repo;
pub mod steerer_repo;
pub mod steering;
pub mod steering_backfill;
pub mod steering_evaluations;
pub mod steering_facts;
pub mod supplied_items;
#[cfg(any(test, feature = "test-seams"))]
pub mod test_support;
pub mod tool_call_reads;
pub mod tool_uses;
pub mod turn_facts;
pub mod turn_tool_call_reads;
pub mod turn_verdicts;
pub mod verifier_replay;
pub mod volatile;
pub mod work_item_attention;
#[cfg(test)]
pub mod work_item_attention_tests;
pub mod work_items_ledger;
pub mod work_items_reconcile;
pub mod work_threads;
pub mod workflow_judge_reads;
pub mod workspace;

// Re-exports of this crate's persistence groups, at the paths callers use.
pub use fleet::{FleetNoteRow, FleetRow, FleetSeatRow, NewFleet};
pub use judge::{NeedsJudgmentQuery, SessionForJudgment, fetch_sessions_needing_judgment};
pub use lesson_relations::{LessonRelationDraft, RelationDecisionOutcome};
pub use tool_call_reads::{BatchToolResultRow, ToolUseRow};
