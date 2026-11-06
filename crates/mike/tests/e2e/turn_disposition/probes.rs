use super::support::{Stack, prompt_body, requests, turns, wait_for_turns};

#[tokio::test]
async fn activity_label_and_recap_probes_leave_the_open_turn_open() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let session = "550e8400-e29b-41d4-a716-4466554400c1";
    stack
        .post(session, None, &prompt_body("build the feature"))
        .await?;
    let before = wait_for_turns(&stack.pool, session, 1).await?;

    stack
        .post(
            session,
            None,
            &prompt_body("Describe your most recent action in 3-5 words"),
        )
        .await?;
    stack
        .post(
            session,
            None,
            &prompt_body("The user stepped away and is coming back. Recap in under 40 words."),
        )
        .await?;

    let after = turns(&stack.pool, session).await?;
    assert_eq!(after.len(), 1, "probes must open no row: {after:?}");
    assert_eq!(after[0].id, before[0].id);
    assert!(after[0].is_open, "probes must not close the open turn");
    let rows = requests(&stack.pool, session).await?;
    assert_eq!(rows[1].operator_turn_id, None);
    assert_eq!(rows[2].operator_turn_id, None);
    stack.finish().await
}
