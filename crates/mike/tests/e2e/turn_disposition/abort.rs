use super::support::{
    ABORT_MARKER, Stack, prompt_body, requests, suggestion_probe_body, turns, wait_for_turns,
};

#[tokio::test]
async fn aborted_suggestion_sidecar_leaves_the_operator_turn_open() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let session = "550e8400-e29b-41d4-a716-4466554400d1";
    stack
        .post(session, None, &prompt_body("fix the bug"))
        .await?;
    let before = wait_for_turns(&stack.pool, session, 1).await?;

    let status = stack
        .post(session, None, &suggestion_probe_body(ABORT_MARKER))
        .await?;
    assert_eq!(
        status, 502,
        "oversize response must abort the sidecar request"
    );

    let rows = requests(&stack.pool, session).await?;
    assert_eq!(rows[1].status, "error");
    assert_eq!(rows[1].turn_origin, "next_input_suggestion_sidecar");
    let after = turns(&stack.pool, session).await?;
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, before[0].id);
    assert!(after[0].is_open, "aborted sidecar must not close the turn");
    stack.finish().await
}
