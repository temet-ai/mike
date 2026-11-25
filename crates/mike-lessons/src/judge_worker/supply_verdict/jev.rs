use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use super::prompt::Answer;
use super::{JudgeError, SupplyJudge};
use crate::llm::decisions::env::{DecisionEnv, head_chars, tail_chars};
use crate::llm::decisions::operator_text::typed_by_operator;
use crate::llm::decisions::point::{Asked, Decider, noul};
use crate::llm::decisions::{Question, record::DecisionSubject};
use crate::store::persistence::supplied_items::PendingVerdict;
use crate::wire::mail_text::MAIL_LEAD;

pub const POINT: &str = "supply_verdict";
pub const QUESTION_SET: &str = "supply_verdict_v3";
const SUBJECT: &str = "subject_matches";
const ACTED: &str = "acted_on_memory";
const PROMPT_CHARS: usize = 1500;
const REPLY_CHARS: usize = 2500;
const NEXT_CHARS: usize = 800;
const MEMORY_CHARS: usize = 1200;
const MAX_FILES: usize = 30;

pub const YES_AT: f64 = 0.7;
pub const NO_AT: f64 = 0.3;

const CARD_TAGS: [&str; 3] = ["[thread:", "[entry:", "[keeper:"];
const REMINDER_OPEN: &str = "<system-reminder";
const REMINDER_CLOSE: &str = "</system-reminder>";
const CHANNEL_OPEN: &str = "<channel ";
const CHANNEL_CLOSE: &str = "</channel>";

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([
        (
            SUBJECT.to_owned(),
            noul(
                "Does the memory's subject match what was corrected or asked in this turn: what `operator_prompt` asks for, or what `operator_next_message` corrects?",
                "The memory is about the same thing the operator asked for or corrected in this turn.",
                "The memory is about something else than what the operator asked for or corrected in this turn.",
            ),
        ),
        (
            ACTED.to_owned(),
            noul(
                "Did the turn act on the memory: do `tools_used`, `files_touched` or `final_reply` follow or rely on what `memory.text` says?",
                "The turn's actions or reply followed or relied on what the memory says.",
                "Nothing the turn did or said follows or relies on the memory.",
            ),
        ),
    ])
}

fn without_card_tags(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = CARD_TAGS.iter().filter_map(|tag| rest.find(tag)).min() {
        let Some(close) = rest[at..].find(']') else {
            break;
        };
        out.push_str(&rest[..at]);
        rest = &rest[at + close + 1..];
    }
    out.push_str(rest);
    out.trim_end().to_owned()
}

fn without_envelope(line: &str) -> &str {
    let mut text = line.trim();
    if text.starts_with(REMINDER_OPEN) {
        text = text.split_once('>').map_or("", |(_, rest)| rest.trim());
    }
    if text.starts_with(CHANNEL_OPEN) {
        text = text.split_once('>').map_or("", |(_, rest)| rest.trim());
    }
    text = text
        .strip_suffix(REMINDER_CLOSE)
        .or_else(|| text.strip_suffix(CHANNEL_CLOSE))
        .unwrap_or(text)
        .trim();
    match text
        .strip_prefix(MAIL_LEAD)
        .and_then(|rest| rest.split_once(": "))
    {
        Some((_, body)) => body,
        None => text,
    }
}

pub fn memory_text(text: &str) -> String {
    text.lines()
        .map(|line| without_card_tags(without_envelope(line)))
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn state(item: &PendingVerdict) -> Value {
    let files: Vec<&String> = item.files_touched.iter().take(MAX_FILES).collect();
    let prompt = item.user_prompt.as_deref().and_then(typed_by_operator);
    json!({
        "memory": {
            "text": head_chars(&memory_text(&item.item_text), MEMORY_CHARS),
        },
        "operator_prompt": head_chars(prompt.as_deref().unwrap_or(""), PROMPT_CHARS),
        "tools_used": item.tools_used,
        "files_touched": files,
        "final_reply": tail_chars(item.final_assistant_text.as_deref().unwrap_or(""), REPLY_CHARS),
        "turn_reaction_recorded_by_the_turn_judge": item.reaction.as_deref().unwrap_or("none"),
        "operator_next_message": item
            .next_message
            .as_deref()
            .and_then(typed_by_operator)
            .map(|m| head_chars(&m, NEXT_CHARS)),
    })
}

pub fn verdict_of(acted: f64, subject: f64, reaction: Option<&str>) -> Option<(&'static str, f64)> {
    if acted >= YES_AT {
        return Some(("used", acted));
    }
    if acted > NO_AT {
        return None;
    }
    let not_acted = 1.0 - acted;
    if reaction != Some("corrected") {
        return Some(("ignored", not_acted));
    }
    if subject >= YES_AT {
        Some(("corrected_anyway", not_acted * subject))
    } else if subject <= NO_AT {
        Some(("ignored", not_acted * (1.0 - subject)))
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Composed {
    Decided(Answer, f64),
    Unsure,
}

pub fn compose(asked: &Asked, reaction: Option<&str>) -> Option<Composed> {
    if asked.failed() {
        return None;
    }
    let acted = asked.noul(ACTED)?;
    let subject = asked.noul(SUBJECT)?;
    let Some((verdict, probability)) = verdict_of(acted, subject, reaction) else {
        return Some(Composed::Unsure);
    };
    Some(Composed::Decided(
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
    inner_refused: AtomicBool,
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
            inner_refused: AtomicBool::new(false),
            last_was_jev: AtomicBool::new(false),
        })
    }

    async fn ask_inner(&self, item: &PendingVerdict, user: &str) -> Result<Answer, JudgeError> {
        if self.inner_refused.load(Ordering::Relaxed) {
            return Err(JudgeError::RateLimited(
                "the model refused earlier in this pass".to_owned(),
            ));
        }
        let outcome = self.inner.judge(item, user).await;
        if matches!(outcome, Err(JudgeError::RateLimited(_))) {
            self.inner_refused.store(true, Ordering::Relaxed);
        }
        outcome
    }

    async fn ask(&self, item: &PendingVerdict) -> Outcome {
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
        let decided = match &composed {
            Some(Composed::Decided(answer, probability)) => Some((answer, *probability)),
            _ => None,
        };
        let applied = decided.map_or("none", |_| "labelled");
        let score = decided.map_or(0.0, |(_, p)| p);
        let chosen = decided.map(|(a, _)| a.verdict.as_str());
        self.decider
            .mark(
                &asked,
                asked.composed(json!({
                    "score": score,
                    "item_id": item.item_id,
                    "verdict": chosen,
                    "reaction": item.reaction,
                    "unsure": matches!(composed, Some(Composed::Unsure)),
                    "yes_at": YES_AT,
                    "no_at": NO_AT,
                })),
                applied,
            )
            .await;
        if !self.decider.applies() {
            return Outcome::Model;
        }
        match composed {
            Some(Composed::Decided(answer, _)) => Outcome::Answer(answer),
            Some(Composed::Unsure) => Outcome::Unsure,
            None => Outcome::Model,
        }
    }
}

enum Outcome {
    Answer(Answer),
    Unsure,
    Model,
}

impl<J: SupplyJudge + Sync> SupplyJudge for JevJudge<'_, J> {
    fn model(&self) -> &str {
        if self.last_was_jev.load(Ordering::Relaxed) {
            &self.model
        } else {
            self.inner.model()
        }
    }

    fn decides_without_model(&self) -> bool {
        !self.tripped.load(Ordering::Relaxed) && self.decider.applies()
    }

    async fn judge(&self, item: &PendingVerdict, user: &str) -> Result<Answer, JudgeError> {
        self.last_was_jev.store(false, Ordering::Relaxed);
        if self.tripped.load(Ordering::Relaxed) {
            return self.inner.judge(item, user).await;
        }
        match self.ask(item).await {
            Outcome::Answer(answer) => {
                self.last_was_jev.store(true, Ordering::Relaxed);
                Ok(answer)
            }
            Outcome::Unsure | Outcome::Model => self.ask_inner(item, user).await,
        }
    }
}

#[cfg(test)]
#[path = "jev_tests.rs"]
mod tests;
