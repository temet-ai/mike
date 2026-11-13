use serde::Serialize;

use super::review::{Decision, Review};
use super::trigger::{QueryResult, TriggerEval};
use crate::lesson_proposer::skill_findings::{Findings, TriggerFinding};

pub const MIN_TRIGGER_SCORE: f64 = 0.80;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Verdict {
    pub accepted: bool,
    pub min_trigger_score: f64,
    pub reasons: Vec<String>,
}

fn weak_trigger(score: f64, which: &str) -> Option<String> {
    (score < MIN_TRIGGER_SCORE)
        .then(|| format!("{which} trigger score {score:.2} is below {MIN_TRIGGER_SCORE:.2}"))
}

fn covered_by(review: &Review) -> String {
    review
        .overlap
        .as_ref()
        .map_or_else(String::new, |name| format!(" (already covered by {name})"))
}

fn destination(review: &Review) -> String {
    let place = review
        .belongs_in
        .and_then(|p| serde_json::to_value(p).ok())
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    match &review.overlap {
        Some(name) => format!(" ({place}: {name})"),
        None => format!(" ({place})"),
    }
}

pub fn decide(first: &TriggerEval, recheck: Option<&TriggerEval>, review: &Review) -> Verdict {
    let reasons: Vec<String> = match review.decision {
        Decision::Approve => weak_trigger(first.score, "the draft's")
            .map(|r| format!("{r} and the reviewer approved it without a patch"))
            .into_iter()
            .collect(),
        Decision::Patch => match recheck {
            Some(again) => weak_trigger(again.score, "the patched draft's")
                .into_iter()
                .collect(),
            None => vec!["the patched draft was not re-checked".to_owned()],
        },
        Decision::NothingToSave => vec![format!(
            "the reviewer found nothing to save{}: {}",
            covered_by(review),
            review.reason
        )],
        Decision::WrongPlace => vec![format!(
            "the reviewer found the content belongs elsewhere{}: {}",
            destination(review),
            review.reason
        )],
        Decision::Unusable => vec![review.reason.clone()],
    };
    Verdict {
        accepted: reasons.is_empty(),
        min_trigger_score: MIN_TRIGGER_SCORE,
        reasons,
    }
}

pub fn findings_of(results: &[QueryResult]) -> Findings {
    let finding = |r: &QueryResult| TriggerFinding {
        query: r.query.clone(),
        loaded: r.loaded.clone(),
    };
    Findings {
        missed_triggers: results
            .iter()
            .filter(|r| r.should_trigger && !r.triggered)
            .map(finding)
            .collect(),
        wrong_triggers: results
            .iter()
            .filter(|r| !r.should_trigger && r.triggered)
            .map(finding)
            .collect(),
    }
}

pub fn findings(first: &TriggerEval, recheck: Option<&TriggerEval>) -> Findings {
    findings_of(&recheck.unwrap_or(first).results)
}
