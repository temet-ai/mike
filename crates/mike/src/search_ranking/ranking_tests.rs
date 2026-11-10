use serde_json::json;

use super::{
    Candidate, KEY_KIND_WEIGHTS, KEY_LEXICAL_WEIGHT, KEY_SEMANTIC_FLOOR, KEY_SEMANTIC_WEIGHT,
    KEY_SEMANTIC_WINDOW, RankingConfig, order,
};

fn candidate(reference: &str, kind: &str, lexical: f64, semantic: f64) -> Candidate {
    Candidate {
        reference: reference.to_owned(),
        kind: kind.to_owned(),
        lexical,
        semantic,
    }
}

fn config_with(key: &str, value: serde_json::Value) -> RankingConfig {
    let mut config = RankingConfig::default();
    assert!(config.apply(key, &value), "{key} must accept {value}");
    config
}

#[test]
fn order_with_default_config_keeps_the_order_sql_fusion_gives() {
    let candidates = vec![
        candidate("lesson:a", "lesson", 0.9, 0.82),
        candidate("lesson:b", "lesson", 0.5, 0.80),
        candidate("note:c", "note", 0.0, 0.78),
    ];

    assert_eq!(order(&candidates, &RankingConfig::default()), vec![0, 1, 2]);
}

#[test]
fn order_with_default_config_ranks_a_semantic_only_match_below_a_double_match() {
    let candidates = vec![
        candidate("note:semantic", "note", 0.0, 0.9),
        candidate("note:both", "note", 0.4, 0.88),
    ];

    assert_eq!(order(&candidates, &RankingConfig::default()), vec![1, 0]);
}

#[test]
fn order_with_a_higher_semantic_weight_moves_the_semantic_leader_first() {
    let candidates = vec![
        candidate("lesson:x", "lesson", 0.9, 0.85),
        candidate("lesson:y", "lesson", 0.1, 0.9),
    ];
    let heavier = config_with(KEY_SEMANTIC_WEIGHT, json!(3.0));

    assert_eq!(order(&candidates, &RankingConfig::default()), vec![0, 1]);
    assert_eq!(order(&candidates, &heavier), vec![1, 0]);
}

#[test]
fn order_with_a_higher_lexical_weight_keeps_the_lexical_leader_first() {
    let candidates = vec![
        candidate("lesson:y", "lesson", 0.1, 0.9),
        candidate("lesson:x", "lesson", 0.9, 0.85),
    ];
    let heavier = config_with(KEY_LEXICAL_WEIGHT, json!(3.0));

    assert_eq!(order(&candidates, &heavier), vec![1, 0]);
}

#[test]
fn order_with_a_narrower_window_drops_a_semantic_match_far_from_the_best() {
    let candidates = vec![
        candidate("lesson:u", "lesson", 0.5, 0.9),
        candidate("lesson:v", "lesson", 0.4, 0.84),
        candidate("lesson:x", "lesson", 0.45, 0.0),
    ];
    let narrow = config_with(KEY_SEMANTIC_WINDOW, json!(0.04));

    assert_eq!(order(&candidates, &RankingConfig::default()), vec![0, 1, 2]);
    assert_eq!(order(&candidates, &narrow), vec![0, 2, 1]);
}

#[test]
fn order_with_a_higher_floor_drops_semantic_matches_below_it() {
    let candidates = vec![
        candidate("lesson:y1", "lesson", 0.1, 0.65),
        candidate("lesson:y2", "lesson", 0.2, 0.0),
        candidate("lesson:y3", "lesson", 0.05, 0.64),
    ];
    let strict = config_with(KEY_SEMANTIC_FLOOR, json!(0.7));

    assert_eq!(order(&candidates, &RankingConfig::default()), vec![0, 2, 1]);
    assert_eq!(order(&candidates, &strict), vec![1, 0, 2]);
}

#[test]
fn order_with_kind_weights_scales_the_named_kind_only() {
    let candidates = vec![
        candidate("lesson:a", "lesson", 0.9, 0.0),
        candidate("turn:b", "turn", 0.8, 0.0),
    ];
    let boosted = config_with(KEY_KIND_WEIGHTS, json!({"turn": 2.0}));
    let silenced = config_with(KEY_KIND_WEIGHTS, json!({"lesson": 0.0}));

    assert_eq!(order(&candidates, &RankingConfig::default()), vec![0, 1]);
    assert_eq!(order(&candidates, &boosted), vec![1, 0]);
    assert_eq!(order(&candidates, &silenced), vec![1, 0]);
}

#[test]
fn order_with_equal_scores_keeps_the_input_order() {
    let candidates = vec![
        candidate("note:first", "note", 0.5, 0.0),
        candidate("note:second", "note", 0.5, 0.0),
        candidate("note:third", "note", 0.5, 0.0),
    ];

    assert_eq!(order(&candidates, &RankingConfig::default()), vec![0, 1, 2]);
}

#[test]
fn order_with_no_candidates_is_empty() {
    assert!(order(&[], &RankingConfig::default()).is_empty());
}

#[test]
fn apply_clamps_each_value_into_its_range() {
    let mut config = RankingConfig::default();

    assert!(config.apply(KEY_LEXICAL_WEIGHT, &json!(9.0)));
    assert!(config.apply(KEY_SEMANTIC_FLOOR, &json!(1.5)));
    assert!(config.apply(KEY_SEMANTIC_WINDOW, &json!(0.5)));
    assert!(config.apply(KEY_KIND_WEIGHTS, &json!({"note": -3, "turn": 1.0})));

    assert_eq!(config.lexical_weight, 5.0);
    assert_eq!(config.semantic_floor, Some(1.0));
    assert_eq!(config.semantic_window, Some(0.2));
    assert_eq!(config.kind_weight("note"), 0.0);
    assert!(!config.kind_weights.contains_key("turn"));
}

#[test]
fn apply_with_a_value_of_the_wrong_type_changes_nothing() {
    let mut config = RankingConfig::default();

    assert!(!config.apply(KEY_LEXICAL_WEIGHT, &json!("heavy")));
    assert!(!config.apply(KEY_KIND_WEIGHTS, &json!([1, 2])));
    assert!(!config.apply(KEY_KIND_WEIGHTS, &json!({"note": "high"})));
    assert!(!config.apply("search.ranking.unknown", &json!(1.0)));

    assert!(config.is_default());
}

#[test]
fn order_with_floor_and_window_unset_counts_every_semantic_match_the_search_kept() {
    let candidates = vec![
        candidate("lesson:z1", "lesson", 0.0, 0.46),
        candidate("lesson:z2", "lesson", 0.0, 0.0),
    ];
    let floored = config_with(KEY_SEMANTIC_FLOOR, json!(0.6));

    assert_eq!(order(&candidates, &RankingConfig::default()), vec![0, 1]);
    assert_eq!(order(&candidates, &floored), vec![0, 1]);
    assert_eq!(
        config_with(KEY_SEMANTIC_FLOOR, json!(null)),
        RankingConfig::default()
    );
    assert_eq!(
        config_with(KEY_SEMANTIC_WINDOW, json!(0.12)).value(KEY_SEMANTIC_WINDOW),
        Some(json!(0.12))
    );
    assert_eq!(
        RankingConfig::default().value(KEY_SEMANTIC_FLOOR),
        Some(json!(null))
    );
}

#[test]
fn seat_rows_count_half_by_default_and_never_reach_operator_weight() {
    let candidates = vec![
        candidate("seat:s", "seat", 0.9, 0.0),
        candidate("lesson:l", "lesson", 0.5, 0.0),
    ];
    let raised = config_with(KEY_KIND_WEIGHTS, json!({"seat": 4.0}));
    let base = config_with(KEY_KIND_WEIGHTS, json!({"seat": 0.5}));

    assert_eq!(RankingConfig::default().kind_weight("seat"), 0.5);
    assert_eq!(
        order(&candidates, &config_with(KEY_LEXICAL_WEIGHT, json!(1.5))),
        vec![1, 0]
    );
    assert_eq!(raised.kind_weight("seat"), 0.9);
    assert_eq!(base, RankingConfig::default());
}

#[test]
fn semantic_cut_keeps_the_model_values_unless_a_setting_overrides_them() {
    use crate::store::persistence::project_knowledge::cut_for;

    let model = cut_for("embeddinggemma", RankingConfig::default().semantic_cut());
    let floored = cut_for(
        "embeddinggemma",
        config_with(KEY_SEMANTIC_FLOOR, json!(0.55)).semantic_cut(),
    );
    let windowed = cut_for(
        "nomic-embed-text:latest",
        config_with(KEY_SEMANTIC_WINDOW, json!(0.05)).semantic_cut(),
    );

    assert_eq!((model.floor, model.window), (0.45, 0.12));
    assert_eq!((floored.floor, floored.window), (0.55, 0.12));
    assert_eq!((windowed.floor, windowed.window), (0.60, 0.05));
}

#[test]
fn is_default_turns_false_after_any_value_moves() {
    assert!(RankingConfig::default().is_default());
    assert!(!config_with(KEY_SEMANTIC_WEIGHT, json!(1.5)).is_default());
    assert!(!config_with(KEY_KIND_WEIGHTS, json!({"note": 0.5})).is_default());
}

#[test]
fn value_round_trips_through_apply() {
    let config = config_with(KEY_KIND_WEIGHTS, json!({"note": 0.5}));
    let mut copy = RankingConfig::default();

    assert!(copy.apply(
        KEY_KIND_WEIGHTS,
        &config.value(KEY_KIND_WEIGHTS).unwrap_or_default()
    ));

    assert_eq!(copy, config);
}

#[test]
fn candidate_from_row_reads_the_search_row_fields() {
    let row = json!({"reference": "lesson:a", "source_kind": "lesson",
        "lexical_rank": 0.25, "semantic_rank": 0.75, "title": "ignored"});

    assert_eq!(
        Candidate::from_row(&row),
        candidate("lesson:a", "lesson", 0.25, 0.75)
    );
}
