use super::definition::FleetDefinition;

pub use super::build_admission::test_seams as admission;

#[path = "managed_control_test_seams.rs"]
pub mod control;
#[path = "managed_execution_test_seams.rs"]
pub mod execution;

pub fn seat_settings(def: &FleetDefinition, seat: &str) -> serde_json::Value {
    super::seat_settings::for_seat(def, seat)
}

#[path = "managed_durable_test_seams.rs"]
pub mod durable;

pub fn prepared_seat_settings(
    def: &FleetDefinition,
    scratch: &std::path::Path,
    seat: &str,
) -> Result<serde_json::Value, String> {
    std::fs::create_dir_all(scratch).map_err(|error| error.to_string())?;
    let settings = super::seat_settings::effective(def, scratch, seat);
    super::browser_guard::prepare(&settings)?;
    super::cargo_policy::prepare(&settings)?;
    Ok(settings)
}

pub fn guarded_terminal_command(program: &str, args: &[String]) -> Vec<String> {
    super::browser_guard::terminal_command(program, args)
}

#[cfg(unix)]
pub mod browser_receipts {
    pub use crate::launch::herdr::browser_guard::cases::{
        activation_waits_for_exact_receipt_and_submits_only_once,
        busy_prompt_times_out_without_submitting_activation,
        combined_helpers_share_one_activation_and_require_both,
        missing_or_mismatched_activation_refuses_without_starting_an_agent,
        ordinary_launches_never_call_the_backend_for_browser_setup,
        receipt_io_failures_and_nonmatching_content_are_not_success,
    };
}

#[cfg(unix)]
pub mod browser_policy {
    pub use crate::launch::fleet::browser_guard::cases::{
        browser_guard_refuses_invalid_or_nonprivate_installation_paths,
        disabled_hooks_keep_prelaunch_containment_and_operator_path,
        explicit_settings_files_and_ordinary_launches_do_not_get_a_guard,
        guard_opt_out_preserves_operator_path_and_does_not_add_a_hook,
        guard_settings_leave_inherited_path_dynamic_and_preserve_operator_hooks,
    };
}

pub use super::gate_failures::test_seams as gate_failures;

pub fn proof_handoff(proof: &super::report_proof::ReportProof) -> String {
    proof.handoff_body()
}

#[cfg(unix)]
pub use super::cargo_policy::test_seams as cargo_policy;
