use std::time::Duration;

use clap::{Args, Subcommand};

use super::endpoints::Endpoints;
use super::environment::{self, Inputs};
use super::error::ServiceError;
use super::exec::Context;
use super::files::Presence;
use super::guard::address_answers;
use super::health::{self, Timing};
use super::manager::Manager;
use super::platform::Platform;
use super::spec::ServiceKind;
use super::status::{self, UnitFacts};
use super::{install, restart, uninstall};
use crate::config::{CliOverrides, ProxyConfig};
use crate::db_provision::SystemRunner;

const RETRY_PAUSE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Args)]
pub struct ServiceArgs {
    #[command(subcommand)]
    pub command: ServiceCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ServiceCommand {
    Install(DryRunArgs),
    Uninstall(DryRunArgs),
    Status,
    Restart(DryRunArgs),
}

#[derive(Debug, Clone, Args)]
pub struct DryRunArgs {
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(args: ServiceArgs, overrides: CliOverrides) -> i32 {
    match dispatch(args.command, overrides).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("mike service: {e}");
            1
        }
    }
}

fn say(line: &str) {
    println!("{line}");
}

pub(crate) fn ensure_allowed(
    sandbox_active: bool,
    action: &'static str,
) -> Result<(), ServiceError> {
    if sandbox_active {
        return Err(ServiceError::Sandbox { action });
    }
    Ok(())
}

fn load_config(overrides: CliOverrides) -> Result<ProxyConfig, ServiceError> {
    crate::config::load(overrides).map_err(|e| ServiceError::Config(format!("{e:#}")))
}

fn context<'a>(manager: &'a Manager, endpoints: &'a Endpoints) -> Context<'a, SystemRunner> {
    Context {
        runner: &SystemRunner,
        manager,
        endpoints,
        listening: address_answers,
        pause: RETRY_PAUSE,
        out: &say,
    }
}

async fn dispatch(command: ServiceCommand, overrides: CliOverrides) -> Result<i32, ServiceError> {
    let manager = Manager::from_process()?;
    match command {
        ServiceCommand::Install(args) => install_command(&manager, args.dry_run, overrides).await,
        ServiceCommand::Uninstall(args) => uninstall_command(&manager, args.dry_run).await,
        ServiceCommand::Status => status_command(&manager, overrides).await,
        ServiceCommand::Restart(args) => restart_command(&manager, args.dry_run, overrides).await,
    }
}

fn print_all(lines: &[String]) {
    for line in lines {
        say(line);
    }
}

async fn install_command(
    manager: &Manager,
    dry_run: bool,
    overrides: CliOverrides,
) -> Result<i32, ServiceError> {
    let inputs = Inputs::from_process()?;
    let state_dir =
        crate::user_config::state_dir().map_err(|e| ServiceError::Config(e.to_string()))?;
    let specs = environment::specs(&inputs, &state_dir);
    if dry_run {
        say(
            "Dry run: nothing is written and no command is run, and the checks for the developer stack and for busy ports are skipped.",
        );
        print_all(&install::plan(manager, &specs));
        return Ok(0);
    }
    ensure_allowed(crate::sandbox::active(), "installing a background service")?;
    let config = load_config(overrides)?;
    let endpoints = Endpoints::from_config(&config);
    install::install(&context(manager, &endpoints), &specs).await?;
    say("Checking that the services answer:");
    health::verify(&endpoints, Timing::AFTER_START, &say).await?;
    if manager.platform == Platform::Systemd {
        say(
            "To keep the services running after you log out, run `sudo loginctl enable-linger $USER` once. mike does not run sudo.",
        );
    }
    Ok(0)
}

async fn uninstall_command(manager: &Manager, dry_run: bool) -> Result<i32, ServiceError> {
    if dry_run {
        say("Dry run: nothing is stopped or removed.");
        print_all(&uninstall::plan(manager));
        return Ok(0);
    }
    ensure_allowed(crate::sandbox::active(), "removing a background service")?;
    let endpoints = Endpoints::from_config(&ProxyConfig::default());
    uninstall::uninstall(&context(manager, &endpoints)).await?;
    say("The log files in the state folder were kept.");
    Ok(0)
}

async fn restart_command(
    manager: &Manager,
    dry_run: bool,
    overrides: CliOverrides,
) -> Result<i32, ServiceError> {
    if dry_run {
        say("Dry run: nothing is restarted.");
        print_all(&restart::plan(manager));
        return Ok(0);
    }
    ensure_allowed(crate::sandbox::active(), "restarting a background service")?;
    let config = load_config(overrides)?;
    let endpoints = Endpoints::from_config(&config);
    restart::restart(&context(manager, &endpoints)).await?;
    say("Checking that the services answer:");
    health::verify(&endpoints, Timing::AFTER_START, &say).await?;
    Ok(0)
}

async fn status_command(manager: &Manager, overrides: CliOverrides) -> Result<i32, ServiceError> {
    let units = status::collect(&SystemRunner, manager).await;
    for unit in &units {
        say(&unit.describe());
    }
    if !units.iter().any(|u| u.presence == Presence::Ours) {
        say("Run `mike service install` to keep mike running in the background.");
        return Ok(1);
    }
    let config = load_config(overrides)?;
    let endpoints = Endpoints::from_config(&config);
    let healthy = match health::verify(&endpoints, Timing::SNAPSHOT, &say).await {
        Ok(()) => true,
        Err(e) => {
            say(&format!("health check: {e}"));
            false
        }
    };
    Ok(i32::from(!(healthy && all_up(&units))))
}

fn all_up(units: &[UnitFacts]) -> bool {
    ServiceKind::ALL
        .iter()
        .all(|kind| units.iter().any(|u| u.kind == *kind && u.is_up()))
}
