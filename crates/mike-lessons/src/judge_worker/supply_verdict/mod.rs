use crate::store::persistence::lessons_repo::LessonsPersist as _;
pub mod jev;
pub mod prompt;
#[cfg(test)]
mod prompt_settings_tests;
#[cfg(test)]
mod tests;

use std::time::Duration;

use chrono::{DateTime, Timelike, Utc};
use tokio::time::{MissedTickBehavior, interval};

use crate::lesson_proposer::ProposerSettings;
use crate::llm::{LlmCallContext, LlmRole, OpenRouterClient};
use crate::settings::meta_memory_loop::SUPPLY_VERDICT_PROMPT_KEY;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::supplied_items::{NewVerdict, PendingVerdict};
pub use prompt::SYSTEM as SYSTEM_PROMPT;
use prompt::{Answer, SYSTEM, schema, user_message, valid};

const LEASE_ROLE: &str = "supply_verdict";
const TICK_INTERVAL: Duration = Duration::from_secs(600);
const BATCH: i64 = 25;
const LOOKBACK_DAYS: i64 = 14;
const SETTLE_MINUTES: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Off,
    OutsideWindow,
    Busy,
    NotLeader,
    NoClient,
    Ran(Pass),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pass {
    pub judged: usize,
    pub failed: usize,
    pub database_failed: bool,
}

pub trait SupplyJudge {
    fn model(&self) -> &str;
    fn judge(
        &self,
        item: &PendingVerdict,
        user: &str,
    ) -> impl Future<Output = Result<Answer, String>> + Send;
}

struct ModelJudge {
    llm: OpenRouterClient,
    model: String,
    system: String,
}

impl SupplyJudge for ModelJudge {
    fn model(&self) -> &str {
        &self.model
    }

    async fn judge(&self, item: &PendingVerdict, user: &str) -> Result<Answer, String> {
        let llm = self.llm.with_call_context(LlmCallContext {
            session_id: Some(item.session_id),
            steerer_id: None,
            operator_turn_id: Some(item.turn_id),
        });
        llm.verdict::<Answer>(&self.system, user, schema())
            .await
            .map_err(|error| error.to_string())
    }
}

pub fn spawn_supply_verdict(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("supply_verdict: no persistence — not spawning");
        return;
    };
    let state = state.clone();
    let shutdown = state.shutdown();
    state.tracker().spawn(async move {
        let mut ticker = interval(TICK_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    let tick = run_tick(&state, &persist, Utc::now()).await;
                    report(tick);
                }
            }
        }
    });
}

pub async fn run_tick(state: &AppState, persist: &PersistHandle, now: DateTime<Utc>) -> Tick {
    if !state.feature_toggle_overlay.load().judge_lessons {
        return Tick::Off;
    }
    let settings = ProposerSettings::load(persist).await;
    if !settings.quiet.in_window(now.hour()) {
        return Tick::OutsideWindow;
    }
    let idle_since = now - chrono::Duration::seconds(settings.quiet.idle_secs);
    if persist
        .has_recent_primary_activity(idle_since)
        .await
        .unwrap_or(true)
    {
        return Tick::Busy;
    }
    if !persist.holds_worker_role(LEASE_ROLE, TICK_INTERVAL).await {
        return Tick::NotLeader;
    }
    judge_with_model(state, persist, settings, now).await
}

pub async fn run_now(state: &AppState, persist: &PersistHandle, now: DateTime<Utc>) -> Tick {
    if !state.feature_toggle_overlay.load().judge_lessons {
        return Tick::Off;
    }
    let settings = ProposerSettings::load(persist).await;
    judge_with_model(state, persist, settings, now).await
}

async fn judge_with_model(
    state: &AppState,
    persist: &PersistHandle,
    settings: ProposerSettings,
    now: DateTime<Utc>,
) -> Tick {
    let Some(llm) = client(state, &settings.effective_model) else {
        return Tick::NoClient;
    };
    let judge = ModelJudge {
        llm,
        model: settings.effective_model,
        system: system_prompt(persist).await,
    };
    let env = state.decision_env();
    match jev::JevJudge::new(&judge, env.as_ref()) {
        Some(decided) => Tick::Ran(judge_batch(persist, &decided, now).await),
        None => Tick::Ran(judge_batch(persist, &judge, now).await),
    }
}

async fn system_prompt(persist: &PersistHandle) -> String {
    let rows = persist.fetch_settings().await.unwrap_or_default();
    prompt_from_rows(&rows)
}

pub fn prompt_from_rows(rows: &[(String, serde_json::Value)]) -> String {
    rows.iter()
        .find(|(key, _)| key == SUPPLY_VERDICT_PROMPT_KEY)
        .and_then(|(_, value)| value.as_str())
        .filter(|text| !text.trim().is_empty())
        .unwrap_or(SYSTEM)
        .to_owned()
}

fn client(state: &AppState, model: &str) -> Option<OpenRouterClient> {
    let config = &state.config().steerer;
    let base = OpenRouterClient::new(
        String::new(),
        model.to_owned(),
        Some(config.openrouter_base_url.clone()),
        Some(Duration::from_secs(config.request_timeout_secs)),
    )
    .ok()?
    .with_attribution(LlmRole::JudgeWorker, state.llm_usage_sink.as_ref());
    crate::llm::key::effective_client(&base, &state.steerer_openrouter_key)
}

pub async fn judge_batch(
    persist: &PersistHandle,
    judge: &impl SupplyJudge,
    now: DateTime<Utc>,
) -> Pass {
    let mut pass = Pass::default();
    let items = match persist
        .lessons()
        .supplied_items_pending_verdict(
            BATCH,
            now - chrono::Duration::days(LOOKBACK_DAYS),
            now - chrono::Duration::minutes(SETTLE_MINUTES),
        )
        .await
    {
        Ok(items) => items,
        Err(error) => {
            tracing::warn!(%error, "supply_verdict: could not list items to judge");
            pass.database_failed = true;
            return pass;
        }
    };
    for item in items {
        match judge_one(persist, judge, &item).await {
            Ok(true) => pass.judged += 1,
            Ok(false) => pass.failed += 1,
            Err(()) => {
                pass.database_failed = true;
                break;
            }
        }
    }
    pass
}

async fn judge_one(
    persist: &PersistHandle,
    judge: &impl SupplyJudge,
    item: &PendingVerdict,
) -> Result<bool, ()> {
    let user = user_message(item);
    let Some(answer) = answer_for(judge, item, &user).await else {
        return Ok(false);
    };
    store_verdict(persist, judge, item, &user, &answer).await
}

async fn answer_for(judge: &impl SupplyJudge, item: &PendingVerdict, user: &str) -> Option<Answer> {
    checked(item.item_id, judge.judge(item, user).await)
}

fn checked(item: i64, outcome: Result<Answer, String>) -> Option<Answer> {
    match outcome {
        Ok(answer) if valid(&answer) => Some(answer),
        Ok(answer) => warn_unknown_verdict(item, &answer),
        Err(error) => warn_judge_failed(item, &error),
    }
}

fn warn_unknown_verdict(item: i64, answer: &Answer) -> Option<Answer> {
    tracing::warn!(item, verdict = %answer.verdict, "supply_verdict: unknown verdict");
    None
}

fn warn_judge_failed(item: i64, error: &str) -> Option<Answer> {
    tracing::warn!(item, %error, "supply_verdict: judge call failed");
    None
}

async fn store_verdict(
    persist: &PersistHandle,
    judge: &impl SupplyJudge,
    item: &PendingVerdict,
    user: &str,
    answer: &Answer,
) -> Result<bool, ()> {
    persist
        .lessons()
        .insert_supply_verdict(&NewVerdict {
            item_id: item.item_id,
            verdict: &answer.verdict,
            rationale: &answer.rationale,
            evidence: &answer.evidence,
            model: judge.model(),
            input_text: user,
        })
        .await
        .map_err(|error| {
            tracing::warn!(item = item.item_id, %error, "supply_verdict: verdict not stored");
        })
}

fn report(tick: Tick) {
    if let Tick::Ran(pass) = tick
        && (pass.judged > 0 || pass.failed > 0 || pass.database_failed)
    {
        tracing::info!(
            judged = pass.judged,
            failed = pass.failed,
            database_failed = pass.database_failed,
            "supply_verdict: pass finished"
        );
    }
}
