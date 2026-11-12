use std::time::Duration;

use crate::config::steerer::SteererConfig;
use crate::llm::{LlmCallContext, LlmRole, OpenRouterClient};
use crate::state::AppState;
use crate::steerer::fleet::JudgeTurnCloseNowOutcome;
use crate::store::models::SystemReminder;
use crate::store::persistence::PersistHandle;
use crate::wire::machine_text;

pub(super) async fn judge_turn_close_before_drain(
    state: &AppState,
    session_id: &str,
    human_prompt: &str,
) -> Option<SystemReminder> {
    if human_prompt.is_empty() || machine_text::classify(human_prompt).is_some() {
        return None;
    }
    let persist = state.persist.as_ref()?;
    let session_uuid = session_id.parse::<uuid::Uuid>().ok()?;
    let (open_turn_id, end_request_id) = open_turn_ref(persist, session_id, session_uuid).await?;
    let live_text = live_turn_text(persist, session_id, end_request_id).await;
    let session_db_id = state.store.read().await.session_id(session_id);
    let llm = build_judge_llm_client(state, session_db_id, open_turn_id)?;

    let verdict = run_judge_with_timeout(
        state,
        persist,
        &llm,
        JudgeCallContext {
            session_id,
            open_turn_id,
            live_text,
            human_prompt,
        },
    )
    .await;

    reminder_from_verdict(verdict, session_id, open_turn_id)
}

fn reminder_from_verdict(
    verdict: Result<JudgeTurnCloseNowOutcome, tokio::time::error::Elapsed>,
    session_id: &str,
    open_turn_id: i64,
) -> Option<SystemReminder> {
    match verdict {
        Ok(outcome) => outcome.fired.map(|(_uuid, reminder)| reminder),
        Err(_elapsed) => {
            tracing::debug!(
                session_id = %session_id,
                open_turn_id,
                "steerer fleet: same-request judge timed out — falling back to async/quiescence delivery"
            );
            None
        }
    }
}

async fn open_turn_ref(
    persist: &PersistHandle,
    session_id: &str,
    session_uuid: uuid::Uuid,
) -> Option<(i64, i64)> {
    match persist.get_open_turn_ref(session_uuid).await {
        Ok(Some(turn_ref)) => Some(turn_ref),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(
                session_id = %session_id,
                error = %e,
                "steerer fleet: open-turn lookup failed — same-request judge skipped"
            );
            None
        }
    }
}

async fn live_turn_text(persist: &PersistHandle, session_id: &str, end_request_id: i64) -> String {
    match persist
        .fetch_accumulated_text_for_request(end_request_id)
        .await
    {
        Ok(text) => text.unwrap_or_default(),
        Err(e) => {
            tracing::warn!(
                session_id = %session_id,
                end_request_id,
                error = %e,
                "steerer fleet: live turn-text fetch failed — judging with empty text"
            );
            String::new()
        }
    }
}

fn build_judge_llm_client(
    state: &AppState,
    session_db_id: Option<i64>,
    open_turn_id: i64,
) -> Option<OpenRouterClient> {
    let steerer_config: &SteererConfig = &state.config().steerer;
    let client = OpenRouterClient::new(
        String::new(),
        steerer_config.model.clone(),
        Some(steerer_config.openrouter_base_url.clone()),
        Some(Duration::from_secs(steerer_config.request_timeout_secs)),
    )
    .ok()?;
    Some(
        client
            .with_attribution(LlmRole::Steerer, state.llm_usage_sink.as_ref())
            .with_live_settings(crate::settings::steerer_live_source(
                &state.settings_overlay,
            ))
            .with_call_context(LlmCallContext {
                session_id: session_db_id,
                steerer_id: None,
                operator_turn_id: Some(open_turn_id),
            }),
    )
}

struct JudgeCallContext<'a> {
    session_id: &'a str,
    open_turn_id: i64,
    live_text: String,
    human_prompt: &'a str,
}

async fn run_judge_with_timeout(
    state: &AppState,
    persist: &PersistHandle,
    llm: &OpenRouterClient,
    ctx: JudgeCallContext<'_>,
) -> Result<JudgeTurnCloseNowOutcome, tokio::time::error::Elapsed> {
    let steerer_config = &state.config().steerer;
    tokio::time::timeout(
        Duration::from_secs(steerer_config.request_timeout_secs),
        crate::steerer::fleet::judge_turn_close_now(
            crate::steerer::fleet::JudgeDeps {
                persist,
                steerer_registry: state.steerer_registry.as_ref(),
                llm,
                key_slot: &state.steerer_openrouter_key,
                config: steerer_config,
                settings: &state.settings_overlay,
            },
            ctx.session_id,
            ctx.open_turn_id,
            ctx.live_text,
            ctx.human_prompt,
            None,
        ),
    )
    .await
}
