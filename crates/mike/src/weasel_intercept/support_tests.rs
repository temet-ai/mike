use super::*;
use crate::store::persistence::capture_classification::classify_author_kind;

fn subagent_original() -> InterceptedRequest {
    let mut original = crate::store::tests::make_test_request(None, "session-360");
    original.turn_origin = "subagent".to_string();
    original.subagent_persona = Some("general-purpose".to_string());
    original.claude_code_agent_id = Some("agent-a1".to_string());
    original.agent_instance_id = Some(77);
    original
}

fn followup_for(original: &InterceptedRequest) -> InterceptedRequest {
    make_correction_intercepted_request(
        RequestId::next(),
        original.session_id.clone(),
        Bytes::from_static(b"{}"),
        proxy_tool_result_prompt("mike_session_roster"),
        chrono::Utc::now(),
        LaneIdentity::from_request(original),
    )
}

#[test]
fn followup_of_a_subagent_call_carries_the_agents_lane_identity() {
    let followup = followup_for(&subagent_original());
    assert_eq!(followup.turn_origin, "subagent");
    assert_eq!(
        followup.subagent_persona.as_deref(),
        Some("general-purpose")
    );
    assert_eq!(followup.claude_code_agent_id.as_deref(), Some("agent-a1"));
    assert_eq!(followup.agent_instance_id, Some(77));
}

#[test]
fn followup_of_a_first_main_lane_call_is_a_plain_primary_row() {
    let mut original = crate::store::tests::make_test_request(None, "session-360");
    original.turn_origin = "primary_first".to_string();
    let followup = followup_for(&original);
    assert_eq!(followup.turn_origin, "primary");
    assert_eq!(followup.claude_code_agent_id, None);
}

#[test]
fn followup_of_a_subagent_call_is_authored_by_an_agent() {
    let followup = followup_for(&subagent_original());
    let author = classify_author_kind(
        &followup.turn_origin,
        &followup.human_prompt,
        &followup.last_user_message,
        followup.is_tool_loop,
        followup.claude_code_agent_id.is_some(),
    );
    assert_eq!(author, "agent");
}

#[test]
fn followup_of_a_main_lane_call_is_never_authored_by_the_operator() {
    let original = crate::store::tests::make_test_request(None, "session-360");
    let followup = followup_for(&original);
    let author = classify_author_kind(
        &followup.turn_origin,
        &followup.human_prompt,
        &followup.last_user_message,
        followup.is_tool_loop,
        false,
    );
    assert_eq!(author, "harness");
}

#[test]
fn weasel_correction_prompt_is_never_authored_by_the_operator() {
    let author = classify_author_kind(
        "primary",
        &weasel_correction_prompt("probably"),
        "",
        false,
        false,
    );
    assert_eq!(author, "harness");
}

#[tokio::test]
async fn lane_identity_of_a_missing_request_is_the_main_lane() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = AppState::new(config).await?;
    let lane = LaneIdentity::of_request(&state, RequestId::from_raw(404_360)).await;
    assert_eq!(lane.turn_origin, "primary");
    assert_eq!(lane.claude_code_agent_id, None);
    Ok(())
}

#[tokio::test]
async fn lane_identity_of_a_stored_subagent_request_carries_its_agent_id() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = AppState::new(config).await?;
    let original = subagent_original();
    let id = original.id;
    state.store.write().await.insert(original);
    let lane = LaneIdentity::of_request(&state, id).await;
    assert_eq!(lane.turn_origin, "subagent");
    assert_eq!(lane.claude_code_agent_id.as_deref(), Some("agent-a1"));
    Ok(())
}
