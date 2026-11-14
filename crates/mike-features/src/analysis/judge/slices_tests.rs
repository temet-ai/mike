use chrono::Utc;

use super::input_v2::{
    INPUT_BUILDER_V2, INPUT_BUILDER_V2_SLICES, JudgeInput, JudgeTurnContext, build_judge_input_v2,
};
use super::slices::{
    CONTEXT_ADDENDUM, JudgeSlice, SliceTexts, UnknownSliceCode, apply_slices,
    compose_system_prompt, needs_addendum, parse_slice_codes, render_turnfacts, slice_codes,
};
use crate::dossier::render::judge::JudgeSectionId;
use crate::store::persistence::turn_facts::TurnFactsRow;

fn base() -> JudgeInput {
    build_judge_input_v2(
        &JudgeTurnContext {
            turn_id: 7,
            session_id: 1,
            user_prompt: "fix the parser".to_owned(),
            final_assistant_text: "done".to_owned(),
            text_cut_at: 4,
            tool_call_count: 3,
            tool_error_count: 1,
            next_genuine_request_id: Some(9),
            next_genuine_text: Some("again, I already said no globals".to_owned()),
            next_genuine_at: None,
            closed_at: Utc::now(),
        },
        INPUT_BUILDER_V2,
    )
}

fn texts() -> SliceTexts {
    SliceTexts {
        asks: Some("first ask".to_owned()),
        directives: Some("no globals".to_owned()),
        corrections: Some("stop that".to_owned()),
        turnfacts: Some("Stop reason of the turn's last main-lane response: end_turn".to_owned()),
    }
}

fn facts(patch: serde_json::Value) -> anyhow::Result<TurnFactsRow> {
    let mut row = serde_json::json!({
        "operator_turn_id": 7,
        "session_id": 1,
        "builder_version": "v",
        "built_at": "2026-11-14T00:00:00Z",
        "max_request_id": 1,
        "prompt_kind": "genuine",
        "prompt_rule": null,
        "prompt_kind_at_close": "genuine",
        "prompt_rule_at_close": null,
        "next_state": "found",
        "next_genuine_request_id": null,
        "next_genuine_turn_id": null,
        "next_genuine_at": null,
        "next_state_at_close": "found",
        "next_genuine_request_id_at_close": null,
        "skipped_before_next": {},
        "reply_window_segment_ids": [],
        "reclassified_at": null,
        "text_chars": 4,
        "text_cut_at": 4,
        "last_text_origin": null,
        "text_spans": [],
        "stop_request_id": null,
        "stop_reason": "end_turn",
        "aborted": false,
        "interrupted": false,
        "close_cause": "next_prompt",
        "checks_run": [],
        "checks_count": 0,
        "checks_failed": 0,
        "ended_on": "question",
        "ended_on_version": "v",
        "session_class": "operator",
        "session_class_rule": "default",
    });
    for (key, value) in patch.as_object().into_iter().flatten() {
        row[key] = value.clone();
    }
    Ok(serde_json::from_value(row)?)
}

#[test]
fn the_vocabulary_round_trips_and_refuses_unknown_codes() {
    for code in [
        "asks",
        "directives",
        "corrections",
        "turnfacts",
        "withhold_next",
    ] {
        assert_eq!(JudgeSlice::from_code(code).map(JudgeSlice::code), Ok(code));
    }
    assert_eq!(
        JudgeSlice::from_code("hunches"),
        Err(UnknownSliceCode("hunches".to_owned()))
    );
    assert_eq!(
        parse_slice_codes(&["asks", "nope"]),
        Err(UnknownSliceCode("nope".to_owned()))
    );
    assert_eq!(
        slice_codes(&[JudgeSlice::Turnfacts, JudgeSlice::Asks]),
        vec!["turnfacts", "asks"]
    );
}

#[test]
fn sections_are_appended_in_the_fixed_order_with_the_dossier_headings() {
    let slices = [
        JudgeSlice::Turnfacts,
        JudgeSlice::Corrections,
        JudgeSlice::Asks,
        JudgeSlice::Directives,
    ];
    let input = apply_slices(base(), &slices, &texts());
    let expected = format!(
        "{}\n\n{}\nfirst ask\n\n{}\nno globals\n\n{}\nstop that\n\n{}\n{}",
        base().text,
        JudgeSectionId::EarlierOperatorMessages.heading(),
        JudgeSectionId::Directives.heading(),
        JudgeSectionId::PriorCorrections.heading(),
        "FACTS ABOUT THIS TURN (recorded by the proxy, not written by the assistant):",
        "Stop reason of the turn's last main-lane response: end_turn",
    );
    assert_eq!(input.text, expected);
    assert_eq!(input.builder_version, INPUT_BUILDER_V2_SLICES);
    assert_eq!(input.turn_id, 7);
    assert_eq!(input.next_genuine_request_id, Some(9));
}

#[test]
fn the_headings_are_the_exact_strings_the_judge_was_validated_on() {
    assert_eq!(
        JudgeSectionId::EarlierOperatorMessages.heading(),
        "EARLIER OPERATOR MESSAGES IN THIS SESSION (oldest first, word for word, all sent before this turn ended):"
    );
    assert_eq!(
        JudgeSectionId::Directives.heading(),
        "STANDING DIRECTIVES THE OPERATOR GAVE EARLIER IN THIS SESSION (quoted):"
    );
    assert_eq!(
        JudgeSectionId::PriorCorrections.heading(),
        "EARLIER CORRECTIONS IN THIS SESSION (the operator's own words, each sent before this turn ended):"
    );
}

#[test]
fn an_empty_or_missing_body_becomes_none() {
    let texts = SliceTexts {
        asks: Some(String::new()),
        ..SliceTexts::default()
    };
    let input = apply_slices(base(), &[JudgeSlice::Asks, JudgeSlice::Directives], &texts);
    assert!(input.text.ends_with(&format!(
        "{}\n(none)\n\n{}\n(none)",
        JudgeSectionId::EarlierOperatorMessages.heading(),
        JudgeSectionId::Directives.heading(),
    )));
}

#[test]
fn withhold_next_replaces_the_next_message_and_appends_nothing() {
    let input = apply_slices(base(), &[JudgeSlice::WithholdNext], &texts());
    assert!(
        input
            .text
            .ends_with("OPERATOR'S NEXT GENUINE MESSAGE (automated events removed):\n(none)")
    );
    assert!(!input.text.contains("no globals"));
    assert!(input.text.starts_with(&base().text[..40]));
    assert_eq!(input.builder_version, INPUT_BUILDER_V2);
}

#[test]
fn withhold_next_combines_with_an_appended_slice() {
    let input = apply_slices(
        base(),
        &[JudgeSlice::WithholdNext, JudgeSlice::Directives],
        &texts(),
    );
    assert!(input.text.contains(
        "OPERATOR'S NEXT GENUINE MESSAGE (automated events removed):\n(none)\n\nSTANDING DIRECTIVES"
    ));
    assert_eq!(input.builder_version, INPUT_BUILDER_V2_SLICES);
}

#[test]
fn no_slices_leave_the_input_untouched() {
    let input = base();
    assert_eq!(apply_slices(input.clone(), &[], &texts()), input);
}

#[test]
fn the_addendum_joins_the_prompt_unless_only_withhold_next_is_set() {
    assert!(!needs_addendum(&[]));
    assert!(!needs_addendum(&[JudgeSlice::WithholdNext]));
    assert!(needs_addendum(&[
        JudgeSlice::WithholdNext,
        JudgeSlice::Asks
    ]));
    assert_eq!(compose_system_prompt("rubric", &[]), "rubric");
    assert_eq!(
        compose_system_prompt("rubric", &[JudgeSlice::Turnfacts]),
        format!("rubric\n\n{CONTEXT_ADDENDUM}")
    );
    assert!(CONTEXT_ADDENDUM.starts_with("CONTEXT SECTIONS (background only)."));
}

#[test]
fn turnfacts_render_the_four_lines() -> anyhow::Result<()> {
    let row = facts(serde_json::json!({
        "interrupted": true,
        "checks_count": 4,
        "checks_failed": 2,
    }))?;
    assert_eq!(
        render_turnfacts(Some(&row)),
        "Stop reason of the turn's last main-lane response: end_turn\n\
         Interrupted (a request in the turn was aborted): yes\n\
         Checks run (test, build, lint, curl or screenshot commands): 4, 2 returned an error\n\
         How the turn ends, by phrase class: question"
    );
    Ok(())
}

#[test]
fn turnfacts_say_none_and_unknown_when_nothing_was_recorded() -> anyhow::Result<()> {
    let row = facts(serde_json::json!({"stop_reason": null, "ended_on": ""}))?;
    let text = render_turnfacts(Some(&row));
    assert!(text.contains("response: unknown\n"));
    assert!(text.contains(": no\n"));
    assert!(text.contains("commands): none\n"));
    assert!(text.ends_with("phrase class: unknown"));
    assert_eq!(render_turnfacts(None), "(none)");
    Ok(())
}
