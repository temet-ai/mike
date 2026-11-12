use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use crate::analysis::judge::input_v2::INPUT_BUILDER_R3;
use crate::analysis::judge::rubric::{R3, effective_rubric};
use crate::analysis::judge::{
    JudgeFailure, JudgeLabel, JudgeOutcome, JudgeVerdict, build_judge_user_message,
    run_judge_with_usage,
};
use crate::circuit_breaker::CircuitBreaker;
use crate::llm::limiter::Reservation;
use crate::llm::{CallUsage, LlmCallContext, OpenRouterClient};
use crate::store::persistence::turn_verdicts::{
    LegacyStamp, TurnVerdictInsert, VerdictJudgement, VerdictMode, VerdictShape,
};
use crate::store::persistence::{JudgeCandidateRow, PersistHandle};

use super::turn_sweep_log::{
    log_already_stamped, log_below_threshold, log_mark_judge_failed_failed, log_stamp_failed,
    log_stamped, log_sweep_breaker_open, log_sweep_deferred, log_sweep_probing,
    log_sweep_stopped_credential_failure, log_sweep_stopped_refused, log_sweep_tripped,
    log_turn_session_lookup_failed,
};
use super::v2_judge::verdict_usage;

pub(crate) struct JudgeSweepParams<'a> {
    pub(crate) model_label: &'a str,
    pub(crate) system_prompt: &'a str,
    pub(crate) rubric_version: &'a str,
    pub(crate) batch: i64,
    pub(crate) threshold: f32,
    pub(crate) backoff_base_secs: i64,
    pub(crate) backoff_max_secs: i64,
    pub(crate) bundle_id: Option<i64>,
}

pub(crate) struct SweepReport {
    pub(crate) labeled: usize,
    pub(crate) defer: Option<Duration>,
}

impl SweepReport {
    pub(super) fn labeled(labeled: usize) -> Self {
        Self {
            labeled,
            defer: None,
        }
    }

    pub(super) fn deferred(defer: Duration) -> Self {
        Self {
            labeled: 0,
            defer: Some(defer),
        }
    }
}

pub(crate) struct TurnJudgeRuntime {
    pub(crate) model: String,
    pub(crate) system_prompt: String,
    pub(crate) rubric_version: String,
    pub(crate) batch_size: i64,
    pub(crate) confidence_threshold: f32,
}

pub(crate) fn resolve_turn_judge_runtime(
    overlay: &crate::settings::SettingsOverlay,
    cfg: &crate::config::steerer::JudgeConfig,
    fallback_model: &str,
) -> TurnJudgeRuntime {
    let model = overlay
        .judge_worker_model
        .as_deref()
        .filter(|m| !m.is_empty())
        .or(cfg.model.as_deref().filter(|m| !m.is_empty()))
        .unwrap_or(fallback_model)
        .to_owned();
    let model = overlay
        .llm_provider
        .resolve(Some(crate::llm::provider::ModelTier::Standard), &model);

    let system_prompt = overlay
        .judge_worker_system_prompt
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| {
            if cfg.system_prompt.trim().is_empty() {
                crate::analysis::judge::judge_system_prompt()
            } else {
                cfg.system_prompt.as_str()
            }
        })
        .to_owned();
    let rubric_version = effective_rubric(&R3, Some(&system_prompt)).id;

    TurnJudgeRuntime {
        model,
        system_prompt,
        rubric_version,
        batch_size: overlay.judge_worker_batch_size.unwrap_or(cfg.batch_size),
        confidence_threshold: overlay
            .judge_worker_confidence_threshold
            .unwrap_or(cfg.confidence_threshold),
    }
}

pub(crate) async fn run_judge_sweep(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    breaker: &mut CircuitBreaker,
    params: &JudgeSweepParams<'_>,
) -> anyhow::Result<SweepReport> {
    let now = Instant::now();
    if breaker.should_skip(now) {
        log_sweep_breaker_open();
        return Ok(SweepReport::labeled(0));
    }
    let probing = breaker.is_probing(now);
    let reservation = match reserve_claim_batch(llm, probing, params.batch) {
        Ok(reservation) => reservation,
        Err(defer) => {
            log_sweep_deferred(defer);
            return Ok(SweepReport::deferred(defer));
        }
    };

    let candidates = persist
        .claim_unjudged_turns(i64::from(reservation.granted()), params.rubric_version)
        .await
        .map_err(|e| anyhow::anyhow!("claim_unjudged_turns failed: {e}"))?;

    if candidates.is_empty() {
        return Ok(SweepReport::labeled(0));
    }
    if probing {
        log_sweep_probing(candidates[0].turn_id);
    }
    build_dossier_for_claimed(persist, &candidates).await;

    let labeled = judge_candidates(persist, llm, &reservation, breaker, params, &candidates).await;
    Ok(SweepReport::labeled(labeled))
}

async fn build_dossier_for_claimed(persist: &PersistHandle, candidates: &[JudgeCandidateRow]) {
    let turn_ids: Vec<i64> = candidates.iter().map(|c| c.turn_id).collect();
    if let Err(e) = crate::dossier::worker::build_for_claimed_turns(persist, &turn_ids).await {
        tracing::warn!(error = %e, "judge sweep: dossier backstop build failed");
    }
}

async fn judge_candidates(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    reservation: &Reservation,
    breaker: &mut CircuitBreaker,
    params: &JudgeSweepParams<'_>,
    candidates: &[JudgeCandidateRow],
) -> usize {
    let mut labeled = 0usize;
    for c in candidates {
        let turn_llm = attribute_turn(persist, llm, reservation, c.turn_id).await;
        match judge_one_turn(persist, &turn_llm, breaker, params, c).await {
            ControlFlow::Break(()) => break,
            ControlFlow::Continue(true) => labeled += 1,
            ControlFlow::Continue(false) => {}
        }
    }
    labeled
}

pub(super) fn reserve_claim_batch(
    llm: &OpenRouterClient,
    probing: bool,
    batch: i64,
) -> Result<Reservation, Duration> {
    let desired = if probing { 1 } else { batch.max(1) };
    let budget = llm.budget();
    let sized = desired.min(i64::from(budget.calls_now));
    if sized == 0 {
        return Err(budget.next_slot_in);
    }
    let reservation = llm.reserve(u32::try_from(sized).unwrap_or(u32::MAX));
    if reservation.granted() == 0 {
        return Err(reservation.next_slot_in());
    }
    Ok(reservation)
}

pub(super) async fn attribute_turn(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    reservation: &Reservation,
    turn_id: i64,
) -> OpenRouterClient {
    let session_id = match persist.session_id_for_turn(turn_id).await {
        Ok(id) => id,
        Err(e) => {
            log_turn_session_lookup_failed(turn_id, &e);
            None
        }
    };
    llm.with_reservation(reservation)
        .with_call_context(LlmCallContext {
            session_id,
            steerer_id: None,
            operator_turn_id: Some(turn_id),
        })
}

async fn judge_one_turn(
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    breaker: &mut CircuitBreaker,
    params: &JudgeSweepParams<'_>,
    c: &JudgeCandidateRow,
) -> ControlFlow<(), bool> {
    let user_msg = build_judge_user_message(c);
    let result = run_judge_with_usage(llm, params.system_prompt, &user_msg, c.turn_id).await;
    let (verdict, usage) = match result {
        Ok(v) => {
            breaker.record(true, Instant::now());
            v
        }
        Err(outcome) => {
            return handle_judge_error(persist, breaker, params, c.turn_id, outcome).await;
        }
    };
    let judged = LiveJudged {
        turn_id: c.turn_id,
        verdict: &verdict,
        usage: &usage,
        input_text: &user_msg,
    };
    ControlFlow::Continue(stamp_and_log(persist, breaker, params, judged).await)
}

async fn handle_judge_error(
    persist: &PersistHandle,
    breaker: &mut CircuitBreaker,
    params: &JudgeSweepParams<'_>,
    turn_id: i64,
    outcome: JudgeOutcome,
) -> ControlFlow<(), bool> {
    match outcome {
        JudgeOutcome::Refused => {
            log_sweep_stopped_refused(turn_id);
            ControlFlow::Break(())
        }
        JudgeOutcome::CredentialFailure => {
            breaker.record(false, Instant::now());
            log_sweep_stopped_credential_failure(turn_id);
            ControlFlow::Break(())
        }
        JudgeOutcome::Failed(failure) => {
            breaker.record(false, Instant::now());
            mark_failed_and_log(persist, params, turn_id, failure).await;
            if breaker.should_skip(Instant::now()) {
                log_sweep_tripped(turn_id);
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(false)
        }
    }
}

async fn mark_failed_and_log(
    persist: &PersistHandle,
    params: &JudgeSweepParams<'_>,
    turn_id: i64,
    failure: JudgeFailure,
) {
    if let Err(e) = persist
        .mark_judge_failed(
            turn_id,
            failure.as_db_code(),
            params.backoff_base_secs,
            params.backoff_max_secs,
        )
        .await
    {
        log_mark_judge_failed_failed(turn_id, &e);
    }
}

fn resolve_outcome_code(verdict: &JudgeVerdict, threshold: f32, turn_id: i64) -> &'static str {
    if verdict.confidence >= threshold {
        return verdict.label.outcome_code();
    }
    log_below_threshold(turn_id, &verdict.label, verdict.confidence, threshold);
    JudgeLabel::Unknown.outcome_code()
}

struct LiveJudged<'a> {
    turn_id: i64,
    verdict: &'a JudgeVerdict,
    usage: &'a CallUsage,
    input_text: &'a str,
}

async fn stamp_and_log(
    persist: &PersistHandle,
    breaker: &mut CircuitBreaker,
    params: &JudgeSweepParams<'_>,
    judged: LiveJudged<'_>,
) -> bool {
    let turn_id = judged.turn_id;
    let outcome_code = resolve_outcome_code(judged.verdict, params.threshold, turn_id);
    let insert = TurnVerdictInsert {
        turn_id,
        mode: VerdictMode::Live,
        bundle_id: params.bundle_id,
        rubric_version: params.rubric_version,
        input_builder_version: INPUT_BUILDER_R3,
        slices: Vec::new(),
        model: params.model_label,
        judgement: VerdictJudgement {
            shape: VerdictShape::Legacy {
                label: outcome_code,
            },
            confidence: judged.verdict.confidence,
            rationale: &judged.verdict.rationale,
            unmet_ask: &judged.verdict.unmet_ask,
            evidence_quality: judged.verdict.evidence_quality.as_db_code(),
        },
        next_genuine_request_id: None,
        as_of: chrono::Utc::now(),
        input_text: judged.input_text,
        usage: verdict_usage(judged.usage),
    };
    match persist
        .insert_live_legacy_verdict_and_stamp(insert, LegacyStamp { outcome_code })
        .await
    {
        Ok((_, true)) => {
            log_stamped(turn_id, outcome_code, judged.verdict.confidence);
            true
        }
        Ok((verdict_id, false)) => {
            log_already_stamped(turn_id, verdict_id);
            false
        }
        Err(e) => {
            breaker.record(false, Instant::now());
            log_stamp_failed(turn_id, &e);
            false
        }
    }
}
