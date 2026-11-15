use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use super::prompt::Answer;
use super::{JudgeError, SupplyJudge};
use crate::llm::decisions::env::{DecisionEnv, head_chars, tail_chars};
use crate::llm::decisions::point::{Asked, Decider, noul};
use crate::llm::decisions::{Question, record::DecisionSubject};
use crate::store::persistence::supplied_items::PendingVerdict;

pub const POINT: &str = "supply_verdict";
pub const QUESTION_SET: &str = "supply_verdict_v2";
const SUBJECT: &str = "subject_matches";
const ACTED: &str = "acted_on_memory";
const PROMPT_CHARS: usize = 1500;
const REPLY_CHARS: usize = 2500;
const NEXT_CHARS: usize = 800;
const MEMORY_CHARS: usize = 1200;
const MAX_FILES: usize = 30;

const FLOOR: f64 = 0.5;

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([
        (
            SUBJECT.to_owned(),
            noul(
                "Does the memory's subject match what was corrected or asked in this turn: what operator_prompt asks for, or what operator_next_message corrects?",
                "The memory is about the same thing the operator asked for or corrected in this turn.",
                "The memory is about something else than what the operator asked for or corrected in this turn.",
            ),
        ),
        (
            ACTED.to_owned(),
            noul(
                "Did the turn act on the memory: do tools_used, files_touched or final_reply follow or rely on what memory.text says?",
                "The turn's actions or reply followed or relied on what the memory says.",
                "Nothing the turn did or said follows or relies on the memory.",
            ),
        ),
    ])
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

pub fn verdict_of(acted: f64, subject: f64, reaction: Option<&str>) -> (&'static str, f64) {
    if acted >= FLOOR {
        return ("used", acted);
    }
    let corrected = reaction == Some("corrected");
    if corrected && subject >= FLOOR {
        return ("corrected_anyway", (1.0 - acted) * subject);
    }
    let not_acted = 1.0 - acted;
    if corrected {
        ("ignored", not_acted * (1.0 - subject))
    } else {
        ("ignored", not_acted)
    }
}

pub fn compose(asked: &Asked, reaction: Option<&str>) -> Option<(Answer, f64)> {
    if asked.failed() {
        return None;
    }
    let acted = asked.noul(ACTED)?;
    let subject = asked.noul(SUBJECT)?;
    let (verdict, probability) = verdict_of(acted, subject, reaction);
    Some((
        Answer {
            verdict: verdict.to_owned(),
            rationale: format!(
                "Jev: acted on memory {acted:.2}, subject matches {subject:.2}, turn reaction {}, so {verdict} (probability {probability:.2})",
                reaction.unwrap_or("none")
            ),
            evidence: String::new(),
        },
        probability,
    ))
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
        let composed = compose(&asked, item.reaction.as_deref());
        let applied = composed.as_ref().map_or("none", |_| "labelled");
        let score = composed.as_ref().map_or(0.0, |(_, p)| *p);
        let chosen = composed.as_ref().map(|(a, _)| a.verdict.as_str());
        self.decider
            .mark(
                &asked,
                asked.composed(json!({
                    "score": score,
                    "item_id": item.item_id,
                    "verdict": chosen,
                    "reaction": item.reaction,
                })),
                applied,
            )
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

    async fn judge(&self, item: &PendingVerdict, user: &str) -> Result<Answer, JudgeError> {
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
