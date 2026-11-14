use serde_json::{Value, json};

use super::questions::{OUTCOME, QUALITY, QUALITY_LEVELS};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;

pub const OUTCOME_AT: f64 = 0.5;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionDecision {
    pub outcome: &'static str,
    pub confidence: f64,
    pub quality: Option<f64>,
}

impl SessionDecision {
    pub fn summary(&self) -> String {
        let quality = self
            .quality
            .map_or_else(|| "none".to_owned(), |q| format!("{q:.2}"));
        format!(
            "decided in code from the session end: outcome {} ({:.2}), quality {quality}",
            self.outcome, self.confidence
        )
    }

    pub fn json(&self) -> Value {
        json!({
            "score": self.confidence,
            "outcome": self.outcome,
            "quality": self.quality,
        })
    }
}

fn known(choice: &str) -> Option<&'static str> {
    match choice {
        "success" => Some("success"),
        "partial" => Some("partial"),
        "failure" => Some("failure"),
        _ => None,
    }
}

pub fn quality_of(answer: &Answer) -> Option<f64> {
    let Answer::Score {
        score,
        probabilities,
        ..
    } = answer
    else {
        return None;
    };
    let top = (QUALITY_LEVELS.len() - 1) as f64;
    let weighted: Option<f64> = QUALITY_LEVELS
        .iter()
        .enumerate()
        .map(|(i, level)| {
            probabilities
                .get(&i.to_string())
                .or_else(|| probabilities.get(*level))
                .map(|p| p * (i as f64 / top))
        })
        .sum();
    let value = match weighted {
        Some(v) if probabilities.len() == QUALITY_LEVELS.len() => v,
        _ => *score / top,
    };
    Some(value.clamp(0.0, 1.0))
}

pub fn compose(asked: &Asked) -> Option<SessionDecision> {
    if asked.failed() {
        return None;
    }
    let (choice, confidence) = asked.choice(OUTCOME)?;
    let outcome = known(choice).filter(|_| confidence >= OUTCOME_AT);
    let quality = outcome
        .and_then(|_| asked.answers.get(QUALITY))
        .and_then(quality_of);
    Some(SessionDecision {
        outcome: outcome.unwrap_or("unknown"),
        confidence,
        quality,
    })
}
