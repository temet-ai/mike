use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::memory_workers::{LESSON_PROPOSER, RERANK_NIGHTLY, SUPPLY_VERDICT};
use crate::store::persistence::learning_loop::LoopCounts;

pub(crate) type Rows = fn(&LoopCounts) -> (i64, Option<DateTime<Utc>>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    Worker,
    Job,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Switch {
    Always,
    Toggle(&'static str),
    RerankTuning,
    Job(&'static str),
}

pub(crate) struct StageDef {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) kind: Kind,
    pub(crate) switch: Switch,
    pub(crate) worker: Option<&'static str>,
    pub(crate) rows: Option<Rows>,
    pub(crate) run_now: Option<&'static str>,
}

pub(crate) const RERANK_TUNING_KEY: &str = "rerank.tuning.enabled";

pub(crate) static STAGES: [StageDef; 12] = [
    StageDef {
        id: "capture",
        label: "Capture",
        kind: Kind::Worker,
        switch: Switch::Always,
        worker: None,
        rows: Some(|c| (c.operator_turns, c.operator_turns_at)),
        run_now: None,
    },
    StageDef {
        id: "turn_judge",
        label: "Turn judge",
        kind: Kind::Worker,
        switch: Switch::Toggle("judge_worker"),
        worker: None,
        rows: Some(|c| (c.turn_verdicts, c.turn_verdicts_at)),
        run_now: None,
    },
    StageDef {
        id: "session_judge",
        label: "Session judge",
        kind: Kind::Worker,
        switch: Switch::Toggle("session_judge"),
        worker: None,
        rows: Some(|c| (c.session_verdicts, c.session_verdicts_at)),
        run_now: None,
    },
    StageDef {
        id: "lesson_proposer",
        label: "Lesson proposer",
        kind: Kind::Worker,
        switch: Switch::Toggle("judge_lessons"),
        worker: Some(LESSON_PROPOSER),
        rows: Some(|c| (c.lessons, c.lessons_at)),
        run_now: Some("/api/lessons/proposer-pass"),
    },
    StageDef {
        id: "lesson_relations",
        label: "Lesson relations",
        kind: Kind::Job,
        switch: Switch::Job("lesson_relations"),
        worker: None,
        rows: Some(|c| (c.lesson_relations, c.lesson_relations_at)),
        run_now: Some("/api/scheduled-jobs/lesson_relations/run"),
    },
    StageDef {
        id: "consolidation",
        label: "Consolidation",
        kind: Kind::Worker,
        switch: Switch::Toggle("lesson_worker"),
        worker: None,
        rows: Some(|c| (c.consolidation_runs, c.consolidation_runs_at)),
        run_now: None,
    },
    StageDef {
        id: "learning_promote",
        label: "Learning promotion",
        kind: Kind::Job,
        switch: Switch::Job("learning_promote"),
        worker: None,
        rows: None,
        run_now: Some("/api/scheduled-jobs/learning_promote/run"),
    },
    StageDef {
        id: "lessons_commit",
        label: "Lesson file commit",
        kind: Kind::Job,
        switch: Switch::Job("lessons_commit"),
        worker: None,
        rows: None,
        run_now: Some("/api/scheduled-jobs/lessons_commit/run"),
    },
    StageDef {
        id: "supply",
        label: "Supply",
        kind: Kind::Worker,
        switch: Switch::Toggle("agent_lessons"),
        worker: None,
        rows: Some(|c| (c.supplied_items, c.supplied_items_at)),
        run_now: None,
    },
    StageDef {
        id: "supply_verdict",
        label: "Supply verdict",
        kind: Kind::Worker,
        switch: Switch::Toggle("judge_lessons"),
        worker: Some(SUPPLY_VERDICT),
        rows: Some(|c| (c.supply_verdicts, c.supply_verdicts_at)),
        run_now: Some("/api/supplied-items/verdict-pass"),
    },
    StageDef {
        id: "search",
        label: "Search",
        kind: Kind::Worker,
        switch: Switch::RerankTuning,
        worker: Some(RERANK_NIGHTLY),
        rows: Some(|c| (c.search_grades, c.search_grades_at)),
        run_now: Some("/api/knowledge/rerank-pass"),
    },
    StageDef {
        id: "memory_review",
        label: "Memory review",
        kind: Kind::Job,
        switch: Switch::Job("memory_review"),
        worker: None,
        rows: Some(|c| (c.memory_reviews, c.memory_reviews_at)),
        run_now: Some("/api/scheduled-jobs/memory_review/run"),
    },
];
