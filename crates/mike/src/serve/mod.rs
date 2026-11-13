mod drain;
pub(crate) mod listener_policy;
mod listeners;
mod observability_tasks;
mod shutdown;
mod startup_sequence;
mod tracing_init;
mod workers;

use anyhow::Context;

use drain::{TRACKER_DRAIN_TIMEOUT, drain_tracker};

use crate::config::{self, CliOverrides};
use crate::state::AppState;
use crate::store::persistence::pool_classes::PoolClass;

pub async fn run(overrides: CliOverrides) -> anyhow::Result<()> {
    let mut config = config::load(overrides).context("failed to load configuration")?;
    listener_policy::validate(&config)?;
    let owner = crate::stack::DaemonOwner::acquire_unless_smoke_run(&config)?;
    config.instance_id = owner.identity.instance_id.clone();
    config.boot_generation = owner.identity.generation.to_string();
    let tracing_guards = tracing_init::init_tracing_and_log_startup(&config)?;

    let shutdown = tokio_util::sync::CancellationToken::new();
    let control = owner
        .owns_installation
        .then(|| crate::stack::ControlServer::bind(&owner, shutdown.clone()))
        .transpose()?;
    let state = AppState::new_with_shutdown(config, shutdown.clone())
        .await
        .context("failed to initialize app state")?;
    if shutdown.is_cancelled() {
        let drained = drain_tracker(state.tracker(), TRACKER_DRAIN_TIMEOUT).await;
        tracing_init::flush_otel(tracing_guards.otel_provider);
        return drained.into_result(TRACKER_DRAIN_TIMEOUT);
    }
    let background = state.on_pool_class(PoolClass::Background);
    startup_sequence::run_startup_sequence(&background).await;
    workers::spawn_background_workers_unless_smoke_run(&background);
    let observability = observability_tasks::spawn_observability_tasks(&background);
    let task_tracker = state.tracker();

    let service_token = state.service_token().clone();
    let targets = listeners::bind_and_log(state).await?;
    announce_ownership(control.as_ref(), &service_token, &targets, &owner)?;

    shutdown::serve_until_shutdown(targets).await?;

    log_shutting_down();

    observability_tasks::wait_for_observability_tasks(observability).await;

    let drained = drain_tracker(task_tracker, TRACKER_DRAIN_TIMEOUT).await;

    tracing_init::flush_otel(tracing_guards.otel_provider);

    drained.into_result(TRACKER_DRAIN_TIMEOUT)?;
    log_shutdown_complete();
    Ok(())
}

fn announce_ownership(
    control: Option<&crate::stack::ControlServer>,
    service_token: &crate::service_token::ServiceToken,
    targets: &listeners::ServeTargets,
    owner: &crate::stack::DaemonOwner,
) -> anyhow::Result<()> {
    let Some(control) = control else {
        tracing::info!(
            "smoke-run: not taking installation ownership, the control socket or the service token"
        );
        return Ok(());
    };
    publish_service_token(service_token, targets, &owner.spec.paths.state_dir);
    control.mark_ready(targets.bound_addresses()?)?;
    Ok(())
}

fn publish_service_token(
    token: &crate::service_token::ServiceToken,
    targets: &listeners::ServeTargets,
    state_dir: &std::path::Path,
) {
    match targets.inspect_listener.local_addr() {
        Ok(addr) => {
            crate::service_token::publish_to_state_dir(token, state_dir, addr.port());
        }
        Err(e) => tracing::warn!(error = %e, "service token not published: inspect port unknown"),
    }
}

fn log_shutting_down() {
    tracing::info!("shutting down");
}

fn log_shutdown_complete() {
    tracing::info!("shutdown complete");
}
