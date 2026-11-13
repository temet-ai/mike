use super::fork_prefix::{earlier_messages_intact, keep_inherited_prefix};
use super::seat_role::SeatRole;
use super::test_support::noop_push_deps;
use super::{RemindersOutcome, SelfIdentityRequest, apply_self_identity};
use crate::store::models::RequestId;

fn message_zero_text() -> &'static str {
    r#"{"role":"user", "content":[{"type":"text","text":"first  prompt"}]}"#
}

fn conversation_of(count: usize) -> Vec<u8> {
    let mut messages = vec![message_zero_text().to_string()];
    for turn in 1..count - 1 {
        let role = if turn % 2 == 1 { "assistant" } else { "user" };
        messages.push(format!(
            r#"{{"role":"{role}","content":[{{"type":"text","text":"turn {turn}"}}]}}"#
        ));
    }
    messages.push(
        r#"{"role":"user","content":[{"type":"text","text":"the fork's own prompt"}]}"#.to_string(),
    );
    format!(r#"{{"model":"m","messages":[{}]}}"#, messages.join(",")).into_bytes()
}

fn messages_of(body: &[u8]) -> anyhow::Result<Vec<serde_json::Value>> {
    let value: serde_json::Value = serde_json::from_slice(body)?;
    Ok(value["messages"].as_array().cloned().unwrap_or_default())
}

#[tokio::test]
async fn a_fork_first_request_of_35_messages_gets_its_blocks_in_the_last_message_only()
-> anyhow::Result<()> {
    let body = conversation_of(35);
    assert_eq!(messages_of(&body)?.len(), 35);
    let outcome = apply_self_identity(SelfIdentityRequest {
        body_bytes: &body,
        session_id: "fork-prefix-session",
        request_id: RequestId::next(),
        push_deps: &noop_push_deps(),
    })
    .await;
    let outcome = keep_inherited_prefix(SeatRole::Fork, &body, outcome);
    let RemindersOutcome::Appended { body: sent, .. } = outcome else {
        anyhow::bail!("expected the identity block to be appended to the fork's newest message");
    };

    let sent_text = String::from_utf8(sent.clone())?;
    assert!(
        sent_text.contains(message_zero_text()),
        "message 0 must appear byte for byte as it came in"
    );
    let before = messages_of(&body)?;
    let after = messages_of(&sent)?;
    assert_eq!(after.len(), 35);
    assert_eq!(&after[..34], &before[..34]);
    assert_eq!(after[34]["content"].as_array().map(Vec::len), Some(2));
    assert!(earlier_messages_intact(&body, &sent));
    Ok(())
}

#[test]
fn a_rewritten_earlier_message_is_not_intact() {
    let body = conversation_of(5);
    let reserialized =
        serde_json::to_vec(&serde_json::from_slice::<serde_json::Value>(&body).unwrap_or_default())
            .unwrap_or_default();
    assert!(!earlier_messages_intact(&body, &reserialized));
}

#[test]
fn a_body_with_no_messages_is_not_intact() {
    assert!(!earlier_messages_intact(b"{}", b"{}"));
}

#[test]
fn a_fork_append_that_rewrites_an_earlier_message_is_dropped() {
    let body = conversation_of(5);
    let reserialized =
        serde_json::to_vec(&serde_json::from_slice::<serde_json::Value>(&body).unwrap_or_default())
            .unwrap_or_default();
    let appended = RemindersOutcome::Appended {
        body: reserialized,
        reminder_text: "x".to_string(),
        source_finding_ids: Vec::new(),
        source_request_id: 0,
    };
    assert!(matches!(
        keep_inherited_prefix(SeatRole::Fork, &body, appended),
        RemindersOutcome::Unchanged
    ));
}

#[tokio::test]
async fn a_fork_stage_leaves_message_zero_as_it_came_in() -> anyhow::Result<()> {
    use super::stage::SelfIdentityStage;
    use super::stage_test_support::{StageCtxFixture, make_ctx};
    use crate::extract::request::TurnOrigin;
    use crate::filter::FilterConfig;
    use crate::lesson_cache::LessonSnapshot;
    use crate::pipeline::{RequestStage, StageBody};

    let state =
        crate::state::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let session_id = "fork-prefix-stage-session";
    super::seat_role::remember_for_test(session_id, SeatRole::Fork);
    let resolved = state.resolve_session_toggles(session_id);
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let origin = TurnOrigin::PrimaryFirst;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &origin,
        session_id,
        project_path: "p",
        is_tool_loop: false,
        subagent_persona: None,
    });
    let raw = conversation_of(35);
    let mut body = StageBody::new(bytes::Bytes::from(raw.clone()), "orig".to_string());
    SelfIdentityStage.apply(&ctx, &mut body).await;
    assert!(body.filter_applied());
    assert!(earlier_messages_intact(&raw, body.body()));
    assert!(String::from_utf8_lossy(body.body()).contains(message_zero_text()));
    Ok(())
}
