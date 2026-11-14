use crate::store::persistence::web_repo::WebPersist as _;
use std::time::Duration;

use super::brief::{EVIDENCE_INPUT, evidence, investigate_brief};
use super::{EngineerContext, RunMode, RunOutcome, RunRequest, StartRefused, reconcile};
use crate::state::AppState;
use crate::store::persistence::wright::Pick;
use crate::store::persistence::{PersistError, PersistHandle};

const LEASE_ROLE: &str = "wright_engineer";
const TICK: Duration = Duration::from_secs(60);
pub(crate) const MAX_ATTEMPTS_PER_EPISODE: i64 = 2;

pub fn spawn_engineer_worker(state: &AppState) {
    if state.persist().is_none() {
        return log_not_spawned();
    }
    let owned = state.clone();
    state.tracker.spawn(run(owned));
    log_spawned();
}

fn log_not_spawned() {
    tracing::debug!("wright: engineer worker not spawned: no persistence");
}

fn log_spawned() {
    tracing::info!(
        tick_secs = TICK.as_secs(),
        "wright: engineer worker spawned; it idles while the wright toggle is off"
    );
}

async fn run(state: AppState) {
    let shutdown = state.shutdown();
    let mut last_unavailable: Option<String> = None;
    loop {
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(TICK) => {}
        }
        if !pass_once(&state, &mut last_unavailable).await {
            return;
        }
    }
}

async fn pass_once(state: &AppState, last_unavailable: &mut Option<String>) -> bool {
    if !state.feature_toggle_overlay.load().wright {
        return true;
    }
    let Some(persist) = state.persist() else {
        return false;
    };
    if !persist.holds_worker_role(LEASE_ROLE, TICK).await {
        return true;
    }
    match EngineerContext::from_state(state).await {
        Ok(ctx) => {
            *last_unavailable = None;
            tick(&ctx).await;
        }
        Err(reason) => note_unavailable(last_unavailable, reason),
    }
    true
}

fn note_unavailable(last_unavailable: &mut Option<String>, reason: String) {
    if last_unavailable.as_ref() == Some(&reason) {
        return;
    }
    log_unavailable(&reason);
    *last_unavailable = Some(reason);
}

fn log_unavailable(reason: &str) {
    tracing::warn!(error = %reason, "wright: the engineer cannot run on this machine");
}

pub(crate) async fn request_for(
    persist: &PersistHandle,
    pick: Pick,
) -> Result<Option<RunRequest>, PersistError> {
    let Some(problem) = persist.web().wright_problem(pick.problem_id).await? else {
        return Ok(None);
    };
    let Some(episode) = persist.web().wright_episode(pick.episode_id).await? else {
        return Ok(None);
    };
    Ok(Some(RunRequest {
        mode: RunMode::Investigate,
        problem_id: Some(problem.id),
        brief: investigate_brief(&problem, &episode),
        inputs: vec![(EVIDENCE_INPUT.to_owned(), evidence(&problem, &episode))],
    }))
}

fn log_outcome(outcome: &RunOutcome) {
    match &outcome.output {
        Ok(_) => log_finding(outcome),
        Err(reason) => log_no_finding(outcome, reason),
    }
}

fn log_finding(outcome: &RunOutcome) {
    let out = outcome.out_dir.display().to_string();
    let end = format!("{:?}", outcome.end);
    tracing::info!(
        investigation = outcome.investigation_id,
        finding = outcome.finding_id,
        session_id = %outcome.session_uuid,
        %out,
        %end,
        "wright: engineer run recorded a finding"
    );
}

fn log_no_finding(outcome: &RunOutcome, reason: &str) {
    let out = outcome.out_dir.display().to_string();
    let end = format!("{:?}", outcome.end);
    tracing::warn!(
        investigation = outcome.investigation_id,
        error = %reason,
        session_id = %outcome.session_uuid,
        %out,
        %end,
        "wright: engineer run ended without a finding"
    );
}

fn log_refused(refused: &StartRefused) {
    match refused {
        StartRefused::Launch { .. } | StartRefused::Unavailable(_) => log_refused_warn(refused),
        StartRefused::DailyLimit { .. } | StartRefused::Disabled => log_refused_info(refused),
        _ => log_refused_debug(refused),
    }
}

fn log_refused_warn(refused: &StartRefused) {
    tracing::warn!(%refused, "wright: engineer run not started");
}

fn log_refused_info(refused: &StartRefused) {
    tracing::info!(%refused, "wright: engineer run not started");
}

fn log_refused_debug(refused: &StartRefused) {
    tracing::debug!(%refused, "wright: engineer run not started");
}

fn log_reconcile_failed(error: &PersistError) {
    tracing::warn!(error = %error, "wright: engineer runs could not be reconciled");
}

fn log_pick_failed(error: &PersistError) {
    tracing::warn!(error = %error, "wright: the next problem could not be picked");
}

fn log_read_failed(error: &PersistError) {
    tracing::warn!(error = %error, "wright: the picked problem could not be read");
}

async fn next_pick(ctx: &EngineerContext) -> Option<Pick> {
    match ctx
        .persist
        .web()
        .pick_wright_problem(MAX_ATTEMPTS_PER_EPISODE)
        .await
    {
        Ok(pick) => pick,
        Err(e) => {
            log_pick_failed(&e);
            None
        }
    }
}

async fn read_request(ctx: &EngineerContext, pick: Pick) -> Option<RunRequest> {
    match request_for(&ctx.persist, pick).await {
        Ok(request) => request,
        Err(e) => {
            log_read_failed(&e);
            None
        }
    }
}

pub(crate) async fn tick(ctx: &EngineerContext) -> Option<RunOutcome> {
    if let Err(e) = reconcile::reconcile(&ctx.persist, &ctx.machine).await {
        log_reconcile_failed(&e);
        return None;
    }
    let pick = next_pick(ctx).await?;
    let request = read_request(ctx, pick).await?;
    run_request(ctx, request).await
}

async fn run_request(ctx: &EngineerContext, request: RunRequest) -> Option<RunOutcome> {
    match super::start_run(ctx, request).await {
        Ok(handle) => {
            let outcome = super::wait(handle).await;
            log_outcome(&outcome);
            Some(outcome)
        }
        Err(refused) => {
            log_refused(&refused);
            None
        }
    }
}
