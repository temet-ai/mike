use clap::Parser;

use crate::service::commands::ensure_allowed;
use crate::service::error::ServiceError;
use crate::service::{ServiceCommand, installed_proxy};

#[derive(Parser)]
struct Wrapper {
    #[command(subcommand)]
    command: ServiceCommand,
}

fn parse(args: &[&str]) -> anyhow::Result<ServiceCommand> {
    let mut all = vec!["service"];
    all.extend_from_slice(args);
    Ok(Wrapper::try_parse_from(all)?.command)
}

#[test]
fn install_and_uninstall_parse_with_and_without_dry_run() -> anyhow::Result<()> {
    assert!(matches!(parse(&["install"])?, ServiceCommand::Install(a) if !a.dry_run));
    assert!(matches!(parse(&["install", "--dry-run"])?, ServiceCommand::Install(a) if a.dry_run));
    assert!(
        matches!(parse(&["uninstall", "--dry-run"])?, ServiceCommand::Uninstall(a) if a.dry_run)
    );
    Ok(())
}

#[test]
fn restart_and_status_parse_and_status_takes_no_dry_run() -> anyhow::Result<()> {
    assert!(matches!(parse(&["restart"])?, ServiceCommand::Restart(a) if !a.dry_run));
    assert!(matches!(parse(&["status"])?, ServiceCommand::Status));
    assert!(parse(&["status", "--dry-run"]).is_err());
    assert!(parse(&["bogus"]).is_err());
    Ok(())
}

#[test]
fn a_change_is_refused_while_the_sandbox_is_on_and_allowed_otherwise() -> anyhow::Result<()> {
    let error = ensure_allowed(true, "installing a background service")
        .err()
        .ok_or_else(|| anyhow::anyhow!("the sandbox must refuse"))?;
    assert!(matches!(error, ServiceError::Sandbox { .. }));
    let text = error.to_string();
    assert!(
        text.contains("installing a background service is switched off"),
        "{text}"
    );
    assert!(text.contains(crate::sandbox::ENV_VAR), "{text}");
    assert!(ensure_allowed(false, "installing a background service").is_ok());
    Ok(())
}

#[test]
fn the_sandbox_hides_an_installed_service_from_the_proxy_supervisor() {
    if crate::sandbox::active() {
        assert!(installed_proxy().is_none());
    }
}
