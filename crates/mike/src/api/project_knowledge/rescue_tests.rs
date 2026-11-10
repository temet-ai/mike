use serde_json::{Value, json};

use super::rescue::{Rescue, keep, pool_query};
use crate::analysis::embeddings::score_floor_for;
use crate::analysis::rerank::DEFAULT_RESCUE_SCORE;
use crate::store::persistence::project_knowledge::{KnowledgeQuery, SemanticCut};

const MODEL: &str = "embeddinggemma";

fn row(reference: &str, semantic: f64) -> Value {
    json!({"reference": reference, "semantic_rank": semantic, "lexical_rank": 0.01})
}

fn rescue() -> Rescue {
    Rescue {
        cut: score_floor_for(MODEL),
        score: DEFAULT_RESCUE_SCORE,
    }
}

fn scored(offset: f64) -> Option<f32> {
    Some((DEFAULT_RESCUE_SCORE + offset) as f32)
}

fn kept(arranged: Vec<(Value, Option<f32>)>) -> Vec<String> {
    keep(arranged, rescue())
        .into_iter()
        .filter_map(|(row, _)| row["reference"].as_str().map(str::to_owned))
        .collect()
}

fn under_floor() -> f64 {
    score_floor_for(MODEL).floor / 2.0
}

#[test]
fn pool_query_takes_every_semantic_hit_and_any_query_term() {
    let input = KnowledgeQuery::default();

    let (pool, cut) = pool_query(&input, MODEL, DEFAULT_RESCUE_SCORE);

    assert_eq!(cut, rescue());
    assert_eq!(pool.semantic_cut.floor, Some(0.0));
    assert_eq!(pool.semantic_cut.window, Some(1.0));
    assert!(pool.any_term && !input.any_term);
}

#[test]
fn pool_query_reports_a_tuned_floor_and_the_configured_score() {
    let tuned = score_floor_for(MODEL).floor + 0.05;
    let input = KnowledgeQuery {
        semantic_cut: SemanticCut {
            floor: Some(tuned),
            window: None,
        },
        ..KnowledgeQuery::default()
    };

    let (_, cut) = pool_query(&input, MODEL, DEFAULT_RESCUE_SCORE + 1.0);

    assert_eq!(cut.cut.floor, tuned);
    assert_eq!(cut.score, DEFAULT_RESCUE_SCORE + 1.0);
}

#[test]
fn a_scored_row_under_the_floor_stays_only_when_the_reranker_scores_it_a_match() {
    let arranged = vec![
        (row("lesson:match", under_floor()), scored(0.0)),
        (row("lesson:wrong", under_floor()), scored(-1.0)),
        (row("turn:lexical-only", 0.0), scored(-1.0)),
    ];

    assert_eq!(kept(arranged), vec!["lesson:match"]);
}

#[test]
fn a_scored_row_over_the_floor_stays_whatever_the_reranker_says() {
    let floor = score_floor_for(MODEL).floor;
    let arranged = vec![(row("lesson:over", floor), scored(-1.0))];

    assert_eq!(kept(arranged), vec!["lesson:over"]);
}

#[test]
fn an_unscored_row_keeps_the_plain_cut() {
    let floor = score_floor_for(MODEL).floor;
    let arranged = vec![
        (row("lesson:over", floor), None),
        (row("lesson:under", under_floor()), None),
        (row("seat:no-vector", 0.0), None),
    ];

    assert_eq!(kept(arranged), vec!["lesson:over", "seat:no-vector"]);
}

#[test]
fn the_window_under_the_best_row_still_applies_to_rows_the_reranker_rejects() {
    let floor = score_floor_for(MODEL);
    let best = floor.floor + floor.window * 2.0;
    let behind = best - floor.window * 1.5;
    let arranged = vec![
        (row("lesson:best", best), None),
        (row("lesson:behind", behind), scored(-1.0)),
        (row("lesson:rescued", behind), scored(0.0)),
    ];

    assert_eq!(kept(arranged), vec!["lesson:best", "lesson:rescued"]);
}
