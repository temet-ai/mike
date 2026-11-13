use crate::store::persistence::steerer_repo::SteererPersist as _;
use chrono::Utc;

use super::policy::forget_all;
use std::collections::HashMap;

use super::{GuardStep, GuardTurn, Trigger, evaluate, evaluate_idle, finish};
use crate::llm::{LlmRole, OpenRouterClient};
use crate::store::persistence::{PersistError, PersistHandle};

pub struct SeamTurn {
    pub session_db_id: i64,
    pub session_uuid: Option<uuid::Uuid>,
    pub operator_turn_id: i64,
    pub quiescence: bool,
    pub agent_id: Option<String>,
    pub text: String,
    pub toggle_on: bool,
    pub turn_judge_live: bool,
}

pub async fn evaluate_guard_turn(
    persist: &PersistHandle,
    openrouter_base: &str,
    seam: SeamTurn,
) -> Result<bool, PersistError> {
    forget_all().await;
    let steerer = persist.steerer().guard_steerer().await?.ok_or_else(|| {
        PersistError::DataInvariant("turn-guard steerer is not seeded".to_owned())
    })?;
    let llm = OpenRouterClient::new(
        "test-key".to_owned(),
        "test-guard-model".to_owned(),
        Some(openrouter_base.to_owned()),
        None,
    )
    .map_err(|e| PersistError::DataInvariant(e.to_string()))?
    .with_attribution(LlmRole::Steerer, None);
    let trigger = if seam.quiescence {
        Trigger::Quiescence
    } else {
        Trigger::TurnEnd
    };
    let turn = GuardTurn {
        persist,
        llm: &llm,
        steerer_id: steerer.id,
        bundle_id: None,
        session_db_id: seam.session_db_id,
        session_uuid: seam.session_uuid,
        operator_turn_id: seam.operator_turn_id,
        trigger,
        trigger_request_id: None,
        agent_id: seam.agent_id,
        persona: None,
        text: seam.text,
        verifier_pattern: None,
        toggle_on: seam.toggle_on,
        turn_judge_live: seam.turn_judge_live,
        note_limits: Some(crate::steerer::note_policy::NoteLimits::from_settings(
            &crate::settings::SettingsOverlay::default(),
        )),
        now: Utc::now(),
    };
    let outcome = match trigger {
        Trigger::TurnEnd => evaluate(&turn).await,
        Trigger::Quiescence => evaluate_idle(&turn).await,
    };
    let step = finish(&turn, outcome, &HashMap::new()).await;
    Ok(step == GuardStep::StopWindow)
}

pub fn arm_draw(
    salt: uuid::Uuid,
    steerer_id: i64,
    question: &str,
    anchor_turn_id: i64,
) -> Option<f64> {
    let question = super::Question::parse(question)?;
    Some(super::holdback::holdback_draw(
        salt,
        &super::holdback::ArmAnchor {
            steerer_id,
            question,
            anchor_turn_id,
        },
    ))
}

pub async fn run_demotion(persist: &PersistHandle) -> Vec<String> {
    forget_all().await;
    super::promotion::demotion::run_demotion_check(persist, Utc::now()).await
}

pub enum SeamPromotion {
    Promoted(i64),
    GateFailed(serde_json::Value),
    Refused(String),
}

pub async fn promote_question(
    persist: &PersistHandle,
    steerer_id: i64,
    question: &str,
    to_mode: &str,
    override_reason: Option<&str>,
) -> Result<SeamPromotion, PersistError> {
    use super::promotion::service::{PromoteRequest, PromotionError, promote};
    forget_all().await;
    let Some(q) = super::Question::parse(question) else {
        return Ok(SeamPromotion::Refused(format!(
            "unknown question {question}"
        )));
    };
    let source = crate::change_log::ChangeSource::new(crate::change_log::ChangeVia::Api)
        .to_setting_json(&crate::change_log::current_build());
    let result = promote(
        persist,
        &PromoteRequest {
            steerer_id,
            question: q,
            to_mode,
            override_reason,
            replaces_steerer_id: None,
            decided_via: "api",
            bundle_id: None,
            source_json: &source,
        },
    )
    .await;
    match result {
        Ok((id, _)) => Ok(SeamPromotion::Promoted(id)),
        Err(PromotionError::GateFailed(gate)) => Ok(SeamPromotion::GateFailed(gate)),
        Err(PromotionError::Persist(e)) => Err(e),
        Err(other) => Ok(SeamPromotion::Refused(other.to_string())),
    }
}
