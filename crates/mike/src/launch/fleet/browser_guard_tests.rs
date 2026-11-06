use super::cases;

#[test]
fn guard_settings_leave_inherited_path_dynamic_and_preserve_operator_hooks() {
    cases::guard_settings_leave_inherited_path_dynamic_and_preserve_operator_hooks();
}

#[test]
fn guard_opt_out_preserves_operator_path_and_does_not_add_a_hook() {
    cases::guard_opt_out_preserves_operator_path_and_does_not_add_a_hook();
}

#[test]
fn disabled_hooks_keep_prelaunch_containment_and_operator_path() {
    cases::disabled_hooks_keep_prelaunch_containment_and_operator_path();
}

#[test]
fn explicit_settings_files_and_ordinary_launches_do_not_get_a_guard() {
    cases::explicit_settings_files_and_ordinary_launches_do_not_get_a_guard();
}

#[cfg(unix)]
#[test]
fn browser_guard_refuses_invalid_or_nonprivate_installation_paths() -> anyhow::Result<()> {
    cases::browser_guard_refuses_invalid_or_nonprivate_installation_paths()
}
