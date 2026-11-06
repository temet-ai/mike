use super::*;

#[test]
fn vector_to_text_literal_roundtrip_shape() {
    let s = vector_to_text_literal(&[1.0, -2.5, 0.0]);
    assert_eq!(s, "[1,-2.5,0]");
}

#[test]
fn vector_to_text_literal_handles_empty_slice() {
    let s = vector_to_text_literal(&[]);
    assert_eq!(s, "[]");
}

use crate::extract::request::{TurnOrigin, TurnType};
use crate::store::models::RequestId;

#[tokio::test]
async fn record_prompt_kind_classification_is_noop_for_non_operator_turn_opener() {
    record_prompt_kind_classification(
        None,
        PromptKindClassificationInput {
            turn_origin: &TurnOrigin::Subagent,
            turn_type: &TurnType::UserPrompt,
            request_id: RequestId::next(),
            session_id: "session-prompt-kind-gate-test",
            session_db_id: Some(1),
            human_prompt: "hello",
            last_user_message: "",
            author: "operator",
            has_agent_id: false,
        },
    )
    .await;
}

#[tokio::test]
async fn record_prompt_kind_classification_is_noop_when_persist_is_unwired() {
    record_prompt_kind_classification(
        None,
        PromptKindClassificationInput {
            turn_origin: &TurnOrigin::Primary,
            turn_type: &TurnType::UserPrompt,
            request_id: RequestId::next(),
            session_id: "session-prompt-kind-gate-test",
            session_db_id: Some(1),
            human_prompt: "hello",
            last_user_message: "",
            author: "operator",
            has_agent_id: false,
        },
    )
    .await;
}
