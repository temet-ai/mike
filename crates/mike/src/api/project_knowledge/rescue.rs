use serde_json::Value;

use crate::analysis::embeddings::ScoreFloor;
use crate::store::persistence::project_knowledge::{KnowledgeQuery, SemanticCut, cut_for};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Rescue {
    pub(super) cut: ScoreFloor,
    pub(super) score: f64,
}

pub(super) fn pool_query(
    input: &KnowledgeQuery,
    model_name: &str,
    score: f64,
) -> (KnowledgeQuery, Rescue) {
    let cut = Rescue {
        cut: cut_for(model_name, input.semantic_cut),
        score,
    };
    let pool = KnowledgeQuery {
        semantic_cut: SemanticCut {
            floor: Some(0.0),
            window: Some(1.0),
        },
        any_term: true,
        ..input.clone()
    };
    (pool, cut)
}

pub(super) fn keep(
    arranged: Vec<(Value, Option<f32>)>,
    rescue: Rescue,
) -> Vec<(Value, Option<f32>)> {
    let best = arranged
        .iter()
        .map(|(row, _)| semantic(row))
        .fold(0.0, f64::max);
    let line = rescue.cut.floor.max(best - rescue.cut.window);
    arranged
        .into_iter()
        .filter(|(row, score)| match score {
            Some(score) => f64::from(*score) >= rescue.score || over_line(row, line),
            None => over_line(row, line) || unscored_without_vector(row),
        })
        .collect()
}

fn over_line(row: &Value, line: f64) -> bool {
    let semantic = semantic(row);
    semantic > 0.0 && semantic >= line
}

fn unscored_without_vector(row: &Value) -> bool {
    semantic(row) == 0.0
}

fn semantic(row: &Value) -> f64 {
    row["semantic_rank"].as_f64().unwrap_or(0.0)
}
