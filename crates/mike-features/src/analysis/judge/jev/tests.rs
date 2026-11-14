use std::collections::BTreeMap;

use super::compose::{Composed, compose};
use super::questions::{self, Shape};
use super::state::{TurnFacts, state};
use crate::analysis::judge::two_axis::{Delivery, Reaction};
use crate::analysis::judge::{EvidenceQuality, JudgeLabel};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;

fn answered(shape: Shape, yes: &[(&str, f64)]) -> Asked {
    let mut answers: BTreeMap<String, Answer> = questions::questions(shape)
        .keys()
        .map(|k| {
            (
                k.clone(),
                Answer::Noul {
                    noul: 0.05,
                    confidence: None,
                },
            )
        })
        .collect();
    for (key, p) in yes {
        answers.insert(
            (*key).to_owned(),
            Answer::Noul {
                noul: *p,
                confidence: None,
            },
        );
    }
    Asked::answered(answers)
}

fn composed(shape: Shape, yes: &[(&str, f64)]) -> anyhow::Result<Composed> {
    compose(&answered(shape, yes), shape).ok_or_else(|| anyhow::anyhow!("no composition"))
}

#[test]
fn compose_legacy_error_admitted_is_corrected() -> anyhow::Result<()> {
    let c = composed(Shape::Legacy, &[(questions::OWN_ERROR, 0.9)])?;
    assert_eq!(c.label, JudgeLabel::Corrected);
    assert!(!c.uncertain());
    Ok(())
}

#[test]
fn compose_legacy_delivered_and_shipped_is_shipped_verified_with_a_check() -> anyhow::Result<()> {
    let c = composed(
        Shape::Legacy,
        &[
            (questions::DELIVERED, 0.9),
            (questions::SHIPPED, 0.8),
            (questions::CHECKED, 0.9),
        ],
    )?;
    assert_eq!(c.label, JudgeLabel::Shipped);
    assert_eq!(c.evidence, EvidenceQuality::Verified);
    assert_eq!(c.delivery, Delivery::Shipped);
    Ok(())
}

#[test]
fn compose_legacy_nothing_clear_is_unknown_and_uncertain() -> anyhow::Result<()> {
    let c = composed(Shape::Legacy, &[(questions::DELIVERED, 0.4)])?;
    assert_eq!(c.label, JudgeLabel::Unknown);
    assert_eq!(c.evidence, EvidenceQuality::Absent);
    assert!(c.uncertain());
    Ok(())
}

#[test]
fn compose_two_axis_without_next_message_is_no_response_and_certain() -> anyhow::Result<()> {
    let shape = Shape::TwoAxis { has_next: false };
    let c = composed(shape, &[(questions::DELIVERED, 0.9)])?;
    assert_eq!(c.reaction, Reaction::NoResponse);
    assert_eq!(c.label, JudgeLabel::Accepted);
    assert!(!c.uncertain());
    Ok(())
}

#[test]
fn compose_two_axis_correction_beats_acceptance() -> anyhow::Result<()> {
    let shape = Shape::TwoAxis { has_next: true };
    let c = composed(
        shape,
        &[
            (questions::CORRECTED, 0.8),
            (questions::ACCEPTED, 0.7),
            (questions::DELIVERED, 0.9),
        ],
    )?;
    assert_eq!(c.reaction, Reaction::Corrected);
    assert_eq!(c.label, JudgeLabel::Corrected);
    Ok(())
}

#[test]
fn compose_two_axis_open_part_is_partial_delivery() -> anyhow::Result<()> {
    let shape = Shape::TwoAxis { has_next: true };
    let c = composed(
        shape,
        &[(questions::ACCEPTED, 0.9), (questions::LEFT_OPEN, 0.8)],
    )?;
    assert_eq!(c.reaction, Reaction::Accepted);
    assert_eq!(c.delivery, Delivery::Partial);
    Ok(())
}

#[test]
fn compose_failed_ask_is_none_the_off_path() {
    let failed = Asked::failed_with("timeout");
    assert!(compose(&failed, Shape::Legacy).is_none());
}

#[test]
fn state_cuts_the_front_of_a_long_turn_and_keeps_its_end() {
    let long = format!("{}THE END", "x".repeat(10_000));
    let facts = TurnFacts {
        turn_id: 1,
        session_id: Some(2),
        operator_request: "do it",
        assistant_text: &long,
        tool_call_count: 3,
        tool_error_count: 0,
        next_message: Some("thanks"),
    };
    let with_next = state(&facts, true);
    let end = with_next["assistant_turn_end"].as_str().unwrap_or_default();
    assert!(end.ends_with("THE END"));
    assert_eq!(end.chars().count(), 3_000);
    assert_eq!(with_next["operator_next_message"], "thanks");
    assert!(state(&facts, false).get("operator_next_message").is_none());
}

#[test]
fn questions_only_ask_for_the_next_message_when_one_exists() {
    let with = questions::questions(Shape::TwoAxis { has_next: true });
    let without = questions::questions(Shape::TwoAxis { has_next: false });
    assert!(with.contains_key(questions::CORRECTED));
    assert!(!without.contains_key(questions::CORRECTED));
    assert!(questions::questions(Shape::Legacy).contains_key(questions::OWN_ERROR));
}
