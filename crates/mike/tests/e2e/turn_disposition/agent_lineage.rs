use super::support::{
    MAIN_SYSTEM, Stack, WORKFLOW_HARNESS_PREFIX_PROMPT, WORKFLOW_SYSTEM_CURRENT, prompt_body,
    requests, turns, wait_for_turns, workflow_agent_body,
};

#[tokio::test]
async fn agent_id_workflow_prompt_is_attached_and_opens_no_turn() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let session = "550e8400-e29b-41d4-a716-4466554400a1";

    stack
        .post(session, None, &prompt_body("fix the parser bug"))
        .await?;
    let before = wait_for_turns(&stack.pool, session, 1).await?;
    stack
        .post(
            session,
            Some("agent-wf-1"),
            &workflow_agent_body(WORKFLOW_SYSTEM_CURRENT, "summarise the failing tests"),
        )
        .await?;

    let rows = requests(&stack.pool, session).await?;
    let agent = &rows[1];
    assert_eq!(agent.turn_origin, "workflow_subagent");
    assert_eq!(agent.author, "agent");
    assert_eq!(agent.operator_turn_id, Some(before[0].id));

    let after = turns(&stack.pool, session).await?;
    assert_eq!(after.len(), 1, "agent prompt must open no row: {after:?}");
    assert!(
        after[0].is_open,
        "agent prompt must not close the open turn"
    );
    assert_eq!(after[0].end_request_id, agent.id);
    stack.finish().await
}

#[tokio::test]
async fn harness_prefix_without_system_marker_is_still_workflow_subagent() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let session = "550e8400-e29b-41d4-a716-4466554400a2";

    stack
        .post(
            session,
            Some("agent-wf-2"),
            &workflow_agent_body(MAIN_SYSTEM, WORKFLOW_HARNESS_PREFIX_PROMPT),
        )
        .await?;

    let rows = requests(&stack.pool, session).await?;
    assert_eq!(rows[0].turn_origin, "workflow_subagent");
    assert_eq!(rows[0].author, "agent");
    assert!(turns(&stack.pool, session).await?.is_empty());
    stack.finish().await
}

#[tokio::test]
async fn plain_agent_id_request_is_subagent_never_primary() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let session = "550e8400-e29b-41d4-a716-4466554400a3";

    stack
        .post(
            session,
            Some("agent-plain"),
            &prompt_body("look at the logs"),
        )
        .await?;

    let rows = requests(&stack.pool, session).await?;
    assert_eq!(rows[0].turn_origin, "subagent");
    assert_eq!(rows[0].author, "agent");
    assert!(turns(&stack.pool, session).await?.is_empty());
    stack.finish().await
}

#[tokio::test]
async fn subagent_seen_first_does_not_claim_the_session_identity() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let session = "550e8400-e29b-41d4-a716-4466554400a4";

    stack
        .post(
            session,
            Some("agent-first"),
            &workflow_agent_body(WORKFLOW_SYSTEM_CURRENT, "first agent step"),
        )
        .await?;
    stack
        .post(session, None, &prompt_body("main thread prompt"))
        .await?;
    stack
        .post(
            session,
            Some("agent-first"),
            &workflow_agent_body(WORKFLOW_SYSTEM_CURRENT, "second agent step"),
        )
        .await?;

    let rows = requests(&stack.pool, session).await?;
    assert_eq!(rows[0].turn_origin, "workflow_subagent");
    assert_eq!(rows[1].turn_origin, "primary_first");
    assert_eq!(rows[2].turn_origin, "workflow_subagent");
    stack.finish().await
}
