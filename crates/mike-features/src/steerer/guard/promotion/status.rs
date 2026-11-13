use crate::store::persistence::eval_repo::EvalPersist as _;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::effect::{EffectEstimate, estimate_effect};
use super::service::gate_for;
use super::verifier_decision::{ReplacementInputs, decide_replacement};
use crate::steerer::guard::Question;
use crate::steerer::guard::bars::gate_bar;
use crate::steerer::guard::policy::GuardSettings;
use crate::store::persistence::steering::promotion::{ArmRow, PolicyRow, PromotionRow};
use crate::store::persistence::{PersistError, PersistHandle};

pub struct StatusRequest {
    pub steerer_id: i64,
    pub bundle_id: Option<i64>,
    pub verifier_id: Option<i64>,
    pub now: DateTime<Utc>,
}

fn count_by(rows: &[&ArmRow], key: impl Fn(&ArmRow) -> Option<String>) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for r in rows {
        *out.entry(key(r).unwrap_or_else(|| "none".to_owned()))
            .or_insert(0) += 1;
    }
    out
}

pub fn live_block(rows: &[ArmRow]) -> Value {
    let treated: Vec<&ArmRow> = rows
        .iter()
        .filter(|r| r.arm.as_deref() == Some("treated"))
        .collect();
    let holdback: Vec<&ArmRow> = rows
        .iter()
        .filter(|r| r.arm.as_deref() == Some("holdback"))
        .collect();
    let carried = treated.iter().filter(|r| r.delivered).count();
    let holdback_right = holdback
        .iter()
        .filter(|r| r.outcome_label.as_deref() == Some("right"))
        .count();
    let holdback_wrong = holdback
        .iter()
        .filter(|r| r.outcome_label.as_deref() == Some("wrong"))
        .count();
    json!({
        "arms": {"treated": treated.len(), "holdback": holdback.len()},
        "reach_at_due": {
            "treated": count_by(&treated, |r| r.reach_at_due.clone()),
            "holdback": count_by(&holdback, |r| r.reach_at_due.clone()),
        },
        "wake_states": count_by(&treated, |r| r.wake_state.clone()),
        "append_outcomes": {"carried": carried, "not_carried": treated.len() - carried},
        "holdback_precision": super::wilson::wilson_bounds(i64::try_from(holdback_right).unwrap_or(0), i64::try_from(holdback_right + holdback_wrong).unwrap_or(0)),
    })
}

async fn effect(
    persist: &PersistHandle,
    rows: &[ArmRow],
    question: Question,
    now: DateTime<Utc>,
) -> EffectEstimate {
    let turns: Vec<i64> = rows
        .iter()
        .filter(|r| r.scored_is_last)
        .filter_map(|r| r.scored_turn_id)
        .collect();
    let ends: HashMap<i64, String> = persist
        .eval()
        .session_end_labels_for_turns(&turns, now)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.end_turn_id, r.label))
        .collect();
    estimate_effect(rows, &ends, Some(question.code()), now.timestamp())
}

async fn verifier_block(
    persist: &PersistHandle,
    req: &StatusRequest,
    question: Question,
    gate_passed: bool,
    since: DateTime<Utc>,
) -> Value {
    let (Question::UnobservedDone, Some(verifier)) = (question, req.verifier_id) else {
        return Value::Null;
    };
    match persist
        .steerer()
        .fetch_verifier_pairs(verifier, req.steerer_id, since)
        .await
    {
        Ok(pairs) => json!(decide_replacement(&ReplacementInputs {
            gate_passed,
            pairs: &pairs,
            overrides_accounted: false,
            exempt_fires: 0,
        })),
        Err(e) => json!({"error": e.to_string()}),
    }
}

async fn one(
    persist: &PersistHandle,
    req: &StatusRequest,
    policy: &PolicyRow,
    promotions: &[PromotionRow],
    settings: &GuardSettings,
) -> Result<Value, PersistError> {
    let Some(question) = Question::parse(&policy.question) else {
        return Ok(Value::Null);
    };
    let bar = gate_bar(question);
    let mut gates = serde_json::Map::new();
    for target in bar.targets {
        let gate = gate_for(persist, req.steerer_id, question, target, req.bundle_id)
            .await
            .map(|g| g.to_json())
            .unwrap_or_else(|e| json!({"error": e.to_string()}));
        gates.insert((*target).to_owned(), gate);
    }
    let rows = persist
        .steerer()
        .fetch_arm_rows(req.steerer_id, Some(question.code()), req.now)
        .await?;
    let append_passes = gates
        .get("append")
        .and_then(|g| g["passes"].as_bool())
        .unwrap_or(false);
    Ok(json!({
        "question": question.code(),
        "policy": policy,
        "effective_holdback_pct": policy.holdback_pct.map_or(settings.holdback_pct, i64::from),
        "bar": bar,
        "gate": gates,
        "live": live_block(&rows),
        "effect": effect(persist, &rows, question, req.now).await,
        "verifier_comparison": verifier_block(persist, req, question, append_passes, policy.updated_at).await,
        "promotions": promotions.iter().filter(|p| p.question == policy.question).collect::<Vec<_>>(),
    }))
}

pub async fn questions_status(
    persist: &PersistHandle,
    req: &StatusRequest,
) -> Result<Value, PersistError> {
    let policies = persist.steerer().policy_rows(req.steerer_id).await?;
    let promotions = persist
        .steerer()
        .question_promotions(req.steerer_id)
        .await?;
    let settings = GuardSettings::load(persist).await;
    let mut questions = Vec::with_capacity(policies.len());
    for p in &policies {
        questions.push(one(persist, req, p, &promotions, &settings).await?);
    }
    Ok(json!({
        "steerer_id": req.steerer_id,
        "bundle_id": req.bundle_id,
        "settings": settings,
        "questions": questions,
        "note": "a promotion applies to fires within 60 s; a demotion also resolves pending wakes at the next tick after that",
    }))
}
