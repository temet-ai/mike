use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use tokio::sync::RwLock;

use crate::analysis::embeddings::EmbeddingService;
use crate::circuit_breaker::CircuitBreaker;
use crate::config::steerer::SteererConfig;
use crate::llm::OpenRouterClient;
use crate::steerer::registry::SteererRegistry;
use crate::store::RequestStore;
use crate::store::persistence::operator_turns::{QuiescenceTarget, QuiescentTurnRow};
use crate::store::persistence::{PersistError, PersistHandle};

pub struct SweepContext<'a> {
    pub persist: &'a PersistHandle,
    pub store: &'a Arc<RwLock<RequestStore>>,
    pub steerer_registry: &'a Arc<ArcSwap<SteererRegistry>>,
    pub key_slot: &'a crate::llm::key::OpenRouterKeySlot,
    pub config: &'a SteererConfig,
    pub settings: &'a crate::settings::SharedSettingsOverlay,
    pub usage_sink: Option<&'a Arc<dyn crate::llm::LlmUsageSink>>,
    pub remote_control_streams: &'a crate::proxy::remote_control::ReplyStreams,
    pub injection_gate: &'a std::sync::Arc<crate::injection::InjectionGate>,
    pub embedding: &'a EmbeddingService,
    pub lessons: &'a dyn crate::state::lesson_hooks::LessonHooks,
    pub toggles: &'a Arc<ArcSwap<crate::feature_toggles::FeatureToggles>>,
}

impl SweepContext<'_> {
    pub fn push_deps(&self) -> crate::proxy::remote_control::inject::PushDeps {
        crate::proxy::remote_control::inject::PushDeps {
            persist: Some(self.persist.clone()),
            remote_control_streams: self.remote_control_streams.clone(),
            gate: self.injection_gate.clone(),
            store: self.store.clone(),
        }
    }
}

pub async fn run_sweep(
    ctx: &SweepContext<'_>,
    quiet_secs: i64,
    batch: i64,
    breaker: &mut CircuitBreaker,
) {
    let targets = super::targets::quiescence_targets(&ctx.steerer_registry.load(), quiet_secs);
    let candidates = fetch_candidates(ctx.persist, &targets, batch).await;
    if candidates.is_empty() {
        return;
    }

    let Some(llm) = build_base_llm_client(ctx) else {
        return;
    };

    for row in &candidates {
        if process_candidate(ctx, &llm, breaker, row).await.is_break() {
            break;
        }
    }
}

async fn fetch_candidates(
    persist: &PersistHandle,
    targets: &[QuiescenceTarget],
    batch: i64,
) -> Vec<QuiescentTurnRow> {
    match persist.list_quiescent_turns(targets, batch).await {
        Ok(rows) => rows,
        Err(e) => {
            log_sweep_query_failed(&e);
            Vec::new()
        }
    }
}

fn log_sweep_query_failed(e: &PersistError) {
    tracing::warn!(
        error = %e,
        "quiescence_worker: sweep query failed — retrying next tick"
    );
}

pub(super) fn build_base_llm_client(ctx: &SweepContext<'_>) -> Option<OpenRouterClient> {
    let config = ctx.config;
    let model = ctx.settings.load().live_steerer_model(&config.model);
    match OpenRouterClient::new(
        String::new(),
        model,
        Some(config.openrouter_base_url.clone()),
        Some(Duration::from_secs(config.request_timeout_secs)),
    ) {
        Ok(llm) => Some(
            llm.with_attribution(crate::llm::LlmRole::Steerer, ctx.usage_sink)
                .with_live_settings(crate::settings::steerer_live_source(ctx.settings)),
        ),
        Err(_) => {
            log_llm_client_build_failed();
            None
        }
    }
}

fn log_llm_client_build_failed() {
    tracing::warn!("quiescence_worker: failed to build base OpenRouter client — skipping sweep");
}

async fn process_candidate(
    ctx: &SweepContext<'_>,
    llm: &OpenRouterClient,
    breaker: &mut CircuitBreaker,
    row: &QuiescentTurnRow,
) -> ControlFlow<()> {
    let live_text = fetch_live_text(ctx.persist, row).await;

    let outcome = crate::steerer::fleet::judge_idle_turn(
        crate::steerer::fleet::JudgeDeps {
            persist: ctx.persist,
            steerer_registry: ctx.steerer_registry.as_ref(),
            llm,
            key_slot: ctx.key_slot,
            config: ctx.config,
            settings: ctx.settings,
        },
        row,
        live_text,
    )
    .await;

    if !outcome.llm_call_refused {
        breaker.record(!outcome.llm_call_failed, Instant::now());
    }

    handle_fire_outcome(ctx, row, outcome.fired).await;

    if outcome.llm_call_refused {
        log_sweep_stopped_refused(row);
        return ControlFlow::Break(());
    }
    if breaker.should_skip(Instant::now()) {
        log_sweep_stopped_breaker(row);
        return ControlFlow::Break(());
    }
    ControlFlow::Continue(())
}

async fn fetch_live_text(persist: &PersistHandle, row: &QuiescentTurnRow) -> String {
    match persist
        .fetch_accumulated_text_for_request(row.end_request_id)
        .await
    {
        Ok(text) => text.unwrap_or_default(),
        Err(e) => {
            log_live_text_fetch_failed(row, &e);
            String::new()
        }
    }
}

fn log_live_text_fetch_failed(row: &QuiescentTurnRow, e: &PersistError) {
    tracing::warn!(
        session_id = %row.session_uuid,
        turn_id = row.turn_id,
        steerer_id = row.steerer_id,
        error = %e,
        "quiescence_worker: live turn-text fetch failed — judging with empty text"
    );
}

async fn handle_fire_outcome(
    ctx: &SweepContext<'_>,
    row: &QuiescentTurnRow,
    fired: Option<(uuid::Uuid, crate::store::models::SystemReminder)>,
) {
    match fired {
        Some((_uuid, reminder)) => {
            crate::proxy::remote_control::inject::deliver_or_enqueue(
                &ctx.push_deps(),
                row.session_uuid,
                reminder,
            )
            .await;
            log_fired(row);
        }
        None => log_no_fire(row),
    }
}

fn log_fired(row: &QuiescentTurnRow) {
    tracing::info!(
        session_id = %row.session_uuid,
        turn_id = row.turn_id,
        steerer_id = row.steerer_id,
        idle_seconds = row.idle_seconds,
        "quiescence_worker: fired — queued for delivery on the session's next request"
    );
}

fn log_no_fire(row: &QuiescentTurnRow) {
    tracing::debug!(
        session_id = %row.session_uuid,
        turn_id = row.turn_id,
        steerer_id = row.steerer_id,
        idle_seconds = row.idle_seconds,
        "quiescence_worker: no fire"
    );
}

fn log_sweep_stopped_refused(row: &QuiescentTurnRow) {
    tracing::warn!(
        session_id = %row.session_uuid,
        turn_id = row.turn_id,
        "quiescence_worker: sweep stopped — call governor refused (ceiling/rate limit), turn left for a later sweep"
    );
}

fn log_sweep_stopped_breaker(row: &QuiescentTurnRow) {
    tracing::warn!(
        session_id = %row.session_uuid,
        turn_id = row.turn_id,
        "quiescence_worker: circuit breaker tripped — stopping sweep early"
    );
}
