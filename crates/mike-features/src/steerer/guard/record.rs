use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::call::Decision;
use super::gates::GATE_VERSION;
use super::reach::Reach;
use super::schema::GuardVerdict;
use super::{GUARD_RUBRIC_VERSION, Question, Trigger};
use crate::dossier::steering_facts::{STEERING_FACTS_VERSION, SteeringFacts};

pub const TOOL_EFFECT_VERSION: i64 = 2;

#[derive(Debug, Clone, Default)]
pub struct GuardRecord {
    pub trigger: Option<Trigger>,
    pub exemption: Option<String>,
    pub facts: Option<SteeringFacts>,
    pub gates: BTreeMap<Question, Value>,
    pub votes: Vec<GuardVerdict>,
    pub decisions: BTreeMap<Question, Decision>,
    pub reach: Option<Reach>,
    pub actor: Value,
    pub relapse_anchor_turn_id: Option<i64>,
    pub recall_sample: BTreeMap<Question, bool>,
    pub input_chars: Option<usize>,
    pub idle_seconds: Option<i64>,
    pub abstentions: BTreeMap<Question, String>,
    pub live: BTreeMap<Question, super::live_decision::LiveDecision>,
    pub calibration: Option<Value>,
    pub relapse_semantic: Option<Value>,
}

pub fn would_be_superseded_by(firing: &[Question]) -> BTreeMap<&'static str, &'static str> {
    let Some(top) = firing.iter().min_by_key(|q| q.rank()) else {
        return BTreeMap::new();
    };
    firing
        .iter()
        .filter(|q| *q != top)
        .map(|q| (q.code(), top.code()))
        .collect()
}

fn keyed<T>(map: &BTreeMap<Question, T>, f: impl Fn(&T) -> Value) -> Value {
    Value::Object(
        map.iter()
            .map(|(q, v)| (q.code().to_owned(), f(v)))
            .collect(),
    )
}

impl GuardRecord {
    pub fn firing(&self) -> Vec<Question> {
        self.decisions
            .values()
            .filter(|d| d.fires)
            .map(|d| d.question)
            .collect()
    }

    pub fn shared_json(&self) -> Value {
        let first = self.votes.first();
        let questions = first.map_or(json!({}), |v| json!(v.questions));
        json!({
            "guard_rubric_version": GUARD_RUBRIC_VERSION,
            "gate_version": GATE_VERSION,
            "steering_facts_version": STEERING_FACTS_VERSION,
            "tool_effect_version": TOOL_EFFECT_VERSION,
            "trigger": self.trigger.map(Trigger::code),
            "exemption": self.exemption,
            "facts": self.facts,
            "gates": keyed(&self.gates, Clone::clone),
            "questions": questions,
            "votes": self.votes.iter().skip(1).map(|v| json!(v.questions)).collect::<Vec<_>>(),
            "first_vote": keyed(&self.decisions, |d| json!({"fires": d.first_vote_fires})),
            "quorum": keyed(&self.decisions, |d| json!({"votes_fire": d.votes_fire, "votes_total": d.votes_total})),
            "reach": self.reach,
            "actor": self.actor,
            "relapse_anchor_turn_id": self.relapse_anchor_turn_id,
            "would_be_superseded_by": would_be_superseded_by(&self.firing()),
            "live": keyed(&self.live, |d| json!(d)),
            "calibration": self.calibration,
            "semantic": null,
            "input_chars": self.input_chars,
            "idle_seconds": self.idle_seconds,
        })
    }

    pub fn row_json(&self, question: Question) -> Value {
        let mut value = self.shared_json();
        if let Value::Object(map) = &mut value {
            map.insert(
                "recall_sample".to_owned(),
                json!(self.recall_sample.get(&question).copied().unwrap_or(false)),
            );
            map.insert(
                "abstention".to_owned(),
                json!(self.abstentions.get(&question)),
            );
            if question == Question::Relapse
                && let Some(semantic) = &self.relapse_semantic
            {
                map.insert("semantic".to_owned(), json!({"relapse": semantic}));
            }
        }
        value
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn rows_share_one_verdict_object() {
        let mut record = GuardRecord {
            trigger: Some(Trigger::TurnEnd),
            ..GuardRecord::default()
        };
        record.abstentions.insert(
            Question::Relapse,
            "gate_closed:no_recent_correction".to_owned(),
        );
        let a = record.row_json(Question::Relapse);
        let b = record.row_json(Question::UnobservedDone);
        assert_eq!(a["gate_version"], b["gate_version"]);
        assert_eq!(a["trigger"], "turn_end");
        assert_eq!(a["semantic"], Value::Null);
        assert_eq!(a["abstention"], "gate_closed:no_recent_correction");
        assert_eq!(b["abstention"], Value::Null);
    }

    #[test]
    fn would_be_superseded_by_follows_c6_order() {
        let map = would_be_superseded_by(&[
            Question::AuthorisedHandback,
            Question::UnobservedDone,
            Question::Relapse,
        ]);
        assert_eq!(map.get("authorised_handback"), Some(&"relapse"));
        assert_eq!(map.get("unobserved_done"), Some(&"relapse"));
        assert!(!map.contains_key("relapse"));
        let two = would_be_superseded_by(&[Question::AuthorisedHandback, Question::UnobservedDone]);
        assert_eq!(two.get("authorised_handback"), Some(&"unobserved_done"));
    }

    #[test]
    fn relapse_anchor_is_correcting_turn() {
        let record = GuardRecord {
            relapse_anchor_turn_id: Some(16_092),
            ..GuardRecord::default()
        };
        assert_eq!(record.shared_json()["relapse_anchor_turn_id"], 16_092);
    }

    #[test]
    fn relapse_semantic_lands_only_on_the_relapse_row() {
        let record = GuardRecord {
            relapse_semantic: Some(json!({"status": "ok"})),
            ..GuardRecord::default()
        };
        let relapse = record.row_json(Question::Relapse);
        let other = record.row_json(Question::UnobservedDone);
        assert_eq!(relapse["semantic"]["relapse"]["status"], "ok");
        assert_eq!(other["semantic"], Value::Null);
    }
}
