use mike::launch::fleet::test_seams::{browser_policy, browser_receipts};

#[tokio::test]
async fn activation_waits_for_exact_receipt_and_submits_only_once() -> anyhow::Result<()> {
    browser_receipts::activation_waits_for_exact_receipt_and_submits_only_once().await
}

#[tokio::test]
async fn missing_or_mismatched_activation_refuses_without_starting_an_agent() -> anyhow::Result<()>
{
    browser_receipts::missing_or_mismatched_activation_refuses_without_starting_an_agent().await
}

#[tokio::test]
async fn busy_prompt_times_out_without_submitting_activation() -> anyhow::Result<()> {
    browser_receipts::busy_prompt_times_out_without_submitting_activation().await
}

#[tokio::test]
async fn ordinary_launches_never_call_the_backend_for_browser_setup() -> anyhow::Result<()> {
    browser_receipts::ordinary_launches_never_call_the_backend_for_browser_setup().await
}

#[test]
fn receipt_io_failures_and_nonmatching_content_are_not_success() -> anyhow::Result<()> {
    browser_receipts::receipt_io_failures_and_nonmatching_content_are_not_success()
}

#[test]
fn guard_settings_leave_inherited_path_dynamic_and_preserve_operator_hooks() {
    browser_policy::guard_settings_leave_inherited_path_dynamic_and_preserve_operator_hooks();
}

#[test]
fn guard_opt_out_preserves_operator_path_and_does_not_add_a_hook() {
    browser_policy::guard_opt_out_preserves_operator_path_and_does_not_add_a_hook();
}

#[test]
fn disabled_hooks_keep_prelaunch_containment_and_operator_path() {
    browser_policy::disabled_hooks_keep_prelaunch_containment_and_operator_path();
}

#[test]
fn explicit_settings_files_and_ordinary_launches_do_not_get_a_guard() {
    browser_policy::explicit_settings_files_and_ordinary_launches_do_not_get_a_guard();
}

#[test]
fn browser_guard_refuses_invalid_or_nonprivate_installation_paths() -> anyhow::Result<()> {
    browser_policy::browser_guard_refuses_invalid_or_nonprivate_installation_paths()
}
