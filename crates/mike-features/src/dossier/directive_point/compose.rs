use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use super::questions::key;
use super::state::Unit;
use crate::dossier::entries::directives::Polarity;
use crate::llm::decisions::point::Asked;
use crate::store::persistence::dossier_entries::NewDossierEntry;

pub const DECIDE_AT: f64 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Authorises,
    Forbids,
    Neither,
}

impl Decision {
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "authorises_now" => Some(Self::Authorises),
            "forbids" => Some(Self::Forbids),
            "neither" => Some(Self::Neither),
            _ => None,
        }
    }

    pub fn polarity(self) -> Option<Polarity> {
        match self {
            Self::Authorises => Some(Polarity::Allow),
            Self::Forbids => Some(Polarity::Forbid),
            Self::Neither => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decided {
    pub index: usize,
    pub decision: Option<Decision>,
    pub probability: f64,
}

pub fn compose(asked: &Asked, group: &[&Unit]) -> Vec<Decided> {
    if asked.failed() {
        return Vec::new();
    }
    (0..group.len())
        .filter_map(|index| {
            let (chosen, probability) = asked.choice(&key(index))?;
            let parsed = Decision::parse(chosen)?;
            Some(Decided {
                index,
                decision: (probability >= DECIDE_AT).then_some(parsed),
                probability,
            })
        })
        .collect()
}

fn rebuilt(
    mut entry: NewDossierEntry,
    request_id: i64,
    class: &str,
    polarity: Polarity,
) -> NewDossierEntry {
    if let Value::Object(map) = &mut entry.data {
        map.insert("polarity".to_owned(), json!(polarity.code()));
        map.insert("decided_by".to_owned(), json!("jev"));
    }
    entry.dedup_key = format!("request:{request_id}:{class}:{}", polarity.code());
    entry
}

pub fn rewrite(
    entries: Vec<NewDossierEntry>,
    decided: &HashMap<(i64, String), (i64, Decision)>,
) -> Vec<NewDossierEntry> {
    let mut placed: HashSet<(i64, String)> = HashSet::new();
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let class = entry
            .data
            .get("class")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let found = entry
            .operator_turn_id
            .and_then(|turn| decided.get(&(turn, class.clone())).map(|d| (turn, *d)));
        let Some((turn, (request_id, decision))) = found else {
            out.push(entry);
            continue;
        };
        let Some(polarity) = decision.polarity() else {
            continue;
        };
        if placed.insert((turn, class.clone())) {
            out.push(rebuilt(entry, request_id, &class, polarity));
        }
    }
    out
}
