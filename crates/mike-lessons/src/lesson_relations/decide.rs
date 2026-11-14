use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde_json::{Value, json};

use super::evidence::{EvidenceBundle, EvidenceBundleParams};
use super::rules::find_artifact_reference;
use super::schema::{AdjudicatorVerdict, ProposerVerdict};
use super::{KIND_CONTRADICTS, KIND_RECURS, KIND_REFINES, KIND_SUPERSEDES, RelationJudge};
use crate::llm::LlmError;
use crate::llm::decisions::env::{DecisionEnv, head_chars};
use crate::llm::decisions::point::{Asked, Decider, choice, noul};
use crate::llm::decisions::{Question, record::DecisionSubject};
use crate::store::persistence::lesson_relations::CandidateLessonPair;

pub const POINT: &str = "lesson_relation";
pub const QUESTION_SET: &str = "lesson_relation_v1";
pub const UNRELATED: &str = "unrelated";
pub const MIN_CITES: f64 = 0.7;
pub const MIN_RELATION: f64 = 0.5;
const RELATION_KEY: &str = "relation";
const CITES_KEY: &str = "cites";
const TEXT_CHARS: usize = 700;

const RELATIONS: &[(&str, &str)] = &[
    (
        KIND_SUPERSEDES,
        "the newer lesson corrects or replaces the older one, so the newer text should win",
    ),
    (
        KIND_REFINES,
        "the newer lesson adds detail or a narrower condition to the older one, and both stay true",
    ),
    (KIND_RECURS, "both lessons state the same rule"),
    (
        KIND_CONTRADICTS,
        "the two cannot both be true and neither is simply the newer version of the other",
    ),
    (UNRELATED, "different subjects, or similar only in topic"),
];

pub struct PairJev {
    decider: Decider,
    model: String,
    left: AtomicUsize,
    tripped: AtomicBool,
}

impl PairJev {
    pub fn new(env: Option<&DecisionEnv>, max_calls: usize) -> Option<Self> {
        let env = env?;
        Some(Self {
            decider: env.decider(POINT)?,
            model: env.model(),
            left: AtomicUsize::new(max_calls),
            tripped: AtomicBool::new(false),
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    fn take(&self) -> bool {
        if self.tripped.load(Ordering::Relaxed) {
            return false;
        }
        self.left
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1))
            .is_ok()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cite {
    pub from_lesson_id: i64,
    pub to_lesson_id: i64,
    pub quote: String,
    pub char_start: usize,
    pub char_end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PairDecision {
    Cites(Cite),
    Relation {
        from_lesson_id: i64,
        to_lesson_id: i64,
        kind: &'static str,
        probability: f64,
    },
    Unrelated(f64),
}

fn kind_code(code: &str) -> Option<&'static str> {
    [KIND_SUPERSEDES, KIND_REFINES, KIND_RECURS, KIND_CONTRADICTS]
        .into_iter()
        .find(|k| *k == code)
}

pub fn newer_first(pair: &CandidateLessonPair) -> (i64, i64) {
    let a_newer = (pair.lesson_a_created_at, pair.lesson_a_id)
        >= (pair.lesson_b_created_at, pair.lesson_b_id);
    if a_newer {
        (pair.lesson_a_id, pair.lesson_b_id)
    } else {
        (pair.lesson_b_id, pair.lesson_a_id)
    }
}

pub fn cite_of(pair: &CandidateLessonPair) -> Option<Cite> {
    let cite = |from: i64, text: &str, to: i64| {
        find_artifact_reference(text, to).map(|(quote, char_start, char_end)| Cite {
            from_lesson_id: from,
            to_lesson_id: to,
            quote,
            char_start,
            char_end,
        })
    };
    cite(pair.lesson_b_id, &pair.lesson_b_text, pair.lesson_a_id)
        .or_else(|| cite(pair.lesson_a_id, &pair.lesson_a_text, pair.lesson_b_id))
}

pub fn questions(pair: &CandidateLessonPair, cite: Option<&Cite>) -> BTreeMap<String, Question> {
    let (newer, older) = newer_first(pair);
    let mut out = BTreeMap::from([(
        RELATION_KEY.to_owned(),
        choice(
            &format!(
                "How does the newer lesson {newer} relate to the older lesson {older}? \"X is broken\" and \"X was fixed\" are a supersedes or contradicts, never recurs."
            ),
            RELATIONS,
        ),
    )]);
    if let Some(c) = cite {
        out.insert(
            CITES_KEY.to_owned(),
            noul(
                &format!(
                    "Lesson {} contains \"{}\". Does that text refer to lesson {} of this memory, rather than an issue, pull request, line or other numbered thing?",
                    c.from_lesson_id, c.quote, c.to_lesson_id
                ),
                "the text names the other lesson by its id",
                "the number is something else, such as an issue or a pull request",
            ),
        );
    }
    out
}

pub fn state(pair: &CandidateLessonPair, cite: Option<&Cite>) -> Value {
    let bundle = EvidenceBundle::build(EvidenceBundleParams {
        lesson_a_id: pair.lesson_a_id,
        lesson_a_text: head_chars(&pair.lesson_a_text, TEXT_CHARS),
        lesson_a_created_at: pair.lesson_a_created_at,
        lesson_b_id: pair.lesson_b_id,
        lesson_b_text: head_chars(&pair.lesson_b_text, TEXT_CHARS),
        lesson_b_created_at: pair.lesson_b_created_at,
        cosine_distance: pair.distance,
    });
    let (newer, older) = newer_first(pair);
    let markers = |spans: &[super::evidence::QuotedSpan]| -> Vec<&'static str> {
        spans.iter().map(|s| s.marker).collect()
    };
    json!({
        "newer_lesson_id": newer,
        "older_lesson_id": older,
        "lesson_a": { "id": bundle.lesson_a_id, "text": bundle.lesson_a_text, "recorded": bundle.lesson_a_created_at, "polarity_markers": markers(&bundle.lesson_a_spans) },
        "lesson_b": { "id": bundle.lesson_b_id, "text": bundle.lesson_b_text, "recorded": bundle.lesson_b_created_at, "polarity_markers": markers(&bundle.lesson_b_spans) },
        "cosine_distance": (pair.distance * 1000.0).round() / 1000.0,
        "id_mention": cite.map(|c| json!({ "in_lesson": c.from_lesson_id, "text": c.quote, "number_refers_to": c.to_lesson_id })),
    })
}

pub fn compose(
    asked: &Asked,
    pair: &CandidateLessonPair,
    cite: Option<&Cite>,
) -> Option<PairDecision> {
    if asked.failed() {
        return None;
    }
    if let Some(c) = cite
        && asked.noul(CITES_KEY).is_some_and(|p| p >= MIN_CITES)
    {
        return Some(PairDecision::Cites(c.clone()));
    }
    let (code, probability) = asked.choice(RELATION_KEY)?;
    let (newer, older) = newer_first(pair);
    if code == UNRELATED || probability < MIN_RELATION {
        return Some(PairDecision::Unrelated(probability));
    }
    Some(PairDecision::Relation {
        from_lesson_id: newer,
        to_lesson_id: older,
        kind: kind_code(code)?,
        probability,
    })
}

fn applied(decision: Option<&PairDecision>) -> &'static str {
    match decision {
        Some(PairDecision::Cites(_) | PairDecision::Relation { .. }) => "chosen",
        _ => "none",
    }
}

fn score(decision: Option<&PairDecision>) -> f64 {
    match decision {
        Some(PairDecision::Relation { probability, .. })
        | Some(PairDecision::Unrelated(probability)) => *probability,
        Some(PairDecision::Cites(_)) => 1.0,
        None => 0.0,
    }
}

pub async fn decide(
    jev: &PairJev,
    pair: &CandidateLessonPair,
) -> Option<(PairDecision, Option<i64>)> {
    if !jev.take() {
        return None;
    }
    let cite = cite_of(pair);
    let asked = jev
        .decider
        .ask(
            QUESTION_SET,
            state(pair, cite.as_ref()),
            questions(pair, cite.as_ref()),
            DecisionSubject::default(),
            json!({ "min_cites": MIN_CITES, "min_relation": MIN_RELATION }),
        )
        .await;
    if asked.failed() {
        jev.tripped.store(true, Ordering::Relaxed);
    }
    let decision = compose(&asked, pair, cite.as_ref());
    jev.decider
        .mark(
            &asked,
            asked.composed(json!({ "score": score(decision.as_ref()) })),
            applied(decision.as_ref()),
        )
        .await;
    decision
        .filter(|_| jev.decider.applies())
        .map(|d| (d, asked.record_id))
}

pub struct JevJudge<'a, J> {
    pub inner: &'a J,
    pub jev: Option<&'a PairJev>,
}

impl<J: RelationJudge> RelationJudge for JevJudge<'_, J> {
    fn propose(
        &self,
        prompt: &str,
    ) -> impl std::future::Future<Output = Result<ProposerVerdict, LlmError>> {
        self.inner.propose(prompt)
    }

    fn adjudicate(
        &self,
        prompt: &str,
    ) -> impl std::future::Future<Output = Result<AdjudicatorVerdict, LlmError>> {
        self.inner.adjudicate(prompt)
    }

    fn proposer_model(&self) -> &str {
        self.inner.proposer_model()
    }

    fn adjudicator_model(&self) -> &str {
        self.inner.adjudicator_model()
    }

    fn jev(&self) -> Option<&PairJev> {
        self.jev
    }
}

pub async fn record_operator_outcome(
    persist: &crate::store::persistence::PersistHandle,
    evidence: &Value,
    label: &str,
) {
    let Some(id) = evidence.get("decision_record_id").and_then(Value::as_i64) else {
        return;
    };
    crate::llm::decisions::point::record_outcome(
        persist,
        id,
        label,
        "operator_relation_decision",
        json!({ "relation_decision": label }),
    )
    .await;
}

#[cfg(test)]
#[path = "decide_tests.rs"]
mod tests;
