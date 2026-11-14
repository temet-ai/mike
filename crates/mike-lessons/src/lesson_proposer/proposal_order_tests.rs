use chrono::{TimeZone, Utc};

use super::*;
use crate::api::proposals::order_by_suggestion;
use crate::llm::decisions::Answer;

fn row(id: i64, decision: Option<&str>) -> PlacementProposalRow {
    PlacementProposalRow {
        lesson_id: id,
        placement: "rule".to_owned(),
        kind: "convention".to_owned(),
        stage: "daily".to_owned(),
        text: format!("lesson {id}"),
        topic: None,
        covered_by: None,
        reason: Some("because".to_owned()),
        agent_name: None,
        user_prompt: None,
        created_at: Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
        placed_at: None,
        decision: decision.map(str::to_owned),
        decided_at: None,
    }
}

fn pick(key: &str, probabilities: &[(&str, f64)]) -> Answer {
    Answer::Choice {
        choice: key.to_owned(),
        probabilities: probabilities
            .iter()
            .map(|(k, p)| ((*k).to_owned(), *p))
            .collect(),
        confidence: None,
    }
}

#[test]
fn compose_reads_the_accept_probability_and_the_top_choice() {
    let rows = [row(1, None), row(2, None)];
    let group: Vec<&PlacementProposalRow> = rows.iter().collect();
    let asked = Asked::answered(BTreeMap::from([
        (
            question_key(1),
            pick("accept", &[("accept", 0.8), ("reject", 0.2)]),
        ),
        (
            question_key(2),
            pick("duplicate", &[("accept", 0.1), ("duplicate", 0.7)]),
        ),
    ]));
    assert_eq!(
        compose(&asked, &group),
        vec![
            (1, 0.8, "accept".to_owned()),
            (2, 0.1, "duplicate".to_owned())
        ]
    );
}

#[test]
fn compose_failed_answer_leaves_the_arrival_order() {
    let rows = [row(1, None)];
    let group: Vec<&PlacementProposalRow> = rows.iter().collect();
    assert!(compose(&Asked::failed_with("timeout"), &group).is_empty());
}

#[test]
fn order_by_suggestion_puts_likely_accepts_first_and_decided_rows_last() {
    let mut rows = vec![
        row(1, None),
        row(2, Some("accepted")),
        row(3, None),
        row(4, None),
    ];
    let suggested = BTreeMap::from([(3, 0.9), (1, 0.2)]);
    order_by_suggestion(&mut rows, &suggested);
    assert_eq!(
        rows.iter().map(|r| r.lesson_id).collect::<Vec<_>>(),
        vec![3, 1, 4, 2]
    );
}

#[test]
fn label_for_compares_the_top_choice_with_the_operator_decision() {
    assert_eq!(label_for("accept", true), "right");
    assert_eq!(label_for("accept", false), "wrong");
    assert_eq!(label_for("duplicate", false), "right");
    assert_eq!(label_for("reject", true), "wrong");
}

#[test]
fn state_and_questions_describe_each_proposal() {
    let rows = [row(7, None)];
    let group: Vec<&PlacementProposalRow> = rows.iter().collect();
    assert_eq!(state(&group)["proposals"][0]["proposed_home"], "rule");
    assert!(questions(&group).contains_key("proposal_7"));
}
