use std::time::Instant;

use uuid::Uuid;

use crate::analysis::EmbeddingService;
use crate::circuit_breaker::CircuitBreaker;
use crate::llm::OpenRouterClient;
use crate::store::persistence::{PersistError, PersistHandle, SessionForJudgment};

use super::cycle::SessionJudgeCtx;
use super::digest::{JudgeVerdict, judge_verdict_schema, outcome_code_to_id};

pub(super) enum JudgeCall {
    Verdict(JudgeVerdict),
    Failed,
    BreakerTripped,
    Refused,
}

pub(super) async fn call_judge_llm(
    llm: &OpenRouterClient,
    breaker: &mut CircuitBreaker,
    session_id: &str,
    system_prompt: &str,
    user_message: &str,
) -> JudgeCall {
    let schema = judge_verdict_schema();
    match llm
        .verdict::<JudgeVerdict>(system_prompt, user_message, schema)
        .await
    {
        Ok(v) => {
            breaker.record(true, Instant::now());
            JudgeCall::Verdict(v)
        }
        Err(e) if e.is_refusal() => {
            log_poll_stopped_refused(session_id, &e);
            JudgeCall::Refused
        }
        Err(e) => {
            breaker.record(false, Instant::now());
            log_llm_call_failed(session_id, &e);
            if breaker.should_skip(Instant::now()) {
                log_poll_stopped_breaker(session_id);
                JudgeCall::BreakerTripped
            } else {
                JudgeCall::Failed
            }
        }
    }
}

pub(super) async fn record_attempt(
    ctx: &SessionJudgeCtx<'_>,
    session: &SessionForJudgment,
    attempted_at: chrono::DateTime<chrono::Utc>,
    failed: bool,
) {
    match ctx
        .persist
        .record_judge_attempt(
            session.session_db_id,
            &ctx.runtime.model,
            ctx.rubric_version,
            attempted_at,
            failed,
        )
        .await
    {
        Ok(window_attempts) if i64::from(window_attempts) == ctx.runtime.max_attempts_per_day => {
            log_daily_cap_reached(&session.session_id, window_attempts);
        }
        Ok(_) => {}
        Err(e) => log_attempt_record_failed(&session.session_id, &e),
    }
}

fn log_daily_cap_reached(session_id: &str, window_attempts: i32) {
    tracing::warn!(
        session_id,
        window_attempts,
        "session_judge: session reached its daily attempt cap; skipped until its 24-hour window rolls over"
    );
}

fn log_attempt_record_failed(session_id: &str, e: &PersistError) {
    tracing::warn!(session_id, error = %e, "session_judge: attempt record failed");
}

fn log_poll_stopped_refused(session_id: &str, e: &crate::llm::LlmError) {
    tracing::warn!(
        session_id,
        error = %e,
        "session_judge: poll cycle stopped — call governor refused (ceiling/rate limit)"
    );
}

fn log_llm_call_failed(session_id: &str, e: &crate::llm::LlmError) {
    tracing::warn!(session_id, error = %e, "session_judge: LLM call failed — skipping session");
}

fn log_poll_stopped_breaker(session_id: &str) {
    tracing::warn!(
        session_id,
        "session_judge: circuit breaker tripped — stopping poll cycle early"
    );
}

pub(super) async fn write_verdict_and_related(
    ctx: &SessionJudgeCtx<'_>,
    breaker: &mut CircuitBreaker,
    session: &SessionForJudgment,
    session_uuid: Uuid,
    verdict: JudgeVerdict,
    neighbour_ids: Vec<Uuid>,
) -> bool {
    let quality_score = verdict.quality_score.clamp(0.0, 1.0);
    let judged_at = chrono::Utc::now();

    let verdict_id = if session.needs_verdict {
        insert_verdict(ctx, breaker, session, &verdict, quality_score, judged_at).await
    } else {
        log_verdict_already_stands(&session.session_id);
        None
    };

    let overview = verdict.overview.trim();
    if !overview.is_empty() && verdict.outcome != "unknown" {
        write_overview(
            ctx.persist,
            session_uuid,
            &session.session_id,
            overview,
            judged_at,
        )
        .await;
    }

    write_knowledge(
        ctx.persist,
        session_uuid,
        &session.session_id,
        &verdict,
        &neighbour_ids,
        verdict_id,
    )
    .await;

    refresh_vector_post_verdict(
        ctx.persist,
        ctx.embedding,
        session_uuid,
        &session.session_id,
    )
    .await;

    !session.needs_verdict || verdict_id.is_some()
}

async fn insert_verdict(
    ctx: &SessionJudgeCtx<'_>,
    breaker: &mut CircuitBreaker,
    session: &SessionForJudgment,
    verdict: &JudgeVerdict,
    quality_score: f64,
    judged_at: chrono::DateTime<chrono::Utc>,
) -> Option<i64> {
    let outcome_id = outcome_code_to_id(&verdict.outcome);
    match ctx
        .persist
        .insert_session_verdict(crate::store::persistence::judge::SessionVerdictInsert {
            session_db_id: session.session_db_id,
            outcome_id,
            quality_score: Some(quality_score),
            rationale: Some(verdict.rationale.as_str()),
            judge_model: &ctx.runtime.model,
            rubric_version: ctx.rubric_version,
            judged_at,
            bundle_id: ctx.runtime.bundle_id,
        })
        .await
    {
        Ok(id) => {
            log_verdict_written(&session.session_id, id, &verdict.outcome, quality_score);
            Some(id)
        }
        Err(e) => {
            breaker.record(false, Instant::now());
            log_verdict_insert_failed(&session.session_id, &e);
            None
        }
    }
}

fn log_verdict_written(session_id: &str, verdict_id: i64, outcome: &str, quality_score: f64) {
    tracing::info!(
        session_id,
        verdict_id,
        outcome,
        quality_score,
        "session_judge: verdict written"
    );
}

fn log_verdict_insert_failed(session_id: &str, e: &PersistError) {
    tracing::warn!(session_id, error = %e, "session_judge: verdict insert failed");
}

fn log_verdict_already_stands(session_id: &str) {
    tracing::debug!(
        session_id,
        "session_judge: verdict already stands — overview-only pass"
    );
}

pub(in crate::judge_worker) async fn write_overview(
    persist: &PersistHandle,
    session_uuid: Uuid,
    session_id: &str,
    overview: &str,
    judged_at: chrono::DateTime<chrono::Utc>,
) {
    match persist
        .set_session_overview_generated(session_uuid, overview, judged_at)
        .await
    {
        Ok(true) => log_overview_written(session_id),
        Ok(false) => log_overview_write_skipped(session_id),
        Err(e) => log_overview_write_failed(session_id, &e),
    }
}

fn log_overview_written(session_id: &str) {
    tracing::info!(session_id, "session_judge: overview written");
}

fn log_overview_write_skipped(session_id: &str) {
    tracing::debug!(
        session_id,
        "session_judge: overview write skipped — operator-authored overview present"
    );
}

fn log_overview_write_failed(session_id: &str, e: &PersistError) {
    tracing::warn!(session_id, error = %e, "session_judge: overview write failed");
}

pub(in crate::judge_worker) async fn write_knowledge(
    persist: &PersistHandle,
    session_uuid: Uuid,
    session_id: &str,
    verdict: &JudgeVerdict,
    neighbour_ids: &[Uuid],
    verdict_id: Option<i64>,
) {
    if let Err(e) = persist
        .upsert_session_knowledge(
            session_uuid,
            &verdict.topics,
            &verdict.learnings,
            neighbour_ids,
            verdict_id,
        )
        .await
    {
        log_knowledge_write_failed(session_id, &e);
    }
}

fn log_knowledge_write_failed(session_id: &str, e: &PersistError) {
    tracing::warn!(session_id, error = %e, "session_judge: knowledge write failed");
}

pub(in crate::judge_worker) async fn refresh_vector_post_verdict(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    session_uuid: Uuid,
    session_id: &str,
) {
    if let Err(e) =
        crate::embed_worker::refresh_session_embedding(persist, embedding, session_uuid).await
    {
        log_post_verdict_refresh_failed(session_id, &e);
    }
}

fn log_post_verdict_refresh_failed(session_id: &str, e: &PersistError) {
    tracing::warn!(session_id, error = %e, "session_judge: post-verdict vector refresh failed");
}
