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
        return tracing::debug!("wright: engineer worker not spawned: no persistence");
    }
    let owned = state.clone();
    state.tracker.spawn(run(owned));
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
        if !state.feature_toggle_overlay.load().wright {
            continue;
        }
        let Some(persist) = state.persist() else {
            return;
        };
        if !persist.holds_worker_role(LEASE_ROLE, TICK).await {
            continue;
        }
        match EngineerContext::from_state(&state).await {
            Ok(ctx) => {
                last_unavailable = None;
                tick(&ctx).await;
            }
            Err(reason) if last_unavailable.as_ref() != Some(&reason) => {
                tracing::warn!(error = %reason, "wright: the engineer cannot run on this machine");
                last_unavailable = Some(reason);
            }
            Err(_) => {}
        }
    }
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
    let out = outcome.out_dir.display().to_string();
    let end = format!("{:?}", outcome.end);
    match &outcome.output {
        Ok(_) => tracing::info!(
            investigation = outcome.investigation_id,
            finding = outcome.finding_id,
            session_id = %outcome.session_uuid,
            %out,
            %end,
            "wright: engineer run recorded a finding"
        ),
        Err(reason) => tracing::warn!(
            investigation = outcome.investigation_id,
            error = %reason,
            session_id = %outcome.session_uuid,
            %out,
            %end,
            "wright: engineer run ended without a finding"
        ),
    }
}

fn log_refused(refused: &StartRefused) {
    match refused {
        StartRefused::Launch { .. } | StartRefused::Unavailable(_) => {
            tracing::warn!(%refused, "wright: engineer run not started");
        }
        _ => tracing::debug!(%refused, "wright: engineer run not started"),
    }
}

pub(crate) async fn tick(ctx: &EngineerContext) -> Option<RunOutcome> {
    if let Err(e) = reconcile::reconcile(&ctx.persist, &ctx.machine).await {
        tracing::warn!(error = %e, "wright: engineer runs could not be reconciled");
        return None;
    }
    let pick = match ctx
        .persist
        .web()
        .pick_wright_problem(MAX_ATTEMPTS_PER_EPISODE)
        .await
    {
        Ok(Some(pick)) => pick,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!(error = %e, "wright: the next problem could not be picked");
            return None;
        }
    };
    let request = match request_for(&ctx.persist, pick).await {
        Ok(Some(request)) => request,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!(error = %e, "wright: the picked problem could not be read");
            return None;
        }
    };
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
