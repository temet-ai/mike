use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use super::SupplyJudge;
use super::prompt::Answer;
use crate::llm::decisions::env::{DecisionEnv, head_chars, tail_chars};
use crate::llm::decisions::point::{Asked, Decider, choice};
use crate::llm::decisions::{Question, record::DecisionSubject};
use crate::store::persistence::supplied_items::PendingVerdict;

pub const POINT: &str = "supply_verdict";
pub const QUESTION_SET: &str = "supply_verdict_v1";
const QUESTION: &str = "verdict";
const PROMPT_CHARS: usize = 1500;
const REPLY_CHARS: usize = 2500;
const NEXT_CHARS: usize = 800;
const MEMORY_CHARS: usize = 1200;
const MAX_FILES: usize = 30;

const VERDICTS: &[(&str, &str)] = &[
    (
        "used",
        "the turn's actions or reply followed or relied on what the memory says, and the operator did not correct the turn on that point",
    ),
    (
        "corrected_anyway",
        "the memory covered what the operator then corrected the turn about, so it was supplied and still not followed",
    ),
    (
        "ignored",
        "no sign the turn acted on the memory, including memory that had nothing to do with the turn",
    ),
];

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        QUESTION.to_owned(),
        choice(
            "Was the supplied memory used, corrected anyway, or ignored in this turn?",
            VERDICTS,
        ),
    )])
}

pub fn state(item: &PendingVerdict) -> Value {
    let files: Vec<&String> = item.files_touched.iter().take(MAX_FILES).collect();
    json!({
        "memory": {
            "kind": item.item_kind,
            "route": item.route,
            "text": head_chars(&item.item_text, MEMORY_CHARS),
        },
        "operator_prompt": head_chars(item.user_prompt.as_deref().unwrap_or(""), PROMPT_CHARS),
        "tools_used": item.tools_used,
        "files_touched": files,
        "final_reply": tail_chars(item.final_assistant_text.as_deref().unwrap_or(""), REPLY_CHARS),
        "turn_reaction_recorded_by_the_turn_judge": item.reaction.as_deref().unwrap_or("none"),
        "operator_next_message": item.next_message.as_deref().map(|m| head_chars(m, NEXT_CHARS)),
    })
}

pub fn compose(asked: &Asked) -> Option<(Answer, f64)> {
    if asked.failed() {
        return None;
    }
    let (verdict, probability) = asked.choice(QUESTION)?;
    VERDICTS.iter().any(|(k, _)| *k == verdict).then(|| {
        (
            Answer {
                verdict: verdict.to_owned(),
                rationale: format!("Jev chose {verdict} (probability {probability:.2})"),
                evidence: String::new(),
            },
            probability,
        )
    })
}

pub struct JevJudge<'a, J> {
    inner: &'a J,
    decider: Decider,
    model: String,
    tripped: AtomicBool,
    last_was_jev: AtomicBool,
}

impl<'a, J: SupplyJudge> JevJudge<'a, J> {
    pub fn new(inner: &'a J, env: Option<&DecisionEnv>) -> Option<Self> {
        let env = env?;
        Some(Self {
            inner,
            decider: env.decider(POINT)?,
            model: env.model(),
            tripped: AtomicBool::new(false),
            last_was_jev: AtomicBool::new(false),
        })
    }

    async fn ask(&self, item: &PendingVerdict) -> Option<Answer> {
        let asked = self
            .decider
            .ask(
                QUESTION_SET,
                state(item),
                questions(),
                DecisionSubject {
                    session_id: Some(item.session_id),
                    operator_turn_id: Some(item.turn_id),
                    ..DecisionSubject::default()
                },
                json!({}),
            )
            .await;
        if asked.failed() {
            self.tripped.store(true, Ordering::Relaxed);
        }
        let composed = compose(&asked);
        let applied = composed.as_ref().map_or("none", |_| "labelled");
        let score = composed.as_ref().map_or(0.0, |(_, p)| *p);
        self.decider
            .mark(&asked, asked.composed(json!({ "score": score })), applied)
            .await;
        composed.filter(|_| self.decider.applies()).map(|(a, _)| a)
    }
}

impl<J: SupplyJudge + Sync> SupplyJudge for JevJudge<'_, J> {
    fn model(&self) -> &str {
        if self.last_was_jev.load(Ordering::Relaxed) {
            &self.model
        } else {
            self.inner.model()
        }
    }

    async fn judge(&self, item: &PendingVerdict, user: &str) -> Result<Answer, String> {
        if !self.tripped.load(Ordering::Relaxed)
            && let Some(answer) = self.ask(item).await
        {
            self.last_was_jev.store(true, Ordering::Relaxed);
            return Ok(answer);
        }
        self.last_was_jev.store(false, Ordering::Relaxed);
        self.inner.judge(item, user).await
    }
}

#[cfg(test)]
#[path = "jev_tests.rs"]
mod tests;
