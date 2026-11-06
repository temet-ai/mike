use super::support::{
    Stack, prompt_body, requests, system_reminder_only_body, tool_loop_body, turns, wait_for_turns,
};

async fn assert_machine_segment_closes_operator_turn(
    stack: &Stack,
    session: &str,
    body: serde_json::Value,
    expected_kind: &str,
) -> anyhow::Result<()> {
    stack
        .post(session, None, &prompt_body("please refactor the store"))
        .await?;
    wait_for_turns(&stack.pool, session, 1).await?;
    stack.post(session, None, &body).await?;
    let rows = wait_for_turns(&stack.pool, session, 2).await?;
    assert!(!rows[0].is_open, "operator turn must close: {rows:?}");
    assert_eq!(rows[0].opened_by.as_deref(), Some("operator"));
    assert_eq!(rows[1].opened_by.as_deref(), Some("machine"));
    assert_eq!(rows[1].prompt_kind.as_deref(), Some(expected_kind));
    assert!(rows[1].is_open);

    stack.post(session, None, &tool_loop_body()).await?;
    let requests = requests(&stack.pool, session).await?;
    let loop_request = requests.last().map(|r| r.operator_turn_id);
    assert_eq!(
        loop_request,
        Some(Some(rows[1].id)),
        "tool_loop after a machine segment must carry that row's id"
    );
    Ok(())
}

#[tokio::test]
async fn hook_feedback_opens_a_machine_segment() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    assert_machine_segment_closes_operator_turn(
        &stack,
        "550e8400-e29b-41d4-a716-4466554400b1",
        prompt_body("Stop hook feedback:\nunacknowledged message"),
        "hook-feedback",
    )
    .await?;
    stack.finish().await
}

#[tokio::test]
async fn teammate_message_opens_a_machine_segment() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    assert_machine_segment_closes_operator_turn(
        &stack,
        "550e8400-e29b-41d4-a716-4466554400b2",
        prompt_body("<teammate-message teammate_id=\"peer\">status?</teammate-message>"),
        "teammate-message",
    )
    .await?;
    stack.finish().await
}

#[tokio::test]
async fn wrapped_system_notification_opens_a_machine_segment() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    assert_machine_segment_closes_operator_turn(
        &stack,
        "550e8400-e29b-41d4-a716-4466554400b3",
        system_reminder_only_body("[SYSTEM NOTIFICATION - NOT USER INPUT]\nBackground task done."),
        "background-event",
    )
    .await?;
    stack.finish().await
}

#[tokio::test]
async fn genuine_prompt_after_machine_segment_reopens_operator_turn() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let session = "550e8400-e29b-41d4-a716-4466554400b4";
    stack
        .post(session, None, &prompt_body("Stop hook feedback:\nnote"))
        .await?;
    stack
        .post(session, None, &prompt_body("now write the tests"))
        .await?;
    let rows = wait_for_turns(&stack.pool, session, 2).await?;
    assert_eq!(rows[0].opened_by.as_deref(), Some("machine"));
    assert_eq!(rows[1].opened_by.as_deref(), Some("operator"));
    assert_eq!(turns(&stack.pool, session).await?.len(), 2);
    stack.finish().await
}
