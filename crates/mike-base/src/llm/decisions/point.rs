use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::record::{Applied, DecisionContext, DecisionMode, DecisionSubject};
use super::{Answer, DecisionRequest, Question};
use crate::llm::OpenRouterClient;
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;

pub const INLINE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(800);

fn breaks(error: &crate::llm::LlmError) -> bool {
    matches!(
        error,
        crate::llm::LlmError::Timeout
            | crate::llm::LlmError::Transport(_)
            | crate::llm::LlmError::Http { .. }
    )
}

#[derive(Clone)]
pub struct Decider {
    client: OpenRouterClient,
    persist: PersistHandle,
    pub point: &'static str,
    pub mode: DecisionMode,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Asked {
    pub record_id: Option<i64>,
    pub answers: BTreeMap<String, Answer>,
    pub expected: usize,
    pub error: Option<String>,
}

pub fn point_mode(toggle_on: bool, overlay: &SettingsOverlay, point: &str) -> Option<DecisionMode> {
    if !toggle_on {
        return None;
    }
    let mode = overlay
        .decisions_point_modes
        .get(point)
        .map_or("on", String::as_str);
    match mode {
        "on" => Some(DecisionMode::On),
        "shadow" => Some(DecisionMode::Shadow),
        _ => None,
    }
}

impl Decider {
    pub fn for_point(
        base: &OpenRouterClient,
        persist: &PersistHandle,
        toggle_on: bool,
        overlay: &SettingsOverlay,
        point: &'static str,
    ) -> Option<Self> {
        let mode = point_mode(toggle_on, overlay, point)?;
        Self::with_mode(base, persist, overlay, point, mode)
    }

    pub fn with_mode(
        base: &OpenRouterClient,
        persist: &PersistHandle,
        overlay: &SettingsOverlay,
        point: &'static str,
        mode: DecisionMode,
    ) -> Option<Self> {
        let client = super::decision_client(base, overlay)
            .ok()?
            .with_decision_store(persist.clone());
        Some(Self {
            client,
            persist: persist.clone(),
            point,
            mode,
        })
    }

    pub fn applies(&self) -> bool {
        self.mode == DecisionMode::On
    }

    pub fn inline(mut self) -> Self {
        self.client = self.client.with_timeout(INLINE_TIMEOUT);
        self
    }

    pub fn model(&self) -> &str {
        self.client.model()
    }

    pub fn persist(&self) -> &PersistHandle {
        &self.persist
    }

    pub async fn ask(
        &self,
        question_set: &'static str,
        state: Value,
        questions: BTreeMap<String, Question>,
        subject: DecisionSubject,
        thresholds: Value,
    ) -> Asked {
        let mut context = DecisionContext::new(self.point, self.mode, question_set);
        context.subject = subject;
        context.thresholds = thresholds;
        let expected = questions.len();
        let request = DecisionRequest {
            model: self.client.model().to_owned(),
            state,
            questions,
        };
        let now = std::time::Instant::now();
        if let super::breaker::Gate::Open { .. } = super::breaker::SHARED.gate(now) {
            return Asked {
                expected,
                error: Some("decisions breaker open after repeated Jev failures".to_owned()),
                ..Asked::default()
            };
        }
        let outcome = self.client.decide(&request, &context).await;
        match &outcome.result {
            Ok(_) => super::breaker::SHARED.succeeded(),
            Err(e) if breaks(e) => super::breaker::SHARED.failed(now),
            Err(_) => {}
        }
        match outcome.result {
            Ok(response) => Asked {
                record_id: outcome.record_id,
                answers: response.answers,
                expected,
                error: None,
            },
            Err(error) => Asked {
                record_id: outcome.record_id,
                expected,
                error: Some(error.to_string()),
                ..Asked::default()
            },
        }
    }

    pub async fn mark(&self, asked: &Asked, composed: Value, applied: &'static str) {
        let Some(id) = asked.record_id else {
            return;
        };
        let mark = Applied {
            composed,
            applied,
            fail_open: asked.failed(),
        };
        if let Err(error) = super::store(&self.persist)
            .mark_decision_applied(id, &mark, None)
            .await
        {
            tracing::warn!(%error, record = id, point = self.point, "decisions: applied mark failed");
        }
    }

    pub async fn outcome(&self, asked: &Asked, label: &str, source: &str, evidence: Value) {
        if let Some(id) = asked.record_id {
            record_outcome(&self.persist, id, label, source, evidence).await;
        }
    }
}

pub async fn record_outcome(
    persist: &PersistHandle,
    record_id: i64,
    label: &str,
    source: &str,
    evidence: Value,
) {
    if let Err(error) = super::store(persist)
        .insert_decision_outcome(record_id, label, source, &evidence)
        .await
    {
        tracing::warn!(%error, record = record_id, "decisions: outcome insert failed");
    }
}

impl Asked {
    pub fn failed_with(error: &str) -> Self {
        Self {
            error: Some(error.to_owned()),
            expected: 1,
            ..Self::default()
        }
    }

    pub fn answered(answers: BTreeMap<String, Answer>) -> Self {
        let expected = answers.len();
        Self {
            answers,
            expected,
            ..Self::default()
        }
    }

    pub fn failed(&self) -> bool {
        self.error.is_some()
            || self.answers.len() < self.expected
            || self.answers.values().any(Answer::is_refusal)
    }

    pub fn noul(&self, question: &str) -> Option<f64> {
        self.answers.get(question).and_then(Answer::noul)
    }

    pub fn choice(&self, question: &str) -> Option<(&str, f64)> {
        match self.answers.get(question)? {
            Answer::Choice {
                choice,
                probabilities,
                confidence,
            } => {
                let p = probabilities
                    .get(choice)
                    .copied()
                    .or(*confidence)
                    .unwrap_or(1.0);
                Some((choice.as_str(), p))
            }
            _ => None,
        }
    }

    pub fn score(&self, question: &str) -> Option<f64> {
        match self.answers.get(question)? {
            Answer::Score { score, .. } => Some(*score),
            _ => None,
        }
    }

    pub fn composed(&self, verdict: Value) -> Value {
        json!({ "verdict": verdict, "answers": self.answers, "error": self.error })
    }
}

pub fn noul(instructions: &str, yes: &str, no: &str) -> Question {
    Question::Noul {
        instructions: instructions.to_owned(),
        criteria: super::NoulCriteria {
            yes: yes.to_owned(),
            no: no.to_owned(),
        },
    }
}

pub fn choice(instructions: &str, options: &[(&str, &str)]) -> Question {
    Question::Choice {
        instructions: instructions.to_owned(),
        criteria: options
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlay_with(point: &str, mode: &str) -> SettingsOverlay {
        let mut o = SettingsOverlay::default();
        o.decisions_point_modes
            .insert(point.to_owned(), mode.to_owned());
        o
    }

    #[test]
    fn point_mode_toggle_off_is_none_for_every_mode() {
        let o = overlay_with("turn_judge", "on");
        assert_eq!(point_mode(false, &o, "turn_judge"), None);
    }

    #[test]
    fn point_mode_defaults_to_on_and_honours_shadow_and_off() {
        let d = SettingsOverlay::default();
        assert_eq!(point_mode(true, &d, "turn_judge"), Some(DecisionMode::On));
        let s = overlay_with("turn_judge", "shadow");
        assert_eq!(
            point_mode(true, &s, "turn_judge"),
            Some(DecisionMode::Shadow)
        );
        let off = overlay_with("turn_judge", "off");
        assert_eq!(point_mode(true, &off, "turn_judge"), None);
    }

    #[test]
    fn asked_failed_on_error_missing_answer_or_refusal() {
        assert!(Asked::failed_with("timeout").failed());
        let mut a = Asked::answered(BTreeMap::from([(
            "q".to_owned(),
            Answer::Noul {
                noul: 0.9,
                confidence: None,
            },
        )]));
        assert!(!a.failed());
        assert_eq!(a.noul("q"), Some(0.9));
        a.expected = 2;
        assert!(a.failed());
        let r = Asked::answered(BTreeMap::from([("q".to_owned(), Answer::Refusal)]));
        assert!(r.failed());
    }

    #[test]
    fn asked_choice_reads_the_chosen_probability() {
        let a = Asked::answered(BTreeMap::from([(
            "pick".to_owned(),
            Answer::Choice {
                choice: "b".to_owned(),
                probabilities: BTreeMap::from([("a".to_owned(), 0.2), ("b".to_owned(), 0.8)]),
                confidence: None,
            },
        )]));
        assert_eq!(a.choice("pick"), Some(("b", 0.8)));
        assert_eq!(a.choice("other"), None);
    }
}
