use serde_json::json;

use super::{Candidate, RankingConfig, SEAT_BASE_WEIGHT, order, order_by_position};

fn candidate(reference: &str, kind: &str, machine: bool) -> Candidate {
    Candidate {
        reference: reference.to_owned(),
        kind: kind.to_owned(),
        lexical: 0.5,
        semantic: 0.5,
        machine,
    }
}

fn kind_weights(weights: serde_json::Value) -> anyhow::Result<RankingConfig> {
    let mut config = RankingConfig::default();
    anyhow::ensure!(config.apply(super::KEY_KIND_WEIGHTS, &weights));
    Ok(config)
}

#[test]
fn order_by_position_with_equal_weights_keeps_the_rerank_order() {
    let candidates: Vec<Candidate> = (0..12)
        .map(|index| candidate(&format!("note:{index}"), "note", false))
        .collect();

    let order = order_by_position(&candidates, &RankingConfig::default());

    assert_eq!(order, (0..12).collect::<Vec<_>>());
}

#[test]
fn order_by_position_with_only_machine_rows_keeps_the_rerank_order() {
    let candidates: Vec<Candidate> = (0..6)
        .map(|index| candidate(&format!("mail:{index}"), "mail", true))
        .collect();

    let order = order_by_position(&candidates, &RankingConfig::default());

    assert_eq!(order, (0..6).collect::<Vec<_>>());
}

#[test]
fn order_by_position_moves_a_machine_row_the_reranker_put_first_below_operator_rows() {
    let candidates = vec![
        candidate("note:brief", "note", true),
        candidate("turn:1", "turn", false),
        candidate("turn:2", "turn", false),
    ];

    let order = order_by_position(&candidates, &RankingConfig::default());

    assert_eq!(order, vec![1, 2, 0]);
}

#[test]
fn order_by_position_ignores_the_kind_weights_of_operator_rows() -> anyhow::Result<()> {
    let candidates = vec![
        candidate("note:a", "note", false),
        candidate("turn:b", "turn", false),
    ];
    let config = kind_weights(json!({"note": 0.0}))?;

    assert_eq!(order_by_position(&candidates, &config), vec![0, 1]);
    Ok(())
}

#[test]
fn order_by_position_on_an_empty_list_returns_nothing() {
    assert!(order_by_position(&[], &RankingConfig::default()).is_empty());
}

#[test]
fn weight_of_a_machine_row_that_is_not_a_seat_row_is_the_seat_base_weight() -> anyhow::Result<()> {
    let config = kind_weights(json!({"mail": 2.0}))?;

    assert_eq!(
        config.weight(&candidate("mail:a", "mail", true)),
        SEAT_BASE_WEIGHT
    );
    assert_eq!(config.weight(&candidate("mail:a", "mail", false)), 2.0);
    Ok(())
}

#[test]
fn weight_of_a_seat_row_is_the_weight_configured_for_its_kind() -> anyhow::Result<()> {
    let config = kind_weights(json!({"seat": 0.8}))?;

    assert_eq!(config.weight(&candidate("seat:a", "seat", true)), 0.8);
    assert_eq!(
        RankingConfig::default().weight(&candidate("seat:a", "seat", true)),
        SEAT_BASE_WEIGHT
    );
    Ok(())
}

#[test]
fn order_sorts_a_machine_turn_below_an_operator_turn_that_matches_equally() {
    let candidates = vec![
        candidate("turn:machine", "turn", true),
        candidate("turn:operator", "turn", false),
    ];

    assert_eq!(order(&candidates, &RankingConfig::default()), vec![1, 0]);
}

#[test]
fn from_row_marks_a_row_written_by_a_machine_and_every_seat_row() {
    let machine = json!({"reference":"note:1","source_kind":"note",
        "provenance":{"written_by":"machine"}});
    let seat = json!({"reference":"seat:1","source_kind":"seat","provenance":{}});
    let operator = json!({"reference":"turn:1","source_kind":"turn","provenance":{}});

    assert!(Candidate::from_row(&machine).machine);
    assert!(Candidate::from_row(&seat).machine);
    assert!(!Candidate::from_row(&operator).machine);
}
