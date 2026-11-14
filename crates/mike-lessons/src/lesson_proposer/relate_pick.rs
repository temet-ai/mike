use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use super::placement::{Home, LessonToPlace, PlacementAnswer};
use super::relate::{CandidateDecision, RelateAnswer, Relation};
use crate::llm::decisions::env::{DecisionEnv, head_chars};
use crate::llm::decisions::point::{Asked, Decider, choice, noul};
use crate::llm::decisions::{Question, record::DecisionSubject};
use crate::store::persistence::lessons::write_time::NeighbourLesson;
use crate::store::persistence::project_knowledge::GuidanceRow;

pub const POINT: &str = "lesson_relate";
pub const RELATE_SET: &str = "lesson_relate_v1";
pub const PLACE_SET: &str = "lesson_relate_placement_v1";
pub const NO_DOCUMENT: &str = "none";
pub const MIN_SKIP: f64 = 0.6;
pub const MIN_PERSONA: f64 = 0.5;
const HOME_KEY: &str = "home";
const COVERED_KEY: &str = "covered_by";
const PERSONA_KEY: &str = "persona";
const LESSON_CHARS: usize = 700;
const MAX_DOCUMENTS: usize = 8;

pub struct Jev {
    decider: Decider,
    tripped: AtomicBool,
}

impl Jev {
    pub fn new(env: Option<&DecisionEnv>) -> Option<Self> {
        Some(Self {
            decider: env?.decider(POINT)?,
            tripped: AtomicBool::new(false),
        })
    }

    pub fn is_tripped(&self) -> bool {
        self.tripped.load(Ordering::Relaxed)
    }

    pub fn trip(&self) {
        self.tripped.store(true, Ordering::Relaxed);
    }

    pub fn decider(&self) -> &Decider {
        &self.decider
    }
}

fn relation_key(id: i64) -> String {
    format!("relation_{id}")
}

fn relation_from(code: &str) -> Option<Relation> {
    match code {
        "store" => Some(Relation::Store),
        "skip" => Some(Relation::Skip),
        "update" => Some(Relation::Update),
        "merge" => Some(Relation::Merge),
        "contradicts" => Some(Relation::Contradicts),
        _ => None,
    }
}

fn home_from(code: &str) -> Option<Home> {
    match code {
        "rule" => Some(Home::Rule),
        "skill" => Some(Home::Skill),
        "agent" => Some(Home::Agent),
        "memory" => Some(Home::Memory),
        "nowhere" => Some(Home::Nowhere),
        _ => None,
    }
}

const RELATIONS: &[(&str, &str)] = &[
    ("store", "unrelated, or similar only in topic; both stay"),
    (
        "skip",
        "the stored lesson already says everything the new one says, with no condition, command, path or value missing",
    ),
    (
        "update",
        "same subject, and the new lesson corrects, replaces or restates the stored one so the new text should win",
    ),
    (
        "merge",
        "complementary parts of one rule, which one lesson should replace both",
    ),
    (
        "contradicts",
        "the two cannot both be true and neither is simply the newer version of the other",
    ),
];

const HOMES: &[(&str, &str)] = &[
    ("rule", "a short standing constraint tied to file paths"),
    (
        "skill",
        "a procedure for a recognisable situation: ordered steps, commands or a checklist",
    ),
    ("agent", "who an agent is, what it owns and how it decides"),
    (
        "memory",
        "a project fact or correction recalled when it becomes relevant",
    ),
    (
        "nowhere",
        "a shown document already says it, or it is not worth keeping",
    ),
];

fn documents(shown: &[GuidanceRow]) -> Vec<&GuidanceRow> {
    shown.iter().take(MAX_DOCUMENTS).collect()
}

pub fn placement_questions(shown: &[GuidanceRow]) -> BTreeMap<String, Question> {
    let mut out = BTreeMap::from([
        (
            HOME_KEY.to_owned(),
            choice(
                "Which home does the lesson belong in? When unsure between two homes, choose memory.",
                HOMES,
            ),
        ),
        (
            PERSONA_KEY.to_owned(),
            noul(
                "Is the lesson about who an agent is, what it owns and how it decides, rather than a how-to, a command, a path or a project fact?",
                "the lesson describes an agent persona",
                "the lesson is a how-to, a command, a path or a project fact",
            ),
        ),
    ]);
    let docs = documents(shown);
    if !docs.is_empty() {
        let mut options: Vec<(&str, &str)> = docs
            .iter()
            .map(|d| (d.relative_path.as_str(), d.title.as_str()))
            .collect();
        options.push((NO_DOCUMENT, "no shown document covers the lesson"));
        out.insert(
            COVERED_KEY.to_owned(),
            choice(
                "Which shown document already covers the lesson, or is the one it belongs in?",
                &options,
            ),
        );
    }
    out
}

fn document_cards(shown: &[GuidanceRow]) -> Vec<Value> {
    documents(shown)
        .iter()
        .map(|d| json!({ "path": d.relative_path, "kind": d.kind_label(), "title": d.title }))
        .collect()
}

pub fn placement_from(asked: &Asked, shown: &[GuidanceRow]) -> Option<PlacementAnswer> {
    let (home_code, home_p) = asked.choice(HOME_KEY)?;
    let home = home_from(home_code)?;
    let covered = asked
        .choice(COVERED_KEY)
        .filter(|(path, _)| *path != NO_DOCUMENT)
        .filter(|(path, _)| shown.iter().any(|d| d.relative_path == *path))
        .map(|(path, _)| path.to_owned())
        .unwrap_or_default();
    let persona = asked.noul(PERSONA_KEY).is_some_and(|p| p >= MIN_PERSONA);
    Some(PlacementAnswer {
        home,
        covered_by: covered,
        reason: format!("Jev placed the lesson as {home_code} (probability {home_p:.2})"),
        persona,
    })
}

pub fn questions(
    neighbours: &[NeighbourLesson],
    shown: &[GuidanceRow],
) -> BTreeMap<String, Question> {
    let mut out = placement_questions(shown);
    for n in neighbours {
        out.insert(
            relation_key(n.id),
            choice(
                &format!(
                    "How does the new lesson relate to stored lesson {}? Near-duplicates in wording can be opposites: \"X is broken\" and \"X was fixed\" are an update or contradicts, never skip.",
                    n.id
                ),
                RELATIONS,
            ),
        );
    }
    out
}

pub fn state(rule: &str, neighbours: &[NeighbourLesson], shown: &[GuidanceRow]) -> Value {
    json!({
        "new_lesson": head_chars(rule.trim(), LESSON_CHARS),
        "stored_lessons": neighbours
            .iter()
            .map(|n| json!({
                "lesson_id": n.id,
                "text": head_chars(&n.text, LESSON_CHARS),
                "similarity": (f64::from(1.0 - n.distance) * 100.0).round() / 100.0,
                "source": n.source,
                "recorded": n.created_at.format("%Y-%m-%d").to_string(),
            }))
            .collect::<Vec<_>>(),
        "shown_documents": document_cards(shown),
    })
}

fn decision_for(asked: &Asked, n: &NeighbourLesson) -> Option<CandidateDecision> {
    let (code, p) = asked.choice(&relation_key(n.id))?;
    let mut relation = relation_from(code)?;
    if relation == Relation::Skip && p < MIN_SKIP {
        relation = Relation::Update;
    }
    Some(CandidateDecision {
        lesson_id: n.id,
        decision: relation,
        reason: format!("Jev chose {code} (probability {p:.2})"),
    })
}

pub fn compose(
    asked: &Asked,
    neighbours: &[NeighbourLesson],
    shown: &[GuidanceRow],
) -> Option<RelateAnswer> {
    if asked.failed() {
        return None;
    }
    let decisions: Option<Vec<CandidateDecision>> =
        neighbours.iter().map(|n| decision_for(asked, n)).collect();
    Some(RelateAnswer {
        decisions: decisions?,
        replacement: String::new(),
        placement: Some(placement_from(asked, shown)?),
    })
}

pub fn needs_replacement(answer: &RelateAnswer) -> bool {
    answer
        .decisions
        .iter()
        .any(|d| matches!(d.decision, Relation::Update | Relation::Merge))
}

fn applied_for(answer: &RelateAnswer) -> &'static str {
    if needs_replacement(answer) {
        "chosen"
    } else if answer
        .decisions
        .iter()
        .any(|d| d.decision == Relation::Skip)
    {
        "skipped"
    } else {
        "none"
    }
}

fn mean_probability(asked: &Asked, neighbours: &[NeighbourLesson]) -> f64 {
    let ps: Vec<f64> = neighbours
        .iter()
        .filter_map(|n| asked.choice(&relation_key(n.id)))
        .map(|(_, p)| p)
        .chain(asked.choice(HOME_KEY).map(|(_, p)| p))
        .collect();
    if ps.is_empty() {
        return 0.0;
    }
    ps.iter().sum::<f64>() / ps.len() as f64
}

pub async fn relate_answer(
    jev: &Jev,
    rule: &str,
    neighbours: &[NeighbourLesson],
    shown: &[GuidanceRow],
    subject: DecisionSubject,
) -> Option<RelateAnswer> {
    if jev.is_tripped() {
        return None;
    }
    let asked = jev
        .decider
        .ask(
            RELATE_SET,
            state(rule, neighbours, shown),
            questions(neighbours, shown),
            subject,
            json!({ "min_skip": MIN_SKIP, "min_persona": MIN_PERSONA }),
        )
        .await;
    if asked.failed() {
        jev.trip();
    }
    let composed = compose(&asked, neighbours, shown);
    let applied = composed.as_ref().map_or("none", applied_for);
    jev.decider
        .mark(
            &asked,
            asked.composed(json!({ "score": mean_probability(&asked, neighbours) })),
            applied,
        )
        .await;
    composed.filter(|_| jev.decider.applies())
}

pub async fn placement_answer(
    jev: &Jev,
    lesson: &LessonToPlace<'_>,
    shown: &[GuidanceRow],
    (lesson_id, subject): (i64, DecisionSubject),
) -> Option<PlacementAnswer> {
    if jev.is_tripped() {
        return None;
    }
    let state = json!({
        "lesson": head_chars(lesson.text.trim(), LESSON_CHARS),
        "kind": lesson.kind,
        "topic": lesson.topic,
        "scoped_to_agent": lesson.agent_name,
        "shown_documents": document_cards(shown),
    });
    let asked = jev
        .decider
        .ask(
            PLACE_SET,
            state,
            placement_questions(shown),
            subject,
            json!({ "min_persona": MIN_PERSONA }),
        )
        .await;
    if asked.failed() {
        jev.trip();
    }
    let composed = placement_from(&asked, shown);
    let applied = composed.as_ref().map_or("none", |_| "chosen");
    let score = asked.choice(HOME_KEY).map_or(0.0, |(_, p)| p);
    jev.decider
        .mark(
            &asked,
            asked.composed(json!({ "score": score, "lesson_ids": [lesson_id] })),
            applied,
        )
        .await;
    composed.filter(|_| jev.decider.applies())
}

#[cfg(test)]
#[path = "relate_pick_tests.rs"]
mod tests;
