mod control;
mod engine;
mod health;
mod instance;
mod lease;
mod listener_reservations;
mod preflight;
mod process;
mod protocol;
mod socket_path;
mod startup_policy;
mod summary;
mod supervisor;

use std::path::PathBuf;

pub(crate) use control::ControlServer;
pub use engine::{probe_selected, stop_stack};
pub(crate) use instance::DaemonOwner;
pub use summary::render_stack_summary;

#[derive(Debug)]
pub enum EnsureOutcome {
    AlreadyRunning,
    Spawned { pid: u32, log_path: PathBuf },
}

#[derive(Debug)]
pub enum StopOutcome {
    NotRunning,
    Stopped { pid: u32 },
}

pub async fn ensure_up(config: &crate::config::ProxyConfig) -> anyhow::Result<EnsureOutcome> {
    if let Some(supervised) = selected_supervisor(config) {
        return ensure_supervised(config, supervised).await;
    }
    engine::ensure_up(config).await
}

pub async fn ensure_serving(config: &crate::config::ProxyConfig) -> anyhow::Result<EnsureOutcome> {
    if selected_supervisor(config).is_none() && engine::serving_any_boot(config).await {
        return Ok(EnsureOutcome::AlreadyRunning);
    }
    ensure_up(config).await
}

fn selected_supervisor(config: &crate::config::ProxyConfig) -> Option<supervisor::Supervised> {
    crate::config::paths::InstallationPaths::resolve(None)
        .is_ok_and(|default| config.config_path == default.config_file)
        .then(|| supervisor::supervised_by(&config.inspect_listen_addr))
        .flatten()
}

pub async fn reconcile(config: &crate::config::ProxyConfig) -> anyhow::Result<EnsureOutcome> {
    if let Some(supervised) = selected_supervisor(config) {
        if probe_selected(config).await {
            return Ok(EnsureOutcome::AlreadyRunning);
        }
        return restart_supervised(config, supervised).await;
    }
    engine::reconcile(config).await
}

pub async fn restart_stack(config: &crate::config::ProxyConfig) -> anyhow::Result<EnsureOutcome> {
    if let Some(supervised) = selected_supervisor(config) {
        return restart_supervised(config, supervised).await;
    }
    engine::restart_stack(config).await
}

async fn restart_supervised(
    config: &crate::config::ProxyConfig,
    supervised: supervisor::Supervised,
) -> anyhow::Result<EnsureOutcome> {
    preflight::requested_boot(config).await?;
    supervised.restart(config).await?;
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(90) {
        if probe_selected(config).await {
            return Ok(EnsureOutcome::AlreadyRunning);
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    anyhow::bail!(
        "the supervised server did not verify the selected configuration after restart; inspect `mike service status` and its configuration overrides"
    )
}

async fn ensure_supervised(
    config: &crate::config::ProxyConfig,
    supervised: supervisor::Supervised,
) -> anyhow::Result<EnsureOutcome> {
    if health::probe(&config.inspect_listen_addr).await {
        return Ok(EnsureOutcome::AlreadyRunning);
    }
    supervised.start().await;
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(90) {
        if health::probe(&config.inspect_listen_addr).await {
            return Ok(EnsureOutcome::AlreadyRunning);
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    anyhow::bail!(supervised.not_answering(&config.inspect_listen_addr, 90))
}

pub fn running_process(
    config: &crate::config::ProxyConfig,
) -> Option<(u32, std::time::SystemTime)> {
    instance::running_process(config)
}

pub async fn probe_health(inspect_addr: &str) -> bool {
    health::probe(inspect_addr).await
}

#[cfg(test)]
#[path = "safety_tests.rs"]
mod tests;
