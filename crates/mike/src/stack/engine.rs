use std::time::Duration;

use anyhow::{Context, Result};

use crate::config::ProxyConfig;

use super::instance::InstanceSpec;
use super::lease::{FileLease, ownership_held};
use super::listener_reservations::{self, Reservations};
use super::process::LaunchedChild;
use super::protocol::RuntimeStatus;
use super::{EnsureOutcome, StopOutcome, control, health};

const START_DEADLINE: Duration = Duration::from_secs(240);
const STOP_DEADLINE: Duration = Duration::from_secs(65);
const POLL_PAUSE: Duration = Duration::from_millis(100);

pub async fn ensure_up(config: &ProxyConfig) -> Result<EnsureOutcome> {
    let spec = InstanceSpec::from_config(config)?;
    crate::config::write::ensure_private_directory(&spec.paths.state_dir)?;
    let _start = FileLease::acquire_start(&spec.start_lock(), START_DEADLINE).await?;
    ensure_locked(config, &spec, None).await
}

async fn ensure_locked(
    config: &ProxyConfig,
    spec: &InstanceSpec,
    reservations: Option<Reservations>,
) -> Result<EnsureOutcome> {
    if ownership_held(&spec.owner_lock())? {
        wait_for_existing(config, spec).await?;
        return Ok(EnsureOutcome::AlreadyRunning);
    }
    if reservations.is_none() {
        refuse_occupied_listener(config).await?;
    }
    super::preflight::requested_boot(config).await?;
    let log_dir = config.log_dir.as_ref().unwrap_or(&spec.paths.log_dir);
    crate::config::write::ensure_private_directory(log_dir)?;
    let log_path = log_dir.join("mike-serve.log");
    if reservations.is_some() {
        tracing::info!(
            "releasing listener reservations for child bind; OS descriptor handoff is not atomic and another process can race startup"
        );
    }
    drop(reservations);
    let mut child = LaunchedChild::spawn(config, &log_path)?;
    let pid = child.pid();
    match wait_for_child(config, spec, &mut child).await {
        Ok(ChildReadiness::Ready) => {}
        Ok(ChildReadiness::Initializing) => {
            child.release();
            anyhow::bail!(
                "the selected daemon still owns an in-progress initialization; it was left running to preserve migrations; retry readiness or cooperatively stop the installation"
            );
        }
        Err(error) => {
            child.cleanup(spec).await;
            return Err(error)
                .context("selected daemon failed to become ready; inspect its private log");
        }
    }
    child.release();
    Ok(EnsureOutcome::Spawned { pid, log_path })
}

enum ChildReadiness {
    Ready,
    Initializing,
}

async fn wait_for_child(
    config: &ProxyConfig,
    spec: &InstanceSpec,
    child: &mut LaunchedChild,
) -> Result<ChildReadiness> {
    let started = std::time::Instant::now();
    let mut last_status = None;
    loop {
        anyhow::ensure!(
            !child.exited()?,
            "newly launched daemon exited before readiness"
        );
        if let Ok(Some(status)) = control::status(spec, true).await {
            anyhow::ensure!(
                status.identity.pid == child.pid(),
                "private daemon identity does not belong to the child just launched"
            );
            anyhow::ensure!(
                status.matches_requested,
                "new daemon boot configuration does not match the requested installation"
            );
            if !status.ready && (status.stopping || started.elapsed() >= START_DEADLINE) {
                return Ok(ChildReadiness::Initializing);
            }
            require_matching_boot(&status)?;
            if health::verified(config, &status).await {
                return Ok(ChildReadiness::Ready);
            }
            last_status = Some(status);
        }
        if started.elapsed() >= START_DEADLINE
            && super::startup_policy::preserve_initializing(
                last_status.as_ref(),
                child.pid(),
                ownership_held(&spec.owner_lock())?,
            )
        {
            return Ok(ChildReadiness::Initializing);
        }
        anyhow::ensure!(
            started.elapsed() < START_DEADLINE,
            "selected daemon startup deadline expired"
        );
        tokio::time::sleep(POLL_PAUSE).await;
    }
}

async fn wait_for_existing(config: &ProxyConfig, spec: &InstanceSpec) -> Result<RuntimeStatus> {
    let started = std::time::Instant::now();
    loop {
        anyhow::ensure!(
            ownership_held(&spec.owner_lock())?,
            "selected daemon released ownership before becoming ready"
        );
        if let Some(status) = control::status(spec, true).await? {
            require_matching_boot(&status)?;
            if health::verified(config, &status).await {
                return Ok(status);
            }
        }
        anyhow::ensure!(
            started.elapsed() < START_DEADLINE,
            "selected daemon owns the installation but is not ready; stop it cooperatively before retrying"
        );
        tokio::time::sleep(POLL_PAUSE).await;
    }
}

fn require_matching_boot(status: &RuntimeStatus) -> Result<()> {
    anyhow::ensure!(
        status.matches_requested,
        "the selected installation is running a different boot configuration; explicitly stop/restart it to apply the requested mode and settings"
    );
    anyhow::ensure!(
        !status.stopping,
        "selected daemon is draining; retry after stop completes"
    );
    Ok(())
}

async fn refuse_occupied_listener(config: &ProxyConfig) -> Result<()> {
    let addresses = [
        Some(config.proxy_listen_addr.as_str()),
        Some(config.inspect_listen_addr.as_str()),
        config.spa_enabled.then_some(config.spa_bind.as_str()),
    ];
    for address in addresses.into_iter().flatten() {
        let occupied = tokio::time::timeout(
            Duration::from_secs(1),
            tokio::net::TcpStream::connect(address),
        )
        .await;
        anyhow::ensure!(
            !matches!(occupied, Ok(Ok(_))),
            "a configured listener is occupied without matching installation ownership; no existing process was stopped or adopted"
        );
    }
    Ok(())
}

pub async fn stop_stack(config: &ProxyConfig) -> Result<StopOutcome> {
    let spec = InstanceSpec::from_config(config)?;
    if !spec.paths.state_dir.try_exists()? {
        refuse_occupied_listener(config).await?;
        return Ok(StopOutcome::NotRunning);
    }
    crate::config::write::ensure_private_directory(&spec.paths.state_dir)?;
    let _start = FileLease::acquire_start(&spec.start_lock(), START_DEADLINE).await?;
    stop_locked(config, &spec).await
}

async fn stop_locked(config: &ProxyConfig, spec: &InstanceSpec) -> Result<StopOutcome> {
    if !ownership_held(&spec.owner_lock())? {
        refuse_occupied_listener(config).await?;
        return Ok(StopOutcome::NotRunning);
    }
    let status = control::status(spec, false).await?.context("selected daemon has ownership but no verified control connection; no process was signalled")?;
    stop_verified(spec, &status).await
}

async fn stop_verified(spec: &InstanceSpec, status: &RuntimeStatus) -> Result<StopOutcome> {
    super::instance::verify_owner(spec, &status.identity)?;
    let stopped = control::stop(spec, &status.identity).await?;
    anyhow::ensure!(
        stopped.identity == status.identity && stopped.stopping,
        "daemon did not acknowledge this generation's cooperative stop"
    );
    let start = std::time::Instant::now();
    while ownership_held(&spec.owner_lock())? {
        if let Ok(Some(current)) = control::status(spec, false).await
            && current.identity.generation != status.identity.generation
        {
            break;
        }
        anyhow::ensure!(
            start.elapsed() < STOP_DEADLINE,
            "selected daemon has not finished draining; it remains owned and no force-stop was attempted"
        );
        tokio::time::sleep(POLL_PAUSE).await;
    }
    Ok(StopOutcome::Stopped {
        pid: status.identity.pid,
    })
}

pub async fn restart_stack(config: &ProxyConfig) -> Result<EnsureOutcome> {
    let spec = InstanceSpec::from_config(config)?;
    crate::config::write::ensure_private_directory(&spec.paths.state_dir)?;
    let _start = FileLease::acquire_start(&spec.start_lock(), START_DEADLINE).await?;
    super::preflight::after_preflight(super::preflight::requested_boot(config), || async {
        let status = current_owner(&spec).await?;
        let reservations = listener_reservations::reserve_replacement(config, status.as_ref())?;
        if let Some(status) = status {
            stop_verified(&spec, &status).await?;
        }
        ensure_locked(config, &spec, Some(reservations)).await
    })
    .await
}

pub async fn reconcile(config: &ProxyConfig) -> Result<EnsureOutcome> {
    let spec = InstanceSpec::from_config(config)?;
    crate::config::write::ensure_private_directory(&spec.paths.state_dir)?;
    let _start = FileLease::acquire_start(&spec.start_lock(), START_DEADLINE).await?;
    super::preflight::after_preflight(super::preflight::requested_boot(config), || async {
        let mut reservations = None;
        if let Some(status) = current_owner(&spec).await?
            && !status.matches_requested
        {
            reservations = Some(listener_reservations::reserve_replacement(
                config,
                Some(&status),
            )?);
            stop_verified(&spec, &status).await?;
        }
        ensure_locked(config, &spec, reservations).await
    })
    .await
}

async fn current_owner(spec: &InstanceSpec) -> Result<Option<RuntimeStatus>> {
    if !ownership_held(&spec.owner_lock())? {
        return Ok(None);
    }
    let status = control::status(spec, true).await?.context(
        "owned installation has no verified control connection; existing daemon was preserved",
    )?;
    Ok(Some(status))
}

pub async fn probe_selected(config: &ProxyConfig) -> bool {
    probe(config, true).await
}

pub(super) async fn serving_any_boot(config: &ProxyConfig) -> bool {
    probe(config, false).await
}

async fn probe(config: &ProxyConfig, compare_boot: bool) -> bool {
    let Ok(spec) = InstanceSpec::from_config(config) else {
        return false;
    };
    if !ownership_held(&spec.owner_lock()).unwrap_or(false) {
        return false;
    }
    let Ok(Some(status)) = control::status(&spec, compare_boot).await else {
        return false;
    };
    health::verified(config, &status).await
}
