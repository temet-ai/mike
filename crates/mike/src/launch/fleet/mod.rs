mod acceptance;
pub(crate) mod acceptance_artifacts;
pub(crate) mod acceptance_register;
#[cfg(test)]
mod acceptance_tests;
pub mod background_result;
pub mod brief_delivery;
pub mod brief_notes;
pub mod brief_notes_sync;
pub mod brief_version;
pub(crate) mod browser_guard;
mod build_timing;
mod build_waits;
pub mod bundle;
pub mod carry;
pub mod check;
mod check_blank;
pub mod check_proof;
pub mod check_report;
mod check_run;
mod check_state;
pub mod checks;
pub mod checks_files;
mod checks_order;
pub mod checks_paths;
pub mod checks_proof;
mod checks_scope;
mod checks_target;
mod cleanup;
mod close_note;
pub mod command;
pub mod commit;
pub mod commit_check;
mod commit_landed;
mod commit_modules;
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
mod focus;
mod fold;
#[cfg(test)]
mod fold_replay_tests;
mod fork_seat;
pub mod gate;
mod gate_blame;
mod gate_failures;
mod gate_queue;
mod gate_rerun;
mod gate_slot;
mod gate_source;
pub mod gate_text;
pub mod goals;
mod here;
pub mod introduce;
pub mod issue_close;
pub mod leaves;
pub mod locate;
pub mod operation;
mod operation_admission;
pub mod operation_profile;
#[cfg(test)]
mod operation_runtime_tests;
mod operation_timing;
pub mod plan;
mod plan_briefs;
pub mod problems;
mod process_tripwire;
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
mod retrospective_launch;
#[cfg(test)]
mod retrospective_tests;
mod retrospective_worker;
pub mod reuse;
mod rule_audit;
pub mod ruling;
pub mod ruling_notice;
pub mod ruling_proof;
mod run_notebook;
pub mod run_record;
mod run_text_notes;
mod seat;
pub(crate) mod seat_helpers;
pub(crate) mod seat_settings;
pub mod seated;
mod shared_tree;
pub mod shutdown;
pub mod staff;
pub mod start;
pub mod start_baseline;
mod start_failure;
pub mod status;
mod status_pane;
mod status_tasks;
mod status_types;
pub mod succession;
mod supervise;
pub mod tell;
pub mod tell_acceptance;
pub mod tell_ask_reference;
pub mod tell_commits;
pub mod tell_context;
pub mod tell_digest;
pub mod tell_reference;
mod tell_tripwires;
mod tick;
mod timed_command;
pub mod told;
mod tool_examples;
mod wake;
mod warm;
pub mod watch;
pub mod work_of;

#[cfg(test)]
mod background_result_tests;
#[cfg(test)]
mod brief_delivery_tests;
#[cfg(test)]
mod brief_notes_tests;
#[cfg(test)]
mod carry_tests;
#[cfg(test)]
mod check_tests;
#[cfg(test)]
mod checks_tests;
#[cfg(test)]
mod cleanup_tests;
#[cfg(test)]
mod commit_modules_tests;
#[cfg(test)]
mod commit_tests;
#[cfg(test)]
mod environment_tests;
#[cfg(test)]
mod events_tests;
#[cfg(test)]
mod gate_tests;
#[cfg(test)]
mod issue_close_tests;
#[cfg(test)]
mod plan_tests;
#[cfg(test)]
mod propose_tests;
#[cfg(test)]
mod propose_tick_tests;
#[cfg(test)]
mod push_tests;
#[cfg(test)]
mod questions_tests;
#[cfg(test)]
mod report_tests;
#[cfg(test)]
mod run_notes_tests;
#[cfg(test)]
mod run_record_tests;
#[cfg(test)]
mod seated_tests;
#[cfg(test)]
mod tests;

pub use seat_settings::background_hook::run as background_hook;
pub use seat_settings::receipt_hook::run as receipt_hook;
pub(crate) use seat_settings::{PROFILE_ENV, profile_for};
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
mod runtime_tests;

mod build_admission;
mod build_admission_root;
mod managed_command;
mod managed_process;
mod managed_queue;
pub mod nextest_filter;
pub(crate) mod owned_execution;
mod owned_spawn_fence;

#[cfg(any(test, feature = "test-seams"))]
mod managed_test_support;
mod owned_git;

#[cfg(feature = "test-seams")]
pub mod test_seams;

mod managed_resources;

pub use owned_execution::managed_containment_capability;
