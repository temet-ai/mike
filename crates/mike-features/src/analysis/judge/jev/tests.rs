use std::collections::BTreeMap;

use super::compose::{Composed, compose, label_of};
use super::questions::{self, Shape};
use super::state::{TurnFacts, state};
use super::{askable, effective_shape};
use crate::analysis::judge::two_axis::{Delivery, Reaction};
use crate::analysis::judge::{EvidenceQuality, JudgeLabel};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;
use crate::store::persistence::operator_turns::projection::map_legacy_outcome;

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
    assert_eq!(c.label, JudgeLabel::Unknown);
    assert!(!c.uncertain());
    let shipped = composed(
        shape,
        &[(questions::DELIVERED, 0.9), (questions::SHIPPED, 0.8)],
    )?;
    assert_eq!(shipped.label, JudgeLabel::Shipped);
    Ok(())
}

#[test]
fn compose_two_axis_operator_acceptance_labels_accepted_even_when_not_seen_as_delivered()
-> anyhow::Result<()> {
    let shape = Shape::TwoAxis { has_next: true };
    let c = composed(
        shape,
        &[
            (questions::ACCEPTED, 0.8),
            (questions::NAMES_AN_ERROR, 0.1),
            (questions::DELIVERED, 0.1),
            (questions::ABANDONED, 0.9),
        ],
    )?;
    assert_eq!(c.reaction, Reaction::Accepted);
    assert_eq!(c.label, JudgeLabel::Accepted);
    Ok(())
}

#[test]
fn compose_two_axis_both_yes_is_corrected_even_when_acceptance_is_stronger() -> anyhow::Result<()> {
    let shape = Shape::TwoAxis { has_next: true };
    let c = composed(
        shape,
        &[(questions::ACCEPTED, 0.9), (questions::PUSHES_BACK, 0.6)],
    )?;
    assert_eq!(c.reaction, Reaction::Corrected);
    assert_eq!(c.label, JudgeLabel::Corrected);
    let below = composed(
        shape,
        &[(questions::ACCEPTED, 0.7), (questions::ASKS_AGAIN, 0.4)],
    )?;
    assert_eq!(below.reaction, Reaction::Accepted);
    Ok(())
}

#[test]
fn compose_two_axis_redirected_maps_like_the_operator_side() -> anyhow::Result<()> {
    let shape = Shape::TwoAxis { has_next: true };
    let redirected = composed(shape, &[(questions::REDIRECTED, 0.8)])?;
    assert_eq!(redirected.reaction, Reaction::Redirected);
    assert_eq!(redirected.label, JudgeLabel::Accepted);
    let shipped = composed(
        shape,
        &[
            (questions::REDIRECTED, 0.8),
            (questions::DELIVERED, 0.9),
            (questions::SHIPPED, 0.9),
        ],
    )?;
    assert_eq!(shipped.label, JudgeLabel::Shipped);
    Ok(())
}

#[test]
fn label_of_equals_the_operator_side_mapping_for_every_input() {
    let reactions = [
        Reaction::Corrected,
        Reaction::Redirected,
        Reaction::Accepted,
        Reaction::NoResponse,
        Reaction::Unclear,
    ];
    let deliveries = [
        Delivery::Shipped,
        Delivery::Delivered,
        Delivery::Partial,
        Delivery::None,
        Delivery::Question,
    ];
    for reaction in reactions {
        for delivery in deliveries {
            for shipped in [false, true] {
                let effective = if shipped { Delivery::Shipped } else { delivery };
                let expected =
                    map_legacy_outcome(reaction.code(), effective.code(), 1.0, 0.0, false);
                assert_eq!(
                    label_of(reaction, delivery, shipped).outcome_code(),
                    expected,
                    "{} {} {shipped}",
                    reaction.code(),
                    delivery.code()
                );
            }
        }
    }
}

#[test]
fn compose_two_axis_no_next_message_shipped_beats_part_left_open() -> anyhow::Result<()> {
    let shape = Shape::TwoAxis { has_next: false };
    let c = composed(
        shape,
        &[
            (questions::DELIVERED, 0.9),
            (questions::SHIPPED, 0.8),
            (questions::LEFT_OPEN, 0.8),
        ],
    )?;
    assert_eq!(c.delivery, Delivery::Partial);
    assert_eq!(c.label, JudgeLabel::Shipped);
    Ok(())
}

#[test]
fn compose_two_axis_correction_beats_acceptance() -> anyhow::Result<()> {
    let shape = Shape::TwoAxis { has_next: true };
    let c = composed(
        shape,
        &[
            (questions::NAMES_AN_ERROR, 0.8),
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
    for key in questions::CORRECTION_SIGNALS {
        assert!(with.contains_key(key), "{key}");
        assert!(!without.contains_key(key), "{key}");
    }
    assert!(questions::questions(Shape::Legacy).contains_key(questions::OWN_ERROR));
}

fn facts<'a>(assistant_text: &'a str, next_message: Option<&'a str>) -> TurnFacts<'a> {
    TurnFacts {
        turn_id: 1,
        session_id: Some(2),
        operator_request: "do it",
        assistant_text,
        tool_call_count: 0,
        tool_error_count: 0,
        next_message,
    }
}

#[test]
fn an_empty_next_message_is_no_next_message() {
    let empty = facts("done", Some("  "));
    assert!(state(&empty, true).get("operator_next_message").is_none());
    assert_eq!(
        effective_shape(&empty, Shape::TwoAxis { has_next: true }),
        Shape::TwoAxis { has_next: false }
    );
    let real = facts("done", Some("thanks"));
    assert_eq!(
        effective_shape(&real, Shape::TwoAxis { has_next: true }),
        Shape::TwoAxis { has_next: true }
    );
    assert_eq!(effective_shape(&empty, Shape::Legacy), Shape::Legacy);
}

#[test]
fn a_turn_with_no_assistant_text_is_not_asked() {
    assert!(!askable(&facts("", None)));
    assert!(!askable(&facts(" \n", Some("thanks"))));
    assert!(askable(&facts("done", None)));
}

#[test]
fn the_accepted_question_no_longer_takes_a_next_step_and_the_others_carry_their_limits() {
    let asked = questions::questions(Shape::TwoAxis { has_next: true });
    let text = |key: &str| format!("{:?}", asked.get(key));
    assert!(!text(questions::ACCEPTED).contains("next step"));
    assert!(text(questions::NAMES_AN_ERROR).contains("assistant's own work"));
    assert!(text(questions::SHIPPED).contains("offer"));
    assert!(text(questions::ACCEPTED).contains("`operator_next_message`"));
    assert!(text(questions::DELIVERED).contains("`operator_request`"));
}

#[test]
fn compose_correction_signals_any_one_at_or_over_half_is_corrected() -> anyhow::Result<()> {
    let shape = Shape::TwoAxis { has_next: true };
    for key in questions::CORRECTION_SIGNALS {
        let c = composed(shape, &[(key, 0.5), (questions::ACCEPTED, 0.9)])?;
        assert_eq!(c.reaction, Reaction::Corrected, "{key}");
        assert_eq!(c.label, JudgeLabel::Corrected, "{key}");
        assert!((c.confidence - 0.5).abs() < 1e-9, "{key}");
    }
    let strongest = composed(
        shape,
        &[
            (questions::NAMES_AN_ERROR, 0.6),
            (questions::SOUNDS_FRUSTRATED, 0.8),
        ],
    )?;
    assert!((strongest.confidence - 0.8).abs() < 1e-9);
    Ok(())
}

#[test]
fn compose_correction_signals_all_under_half_fall_through() -> anyhow::Result<()> {
    let shape = Shape::TwoAxis { has_next: true };
    let signals: Vec<(&str, f64)> = questions::CORRECTION_SIGNALS
        .iter()
        .map(|key| (*key, 0.49))
        .collect();
    let mut accepted = signals.clone();
    accepted.push((questions::ACCEPTED, 0.8));
    assert_eq!(composed(shape, &accepted)?.reaction, Reaction::Accepted);
    let unclear = composed(shape, &signals)?;
    assert_eq!(unclear.reaction, Reaction::Unclear);
    assert!((unclear.confidence - 0.51).abs() < 1e-9);
    Ok(())
}

#[test]
fn state_sends_no_tool_counts() {
    let mut f = facts("done", Some("thanks"));
    f.tool_call_count = 7;
    f.tool_error_count = 2;
    let sent = state(&f, true);
    assert!(sent.get("tool_call_count").is_none());
    assert!(sent.get("tool_error_count").is_none());
}

#[test]
fn state_flags_a_next_message_that_repeats_the_request() {
    let repeated = facts("done", Some("  do it \n"));
    let sent = state(&repeated, true);
    assert_eq!(sent["next_message_repeats_the_request"], true);
    assert_eq!(sent["operator_next_message"], "do it");
    let fresh = state(&facts("done", Some("now the docs")), true);
    assert_eq!(fresh["next_message_repeats_the_request"], false);
    assert!(
        state(&facts("done", None), true)
            .get("next_message_repeats_the_request")
            .is_none()
    );
}

#[test]
fn state_drops_a_machine_text_next_message_and_cleans_a_mixed_one() {
    let machine = facts(
        "done",
        Some("<task-notification>\n<status>completed</status>\n</task-notification>"),
    );
    assert!(state(&machine, true).get("operator_next_message").is_none());
    assert_eq!(
        effective_shape(&machine, Shape::TwoAxis { has_next: true }),
        Shape::TwoAxis { has_next: false }
    );
    let slash = facts("done", Some("/commit"));
    assert!(state(&slash, true).get("operator_next_message").is_none());
    let mixed = facts(
        "done",
        Some("<system-reminder>date is today</system-reminder>\nthat is still broken"),
    );
    assert_eq!(
        state(&mixed, true)["operator_next_message"],
        "that is still broken"
    );
}
