use serde_json::Value;

use crate::analysis::embeddings::ScoreFloor;
use crate::api::rerank_search::RerankReport;
use crate::search_ranking::identifier_hits;
use crate::store::persistence::project_knowledge::{KnowledgeQuery, SemanticCut, cut_for};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Rescue {
    pub(super) cut: ScoreFloor,
    pub(super) score: f64,
    pub(super) min_kept: usize,
}

pub(super) fn pool_query(
    input: &KnowledgeQuery,
    model_name: &str,
    (score, min_kept): (f64, usize),
) -> (KnowledgeQuery, Rescue) {
    let cut = Rescue {
        cut: cut_for(model_name, input.semantic_cut),
        score,
        min_kept,
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

pub(super) fn keep_reporting(
    arranged: Vec<(Value, Option<f32>)>,
    rescue: Rescue,
    report: RerankReport,
) -> (Vec<(Value, Option<f32>)>, RerankReport) {
    let before = arranged.len();
    let kept = keep(arranged, rescue);
    let left_out = before - kept.len();
    (kept, report.with_left_out(left_out, rescue.score))
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
        .enumerate()
        .filter(|(rank, (row, score))| {
            identifier_hits(row) > 0 || passes(*rank, (row, *score), (rescue, line))
        })
        .map(|(_, kept)| kept)
        .collect()
}

fn passes(rank: usize, (row, score): (&Value, Option<f32>), (rescue, line): (Rescue, f64)) -> bool {
    match score {
        Some(score) => {
            rank < rescue.min_kept || f64::from(score) >= rescue.score || over_line(row, line)
        }
        None => over_line(row, line) || unscored_without_vector(row),
    }
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
