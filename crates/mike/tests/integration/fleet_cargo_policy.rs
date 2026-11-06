use mike::launch::fleet::{FleetDefinition, test_seams};
use serde_json::json;
use std::path::Path;
use test_seams::cargo_policy;

fn cargo_fleet(seat_settings: Option<serde_json::Value>) -> FleetDefinition {
    let mut definition = json!({"execution": {"cargo": true}});
    if let Some(settings) = seat_settings {
        definition["seat_settings"] = settings;
    }
    FleetDefinition::from_value(&definition).unwrap_or_default()
}

#[test]
fn narrow_queries_and_builtin_help_pass() {
    cargo_policy::classifier::narrow_queries_and_builtin_help_pass()
}

#[test]
fn heavy_aliases_and_test_harness_help_are_refused() {
    cargo_policy::classifier::heavy_aliases_and_test_harness_help_are_refused()
}

#[test]
fn formatter_is_an_explicit_separate_refusal() {
    cargo_policy::classifier::formatter_is_an_explicit_separate_refusal()
}

#[test]
fn every_role_and_successor_gets_the_same_default_policy() -> Result<(), Box<dyn std::error::Error>>
{
    cargo_policy::settings::every_role_and_successor_gets_the_same_default_policy()
}

#[test]
fn operator_optout_and_disabled_hooks_preserve_settings() -> Result<(), Box<dyn std::error::Error>>
{
    cargo_policy::settings::operator_optout_and_disabled_hooks_preserve_settings()
}

#[test]
fn browser_and_operator_prefixes_and_permissions_are_preserved()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::settings::browser_and_operator_prefixes_and_permissions_are_preserved()
}

#[test]
fn malformed_hooks_and_unsupported_paths_fail_transactionally() {
    cargo_policy::settings::malformed_hooks_and_unsupported_paths_fail_transactionally()
}

#[test]
fn relative_empty_and_non_utf8_path_entries_keep_their_original_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::resolution::relative_empty_and_non_utf8_path_entries_keep_their_original_bytes()
}

#[test]
fn rustup_named_symlink_keeps_the_cargo_path_instead_of_its_target()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::resolution::rustup_named_symlink_keeps_the_cargo_path_instead_of_its_target()
}

#[test]
fn aliases_of_verified_shim_directories_are_removed_but_other_wrappers_remain()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::resolution::aliases_of_verified_shim_directories_are_removed_but_other_wrappers_remain()
}

#[test]
fn removed_all_entries_is_distinct_from_a_preserved_empty_entry()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::resolution::removed_all_entries_is_distinct_from_a_preserved_empty_entry()
}

#[test]
fn replaced_or_unmarked_shim_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::resolution::replaced_or_unmarked_shim_is_refused()
}

#[test]
fn a_hardlinked_launcher_outside_the_shim_cannot_recurse() -> Result<(), Box<dyn std::error::Error>>
{
    cargo_policy::resolution::a_hardlinked_launcher_outside_the_shim_cannot_recurse()
}

#[test]
fn missing_or_non_executable_cargo_is_not_selected() -> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::resolution::missing_or_non_executable_cargo_is_not_selected()
}

#[test]
fn heavy_and_unknown_commands_never_reach_cargo_or_queue() -> Result<(), Box<dyn std::error::Error>>
{
    cargo_policy::subprocess::heavy_and_unknown_commands_never_reach_cargo_or_queue(Path::new(
        env!("CARGO_BIN_EXE_mike"),
    ))
}

#[test]
fn queries_preserve_toolchain_non_utf8_stdin_environment_cwd_and_exit()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::subprocess::queries_preserve_toolchain_non_utf8_stdin_environment_cwd_and_exit(
        Path::new(env!("CARGO_BIN_EXE_mike")),
    )
}

#[test]
fn formatter_and_help_policy_is_explicit() -> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::subprocess::formatter_and_help_policy_is_explicit(Path::new(env!(
        "CARGO_BIN_EXE_mike"
    )))
}

#[test]
fn inherited_policy_is_removed_only_for_explicit_managed_launch()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::subprocess::inherited_policy_is_removed_only_for_explicit_managed_launch(
        Path::new(env!("CARGO_BIN_EXE_mike")),
    )
}

#[test]
fn cx_style_wrapper_is_caught_but_absolute_cargo_is_explicitly_uncovered()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::subprocess::cx_style_wrapper_is_caught_but_absolute_cargo_is_explicitly_uncovered(
        Path::new(env!("CARGO_BIN_EXE_mike")),
    )
}

#[test]
fn session_hook_appends_to_actual_env_file_and_preserves_other_fragments()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::subprocess::session_hook_appends_to_actual_env_file_and_preserves_other_fragments(
        Path::new(env!("CARGO_BIN_EXE_mike")),
    )
}

#[test]
fn shell_activation_preserves_browser_prefix_and_handles_quoted_paths()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::subprocess::shell_activation_preserves_browser_prefix_and_handles_quoted_paths(
        Path::new(env!("CARGO_BIN_EXE_mike")),
    )
}

#[test]
fn explicit_optout_does_not_install_or_change_operator_path()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::subprocess::explicit_optout_does_not_install_or_change_operator_path(Path::new(
        env!("CARGO_BIN_EXE_mike"),
    ))
}

#[test]
fn allowed_query_keeps_policy_for_unbudgeted_nested_cargo() -> Result<(), Box<dyn std::error::Error>>
{
    cargo_policy::subprocess::allowed_query_keeps_policy_for_unbudgeted_nested_cargo(Path::new(
        env!("CARGO_BIN_EXE_mike"),
    ))
}

#[test]
fn invalid_or_replaced_policy_marker_refuses_managed_sanitization()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::subprocess::invalid_or_replaced_policy_marker_refuses_managed_sanitization(
        Path::new(env!("CARGO_BIN_EXE_mike")),
    )
}

#[test]
fn missing_real_cargo_and_recursive_alias_refuse_without_looping()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::subprocess::missing_real_cargo_and_recursive_alias_refuse_without_looping(
        Path::new(env!("CARGO_BIN_EXE_mike")),
    )
}

#[test]
fn missing_helper_refuses_installation_without_creating_policy_files()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::subprocess::missing_helper_refuses_installation_without_creating_policy_files(
        Path::new(env!("CARGO_BIN_EXE_mike")),
    )
}

#[test]
fn fifo_symlink_and_oversized_markers_fail_with_bounded_children()
-> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::subprocess::fifo_symlink_and_oversized_markers_fail_with_bounded_children(
        Path::new(env!("CARGO_BIN_EXE_mike")),
    )
}

#[test]
fn unsafe_installation_files_do_not_hang_or_change_data() -> Result<(), Box<dyn std::error::Error>>
{
    cargo_policy::subprocess::unsafe_installation_files_do_not_hang_or_change_data(Path::new(env!(
        "CARGO_BIN_EXE_mike"
    )))
}

#[test]
fn default_policy_stays_inactive_for_all_roles_until_compatibility_activation() {
    for role in [
        "worker-T1",
        "template-rust",
        "coordinator",
        "acceptance",
        "acceptance-2",
        "observer",
        "reference-3",
    ] {
        let settings = test_seams::seat_settings(&cargo_fleet(None), role);
        assert_eq!(settings["env"]["MIKE_FLEET_CARGO_POLICY"], "0", "{role}");
        assert!(settings["env"]["MIKE_FLEET_CARGO_POLICY_BIN"].is_null());
    }
}

#[test]
fn explicit_policy_optout_remains_independent_of_browser_defaults() {
    let definition = FleetDefinition {
        seat_settings: Some(json!({"env": {"MIKE_FLEET_CARGO_POLICY": "0"}})),
        ..Default::default()
    };
    let settings = test_seams::seat_settings(&definition, "acceptance-2");
    assert_eq!(settings["env"]["MIKE_FLEET_CARGO_POLICY"], "0");
    assert_eq!(settings["env"]["MIKE_FLEET_BROWSER_GUARD"], "1");
    assert!(settings["env"]["MIKE_FLEET_CARGO_POLICY_BIN"].is_null());
}

#[test]
fn combined_terminal_setup_sources_both_guards_and_preserves_runtime_path() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let definition = cargo_fleet(Some(json!({"env": {"MIKE_FLEET_CARGO_POLICY": "1"}})));
    let settings = cargo_policy::prepared_settings(
        &definition,
        root.path(),
        "coordinator-2",
        Path::new(env!("CARGO_BIN_EXE_mike")),
    )
    .map_err(anyhow::Error::msg)?;
    let args = vec!["--settings".into(), settings.to_string()];
    let launch =
        cargo_policy::combined_terminal_command("/bin/sh", &args).map_err(anyhow::Error::msg)?;
    assert_eq!(launch[0], "/bin/sh");
    assert_eq!(launch[2].matches("/activate").count(), 2);
    let output = std::process::Command::new(&launch[0])
        .args([
            &launch[1],
            &launch[2],
            &launch[3],
            "/bin/sh",
            "-c",
            "printf '%s' \"$PATH\"",
        ])
        .env("PATH", "/operator/runtime/bin")
        .output()?;
    assert!(output.status.success());
    let expected = format!(
        "{}/cargo-policy/bin:{}/browser-guard:/operator/runtime/bin",
        root.path().display(),
        root.path().display()
    );
    assert_eq!(String::from_utf8(output.stdout)?, expected);
    Ok(())
}

#[tokio::test]
async fn combined_herdr_activation_has_one_receipt_and_no_partial_success() -> anyhow::Result<()> {
    test_seams::browser_receipts::combined_helpers_share_one_activation_and_require_both().await
}

#[tokio::test]
async fn real_managed_owner_strips_inherited_policy() -> Result<(), Box<dyn std::error::Error>> {
    cargo_policy::subprocess::real_managed_owner_strips_inherited_policy(Path::new(env!(
        "CARGO_BIN_EXE_mike"
    )))
    .await
}
