use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::gate_blame::{Blame, Change, Evidence, Suspect};
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider, choice};
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::tail_chars;

pub const POINT: &str = "gate_blame";
pub const QUESTION_SET: &str = "gate_blame_v1";
const WHOSE: &str = "whose";
const NONE: &str = "none";
const MAX_CANDIDATES: usize = 12;
const MAX_FILES: usize = 8;
const OUTPUT_CHARS: usize = 3000;

pub struct Candidate<'a> {
    pub commit: &'a str,
    pub seat: &'a str,
    pub change: &'a Change,
    pub lexical: Option<&'a Evidence>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pick {
    Candidate { index: usize, probability: f64 },
    Nobody { probability: f64 },
}

fn key(position: usize) -> String {
    format!("c{}", position + 1)
}

fn short(commit: &str) -> &str {
    commit.get(..10).unwrap_or(commit)
}

fn files_of(change: &Change) -> Vec<&str> {
    change
        .files
        .iter()
        .take(MAX_FILES)
        .map(String::as_str)
        .collect()
}

fn meaning(candidate: &Candidate<'_>) -> String {
    format!(
        "Commit {} by seat {}, which changed {}.",
        short(candidate.commit),
        candidate.seat,
        files_of(candidate.change).join(", ")
    )
}

pub fn questions(candidates: &[Candidate<'_>]) -> BTreeMap<String, Question> {
    let mut options: Vec<(String, String)> = candidates
        .iter()
        .enumerate()
        .map(|(at, candidate)| (key(at), meaning(candidate)))
        .collect();
    options.push((
        NONE.to_owned(),
        "None of these commits caused the failure.".to_owned(),
    ));
    let listed: Vec<(&str, &str)> = options
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    BTreeMap::from([(
        WHOSE.to_owned(),
        choice(
            "A gate command failed. Which of these commits, made since the gate last passed, caused the failure shown in the output?",
            &listed,
        ),
    )])
}

pub fn state(output: &str, candidates: &[Candidate<'_>]) -> Value {
    let listed: Vec<Value> = candidates
        .iter()
        .enumerate()
        .map(|(at, candidate)| {
            json!({
                "id": key(at),
                "commit": short(candidate.commit),
                "seat": candidate.seat,
                "files": files_of(candidate.change),
                "name_match": candidate.lexical.map(Evidence::reason),
            })
        })
        .collect();
    json!({
        "failing_output_tail": tail_chars(output, OUTPUT_CHARS),
        "commits_oldest_first": listed,
    })
}

pub fn compose(asked: &Asked, candidates: usize) -> Option<Pick> {
    if asked.failed() {
        return None;
    }
    let (picked, probability) = asked.choice(WHOSE)?;
    if picked == NONE {
        return Some(Pick::Nobody { probability });
    }
    let position = (0..candidates).find(|at| key(*at) == picked)?;
    Some(Pick::Candidate {
        index: position,
        probability,
    })
}

pub fn blame_of(pick: &Pick, candidates: &[Candidate<'_>], since: usize) -> Blame {
    match pick {
        Pick::Nobody { .. } => Blame::NoMatch { since },
        Pick::Candidate { index, probability } => {
            let chosen = &candidates[*index];
            Blame::Suspects(vec![Suspect {
                commit: chosen.commit.to_owned(),
                seat: chosen.seat.to_owned(),
                since,
                evidence: Evidence::Judged {
                    percent: (probability * 100.0).round() as u32,
                },
            }])
        }
    }
}

pub fn candidates_of<'a>(
    commits: &'a [(String, String)],
    changes: &'a [Change],
    lexical: &'a [(usize, Evidence)],
) -> Vec<Candidate<'a>> {
    let first = commits.len().saturating_sub(MAX_CANDIDATES);
    (first..commits.len())
        .map(|index| Candidate {
            commit: &commits[index].0,
            seat: &commits[index].1,
            change: &changes[index],
            lexical: lexical
                .iter()
                .find(|(at, _)| *at == index)
                .map(|(_, found)| found),
        })
        .collect()
}

pub async fn judged(
    decider: &Decider,
    candidates: &[Candidate<'_>],
    output: &str,
    since: usize,
) -> Option<Blame> {
    let asked = decider
        .ask(
            QUESTION_SET,
            state(output, candidates),
            questions(candidates),
            DecisionSubject {
                commit_sha: candidates.last().map(|c| c.commit.to_owned()),
                ..DecisionSubject::default()
            },
            json!({}),
        )
        .await;
    let pick = compose(&asked, candidates.len());
    let score = match &pick {
        Some(Pick::Candidate { probability, .. } | Pick::Nobody { probability }) => *probability,
        None => 1.0,
    };
    let applied = match (&pick, decider.applies()) {
        (Some(_), true) => "chosen",
        _ => "none",
    };
    decider
        .mark(&asked, asked.composed(json!({"score": score})), applied)
        .await;
    pick.filter(|_| decider.applies())
        .map(|pick| blame_of(&pick, candidates, since))
}

#[cfg(test)]
#[path = "gate_blame_jev_tests.rs"]
mod tests;
