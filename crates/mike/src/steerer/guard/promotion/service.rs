use crate::store::persistence::steerer_repo::SteererPersist as _;
use chrono::Utc;
use serde_json::{Value, json};

use super::gate::{GateEvidence, GateInputs, Precondition, evaluate_gate, target_allowed};
use super::verifier_decision::{ReplacementInputs, decide_replacement};
use crate::dossier::steering_facts::wait_backing::REGISTRY_PARSES_SCHEDULED_LAUNCHES;
use crate::steerer::guard::Question;
use crate::steerer::guard::policy::{GuardSettings, forget_policies};
use crate::store::persistence::steering::promotion::{GateWindow, ModeChange, PolicyRow};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, thiserror::Error)]
pub(crate) enum PromotionError {
    #[error("no policy row for {0}")]
    UnknownPolicy(String),
    #[error("{0}")]
    TargetForbidden(String),
    #[error("{0}")]
    Refused(String),
    #[error("the gate fails; give an override reason to promote anyway")]
    GateFailed(Value),
    #[error("persistence error: {0}")]
    Persist(#[from] PersistError),
}

pub(crate) struct PromoteRequest<'a> {
    pub(crate) steerer_id: i64,
    pub(crate) question: Question,
    pub(crate) to_mode: &'a str,
    pub(crate) override_reason: Option<&'a str>,
    pub(crate) replaces_steerer_id: Option<i64>,
    pub(crate) decided_via: &'a str,
    pub(crate) bundle_id: Option<i64>,
    pub(crate) source_json: &'a str,
}

pub(crate) struct DemoteRequest<'a> {
    pub(crate) steerer_id: i64,
    pub(crate) question: Question,
    pub(crate) to_mode: &'a str,
    pub(crate) reason: &'a str,
    pub(crate) restore_steerer_id: Option<i64>,
    pub(crate) decided_via: &'a str,
    pub(crate) evidence: Value,
    pub(crate) source_json: &'a str,
}

async fn policy(
    persist: &PersistHandle,
    steerer_id: i64,
    question: Question,
) -> Result<PolicyRow, PromotionError> {
    persist
        .steerer()
        .policy_row(steerer_id, question.code())
        .await?
        .ok_or_else(|| PromotionError::UnknownPolicy(question.code().to_owned()))
}

async fn turn_judge_promoted(persist: &PersistHandle) -> bool {
    persist
        .fetch_settings()
        .await
        .unwrap_or_default()
        .iter()
        .find(|(k, _)| k == "turn_judge.live_bundle_id")
        .and_then(|(_, v)| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .is_some_and(|id| id > 0)
}

async fn preconditions(persist: &PersistHandle, question: Question) -> Vec<Precondition> {
    match question {
        Question::Relapse => vec![Precondition {
            name: "turn_judge_promoted",
            met: turn_judge_promoted(persist).await,
        }],
        Question::UnbackedWait => vec![Precondition {
            name: "registry_parses_scheduled_launches",
            met: REGISTRY_PARSES_SCHEDULED_LAUNCHES,
        }],
        _ => Vec::new(),
    }
}

pub(crate) async fn gate_for(
    persist: &PersistHandle,
    steerer_id: i64,
    question: Question,
    target: &str,
    bundle_id: Option<i64>,
) -> Result<GateEvidence, PromotionError> {
    let p = policy(persist, steerer_id, question).await?;
    let settings = GuardSettings::load(persist).await;
    let counts = persist
        .steerer()
        .fetch_gate_counts(&GateWindow {
            steerer_id,
            question: question.code(),
            bundle_id,
            since: p.updated_at,
            grace_secs: settings.grace_secs(question),
        })
        .await?;
    Ok(evaluate_gate(GateInputs {
        question,
        target,
        bundle_id,
        window_start: p.updated_at,
        counts,
        preconditions: preconditions(persist, question).await,
    }))
}

async fn verifier_check(
    persist: &PersistHandle,
    req: &PromoteRequest<'_>,
    gate: &GateEvidence,
) -> Result<Option<Value>, PromotionError> {
    if req.question != Question::UnobservedDone {
        return Ok(None);
    }
    let Some(verifier) = req.replaces_steerer_id else {
        if req.override_reason.is_none() {
            return Err(PromotionError::Refused(
                "promoting unobserved_done needs --replaces-steerer <verifier id> or an override reason: both would nudge the same turns".to_owned(),
            ));
        }
        return Ok(None);
    };
    let pairs = persist
        .steerer()
        .fetch_verifier_pairs(verifier, req.steerer_id, gate.window_start)
        .await?;
    let decision = decide_replacement(&ReplacementInputs {
        gate_passed: gate.passes,
        pairs: &pairs,
        overrides_accounted: req.override_reason.is_some(),
        exempt_fires: gate.counts.template_fires + gate.counts.other_fires,
    });
    Ok(Some(json!(decision)))
}

pub(crate) async fn promote(
    persist: &PersistHandle,
    req: &PromoteRequest<'_>,
) -> Result<(i64, Value), PromotionError> {
    if !target_allowed(req.question, req.to_mode) {
        return Err(PromotionError::TargetForbidden(format!(
            "{} cannot be promoted to {}",
            req.question.code(),
            req.to_mode
        )));
    }
    let p = policy(persist, req.steerer_id, req.question).await?;
    let gate = gate_for(
        persist,
        req.steerer_id,
        req.question,
        req.to_mode,
        req.bundle_id,
    )
    .await?;
    let verifier = verifier_check(persist, req, &gate).await?;
    let verifier_passes = verifier
        .as_ref()
        .is_none_or(|v| v["passes"].as_bool().unwrap_or(false));
    let passes = gate.passes && verifier_passes;
    let mut gate_json = gate.to_json();
    gate_json["verifier_comparison"] = verifier.unwrap_or(Value::Null);
    if !passes && req.override_reason.is_none() {
        return Err(PromotionError::GateFailed(gate_json));
    }
    let id = persist
        .steerer()
        .apply_mode_change(&ModeChange {
            policy_id: p.id,
            steerer_id: req.steerer_id,
            question: req.question.code(),
            from_mode: &p.mode,
            to_mode: req.to_mode,
            action: "promote",
            decided_via: req.decided_via,
            gate_passed: passes,
            gate_json: &gate_json,
            override_reason: req.override_reason,
            replaces_steerer_id: req.replaces_steerer_id,
            restored_steerer_id: None,
            source_json: req.source_json,
        })
        .await?;
    forget_policies(req.steerer_id).await;
    Ok((id, gate_json))
}

fn demotion_allowed(from: &str, to: &str) -> bool {
    matches!(to, "shadow" | "off") || (from == "wake" && to == "append")
}

pub(crate) async fn demote(
    persist: &PersistHandle,
    req: &DemoteRequest<'_>,
) -> Result<i64, PromotionError> {
    let p = policy(persist, req.steerer_id, req.question).await?;
    if !matches!(p.mode.as_str(), "append" | "wake") || !demotion_allowed(&p.mode, req.to_mode) {
        return Err(PromotionError::Refused(format!(
            "{} is {}; it cannot be demoted to {}",
            req.question.code(),
            p.mode,
            req.to_mode
        )));
    }
    let action = if req.decided_via == "auto" {
        "auto_demote"
    } else {
        "demote"
    };
    let evidence = json!({"reason": req.reason, "evidence": req.evidence, "at": Utc::now()});
    let id = persist
        .steerer()
        .apply_mode_change(&ModeChange {
            policy_id: p.id,
            steerer_id: req.steerer_id,
            question: req.question.code(),
            from_mode: &p.mode,
            to_mode: req.to_mode,
            action,
            decided_via: req.decided_via,
            gate_passed: false,
            gate_json: &evidence,
            override_reason: None,
            replaces_steerer_id: None,
            restored_steerer_id: req.restore_steerer_id,
            source_json: req.source_json,
        })
        .await?;
    if req.to_mode != "wake" {
        persist
            .steerer()
            .disable_pending_for_question(req.steerer_id, req.question.code())
            .await?;
    }
    forget_policies(req.steerer_id).await;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::demotion_allowed;

    #[test]
    fn demotion_goes_down_only() {
        assert!(demotion_allowed("wake", "append"));
        assert!(demotion_allowed("append", "shadow"));
        assert!(!demotion_allowed("append", "wake"));
    }
}
