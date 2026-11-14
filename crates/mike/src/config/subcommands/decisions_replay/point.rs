use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::Asked;

pub(super) struct Prepared {
    pub state: Value,
    pub questions: BTreeMap<String, Question>,
}

pub(super) struct Judged {
    pub composed: Value,
    pub score: f64,
    pub positive: Option<bool>,
    pub decision_right: Option<bool>,
    pub agrees_current: Option<bool>,
}

pub(super) trait ReplayPoint {
    type Item;
    const POINT: &'static str;
    const QUESTION_SET: &'static str;
    const OUTCOME_SOURCE: &'static str;

    fn item_id(item: &Self::Item) -> String;
    fn item_at(item: &Self::Item) -> DateTime<Utc>;
    fn label(item: &Self::Item) -> String;
    fn prepare(item: &Self::Item) -> Prepared;
    fn compose(item: &Self::Item, asked: &Asked) -> Option<Judged>;
    fn question_positive(item: &Self::Item, question: &str) -> Option<bool>;
    fn current_positive(item: &Self::Item, question: &str) -> Option<bool>;
}

pub(in crate::config::subcommands) struct KnownPoint {
    pub point: &'static str,
    pub question_set: &'static str,
    pub outcome_source: &'static str,
}

fn known<P: ReplayPoint>() -> KnownPoint {
    KnownPoint {
        point: P::POINT,
        question_set: P::QUESTION_SET,
        outcome_source: P::OUTCOME_SOURCE,
    }
}

pub(in crate::config::subcommands) fn known_point(name: &str) -> Option<KnownPoint> {
    use super::{correction, turn_judge, verifier, wake};
    [
        known::<turn_judge::TurnJudge>(),
        known::<correction::Correction>(),
        known::<verifier::VerifierPrecheck>(),
        known::<wake::WakeRecheck>(),
    ]
    .into_iter()
    .find(|k| k.point == name)
}
