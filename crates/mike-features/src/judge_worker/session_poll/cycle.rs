use crate::store::persistence::eval_repo::EvalPersist as _;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::ops::ControlFlow;
use std::time::Instant;

use uuid::Uuid;

use crate::analysis::EmbeddingService;
use crate::circuit_breaker::CircuitBreaker;
use crate::config::session_judge::SessionJudgeConfig;
use crate::llm::limiter::Reservation;
use crate::llm::{LlmCallContext, OpenRouterClient};
use crate::store::persistence::judge::JudgeDigestRow;
use crate::store::persistence::{PersistError, PersistHandle, SessionForJudgment};

use super::SessionJudgeRuntime;
use super::digest::{build_digest, build_related_sessions_block};
use super::digest_repeat::{self, digest_sha256};
use super::related::fetch_related_contexts;
use super::verdict_write::{JudgeCall, call_judge_llm, record_attempt, write_verdict_and_related};

pub struct SessionPollDeps<'a> {
    pub persist: &'a PersistHandle,
    pub llm: &'a OpenRouterClient,
    pub embedding: &'a EmbeddingService,
    pub key_slot: &'a crate::llm::key::OpenRouterKeySlot,
    pub config: &'a SessionJudgeConfig,
    pub toggles: &'a std::sync::Arc<arc_swap::ArcSwap<crate::feature_toggles::FeatureToggles>>,
    pub settings: &'a std::sync::Arc<arc_swap::ArcSwap<crate::settings::SettingsOverlay>>,
}

pub async fn run_poll(
    deps: &SessionPollDeps<'_>,
    runtime: &SessionJudgeRuntime,
    breaker: &mut CircuitBreaker,
) {
    let now = Instant::now();
    if breaker.should_skip(now) {
        log_poll_breaker_open();
        return;
    }
    let probing = breaker.is_probing(now);
    if runtime.live_bundle_id > 0 {
        run_v3_poll(deps, runtime, breaker, probing).await;
        return;
    }

    let Some(llm) = resolve_effective_llm(deps.llm, deps.key_slot, &runtime.model) else {
        return;
    };

    let effective_batch = if probing { 1 } else { runtime.batch_size };
    let Some(reservation) = reserve_poll_batch(&llm, effective_batch) else {
        return;
    };
    let llm = llm.with_reservation(&reservation);
    let jev = crate::llm::decisions::point::Decider::for_point(
        &llm,
        deps.persist,
        deps.toggles.load().decisions,
        &deps.settings.load(),
        super::jev::POINT,
    );
    let rubric_version = i32::try_from(deps.config.rubric_version).unwrap_or(1);
    let settled_before = settled_before_cutoff(runtime.min_session_age_secs);

    let query = crate::store::persistence::NeedsJudgmentQuery {
        judge_model: &runtime.model,
        rubric_version,
        settled_before,
        limit: i64::from(reservation.granted()),
        max_attempts_per_day: runtime.max_attempts_per_day,
    };
    let sessions = fetch_sessions(deps.persist, &query).await;
    if sessions.is_empty() {
        return;
    }
    if probing {
        log_poll_probing();
    }
    log_judging(sessions.len());

    for session in sessions {
        let session_llm = llm.with_call_context(LlmCallContext {
            session_id: Some(session.session_db_id),
            steerer_id: None,
            operator_turn_id: None,
        });
        let ctx = SessionJudgeCtx {
            persist: deps.persist,
            llm: &session_llm,
            embedding: deps.embedding,
            runtime,
            rubric_version,
            jev: jev.as_ref(),
        };
        if judge_one_session(&ctx, breaker, session).await.is_break() {
            break;
        }
    }
}

async fn run_v3_poll(
    deps: &SessionPollDeps<'_>,
    runtime: &SessionJudgeRuntime,
    breaker: &mut CircuitBreaker,
    probing: bool,
) {
    let Some(llm) = crate::llm::key::keyed_client(deps.llm, deps.key_slot) else {
        log_client_not_built();
        return;
    };
    let batch = if probing { 1 } else { runtime.batch_size };
    let Some(reservation) = reserve_poll_batch(&llm, batch) else {
        return;
    };
    let llm = llm.with_reservation(&reservation);
    let poll = crate::judge_worker::session_end::live::LivePoll {
        persist: deps.persist,
        llm: &llm,
        embedding: deps.embedding,
        bundle_id: runtime.live_bundle_id,
        lookback_hours: runtime.v3_lookback_hours,
        max_attempts_per_day: runtime.max_attempts_per_day,
        batch: i64::from(reservation.granted()),
        turn_judge_live_bundle_id: runtime.turn_judge_live_bundle_id,
        turn_confidence_threshold: runtime.turn_confidence_threshold,
    };
    let judged = poll.run(breaker).await;
    if i64::try_from(judged).unwrap_or(0) < batch {
        crate::judge_worker::session_end::rejudge::drain(
            &poll,
            runtime.rejudge_daily_call_cap,
            breaker,
        )
        .await;
    }
}

fn reserve_poll_batch(llm: &OpenRouterClient, batch: i64) -> Option<Reservation> {
    let budget = llm.budget();
    let sized = batch.min(i64::from(budget.calls_now));
    if sized <= 0 {
        log_poll_deferred(budget.next_slot_in);
        return None;
    }
    let reservation = llm.reserve(u32::try_from(sized).unwrap_or(u32::MAX));
    if reservation.granted() == 0 {
        log_poll_deferred(reservation.next_slot_in());
        return None;
    }
    Some(reservation)
}

fn log_poll_deferred(next_slot_in: std::time::Duration) {
    tracing::debug!(
        next_slot_ms = next_slot_in.as_millis(),
        "session_judge: no llm budget available — poll cycle skipped before fetching"
    );
}

fn log_poll_breaker_open() {
    tracing::debug!("session_judge: circuit breaker open — skipping poll cycle");
}

fn log_poll_probing() {
    tracing::info!("session_judge: circuit breaker probing recovery");
}

fn log_judging(count: usize) {
    tracing::info!(count, "session_judge: judging {} session(s)", count);
}

pub fn resolve_effective_llm(
    llm: &OpenRouterClient,
    key_slot: &crate::llm::key::OpenRouterKeySlot,
    model: &str,
) -> Option<OpenRouterClient> {
    let effective = crate::llm::key::effective_client_for(llm, key_slot, model);
    if effective.is_none() {
        log_no_api_key(model);
    }
    effective
}

fn log_no_api_key(model: &str) {
    tracing::warn!(
        model,
        "session_judge: the model runs on OpenRouter and no OpenRouter API key is configured — skipping this poll cycle"
    );
}

fn log_client_not_built() {
    tracing::warn!(
        "session_judge: the judge LLM client could not be built — skipping this poll cycle"
    );
}

fn settled_before_cutoff(min_session_age_secs: u64) -> chrono::DateTime<chrono::Utc> {
    let min_age = std::time::Duration::from_secs(min_session_age_secs);
    chrono::Utc::now()
        - chrono::Duration::from_std(min_age).unwrap_or(chrono::Duration::seconds(60))
}

async fn fetch_sessions(
    persist: &PersistHandle,
    query: &crate::store::persistence::NeedsJudgmentQuery<'_>,
) -> Vec<SessionForJudgment> {
    match persist.eval().fetch_sessions_needing_judgment(query).await {
        Ok(s) => s,
        Err(e) => {
            log_poll_query_failed(&e);
            Vec::new()
        }
    }
}

fn log_poll_query_failed(e: &PersistError) {
    tracing::warn!(error = %e, "session_judge: poll query failed — will retry");
}

pub struct SessionJudgeCtx<'a> {
    pub persist: &'a PersistHandle,
    pub llm: &'a OpenRouterClient,
    pub embedding: &'a EmbeddingService,
    pub runtime: &'a SessionJudgeRuntime,
    pub rubric_version: i32,
    pub jev: Option<&'a crate::llm::decisions::point::Decider>,
}

async fn judge_one_session(
    ctx: &SessionJudgeCtx<'_>,
    breaker: &mut CircuitBreaker,
    session: SessionForJudgment,
) -> ControlFlow<()> {
    let session_id = &session.session_id;
    let session_db_id = session.session_db_id;
    let attempted_at = chrono::Utc::now();

    let Some(turns) = fetch_digest_turns(ctx.persist, session_db_id, session_id).await else {
        return ControlFlow::Continue(());
    };

    let Ok(session_uuid) = session_id.parse::<Uuid>() else {
        log_invalid_session_uuid(session_id);
        return ControlFlow::Continue(());
    };

    let has_vector = ensure_session_vector(ctx, session_uuid, session_id).await;
    let (neighbour_ids, related_contexts) =
        fetch_related_contexts(ctx.persist, session_uuid, session_id, has_vector).await;

    let digest_text = build_digest(
        &turns,
        ctx.runtime.max_turns_in_prompt,
        ctx.runtime.max_chars_per_turn,
    );
    let mut user_message = digest_text.clone();
    user_message.push_str(&build_related_sessions_block(&related_contexts));

    let digest = digest_sha256(&ctx.runtime.system_prompt, &user_message);
    if digest_repeat::is_repeat(ctx, &session, &digest, attempted_at).await {
        return ControlFlow::Continue(());
    }

    let jev = decide_with_jev(ctx, &session, &turns, &digest_text).await;
    let call = call_judge_llm(
        ctx.llm,
        breaker,
        session_id,
        &ctx.runtime.system_prompt,
        &user_message,
    )
    .await;
    let call = super::jev::reconcile(call, jev.as_ref());
    match call {
        JudgeCall::Refused => ControlFlow::Break(()),
        JudgeCall::BreakerTripped => {
            record_attempt(ctx, &session, attempted_at, true).await;
            ControlFlow::Break(())
        }
        JudgeCall::Failed => {
            record_attempt(ctx, &session, attempted_at, true).await;
            ControlFlow::Continue(())
        }
        JudgeCall::Verdict(verdict) => {
            let stored = write_verdict_and_related(
                ctx,
                breaker,
                &session,
                session_uuid,
                verdict,
                neighbour_ids,
            )
            .await;
            record_attempt(ctx, &session, attempted_at, !stored).await;
            if stored {
                digest_repeat::remember(ctx, &session, &digest).await;
                mark_jev_stored(ctx, jev.as_ref()).await;
            }
            ControlFlow::Continue(())
        }
    }
}

async fn decide_with_jev(
    ctx: &SessionJudgeCtx<'_>,
    session: &SessionForJudgment,
    turns: &[JudgeDigestRow],
    digest_text: &str,
) -> Option<super::jev::SessionJev> {
    let decider = ctx.jev?;
    let first_ask = turns.first().map_or("", |t| t.user_prompt.as_str());
    super::jev::decide(
        decider,
        session.session_db_id,
        first_ask,
        digest_text,
        turns.len(),
    )
    .await
}

async fn mark_jev_stored(ctx: &SessionJudgeCtx<'_>, jev: Option<&super::jev::SessionJev>) {
    if let (Some(decider), Some(jev)) = (ctx.jev, jev) {
        super::jev::mark_stored(decider, jev).await;
    }
}

fn log_invalid_session_uuid(session_id: &str) {
    tracing::warn!(
        session_id,
        "session_judge: session_id is not a valid UUID — skipping session"
    );
}

async fn fetch_digest_turns(
    persist: &PersistHandle,
    session_db_id: i64,
    session_id: &str,
) -> Option<Vec<JudgeDigestRow>> {
    let turns = match persist.eval().fetch_judge_digest(session_db_id).await {
        Ok(t) => t,
        Err(e) => {
            log_digest_fetch_failed(session_id, &e);
            return None;
        }
    };
    if turns.is_empty() {
        log_no_turns_yet(session_id);
        return None;
    }
    Some(turns)
}

fn log_digest_fetch_failed(session_id: &str, e: &PersistError) {
    tracing::warn!(
        session_id,
        error = %e,
        "session_judge: digest fetch failed — skipping session"
    );
}

fn log_no_turns_yet(session_id: &str) {
    tracing::debug!(
        session_id,
        "session_judge: no operator turns yet — leaving unjudged"
    );
}

async fn ensure_session_vector(
    ctx: &SessionJudgeCtx<'_>,
    session_uuid: Uuid,
    session_id: &str,
) -> bool {
    let mut has_vector = check_embedding_status(ctx.persist, session_uuid, session_id).await;
    if !has_vector {
        if let Err(e) =
            crate::embed_worker::refresh_session_embedding(ctx.persist, ctx.embedding, session_uuid)
                .await
        {
            log_pre_prompt_refresh_failed(session_id, &e);
        } else {
            has_vector = recheck_embedding_status(ctx.persist, session_uuid, session_id).await;
        }
    }
    has_vector
}

fn log_pre_prompt_refresh_failed(session_id: &str, e: &PersistError) {
    tracing::warn!(session_id, error = %e, "session_judge: pre-prompt vector refresh failed");
}

async fn check_embedding_status(
    persist: &PersistHandle,
    session_uuid: Uuid,
    session_id: &str,
) -> bool {
    match persist
        .knowledge()
        .session_embedding_status(session_uuid)
        .await
    {
        Ok(status) => status.is_some(),
        Err(e) => {
            log_embedding_status_check_failed(session_id, &e);
            false
        }
    }
}

fn log_embedding_status_check_failed(session_id: &str, e: &PersistError) {
    tracing::warn!(session_id, error = %e, "session_judge: embedding status check failed");
}

async fn recheck_embedding_status(
    persist: &PersistHandle,
    session_uuid: Uuid,
    session_id: &str,
) -> bool {
    match persist
        .knowledge()
        .session_embedding_status(session_uuid)
        .await
    {
        Ok(status) => status.is_some(),
        Err(e) => {
            log_embedding_status_recheck_failed(session_id, &e);
            false
        }
    }
}

fn log_embedding_status_recheck_failed(session_id: &str, e: &PersistError) {
    tracing::warn!(session_id, error = %e, "session_judge: embedding status re-check failed");
}
