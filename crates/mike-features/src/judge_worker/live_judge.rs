use crate::store::persistence::eval_repo::EvalPersist as _;
use std::ops::ControlFlow;
use std::time::Instant;

use chrono::Utc;

use crate::analysis::judge::{JudgeFailure, JudgeOutcome};
use crate::circuit_breaker::CircuitBreaker;
use crate::llm::OpenRouterClient;
use crate::llm::limiter::Reservation;
use crate::logged::ok_or_warn;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_bundles::StatusChange;
use crate::store::persistence::judge_ownership::JudgeOwnership;
use crate::store::persistence::operator_turns::projection::{
    ProjectionMode, ProjectionParams, PromoteOutcome,
};
use crate::store::persistence::operator_turns::{LiveClaim, LiveFailure};
use crate::store::persistence::turn_verdicts::VerdictMode;

use super::live_shared::LiveJudgeShared;
use super::turn_sweep::{SweepReport, attribute_turn, reserve_claim_batch};
use super::v2_judge::{V2InsertTarget, V2JudgeConfig, context_from_row, judge_turn_v2};

const ANNOUNCE_SOURCE: &str = r#"{"actor":"judge_worker","reason":"turn_judge.live_bundle_id"}"#;

pub struct LiveBundle {
    pub id: i64,
    pub status: String,
    pub cfg: V2JudgeConfig,
}

pub struct LiveJudge<'a> {
    pub persist: &'a PersistHandle,
    pub llm: &'a OpenRouterClient,
    pub shared: &'a LiveJudgeShared,
    pub bundle: &'a LiveBundle,
    pub threshold: f32,
    pub session_judge_bundle_id: i64,
    pub backoff_base_secs: i64,
    pub backoff_max_secs: i64,
    pub jev: Option<crate::llm::decisions::point::Decider>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JudgeRun {
    pub promoted: usize,
    pub stopped: bool,
}

impl LiveJudge<'_> {
    pub fn params(&self, mode: ProjectionMode) -> ProjectionParams {
        ProjectionParams {
            live_bundle_id: self.bundle.id,
            confidence_threshold: self.threshold,
            mode,
            session_judge_live_bundle_id: self.session_judge_bundle_id,
        }
    }
}

pub async fn load_live_bundle(persist: &PersistHandle, bundle_id: i64) -> Option<LiveBundle> {
    let Some(row) = ok_or_warn(
        persist.eval().get_bundle(bundle_id).await,
        "live_judge.read_bundle",
    )?
    else {
        warn_missing_bundle(bundle_id);
        return None;
    };
    let mut cfg = ok_or_warn(
        V2JudgeConfig::from_bundle_components(&row.components),
        "live_judge.bundle_components",
    )?;
    cfg.model =
        crate::llm::provider::resolve(Some(crate::llm::provider::ModelTier::Standard), &cfg.model);
    Some(LiveBundle {
        id: row.id,
        status: row.status,
        cfg,
    })
}

fn warn_missing_bundle(bundle_id: i64) {
    tracing::warn!(
        bundle_id,
        "live judge: turn_judge.live_bundle_id names no eval.bundles row; not judging"
    );
}

pub async fn announce_bundle(
    persist: &PersistHandle,
    shared: &LiveJudgeShared,
    bundle: &LiveBundle,
) {
    if !shared.first_sight_of_bundle(bundle.id) || bundle.status == "live" {
        return;
    }
    mark_bundle_live(persist, bundle.id, ANNOUNCE_SOURCE).await;
}

pub async fn mark_bundle_live(persist: &PersistHandle, bundle_id: i64, source_json: &'static str) {
    let gate = ok_or_warn(
        persist.eval().passing_holdout_gate(bundle_id).await,
        "live_bundle.holdout_gate",
    )
    .flatten();
    let change = StatusChange {
        id: bundle_id,
        status: if gate.is_some() {
            "live"
        } else {
            "live_ungated"
        },
        gate_run_id: gate,
        override_reason: None,
        source_json,
    };
    ok_or_warn(
        persist.eval().set_bundle_status(&change).await,
        "live_bundle.mark_live",
    );
}

async fn mark_failed(judge: &LiveJudge<'_>, turn_id: i64, failure: JudgeFailure) {
    let failure = LiveFailure {
        turn_id,
        reason: failure.as_db_code(),
        bundle_id: judge.bundle.id,
        backoff_base_secs: judge.backoff_base_secs,
        backoff_max_secs: judge.backoff_max_secs,
    };
    if let Err(e) = judge.persist.mark_live_judge_failed(failure).await {
        tracing::warn!(turn_id, error = %e, "live judge: could not record the failure");
    }
}

fn log_refused(turn_id: i64) {
    tracing::info!(
        turn_id,
        "live judge: budget refused; the turn waits for the next sweep"
    );
}

fn log_credential_failure(turn_id: i64) {
    tracing::warn!(turn_id, "live judge: credential failure; stopping");
}

async fn on_error(
    judge: &LiveJudge<'_>,
    breaker: &mut CircuitBreaker,
    turn_id: i64,
    outcome: JudgeOutcome,
) -> ControlFlow<()> {
    match outcome {
        JudgeOutcome::Refused => {
            log_refused(turn_id);
            ControlFlow::Break(())
        }
        JudgeOutcome::CredentialFailure => {
            breaker.record(false, Instant::now());
            log_credential_failure(turn_id);
            ControlFlow::Break(())
        }
        JudgeOutcome::Failed(failure) => {
            breaker.record(false, Instant::now());
            mark_failed(judge, turn_id, failure).await;
            if breaker.should_skip(Instant::now()) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        }
    }
}

fn log_promote(turn_id: i64, verdict_id: i64, outcome: &PromoteOutcome) -> bool {
    match outcome {
        PromoteOutcome::Projected { outcome, .. } => {
            log_promoted(turn_id, verdict_id, outcome);
            true
        }
        PromoteOutcome::NotLive => {
            log_not_live(turn_id, verdict_id);
            false
        }
    }
}

fn log_promoted(turn_id: i64, verdict_id: i64, outcome: &impl std::fmt::Display) {
    tracing::info!(turn_id, verdict_id, outcome = %outcome, "live judge: verdict promoted");
}

fn log_not_live(turn_id: i64, verdict_id: i64) {
    tracing::warn!(
        turn_id,
        verdict_id,
        "live judge: verdict not live; nothing projected"
    );
}

async fn store_verdict(
    judge: &LiveJudge<'_>,
    breaker: &mut CircuitBreaker,
    turn_id: i64,
    judged: &super::v2_judge::V2Judged,
) -> bool {
    let target = V2InsertTarget {
        mode: VerdictMode::Live,
        bundle_id: Some(judge.bundle.id),
    };
    let insert = judged.to_insert(&judge.bundle.cfg, target);
    let params = judge.params(ProjectionMode::Apply);
    let Some((verdict_id, outcome)) = ok_or_warn(
        judge
            .persist
            .eval()
            .insert_live_verdict_and_promote(insert, &params)
            .await,
        "live_judge.store_verdict",
    ) else {
        breaker.record(false, Instant::now());
        return false;
    };
    let promoted = log_promote(turn_id, verdict_id, &outcome);
    if let (Some(decider), Some(jev)) = (judge.jev.as_ref(), judged.jev.as_ref()) {
        crate::analysis::judge::jev::mark_stored(decider, jev, judge.threshold).await;
    }
    promoted
}

async fn judge_one(
    judge: &LiveJudge<'_>,
    reservation: &Reservation,
    breaker: &mut CircuitBreaker,
    turn_id: i64,
) -> ControlFlow<(), bool> {
    let Some(_guard) = judge.shared.try_claim(turn_id) else {
        return ControlFlow::Continue(false);
    };
    let Some(mut ownership) = ok_or_warn(
        judge
            .persist
            .try_lock_judge_turn(judge.bundle.id, turn_id)
            .await,
        "live_judge.acquire_ownership",
    )
    .flatten() else {
        return ControlFlow::Continue(false);
    };
    let outcome = judge_owned(judge, reservation, breaker, turn_id, &mut ownership).await;
    if ok_or_warn(ownership.release().await, "live_judge.release_ownership").is_none() {
        return ControlFlow::Break(());
    }
    outcome.unwrap_or(ControlFlow::Continue(false))
}

async fn judge_owned(
    judge: &LiveJudge<'_>,
    reservation: &Reservation,
    breaker: &mut CircuitBreaker,
    turn_id: i64,
    ownership: &mut JudgeOwnership,
) -> Option<ControlFlow<(), bool>> {
    let result = tokio::select! {
        biased;
        _ = super::ownership::lost(ownership) => return None,
        result = judge_fresh(judge, reservation, turn_id) => result?,
    };
    ok_or_warn(
        ownership.check_connection().await,
        "live_judge.check_ownership",
    )?;
    Some(match result {
        Ok(judged) => {
            breaker.record(true, Instant::now());
            ControlFlow::Continue(store_verdict(judge, breaker, turn_id, &judged).await)
        }
        Err(outcome) => match on_error(judge, breaker, turn_id, outcome).await {
            ControlFlow::Break(()) => ControlFlow::Break(()),
            ControlFlow::Continue(()) => ControlFlow::Continue(false),
        },
    })
}

async fn judge_fresh(
    judge: &LiveJudge<'_>,
    reservation: &Reservation,
    turn_id: i64,
) -> Option<Result<super::v2_judge::V2Judged, JudgeOutcome>> {
    let row = ok_or_warn(
        judge
            .persist
            .eval()
            .eligible_judge_turn_context(judge.bundle.id, turn_id)
            .await,
        "live_judge.refresh_context",
    )??;
    let ctx = context_from_row(&row)?;
    let turn_llm = attribute_turn(judge.persist, judge.llm, reservation, turn_id).await;
    if let Some(decider) = judge.jev.as_ref()
        && let Some(judged) =
            super::jev_turn::judge_v2(decider, &turn_llm, &judge.bundle.cfg, &ctx, judge.threshold)
                .await
    {
        return Some(Ok(judged));
    }
    Some(judge_turn_v2(&turn_llm, &judge.bundle.cfg, &ctx).await)
}

pub async fn judge_and_promote(
    judge: &LiveJudge<'_>,
    reservation: &Reservation,
    breaker: &mut CircuitBreaker,
    turn_ids: &[i64],
) -> JudgeRun {
    let run = judge_claimed_turns(judge, reservation, breaker, turn_ids).await;
    ok_or_warn(
        judge.persist.release_judge_leases(turn_ids).await,
        "live_judge.release_leases",
    );
    if run.promoted > 0 {
        release_transient_backoff(judge).await;
    }
    run
}

async fn judge_claimed_turns(
    judge: &LiveJudge<'_>,
    reservation: &Reservation,
    breaker: &mut CircuitBreaker,
    turn_ids: &[i64],
) -> JudgeRun {
    let mut run = JudgeRun::default();
    for (n, &turn_id) in turn_ids.iter().enumerate() {
        ok_or_warn(
            judge.persist.renew_judge_leases(&turn_ids[n..]).await,
            "live_judge.renew_leases",
        );
        match judge_one(judge, reservation, breaker, turn_id).await {
            ControlFlow::Break(()) => {
                run.stopped = true;
                break;
            }
            ControlFlow::Continue(true) => run.promoted += 1,
            ControlFlow::Continue(false) => {}
        }
    }
    run
}

pub async fn run_live_sweep(
    judge: &LiveJudge<'_>,
    breaker: &mut CircuitBreaker,
    batch: i64,
    window_days: i64,
) -> SweepReport {
    let now = Instant::now();
    if breaker.should_skip(now) {
        return SweepReport::labeled(0);
    }
    let reservation = match reserve_claim_batch(judge.llm, breaker.is_probing(now), batch) {
        Ok(reservation) => reservation,
        Err(defer) => return SweepReport::deferred(defer),
    };
    let ids = claim_live(judge, i64::from(reservation.granted()), window_days).await;
    if ids.is_empty() {
        return SweepReport::labeled(0);
    }
    ok_or_warn(
        crate::dossier::worker::build_for_claimed_turns(judge.persist, &ids).await,
        "live_judge.dossier_backstop",
    );
    let run = judge_and_promote(judge, &reservation, breaker, &ids).await;
    SweepReport::labeled(run.promoted)
}

async fn release_transient_backoff(judge: &LiveJudge<'_>) {
    let released = ok_or_warn(
        judge.persist.clear_transient_judge_backoff().await,
        "live_judge.clear_transient_backoff",
    )
    .unwrap_or_default();
    if released > 0 {
        tracing::info!(
            released,
            "live judge: a verdict landed; turns backed off for transient failures are claimable again"
        );
    }
}

async fn claim_live(judge: &LiveJudge<'_>, limit: i64, window_days: i64) -> Vec<i64> {
    let claim = LiveClaim {
        bundle_id: judge.bundle.id,
        since: Utc::now() - chrono::Duration::days(window_days),
        limit,
    };
    let Some(claimed) = ok_or_warn(
        judge.persist.claim_live_candidates_dated(claim).await,
        "live_judge.claim",
    ) else {
        return Vec::new();
    };
    super::live_backlog::publish(super::live_backlog::LiveQueueReading::from_claim(
        &claimed,
        limit,
        Utc::now(),
    ));
    let (usable, in_flight): (Vec<i64>, Vec<i64>) = claimed
        .into_iter()
        .map(|(id, _)| id)
        .partition(|id| !judge.shared.is_in_flight(*id));
    if !in_flight.is_empty() {
        ok_or_warn(
            judge.persist.release_judge_leases(&in_flight).await,
            "live_judge.release_in_flight",
        );
    }
    usable
}

pub async fn reproject_final(judge: &LiveJudge<'_>, window_days: i64) {
    let params = judge.params(ProjectionMode::ResetWithoutVerdict);
    let days = i32::try_from(window_days).unwrap_or(7);
    let rows = ok_or_warn(
        judge.persist.reproject_final_turns(&params, days).await,
        "live_judge.reproject_final",
    )
    .unwrap_or_default();
    if !rows.is_empty() {
        tracing::info!(
            reprojected = rows.len(),
            "live judge: final turns re-projected"
        );
    }
}
