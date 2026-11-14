use super::{Words, facts_of, legacy_verdict, two_axis_verdict};
use crate::analysis::judge::input_v2::JudgeTurnContext;
use crate::analysis::judge::jev::Composed;
use crate::analysis::judge::two_axis::{Delivery, Reaction};
use crate::analysis::judge::{EvidenceQuality, JudgeLabel};

fn composed() -> Composed {
    Composed {
        label: JudgeLabel::Accepted,
        reaction: Reaction::Accepted,
        delivery: Delivery::Delivered,
        confidence: 0.9,
        evidence: EvidenceQuality::Verified,
    }
}

fn context(next: Option<&str>) -> JudgeTurnContext {
    JudgeTurnContext {
        turn_id: 5,
        session_id: 6,
        user_prompt: "fix it".to_owned(),
        final_assistant_text: "fixed, tests pass".to_owned(),
        text_cut_at: 0,
        tool_call_count: 4,
        tool_error_count: 1,
        next_genuine_request_id: None,
        next_genuine_text: next.map(str::to_owned),
        next_genuine_at: None,
        closed_at: chrono::Utc::now(),
    }
}

#[test]
fn two_axis_verdict_without_words_carries_the_code_summary_and_no_unmet_ask() {
    let v = two_axis_verdict(&composed(), None);
    assert_eq!(v.reaction, Reaction::Accepted);
    assert_eq!(v.delivery, Delivery::Delivered);
    assert!(v.rationale.contains("decided in code"));
    assert!(v.unmet_ask.is_empty());
}

#[test]
fn legacy_verdict_takes_the_old_models_words_when_given() {
    let words = Words {
        rationale: "the operator asked for X and got Y".to_owned(),
        unmet_ask: "X".to_owned(),
    };
    let v = legacy_verdict(&composed(), Some(words));
    assert_eq!(v.label, JudgeLabel::Accepted);
    assert_eq!(v.unmet_ask, "X");
    assert!((v.confidence - 0.9).abs() < 1e-6);
}

#[test]
fn facts_of_reads_the_next_message_only_when_one_exists() {
    let with = context(Some("thanks"));
    assert_eq!(facts_of(&with).next_message, Some("thanks"));
    let without = context(None);
    assert_eq!(facts_of(&without).next_message, None);
    assert_eq!(facts_of(&without).session_id, Some(6));
}
