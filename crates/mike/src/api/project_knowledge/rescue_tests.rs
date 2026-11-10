use serde_json::{Value, json};

use super::rescue::{Rescue, keep, keep_reporting, pool_query};
use crate::analysis::embeddings::score_floor_for;
use crate::analysis::rerank::DEFAULT_RESCUE_SCORE;
use crate::api::rerank_search::RerankReport;
use crate::store::persistence::project_knowledge::{KnowledgeQuery, SemanticCut};

const MODEL: &str = "embeddinggemma";

fn row(reference: &str, semantic: f64) -> Value {
    json!({"reference": reference, "semantic_rank": semantic, "lexical_rank": 0.01})
}

fn rescue() -> Rescue {
    Rescue {
        cut: score_floor_for(MODEL),
        score: DEFAULT_RESCUE_SCORE,
        min_kept: 0,
    }
}

#[test]
fn the_best_scored_rows_stay_up_to_min_kept_even_under_every_line() {
    let arranged = vec![
        (row("turn:best", under_floor()), scored(-3.0)),
        (row("turn:second", under_floor()), scored(-4.0)),
        (row("turn:third", under_floor()), scored(-5.0)),
        (row("seat:unscored", under_floor()), None),
    ];
    let rule = Rescue {
        min_kept: 2,
        ..rescue()
    };

    let kept: Vec<String> = keep(arranged, rule)
        .into_iter()
        .filter_map(|(row, _)| row["reference"].as_str().map(str::to_owned))
        .collect();

    assert_eq!(kept, vec!["turn:best", "turn:second"]);
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

    let (pool, cut) = pool_query(&input, MODEL, (DEFAULT_RESCUE_SCORE, 0));

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

    let (_, cut) = pool_query(&input, MODEL, (DEFAULT_RESCUE_SCORE + 1.0, 0));

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

#[test]
fn keep_reporting_counts_the_rows_it_drops_and_says_so() {
    let arranged = vec![
        (row("lesson:match", under_floor()), scored(0.0)),
        (row("lesson:wrong", under_floor()), scored(-1.0)),
        (row("turn:lexical-only", 0.0), scored(-1.0)),
    ];

    let (kept, report) = keep_reporting(arranged, rescue(), RerankReport::reranked());

    assert_eq!(kept.len(), 1);
    assert_eq!(report.left_out, Some(2));
    assert_eq!(
        report.detail.as_deref(),
        Some("2 candidates scored below the relevance line -1.25 and were left out")
    );
    assert_eq!(report.to_json()["left_out"], 2);
}

#[test]
fn keep_reporting_with_nothing_dropped_reports_zero_and_no_detail() {
    let floor = score_floor_for(MODEL).floor;
    let arranged = vec![(row("lesson:over", floor), scored(-1.0))];

    let (kept, report) = keep_reporting(arranged, rescue(), RerankReport::reranked());

    assert_eq!(kept.len(), 1);
    assert_eq!(report.left_out, Some(0));
    assert_eq!(report.detail, None);
    assert_eq!(report.to_json()["left_out"], 0);
}

#[test]
fn keep_reporting_with_one_dropped_row_uses_the_singular() {
    let arranged = vec![(row("lesson:wrong", under_floor()), scored(-1.0))];

    let (_, report) = keep_reporting(arranged, rescue(), RerankReport::reranked());

    assert_eq!(
        report.detail.as_deref(),
        Some("1 candidate scored below the relevance line -1.25 and was left out")
    );
}
