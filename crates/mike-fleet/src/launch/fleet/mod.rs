pub use mike_launch::launch::fleet::*;
pub mod acceptance;
pub mod acceptance_register;
#[cfg(test)]
pub mod acceptance_tests;
pub mod background_result;
pub mod brief_delivery;
pub mod brief_notes;
pub mod brief_notes_sync;
pub mod brief_version;
pub mod build_timing;
pub mod build_waits;
pub mod bundle;
pub mod capacity;
pub mod carry;
pub mod check;
pub mod check_blank;
pub mod check_proof;
pub mod check_report;
pub mod check_run;
pub mod check_state;
pub mod checks;
pub mod checks_files;
pub mod checks_order;
pub mod checks_paths;
pub mod checks_proof;
pub mod checks_scope;
pub mod checks_target;
pub mod cleanup;
pub mod close_note;
pub mod command;
pub mod commit;
pub mod commit_check;
pub mod commit_landed;
pub mod commit_modules;
pub mod commit_note;
pub mod commit_paths;
pub mod commit_verify;
pub mod definition;
pub mod diagnostics;
pub mod discovery;
pub mod display;
pub mod environment;
pub mod events;
pub mod floor;
pub mod focus;
pub mod fold;
#[cfg(test)]
pub mod fold_replay_tests;
pub mod fork_seat;
pub mod gate;
pub mod gate_blame;
pub mod gate_failures;
pub mod gate_queue;
pub mod gate_rerun;
pub mod gate_slot;
pub mod gate_source;
pub mod gate_text;
pub mod goals;
pub mod here;
pub mod host_memory;
pub mod introduce;
pub mod issue_close;
pub mod leaves;
pub mod locate;
pub mod operation;
pub mod operation_admission;
pub mod operation_profile;
#[cfg(test)]
pub mod operation_runtime_tests;
pub mod operation_timing;
pub mod plan;
pub mod plan_briefs;
pub mod problems;
pub mod process_tripwire;
pub mod propose;
pub mod propose_tick;
pub mod push;
pub mod questions;
pub mod regression;
pub mod release;
pub mod render;
pub mod report;
pub mod report_commit;
pub mod report_proof;
pub mod retrospective;
pub mod retrospective_launch;
#[cfg(test)]
pub mod retrospective_tests;
pub mod retrospective_worker;
pub mod reuse;
pub mod rule_audit;
pub mod ruling;
pub mod ruling_notice;
pub mod ruling_proof;
pub mod run_notebook;
pub mod run_record;
pub mod run_text_notes;
pub mod seat;
pub mod seat_args;
pub mod seat_settings;
pub mod seated;
pub mod shared_tree;
pub mod shutdown;
pub mod staff;
pub mod staff_claims;
pub mod staff_limits;
pub mod start;
pub mod start_baseline;
pub mod start_failure;
pub mod status;
pub mod status_pane;
pub mod status_tasks;
pub mod status_types;
pub mod succession;
pub mod supervise;
pub mod task_prompt;
pub mod tell;
pub mod tell_acceptance;
pub mod tell_ask_reference;
pub mod tell_commits;
pub mod tell_context;
pub mod tell_digest;
pub mod tell_reference;
pub mod tell_tripwires;
pub mod tick;
pub mod timed_command;
pub mod told;
pub mod tool_examples;
pub mod wake;
pub mod warm;
pub mod watch;
pub mod work_of;

#[cfg(test)]
pub mod background_result_tests;
#[cfg(test)]
pub mod brief_delivery_tests;
#[cfg(test)]
pub mod brief_notes_tests;
#[cfg(test)]
pub mod carry_tests;
#[cfg(test)]
pub mod check_tests;
#[cfg(test)]
pub mod checks_tests;
#[cfg(test)]
pub mod cleanup_tests;
#[cfg(test)]
pub mod commit_modules_tests;
#[cfg(test)]
pub mod commit_tests;
#[cfg(test)]
pub mod environment_tests;
#[cfg(test)]
pub mod events_tests;
#[cfg(test)]
pub mod gate_tests;
#[cfg(test)]
pub mod issue_close_tests;
#[cfg(test)]
pub mod plan_tests;
#[cfg(test)]
pub mod propose_tests;
#[cfg(test)]
pub mod propose_tick_tests;
#[cfg(test)]
pub mod push_tests;
#[cfg(test)]
pub mod questions_tests;
#[cfg(test)]
pub mod report_tests;
#[cfg(test)]
pub mod run_record_tests;
#[cfg(test)]
pub mod seated_tests;
#[cfg(any(test, feature = "test-seams"))]
pub mod test_fixtures;
#[cfg(test)]
pub mod tests;

pub use seat_settings::background_hook::run as background_hook;
pub use seat_settings::receipt_hook::run as receipt_hook;
pub use seat_settings::{PROFILE_ENV, profile_for};
pub use timed_command::run as timed_run;

pub use checks::{Missing, check_definition};
pub use command::{FleetArgs, FleetCommand, run};
pub use definition::{FleetDefinition, merge};
pub use environment::{Environment, Starts, check_environment, gather, starts};
pub use fold::done_tasks_of;
pub use plan::{ControlSeats, FleetFacts, SeatPlan};
pub use render::{FleetListing, render, render_list};
pub use status::FleetStatus;

#[cfg(test)]
pub mod runtime_tests;

pub mod build_admission;
pub mod build_admission_root;
pub mod managed_command;
pub mod managed_process;
pub mod managed_queue;
pub mod nextest_filter;
pub mod owned_execution;
pub mod owned_spawn_fence;

#[cfg(any(test, feature = "test-seams"))]
pub mod managed_test_support;
pub mod owned_git;

#[cfg(feature = "test-seams")]
pub mod test_seams;

pub mod managed_resources;

pub use owned_execution::managed_containment_capability;
