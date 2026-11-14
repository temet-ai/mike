use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde_json::{Value, json};
use uuid::Uuid;

use super::*;
use crate::llm::decisions::Question;
use crate::llm::decisions::env::Pace;
use crate::llm::decisions::point::{Asked, Decider, noul};
use crate::llm::decisions::record::DecisionSubject;
use crate::store::persistence::project_merge_candidates::MergeCandidate;
use crate::store::persistence::web_repo::WebPersist as _;

pub(crate) const POINT: &str = "project_merge";
pub(crate) const QUESTION_SET: &str = "project_merge_v1";
const SAME: &str = "same";
const SUGGEST_AT: f64 = 0.7;
const CANDIDATE_PROJECTS: i64 = 200;
const MAX_PAIRS: usize = 40;
const MAX_ASKS_PER_READ: usize = 12;
const REMEMBER_FOR: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct Suggestion {
    pub(crate) source_path: String,
    pub(crate) target_uuid: Uuid,
    pub(crate) target_path: String,
    pub(crate) probability: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct Suggestions {
    pub(crate) layer: &'static str,
    pub(crate) suggestions: Vec<Suggestion>,
}

pub(crate) struct Pair<'a> {
    pub(crate) source: &'a MergeCandidate,
    pub(crate) target: &'a MergeCandidate,
}

fn last_segment(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

fn nested(a: &str, b: &str) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let short = short.trim_end_matches('/');
    long.strip_prefix(short)
        .is_some_and(|rest| rest.starts_with('/') || rest.starts_with('-'))
}

pub(crate) fn may_be_the_same(a: &MergeCandidate, b: &MergeCandidate) -> bool {
    let shared_origin = a.origin_urls.iter().any(|url| b.origin_urls.contains(url));
    let same_name = !a.display_name.is_empty() && a.display_name == b.display_name;
    shared_origin
        || same_name
        || last_segment(&a.project_path) == last_segment(&b.project_path)
        || nested(&a.project_path, &b.project_path)
}

pub(crate) fn pairs(candidates: &[MergeCandidate]) -> Vec<Pair<'_>> {
    let mut found = Vec::new();
    for (at, a) in candidates.iter().enumerate() {
        for b in &candidates[at + 1..] {
            if !may_be_the_same(a, b) {
                continue;
            }
            let (source, target) = if a.last_seen >= b.last_seen {
                (b, a)
            } else {
                (a, b)
            };
            found.push(Pair { source, target });
        }
    }
    found.truncate(MAX_PAIRS);
    found
}

fn described(project: &MergeCandidate) -> Value {
    json!({
        "path": project.project_path,
        "display_name": project.display_name,
        "git_remotes": project.origin_urls,
        "last_seen": project.last_seen,
    })
}

pub(crate) fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        SAME.to_owned(),
        noul(
            "Two projects were captured from different working folders. Are they the same repository, one being a worktree, a copy or a differently named folder of the other?",
            "They are the same repository.",
            "They are different repositories.",
        ),
    )])
}

pub(crate) fn state(pair: &Pair<'_>) -> Value {
    json!({"project_a": described(pair.source), "project_b": described(pair.target)})
}

pub(crate) fn compose(asked: &Asked) -> Option<f64> {
    if asked.failed() {
        return None;
    }
    asked.noul(SAME)
}

type PairKey = (Uuid, Uuid);
type Remembered = (Instant, Option<f64>);

fn remembered() -> &'static DashMap<PairKey, Remembered> {
    static MEMORY: OnceLock<DashMap<PairKey, Remembered>> = OnceLock::new();
    MEMORY.get_or_init(DashMap::new)
}

async fn probability_of(decider: &Decider, pair: &Pair<'_>, pace: &mut Pace) -> Option<f64> {
    let pair_key = (pair.source.project_uuid, pair.target.project_uuid);
    if let Some(known) = remembered().get(&pair_key)
        && known.0.elapsed() < REMEMBER_FOR
    {
        return known.1;
    }
    if !pace.may_ask() {
        return None;
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            state(pair),
            questions(),
            DecisionSubject {
                project_id: Some(pair.target.id),
                ..DecisionSubject::default()
            },
            json!({"suggest_at": SUGGEST_AT}),
        )
        .await;
    pace.spent(&asked);
    let probability = compose(&asked);
    let suggested = probability.is_some_and(|p| p >= SUGGEST_AT);
    let applied = if suggested && decider.applies() {
        "suggested"
    } else {
        "none"
    };
    decider
        .mark(
            &asked,
            asked.composed(json!({
                "score": probability.unwrap_or(1.0),
                "source_project_id": pair.source.id,
                "target_project_id": pair.target.id,
            })),
            applied,
        )
        .await;
    if !asked.failed() {
        remembered().insert(pair_key, (Instant::now(), probability));
    }
    probability
}

pub(crate) async fn suggest(decider: &Decider, candidates: &[MergeCandidate]) -> Vec<Suggestion> {
    let mut pace = Pace::new(MAX_ASKS_PER_READ);
    let mut found = Vec::new();
    for pair in pairs(candidates) {
        let Some(probability) = probability_of(decider, &pair, &mut pace).await else {
            continue;
        };
        if probability >= SUGGEST_AT {
            found.push(Suggestion {
                source_path: pair.source.project_path.clone(),
                target_uuid: pair.target.project_uuid,
                target_path: pair.target.project_path.clone(),
                probability,
            });
        }
    }
    found.sort_by(|a, b| b.probability.total_cmp(&a.probability));
    found
}

pub(crate) async fn get_project_merge_suggestions(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist.as_ref() else {
        return persist_unavailable_response();
    };
    let Some(decider) = state.decider(POINT).filter(Decider::applies) else {
        return Json(Suggestions {
            layer: "off",
            suggestions: Vec::new(),
        })
        .into_response();
    };
    match persist.web().merge_candidates(CANDIDATE_PROJECTS).await {
        Ok(candidates) => Json(Suggestions {
            layer: "on",
            suggestions: suggest(&decider, &candidates).await,
        })
        .into_response(),
        Err(error) => persist_error_response(error),
    }
}

#[cfg(test)]
#[path = "project_merge_suggest_tests.rs"]
mod tests;
