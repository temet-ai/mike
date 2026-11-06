use crate::db_helpers;
use serde_json::Value;

use super::support::{
    InsertRequestParams, InsertTurnParams, bootstrap_project_session, insert_classification,
    insert_request, insert_turn,
};

#[tokio::test]
async fn session_head_originating_operator_prompt_is_first_operator_turn() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "4a000000-0000-4000-8000-000000000001";
    let project_id = bootstrap_project_session(&pool, "pk-session-head", session_id).await?;

    let turns_data = [
        (
            "Describe your most recent action in 3-5 words: doing setup",
            "activity-label",
        ),
        ("implement the authentication flow", "operator"),
        ("Stop hook feedback: pre-commit lint error", "hook-feedback"),
        ("add tests for the auth flow", "operator"),
    ];
    for (i, (prompt, kind)) in turns_data.iter().enumerate() {
        let r = insert_request(
            &pool,
            InsertRequestParams {
                session_id,
                project_id,
                turn_origin: "primary",
                turn_type: "user_prompt",
                status: "complete",
                human_prompt: prompt,
                completed: true,
                created_at_offset_ms: (i as i64) * 1_000,
            },
        )
        .await?;
        insert_turn(
            &pool,
            InsertTurnParams {
                session_id,
                project_id,
                start_request_id: r,
                end_request_id: r,
                user_prompt: Some(prompt),
                outcome: "accepted",
                closed: true,
            },
        )
        .await?;
        insert_classification(&pool, r, session_id, kind).await?;
    }

    let client = reqwest::Client::new();
    let head: Value = client
        .get(format!("{inspect_url}/api/sessions/{session_id}/head"))
        .send()
        .await?
        .json()
        .await?;

    assert_eq!(
        head["originating_operator_prompt"], "implement the authentication flow",
        "originating_operator_prompt must be the FIRST operator-kind turn's user_prompt, \
         not the first turn overall (which is activity-label): {head}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn session_head_originating_operator_prompt_null_when_no_operator_turns() -> anyhow::Result<()>
{
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "5a000000-0000-4000-8000-000000000001";
    let project_id = bootstrap_project_session(&pool, "pk-no-operator", session_id).await?;

    let r = insert_request(
        &pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            status: "complete",
            human_prompt: "Describe your most recent action in 3-5 words: just label",
            completed: true,
            created_at_offset_ms: 0,
        },
    )
    .await?;
    insert_turn(
        &pool,
        InsertTurnParams {
            session_id,
            project_id,
            start_request_id: r,
            end_request_id: r,
            user_prompt: Some("Describe your most recent action in 3-5 words: just label"),
            outcome: "accepted",
            closed: true,
        },
    )
    .await?;
    insert_classification(&pool, r, session_id, "activity-label").await?;

    let client = reqwest::Client::new();
    let head: Value = client
        .get(format!("{inspect_url}/api/sessions/{session_id}/head"))
        .send()
        .await?
        .json()
        .await?;

    assert!(
        head["originating_operator_prompt"].is_null(),
        "sessions with no operator turns must return null for originating_operator_prompt: {head}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
