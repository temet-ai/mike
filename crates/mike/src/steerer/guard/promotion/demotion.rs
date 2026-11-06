use std::collections::HashMap;

use crate::logged::ok_or_warn;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::json;

use super::effect::{EffectEstimate, estimate_effect};
use super::service::{DemoteRequest, demote};
use super::wilson::wilson_bounds;
use crate::steerer::guard::Question;
use crate::steerer::guard::bars::promotion_bar;
use crate::steerer::guard::policy::{GuardSettings, QuestionMode, question_policies};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_metrics::{AlarmKind, Firing};

pub const LIVE_WINDOW_N: i64 = 30;
pub const PRECISION_MARGIN: f64 = 0.10;
pub const HARM_MIN_HOLDBACK: usize = 10;
pub const EARLY_MIN_LABELS: i64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DemotionParams {
    pub bar: f64,
    pub wrong_wake_ceiling_per_week: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum DemotionRule {
    PrecisionLoss { right: i64, wrong: i64 },
    MeasuredHarm { ci: (f64, f64), holdback_n: usize },
    WrongWakes { this_week: f64, last_week: f64 },
}

impl DemotionRule {
    pub fn alarm(&self) -> AlarmKind {
        match self {
            Self::PrecisionLoss { .. } => AlarmKind::SteerQuestionDemoted,
            Self::MeasuredHarm { .. } => AlarmKind::SteerHoldbackHarm,
            Self::WrongWakes { .. } => AlarmKind::SteerWrongWakeCeiling,
        }
    }
}

pub struct DemotionInputs<'a> {
    pub mode: QuestionMode,
    pub window: (i64, i64),
    pub effect: &'a EffectEstimate,
    pub wrong_wakes: (f64, f64),
    pub params: DemotionParams,
}

pub fn decide_demotion(i: &DemotionInputs<'_>) -> Option<(DemotionRule, &'static str)> {
    let (right, wrong) = i.window;
    let n = right + wrong;
    if n >= LIVE_WINDOW_N && (right as f64 / n as f64) < i.params.bar - PRECISION_MARGIN {
        return Some((DemotionRule::PrecisionLoss { right, wrong }, "shadow"));
    }
    if (EARLY_MIN_LABELS..LIVE_WINDOW_N).contains(&n)
        && wilson_bounds(right, n)
            .high
            .is_some_and(|upper| upper < i.params.bar)
    {
        return Some((DemotionRule::PrecisionLoss { right, wrong }, "shadow"));
    }
    if let Some(ci) = i.effect.primary.ci
        && i.effect.primary.holdback.n >= HARM_MIN_HOLDBACK
        && ci.1 < 0.0
    {
        return Some((
            DemotionRule::MeasuredHarm {
                ci,
                holdback_n: i.effect.primary.holdback.n,
            },
            "shadow",
        ));
    }
    let (this_week, last_week) = i.wrong_wakes;
    let ceiling = i.params.wrong_wake_ceiling_per_week;
    if i.mode == QuestionMode::Wake && this_week > ceiling && last_week > ceiling {
        return Some((
            DemotionRule::WrongWakes {
                this_week,
                last_week,
            },
            "append",
        ));
    }
    None
}

async fn wrong_wakes(
    persist: &PersistHandle,
    steerer_id: i64,
    question: Question,
    window: (i64, i64),
    now: DateTime<Utc>,
) -> (f64, f64) {
    let holdback_precision = {
        let n = window.0 + window.1;
        if n > 0 {
            window.0 as f64 / n as f64
        } else {
            1.0
        }
    };
    let mut weeks = [0.0; 2];
    for (i, w) in weeks.iter_mut().enumerate() {
        let to = now - Duration::weeks(i64::try_from(i).unwrap_or(0));
        let (pushed, labelled_wrong) = persist
            .fetch_wake_counts(steerer_id, question.code(), to - Duration::weeks(1), to)
            .await
            .unwrap_or_default();
        *w = labelled_wrong as f64 + pushed as f64 * (1.0 - holdback_precision);
    }
    (weeks[0], weeks[1])
}

async fn effect_for(
    persist: &PersistHandle,
    steerer_id: i64,
    question: Question,
    now: DateTime<Utc>,
) -> EffectEstimate {
    let rows = persist
        .fetch_arm_rows(steerer_id, Some(question.code()), now)
        .await
        .unwrap_or_default();
    let turns: Vec<i64> = rows
        .iter()
        .filter(|r| r.scored_is_last)
        .filter_map(|r| r.scored_turn_id)
        .collect();
    let ends: HashMap<i64, String> = persist
        .session_end_labels_for_turns(&turns, now)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.end_turn_id, r.label))
        .collect();
    estimate_effect(&rows, &ends, Some(question.code()), now.timestamp())
}

async fn raise(
    persist: &PersistHandle,
    rule: &DemotionRule,
    question: Question,
    evidence: serde_json::Value,
) {
    let firing = Firing {
        kind: rule.alarm(),
        subject: format!("turn-guard/{}", question.code()),
        detail: evidence,
    };
    ok_or_warn(
        persist.apply_alarm_pass(&[], &[firing], i32::MAX).await,
        "turn_guard.demotion_alarm",
    );
}

async fn demotion_for(
    persist: &PersistHandle,
    guard_id: i64,
    question: Question,
    policy: &crate::steerer::guard::policy::QuestionPolicy,
    ctx: (&GuardSettings, DateTime<Utc>),
) -> Option<(DemotionRule, &'static str, serde_json::Value)> {
    let (settings, now) = ctx;
    let window = ok_or_warn(
        persist
            .fetch_live_precision_window(guard_id, question.code(), LIVE_WINDOW_N)
            .await,
        "turn_guard.demotion_window",
    )
    .unwrap_or_default();
    let effect = effect_for(persist, guard_id, question, now).await;
    let inputs = DemotionInputs {
        mode: policy.mode,
        window,
        effect: &effect,
        wrong_wakes: wrong_wakes(persist, guard_id, question, window, now).await,
        params: DemotionParams {
            bar: promotion_bar(question),
            wrong_wake_ceiling_per_week: settings.wrong_wake_ceiling_per_week as f64,
        },
    };
    let (rule, to_mode) = decide_demotion(&inputs)?;
    let evidence =
        json!({"rule": rule, "effect": effect, "window": {"right": window.0, "wrong": window.1}});
    Some((rule, to_mode, evidence))
}

async fn apply_demotion(
    persist: &PersistHandle,
    guard_id: i64,
    question: Question,
    decided: (DemotionRule, &'static str, serde_json::Value),
) -> bool {
    let (rule, to_mode, evidence) = decided;
    let source = crate::change_log::ChangeSource::new(crate::change_log::ChangeVia::Mike)
        .with_detail("action", json!("auto_demote"))
        .with_detail("rule", json!(rule))
        .to_setting_json(&crate::change_log::current_build());
    let request = DemoteRequest {
        steerer_id: guard_id,
        question,
        to_mode,
        reason: "automatic demotion",
        restore_steerer_id: None,
        decided_via: "auto",
        evidence: evidence.clone(),
        source_json: &source,
    };
    if ok_or_warn(demote(persist, &request).await, "turn_guard.auto_demote").is_none() {
        return false;
    }
    raise(persist, &rule, question, evidence).await;
    true
}

pub async fn run_demotion_check(persist: &PersistHandle, now: DateTime<Utc>) -> Vec<String> {
    let settings = GuardSettings::load(persist).await;
    if !settings.auto_demote {
        return Vec::new();
    }
    let Some(guard) =
        ok_or_warn(persist.guard_steerer().await, "turn_guard.demotion_steerer").flatten()
    else {
        return Vec::new();
    };
    let policies = question_policies(persist, guard.id).await;
    let mut demoted = Vec::new();
    for (question, policy) in policies.iter().filter(|(_, p)| p.mode.is_live()) {
        let Some(decided) =
            demotion_for(persist, guard.id, *question, policy, (&settings, now)).await
        else {
            continue;
        };
        if apply_demotion(persist, guard.id, *question, decided).await {
            demoted.push(question.code().to_owned());
        }
    }
    demoted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steerer::guard::promotion::effect::{ArmSummary, Estimate};

    fn effect(ci: Option<(f64, f64)>, holdback_n: usize) -> EffectEstimate {
        let s = |n| ArmSummary {
            n,
            bad: 0,
            p_bad: None,
        };
        let e = Estimate {
            treated: s(40),
            holdback: s(holdback_n),
            rd: None,
            ci,
        };
        EffectEstimate {
            question: None,
            as_of_secs: 0,
            primary: e.clone(),
            secondary: e.clone(),
            per_protocol_not_randomised: e,
            missing: Default::default(),
            events: 0,
            status: "monitoring",
        }
    }

    fn inputs(window: (i64, i64), e: &EffectEstimate, mode: QuestionMode) -> DemotionInputs<'_> {
        DemotionInputs {
            mode,
            window,
            effect: e,
            wrong_wakes: (0.0, 0.0),
            params: DemotionParams {
                bar: 0.85,
                wrong_wake_ceiling_per_week: 2.0,
            },
        }
    }

    #[test]
    fn precision_below_the_bar_demotes_to_shadow() {
        let e = effect(None, 0);
        let d = decide_demotion(&inputs((20, 10), &e, QuestionMode::Append));
        assert!(matches!(
            d,
            Some((DemotionRule::PrecisionLoss { .. }, "shadow"))
        ));
        assert!(decide_demotion(&inputs((27, 3), &e, QuestionMode::Append)).is_none());
    }

    #[test]
    fn a_question_the_operator_marks_wrong_every_time_demotes_before_thirty_labels() {
        let e = effect(None, 0);
        assert!(matches!(
            decide_demotion(&inputs((0, 3), &e, QuestionMode::Append)),
            Some((DemotionRule::PrecisionLoss { .. }, "shadow"))
        ));
        assert!(decide_demotion(&inputs((0, 2), &e, QuestionMode::Append)).is_none());
        assert!(decide_demotion(&inputs((12, 1), &e, QuestionMode::Append)).is_none());
    }

    #[test]
    fn measured_harm_demotes_only_with_enough_holdback() {
        let harm = effect(Some((-0.4, -0.05)), 12);
        assert!(matches!(
            decide_demotion(&inputs((5, 0), &harm, QuestionMode::Append)),
            Some((DemotionRule::MeasuredHarm { .. }, _))
        ));
        let small = effect(Some((-0.4, -0.05)), 5);
        assert!(decide_demotion(&inputs((5, 0), &small, QuestionMode::Append)).is_none());
    }

    #[test]
    fn wrong_wakes_two_weeks_running_move_wake_to_append() {
        let e = effect(None, 0);
        let mut i = inputs((5, 0), &e, QuestionMode::Wake);
        i.wrong_wakes = (3.0, 2.5);
        assert!(matches!(
            decide_demotion(&i),
            Some((DemotionRule::WrongWakes { .. }, "append"))
        ));
        i.wrong_wakes = (3.0, 1.0);
        assert!(decide_demotion(&i).is_none());
    }
}
