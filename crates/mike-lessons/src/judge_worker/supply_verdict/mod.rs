use crate::store::persistence::lessons_repo::LessonsPersist as _;
pub mod prompt;
#[cfg(test)]
mod prompt_settings_tests;
#[cfg(test)]
mod tests;

use std::time::Duration;

use chrono::{DateTime, Timelike, Utc};
use serde_json::{Value, json};
use tokio::time::{MissedTickBehavior, interval};

use crate::lesson_proposer::ProposerSettings;
use crate::lessons_settings::meta_memory_loop::SUPPLY_VERDICT_PROMPT_KEY;
use crate::llm::{LlmCallContext, LlmError, LlmRole, OpenRouterClient};
use crate::memory_workers::{self, SUPPLY_VERDICT, Skip};
use crate::settings::memory_prompts::text_from_rows;
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
    pub rate_limited: bool,
}

impl Pass {
    pub fn counts(&self) -> Value {
        json!({
            "judged": self.judged,
            "failed": self.failed,
            "database_failed": self.database_failed,
            "rate_limited": self.rate_limited,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JudgeError {
    #[error("{0}")]
    RateLimited(String),
    #[error("{0}")]
    Failed(String),
}

impl From<String> for JudgeError {
    fn from(error: String) -> Self {
        Self::Failed(error)
    }
}

impl From<LlmError> for JudgeError {
    fn from(error: LlmError) -> Self {
        if error.is_refusal() {
            Self::RateLimited(error.to_string())
        } else {
            Self::Failed(error.to_string())
        }
    }
}

pub trait SupplyJudge {
    fn model(&self) -> &str;
    fn judge(
        &self,
        item: &PendingVerdict,
        user: &str,
    ) -> impl Future<Output = Result<Answer, JudgeError>> + Send;
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

    async fn judge(&self, item: &PendingVerdict, user: &str) -> Result<Answer, JudgeError> {
        let llm = self.llm.with_call_context(LlmCallContext {
            session_id: Some(item.session_id),
            steerer_id: None,
            operator_turn_id: Some(item.turn_id),
        });
        Ok(llm.verdict::<Answer>(&self.system, user, schema()).await?)
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
        seed_last_run(&persist).await;
        let mut ticker = interval(TICK_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    run_tick(&state, &persist, Utc::now()).await;
                }
            }
        }
    });
}

async fn seed_last_run(persist: &PersistHandle) {
    let newest = crate::store::persistence::judge::newest_supply_verdict_at(persist.pool()).await;
    memory_workers::seed(SUPPLY_VERDICT, newest.ok().flatten());
}

pub async fn run_tick(state: &AppState, persist: &PersistHandle, now: DateTime<Utc>) -> Tick {
    let tick = gated_tick(state, persist, now).await;
    record(tick, now);
    tick
}

async fn gated_tick(state: &AppState, persist: &PersistHandle, now: DateTime<Utc>) -> Tick {
    if !state.feature_toggle_overlay.load().judge_lessons {
        return Tick::Off;
    }
    let settings = ProposerSettings::load(persist).await;
    let idle_since = now - chrono::Duration::seconds(settings.quiet.idle_secs);
    let idle = !persist
        .has_recent_primary_activity(idle_since)
        .await
        .unwrap_or(true);
    let in_window = settings.quiet.in_window(now.hour());
    let last_ran_at = memory_workers::snapshot(SUPPLY_VERDICT).last_ran_at;
    match memory_workers::gate(in_window, last_ran_at, now, idle) {
        Some(Skip::OutsideWindow) => return Tick::OutsideWindow,
        Some(_) => return Tick::Busy,
        None => {}
    }
    if !persist.holds_worker_role(LEASE_ROLE, TICK_INTERVAL).await {
        return Tick::NotLeader;
    }
    judge_with_model(state, persist, settings, now).await
}

pub async fn run_now(state: &AppState, persist: &PersistHandle, now: DateTime<Utc>) -> Tick {
    let tick = if state.feature_toggle_overlay.load().judge_lessons {
        let settings = ProposerSettings::load(persist).await;
        judge_with_model(state, persist, settings, now).await
    } else {
        Tick::Off
    };
    record(tick, now);
    tick
}

pub fn skip_of(tick: Tick) -> Result<Pass, Skip> {
    match tick {
        Tick::Ran(pass) => Ok(pass),
        Tick::Off => Err(Skip::Off),
        Tick::OutsideWindow => Err(Skip::OutsideWindow),
        Tick::Busy => Err(Skip::Busy),
        Tick::NotLeader => Err(Skip::NotLeader),
        Tick::NoClient => Err(Skip::NoClient),
    }
}

fn record(tick: Tick, now: DateTime<Utc>) {
    match skip_of(tick) {
        Ok(pass) => memory_workers::record_ran(SUPPLY_VERDICT, Utc::now(), &pass.counts()),
        Err(reason) => memory_workers::record_skip(SUPPLY_VERDICT, now, reason),
    }
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
    memory_workers::mark_running(SUPPLY_VERDICT, Utc::now());
    Tick::Ran(judge_batch(persist, &judge, now).await)
}

async fn system_prompt(persist: &PersistHandle) -> String {
    let rows = persist.fetch_settings().await.unwrap_or_default();
    prompt_from_rows(&rows)
}

pub fn prompt_from_rows(rows: &[(String, Value)]) -> String {
    text_from_rows(rows, SUPPLY_VERDICT_PROMPT_KEY, SYSTEM)
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

enum Judged {
    Stored,
    Failed,
    RateLimited,
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
            Ok(Judged::Stored) => pass.judged += 1,
            Ok(Judged::Failed) => pass.failed += 1,
            Ok(Judged::RateLimited) => {
                pass.rate_limited = true;
                break;
            }
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
) -> Result<Judged, ()> {
    let user = user_message(item);
    let answer = match judge.judge(item, &user).await {
        Err(JudgeError::RateLimited(error)) => return Ok(rate_limited(item.item_id, &error)),
        outcome => checked(item.item_id, outcome),
    };
    let Some(answer) = answer else {
        return Ok(Judged::Failed);
    };
    let stored = store_verdict(persist, judge, item, &user, &answer).await?;
    Ok(if stored {
        Judged::Stored
    } else {
        Judged::Failed
    })
}

fn rate_limited(item: i64, error: &str) -> Judged {
    tracing::info!(
        item,
        %error,
        "supply_verdict: the call limiter refused; the rest of the batch waits for the next pass"
    );
    Judged::RateLimited
}

fn checked(item: i64, outcome: Result<Answer, JudgeError>) -> Option<Answer> {
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

fn warn_judge_failed(item: i64, error: &JudgeError) -> Option<Answer> {
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
