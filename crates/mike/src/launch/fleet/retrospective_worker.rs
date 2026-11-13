use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::time::{MissedTickBehavior, interval};

use super::definition::FleetDefinition;
use super::retrospective::{Job, Phase, save};
use crate::circuit_breaker::CircuitBreaker;
use crate::launch::LaunchError;
use crate::state::AppState;
use crate::store::persistence::fleet::FleetRow;
use crate::store::persistence::launch::{LaunchListFilter, LaunchRow};
use crate::store::persistence::{PersistError, PersistHandle};

const POLL_SECONDS: i64 = 20;
const LEASE_SECONDS: i64 = 180;
const MAX_ATTEMPTS: u32 = 3;

pub(super) fn spawn(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        return;
    };
    let state = state.clone();
    state.tracker.clone().spawn(async move {
        let shutdown = state.shutdown();
        let mut ticker = interval(Duration::from_secs(20));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut breaker = CircuitBreaker::new(3, Duration::from_secs(60), Duration::from_secs(900));
        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                _ = ticker.tick() => tick(&state, &persist, &mut breaker).await,
            }
        }
    });
}

async fn tick(state: &AppState, persist: &PersistHandle, breaker: &mut CircuitBreaker) {
    super::rule_audit::close_finished(state, persist).await;
    if breaker.should_skip(Instant::now()) {
        return;
    }
    let rows = match persist.fleet().pending_fleet_retrospectives().await {
        Ok(rows) => rows,
        Err(err) => {
            tracing::warn!(error = %err, "fleet retrospective queue could not be read");
            return;
        }
    };
    for row in rows {
        if breaker.should_skip(Instant::now()) {
            break;
        }
        process_owned(state, persist, &row, breaker).await;
    }
}

async fn process_owned(
    state: &AppState,
    persist: &PersistHandle,
    row: &FleetRow,
    breaker: &mut CircuitBreaker,
) {
    let owner = row
        .definition
        .pointer("/retrospective_job/host")
        .and_then(serde_json::Value::as_str);
    let host = super::retrospective::local_host();
    if owner.is_some_and(|owner| !owner.is_empty() && owner != host) {
        return;
    }
    if host.is_empty() && !super::here::checkout_present(&row.project_path) {
        return;
    }
    if let Err(err) = process(state, persist, row, breaker).await {
        tracing::warn!(fleet = %row.slug, error = %err, "fleet retrospective remains pending");
    }
}

pub(super) fn observe(job: &mut Job, launch: Option<&LaunchRow>, alive: impl Fn(i32) -> bool) {
    let Some(launch) = launch else { return };
    let live = launch.pid.is_some_and(&alive);
    if live || (launch.opened && launch.closed_at.is_none() && launch.pid.is_none()) {
        job.phase = Phase::Running;
        job.problem = None;
    } else if launch.opened {
        job.phase = Phase::Cleanup;
    } else {
        job.phase = Phase::Blocked;
        job.problem = Some("launch outcome is uncertain; inspect the recorded session before retrying or removing its checkout".to_string());
    }
}

pub(super) fn failed(job: &mut Job, problem: String, transient: bool) {
    job.problem = Some(problem);
    job.phase = match (transient, job.attempts < MAX_ATTEMPTS) {
        (true, true) => Phase::Pending,
        (true, false) => Phase::Blocked,
        (false, _) => Phase::Cleanup,
    };
    job.next_at = Utc::now() + chrono::Duration::seconds(60 * 2_i64.pow(job.attempts.min(6)));
}

async fn process(
    state: &AppState,
    persist: &PersistHandle,
    row: &FleetRow,
    breaker: &mut CircuitBreaker,
) -> Result<(), PersistError> {
    let def = FleetDefinition::from_value(&row.definition).map_err(PersistError::DataInvariant)?;
    let Some(mut job) = def.retrospective_job else {
        return Ok(());
    };
    if job.next_at > Utc::now() {
        return Ok(());
    }
    if job.phase == Phase::Cleanup {
        return cleanup(persist, row, job).await;
    }
    let launch = persist.launch().find_launch_by_session(job.session).await?;
    if launch.is_some() {
        observe(
            &mut job,
            launch.as_ref(),
            crate::workspace_reaper::is_process_alive,
        );
        job.next_at = Utc::now() + chrono::Duration::seconds(POLL_SECONDS);
        save(persist, row, &job).await?;
        return Ok(());
    }
    begin(state, persist, row, job, breaker).await
}

async fn begin(
    state: &AppState,
    persist: &PersistHandle,
    row: &FleetRow,
    mut job: Job,
    breaker: &mut CircuitBreaker,
) -> Result<(), PersistError> {
    if job.attempts >= MAX_ATTEMPTS {
        failed(
            &mut job,
            "retrospective retry budget exhausted".to_string(),
            true,
        );
        save(persist, row, &job).await?;
        return Ok(());
    }
    let def =
        FleetDefinition::from_value(&job.source_definition).map_err(PersistError::DataInvariant)?;
    if def.retrospective.write.is_some()
        && let Some(problem) = write_blocker(persist, row, &job, &def).await?
    {
        job.problem = Some(problem);
        job.next_at = Utc::now() + chrono::Duration::minutes(5);
        save(persist, row, &job).await?;
        return Ok(());
    }
    job.phase = Phase::Launching;
    job.attempts += 1;
    job.next_at = Utc::now() + chrono::Duration::seconds(LEASE_SECONDS);
    let Some(claimed) = save(persist, row, &job).await? else {
        return Ok(());
    };
    let outcome = tokio::time::timeout(
        Duration::from_secs(120),
        super::retrospective_launch::launch(state, persist, row, &job),
    )
    .await;
    match outcome {
        Ok(Ok(())) => {
            breaker.record(true, Instant::now());
            job.problem = None;
            save(persist, &claimed, &job).await?;
        }
        Ok(Err(err)) => {
            breaker.record(false, Instant::now());
            failed(&mut job, err.to_string(), transient(&err));
            save(persist, &claimed, &job).await?;
        }
        Err(_) => {
            breaker.record(false, Instant::now());
            job.problem =
                Some("launch timed out; checking the durable launch before any retry".to_string());
            save(persist, &claimed, &job).await?;
        }
    }
    Ok(())
}

fn transient(err: &LaunchError) -> bool {
    matches!(
        err,
        LaunchError::Io { .. }
            | LaunchError::Timeout { .. }
            | LaunchError::BackendResponse { .. }
            | LaunchError::BackendUnavailable { .. }
            | LaunchError::Failed(_)
    )
}

async fn cleanup(
    persist: &PersistHandle,
    row: &FleetRow,
    mut job: Job,
) -> Result<(), PersistError> {
    let def =
        FleetDefinition::from_value(&job.source_definition).map_err(PersistError::DataInvariant)?;
    if let Some(launch) = persist.launch().find_launch_by_session(job.session).await?
        && !safe_to_cleanup(&launch, crate::workspace_reaper::is_process_alive)
    {
        job.phase = Phase::Blocked;
        job.problem =
            Some("retrospective process may still be alive; checkout retained".to_string());
        save(persist, row, &job).await?;
        return Ok(());
    }
    persist.launch().close_run(&job.run_id).await?;
    super::cleanup::after_close(persist, row, &def, &job.source_run).await;
    job.phase = if job.problem.is_some() {
        Phase::Failed
    } else {
        Phase::Finished
    };
    save(persist, row, &job).await?;
    Ok(())
}

pub(super) fn safe_to_cleanup(launch: &LaunchRow, alive: impl Fn(i32) -> bool) -> bool {
    match launch.pid {
        Some(pid) => !alive(pid),
        None => launch.closed_at.is_some() && launch.close_process_exited == Some(true),
    }
}

async fn write_blocker(
    persist: &PersistHandle,
    row: &FleetRow,
    job: &Job,
    def: &FleetDefinition,
) -> Result<Option<String>, PersistError> {
    let fleets = persist.fleet().fleet_checkout_users().await?;
    if let Some(other) = super::cleanup::other_user_of(&def.repo, row.fleet_uuid, &fleets) {
        return Ok(Some(format!(
            "write-capable retrospective waits for fleet {other} to release its checkout"
        )));
    }
    let launches = persist
        .launch()
        .list_launches(&LaunchListFilter {
            run_id: Some(job.source_run.clone()),
            open_only: false,
            backend: None,
            limit: i64::MAX,
        })
        .await?;
    let still_live = launches.iter().any(|launch| {
        launch
            .pid
            .is_some_and(crate::workspace_reaper::is_process_alive)
            || (launch.opened && launch.pid.is_none() && launch.closed_at.is_none())
    });
    Ok(still_live.then(|| {
        "write-capable retrospective waits for the source run's processes to exit".to_string()
    }))
}
