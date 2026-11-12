mod args;
mod brief_delivery;
mod channel;
mod connect;
mod delivery;
mod doctor;
mod fleet_settings;
mod folder;
mod fork;
mod kill;
mod knowledge_location;
mod ledger;
pub(crate) mod ledger_wire;
mod list;
mod mcp_preflight;
mod open;
mod open_gates;
mod open_outcome;
mod open_record;
mod pinned_launch;
mod prompt_turn;
mod proxy_env;
mod proxy_ledger;
mod queue;
mod queue_replay;
mod reap;
mod registration;
mod render;
mod run_seat;
mod runner;
mod seat_health;
mod server;
mod sidecar;
mod startup;
mod templates;
#[cfg(any(test, feature = "test-seams"))]
pub mod test_support;
#[cfg(test)]
mod tests;
mod warm;

pub use args::{
    DoctorArgs, ForkArgs, KillArgs, ListArgs, OpenArgs, ReapArgs, SpawnArgs, SpawnCommand,
    TemplatesArgs, TemplatesCommand, WarmArgs,
};
pub(crate) use doctor::DoctorReport;
pub(crate) use kill::KillReport;
pub(crate) use proxy_env::{ca_cert_candidates, resolve_ca_cert, resolve_proxy_env};
pub(crate) use server::{
    ServerOpenRequest, ServerOpenStart, doctor_for_server, kill_for_server, open_for_server,
    open_unwatched,
};
pub(crate) use sidecar::{locate_executable, sidecar_reachable};
pub(crate) use startup::spawn_queue_replay;

use connect::Reach;
use ledger::Ledger;
use runner::Runner;

use crate::config::{CliOverrides, ProxyConfig};
use crate::launch::backend_choice::{self, CallerTerminal};
use crate::launch::{BackendId, LaunchError, TerminalBackend, TrustGate, brief};

pub async fn run(args: SpawnArgs, overrides: CliOverrides) -> i32 {
    match dispatch(args, overrides).await {
        Ok(code) => code,
        Err(error) => {
            render::report_error(&error);
            error.exit_code()
        }
    }
}

async fn dispatch(args: SpawnArgs, overrides: CliOverrides) -> Result<i32, LaunchError> {
    if let SpawnCommand::Reap(reap_args) = &args.command {
        let report = reap::execute(reap_args.yes).await?;
        return Ok(render::print_reap(&report, reap_args.json));
    }

    let file = pinned_launch::named_file(&overrides);
    let pinned = pinned_launch::PinnedLaunch::from_sources(
        &overrides,
        crate::config::env_text,
        file.as_ref(),
    );
    let explicit: Option<BackendId> = pinned.asked.as_deref().map(str::parse).transpose()?;
    let mut config = crate::config::load(overrides)
        .map_err(|e| LaunchError::Failed(format!("failed to load configuration: {e:#}")))?;
    let reach = connect::reach(&config).await;
    connect::fold_settings(&mut config, &reach).await;
    pinned.apply(&mut config.launch);
    let caller = CallerTerminal::from_env();
    let (choice, backend) = backend_choice::resolve(&config.launch, explicit, &caller).await?;
    tracing::debug!(backend = %choice.id, reason = %choice.reason, "mike spawn: chose the terminal backend");
    run_command(
        &config,
        backend.as_ref(),
        reach,
        args.command,
        &choice.reason,
    )
    .await
}

async fn run_command(
    config: &ProxyConfig,
    backend: &dyn TerminalBackend,
    reach: Reach,
    command: SpawnCommand,
    reason: &str,
) -> Result<i32, LaunchError> {
    match command {
        SpawnCommand::Open(open) => run_open(config, backend, reach, open.tokens, reason).await,
        SpawnCommand::Kill(kill) => {
            let runner = runner_for(config, backend, reach).await?;
            let report = kill::execute(&runner, kill.session).await?;
            Ok(render::print_kill(&report, kill.json))
        }
        SpawnCommand::List(list) => {
            let runner = runner_for(config, backend, reach).await?;
            let rows = list::execute(&runner, list.run.as_deref()).await?;
            Ok(render::print_list(&rows, list.json))
        }
        SpawnCommand::Doctor(doctor) => {
            let runner = runner_for(config, backend, reach).await?;
            let env = doctor_env(config)?;
            let report = doctor::execute(&runner, &env).await;
            Ok(render::print_doctor(&report, doctor.json))
        }
        SpawnCommand::Warm(warm) => run_warm(config, backend, reach, warm.tokens, reason).await,
        SpawnCommand::Fork(fork) => run_fork(config, backend, reach, fork.tokens, reason).await,
        SpawnCommand::Templates(templates_args) => {
            run_templates(config, backend, reach, templates_args).await
        }
        SpawnCommand::Reap(_) => unreachable!("handled before backend resolution above"),
    }
}

async fn launch_runner<'a>(
    config: &'a ProxyConfig,
    backend: &'a dyn TerminalBackend,
    reach: Reach,
    tokens: &[String],
    reason: &str,
) -> Result<Runner<'a>, LaunchError> {
    let json = tokens
        .iter()
        .take_while(|t| *t != "--")
        .any(|t| *t == "--json");
    if backend.id() == BackendId::None {
        return build_runner(config, backend, Ledger::none());
    }
    let outcome = async {
        crate::launch::stack_starter::starter()?
            .ensure_serving(config)
            .await
    }
    .await
    .map_err(|e| LaunchError::Failed(format!("mike could not use the proxy stack: {e:#}")))?;
    if !json {
        eprintln!("mike spawn: {}", stack_line(config, &outcome));
        eprintln!("mike spawn: opening in {} because {reason}", backend.id());
    }
    let mut runner = runner_for(config, backend, reach).await?;
    runner.backend_reason = Some(reason.to_string());
    Ok(runner)
}

async fn run_warm(
    config: &ProxyConfig,
    backend: &dyn TerminalBackend,
    reach: Reach,
    tokens: Vec<String>,
    reason: &str,
) -> Result<i32, LaunchError> {
    let json = tokens
        .iter()
        .take_while(|t| *t != "--")
        .any(|t| t == "--json");
    let runner = launch_runner(config, backend, reach, &tokens, reason).await?;
    let outcome = warm::execute(&runner, tokens, &mut std::io::stdin()).await;
    Ok(render::print_warm(&outcome, json))
}

async fn run_fork(
    config: &ProxyConfig,
    backend: &dyn TerminalBackend,
    reach: Reach,
    tokens: Vec<String>,
    reason: &str,
) -> Result<i32, LaunchError> {
    let json = tokens
        .iter()
        .take_while(|t| *t != "--")
        .any(|t| t == "--json");
    let runner = launch_runner(config, backend, reach, &tokens, reason).await?;
    let result = fork::execute(&runner, tokens, &mut std::io::stdin()).await;
    Ok(render::print_open(&result, json))
}

async fn run_templates(
    config: &ProxyConfig,
    backend: &dyn TerminalBackend,
    reach: Reach,
    args: TemplatesArgs,
) -> Result<i32, LaunchError> {
    let runner = runner_for(config, backend, reach).await?;
    match args.command {
        None => {
            let rows = templates::list(&runner).await?;
            Ok(render::print_templates_list(&rows, args.json))
        }
        Some(TemplatesCommand::Show(show)) => {
            let row = templates::show(&runner, &show.name).await?;
            Ok(render::print_templates_show(&row, show.json))
        }
        Some(TemplatesCommand::Delete(delete)) => {
            let closed = templates::delete(&runner, &delete.name, delete.yes).await?;
            Ok(render::print_templates_delete(&delete.name, closed))
        }
    }
}

async fn run_open(
    config: &ProxyConfig,
    backend: &dyn TerminalBackend,
    reach: Reach,
    tokens: Vec<String>,
    reason: &str,
) -> Result<i32, LaunchError> {
    let json = tokens
        .iter()
        .take_while(|t| *t != "--")
        .any(|t| t == "--json");
    let runner = launch_runner(config, backend, reach, &tokens, reason).await?;
    let result = open::execute(&runner, tokens, &mut std::io::stdin()).await;
    Ok(render::print_open(&result, json))
}

fn stack_line(
    config: &ProxyConfig,
    outcome: &crate::launch::stack_starter::EnsureOutcome,
) -> String {
    let did = match outcome {
        crate::launch::stack_starter::EnsureOutcome::AlreadyRunning => {
            "reusing the running proxy".to_string()
        }
        crate::launch::stack_starter::EnsureOutcome::Spawned { pid, log_path } => {
            format!("started the proxy (pid {pid}, log {})", log_path.display())
        }
    };
    format!(
        "{did}; proxy http://{}/v1, inspect http://{}/api",
        config.proxy_listen_addr, config.inspect_listen_addr
    )
}

async fn runner_for<'a>(
    config: &'a ProxyConfig,
    backend: &'a dyn TerminalBackend,
    reach: Reach,
) -> Result<Runner<'a>, LaunchError> {
    let (ledger, _replay) = connect::ledger(config, reach).await;
    let runner = build_runner(config, backend, ledger)?;
    Ok(runner)
}

fn cwd() -> Option<std::path::PathBuf> {
    std::env::current_dir().ok()
}

fn build_runner<'a>(
    config: &'a ProxyConfig,
    backend: &'a dyn TerminalBackend,
    ledger: Ledger,
) -> Result<Runner<'a>, LaunchError> {
    let cwd = cwd();
    Ok(Runner {
        backend,
        ledger,
        config: &config.launch,
        proxy: resolve_proxy_env(&config.launch, cwd.as_deref()),
        trust: TrustGate::from_home()?,
        briefs_dir: brief::default_briefs_dir()?,
        inspect_addr: Some(config.inspect_listen_addr.clone()),
        mcp_server: mcp_preflight::find_server(),
        backend_reason: None,
    })
}

fn doctor_env(config: &ProxyConfig) -> Result<doctor::DoctorEnv, LaunchError> {
    let cwd = cwd();
    Ok(doctor::DoctorEnv {
        sidecar_url: config.launch.proxy_forward_url.clone(),
        inspect_addr: config.inspect_listen_addr.clone(),
        ca_candidates: proxy_env::ca_cert_candidates(&config.launch, cwd.as_deref()),
        claude_bin: crate::launch::claude_bin::claude_binary(),
    })
}
