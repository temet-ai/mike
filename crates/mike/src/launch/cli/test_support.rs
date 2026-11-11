use std::path::{Path, PathBuf};

use super::ledger::Ledger;
use super::runner::Runner;
use crate::config::LaunchConfig;
use crate::launch::{TerminalBackend, TrustGate};

pub(crate) fn runner<'a>(
    backend: &'a dyn TerminalBackend,
    config: &'a LaunchConfig,
    home: &Path,
) -> anyhow::Result<Runner<'a>> {
    let claude_json = home.join(".claude.json");
    std::fs::write(
        &claude_json,
        r#"{"mcpServers":{"mike":{"command":"mike-mcp"}}}"#,
    )?;
    Ok(Runner {
        backend,
        ledger: Ledger::none(),
        config,
        proxy: None,
        trust: TrustGate::new(claude_json),
        briefs_dir: home.join("briefs"),
        inspect_addr: None,
        mcp_server: None,
        backend_reason: None,
    })
}

pub(crate) fn project_dir(home: &Path) -> anyhow::Result<PathBuf> {
    let dir = home.join("proj");
    std::fs::create_dir_all(&dir)?;
    Ok(std::fs::canonicalize(dir)?)
}

#[cfg(all(feature = "test-seams", not(test)))]
pub async fn stopped_fleet_refuses_actual_open() -> anyhow::Result<()> {
    super::open::admission_tests::stopped_fleet_explicit_settings_cannot_bypass_actual_open_fence()
        .await
}

#[cfg(all(feature = "test-seams", not(test)))]
pub async fn accepted_open_blocks_stop_until_durable() -> anyhow::Result<()> {
    super::open::admission_tests::accepted_open_reservation_holds_stop_until_backend_open_is_durable()
        .await
}

#[cfg(all(feature = "test-seams", not(test)))]
pub async fn failed_backend_open_stays_unresolved() -> anyhow::Result<()> {
    super::open::admission_tests::backend_open_error_keeps_fleet_reservation_unresolved().await
}

#[cfg(feature = "test-seams")]
pub async fn unconfirmed_kill_does_not_record_closure() -> anyhow::Result<()> {
    super::kill::closure_cases::unconfirmed_kill_does_not_record_closure().await
}

#[cfg(feature = "test-seams")]
pub async fn confirmed_pane_closure_preserves_exit_evidence() -> anyhow::Result<()> {
    super::kill::closure_cases::confirmed_pane_closure_preserves_exit_evidence().await
}

#[cfg(all(feature = "test-seams", not(test)))]
pub fn root_origin_contract() -> anyhow::Result<()> {
    crate::launch::local_root::tests::foreign_origin_and_witness_loss_never_prove_exit()?;
    crate::launch::local_root::tests::linux_and_macos_marker_parsers_require_exact_identity();
    crate::launch::local_root::tests::a_herdr_core_seat_has_a_root_key();
    Ok(())
}

#[cfg(feature = "test-seams")]
pub fn remember_owned_root_fixture(launch: i64, run: &str, session: uuid::Uuid, pid: u32) -> bool {
    crate::launch::local_root::tests::remember_owned_fixture(launch, run, session, pid)
}
