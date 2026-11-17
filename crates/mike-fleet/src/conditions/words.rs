use chrono::{DateTime, Utc};

use super::fire::send_mail;
use super::words_prompt::{SYSTEM_PROMPT, WordsAnswer, WordsVerdict, ask_body, digest, schema};
use crate::base_state::BaseState;
use crate::llm::side_call::SideCallChain;
use crate::roster_worker::build_client;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::conditions::holds::sql_check_due;
use crate::store::persistence::conditions::{ConditionRow, WordsSpec};
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;

const TURNS_READ: i64 = 3;
const TURN_CHARS: i32 = 2000;

pub async fn judge(
    state: &BaseState,
    persist: &PersistHandle,
    row: &ConditionRow,
    spec: &WordsSpec,
    now: DateTime<Utc>,
) -> Result<bool, String> {
    if !state.feature_toggle_overlay.load().side_calls || !sql_check_due(row, now) {
        return Ok(false);
    }
    let since = spec.since.unwrap_or(row.not_before);
    let gate = persist
        .fleet()
        .words_gate(row.id, spec.session_uuid, since)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("watched session {} is not known to mike", spec.session_uuid))?;
    if !gate.turn_landed {
        return Ok(false);
    }
    persist
        .fleet()
        .stamp_condition_checked(row.id)
        .await
        .map_err(|e| e.to_string())?;
    let read_at = Utc::now();
    let turns = persist
        .launch()
        .roster_recent_turns(gate.session_id, TURNS_READ, TURN_CHARS)
        .await
        .map_err(|e| e.to_string())?;
    let Some(answer) = ask_the_chain(state, row, &digest(&spec.text, &turns)).await else {
        return Ok(false);
    };
    persist
        .fleet()
        .mark_condition_judged(row.id, read_at)
        .await
        .map_err(|e| e.to_string())?;
    if answer.answer == WordsVerdict::Unsure {
        ask_once(state, persist, row, spec).await;
    }
    Ok(answer.answer == WordsVerdict::Holds)
}

async fn ask_the_chain(state: &BaseState, row: &ConditionRow, user: &str) -> Option<WordsAnswer> {
    let models = state.settings_overlay.load().side_call_models.clone();
    let chain = SideCallChain::from_models(&models, |model| build_client(state, model));
    match chain
        .verdict::<WordsAnswer>(SYSTEM_PROMPT, user, schema())
        .await
    {
        Ok(outcome) => {
            tracing::info!(
                condition = %row.condition_uuid,
                model = outcome.model,
                answer = ?outcome.value.answer,
                reason = outcome.value.reason,
                "conditions: words judged"
            );
            Some(outcome.value)
        }
        Err(error) => {
            tracing::warn!(condition = %row.condition_uuid, %error, "conditions: no model judged the words; trying again later");
            None
        }
    }
}

async fn ask_once(
    state: &BaseState,
    persist: &PersistHandle,
    row: &ConditionRow,
    spec: &WordsSpec,
) {
    match persist.fleet().mark_condition_asked(row.id).await {
        Ok(true) => {
            if let Err(error) =
                send_mail(state, persist, spec.session_uuid, &ask_body(&spec.text)).await
            {
                tracing::warn!(condition = %row.condition_uuid, error, "conditions: asking the watched session failed");
            }
        }
        Ok(false) => {}
        Err(error) => {
            tracing::warn!(condition = %row.condition_uuid, %error, "conditions: recording the ask failed");
        }
    }
}
