use crate::db_helpers;

use super::support::{
    InsertOperatorTurnParams, InsertRequestParams, bootstrap_project_session, insert_operator_turn,
    insert_request,
};
use sqlx::PgPool;

async fn tag_implement_turn(pool: &PgPool, turn_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.operator_turns
            SET subject_command = '/implement', subject_agent = 'rust-developer',
                skills_used = ARRAY['ast-grep']
          WHERE id = $1",
    )
    .bind(turn_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn tag_review_turn(pool: &PgPool, turn_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.operator_turns
            SET subject_command = '/review', subject_agent = 'frontend-developer',
                skills_used = ARRAY[]::text[]
          WHERE id = $1",
    )
    .bind(turn_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_subject_and_skill_turns(
    pool: &PgPool,
    project_id: i64,
    session_id: &str,
) -> anyhow::Result<(i64, i64, i64)> {
    let r1 = insert_request(
        pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: "/implement the thing",
            created_at_offset_ms: 0,
        },
    )
    .await?;
    let r2 = insert_request(
        pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: "/review the diff",
            created_at_offset_ms: 1_000,
        },
    )
    .await?;
    let r3 = insert_request(
        pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: "just do the thing directly",
            created_at_offset_ms: 2_000,
        },
    )
    .await?;

    let turn1 = insert_operator_turn(
        pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: r1,
            end_request_id: r1,
            user_prompt: "/implement the thing",
            outcome: "shipped",
            outcome_evidence: None,
        },
    )
    .await?;
    let turn2 = insert_operator_turn(
        pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: r2,
            end_request_id: r2,
            user_prompt: "/review the diff",
            outcome: "unknown",
            outcome_evidence: None,
        },
    )
    .await?;
    let turn3 = insert_operator_turn(
        pool,
        InsertOperatorTurnParams {
            session_id,
            project_id,
            start_request_id: r3,
            end_request_id: r3,
            user_prompt: "just do the thing directly",
            outcome: "unknown",
            outcome_evidence: None,
        },
    )
    .await?;

    tag_implement_turn(pool, turn1).await?;
    tag_review_turn(pool, turn2).await?;

    Ok((turn1, turn2, turn3))
}

async fn fetch_turns(
    client: &reqwest::Client,
    inspect_url: &str,
    query: &str,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let url = format!("{inspect_url}/api/operator-turns{query}");
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    Ok(resp.json().await?)
}

fn turn_ids(body: &[serde_json::Value]) -> Vec<i64> {
    body.iter().filter_map(|r| r["turn_id"].as_i64()).collect()
}

#[tokio::test]
async fn list_operator_turns_filters_by_subject_and_skill() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, test_db) = proxy.into_parts();

    let project_path = "test-operator-turns-subject-filter";
    let session_id = "22000000-0000-4000-8000-000000000001";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;
    let (turn1, turn2, turn3) = seed_subject_and_skill_turns(&pool, project_id, session_id).await?;

    let client = reqwest::Client::new();

    let body = fetch_turns(
        &client,
        &inspect_url,
        &format!("?project={project_path}&subject_command=/implement"),
    )
    .await?;
    let ids = turn_ids(&body);
    assert_eq!(
        ids,
        vec![turn1],
        "subject_command=/implement must return only turn1; got {ids:?}"
    );

    let body = fetch_turns(
        &client,
        &inspect_url,
        &format!("?project={project_path}&subject_agent=frontend-developer"),
    )
    .await?;
    let ids = turn_ids(&body);
    assert_eq!(
        ids,
        vec![turn2],
        "subject_agent=frontend-developer must return only turn2; got {ids:?}"
    );

    let body = fetch_turns(
        &client,
        &inspect_url,
        &format!("?project={project_path}&skill=ast-grep"),
    )
    .await?;
    let ids = turn_ids(&body);
    assert_eq!(
        ids,
        vec![turn1],
        "skill=ast-grep must return only turn1; got {ids:?}"
    );

    let body = fetch_turns(
        &client,
        &inspect_url,
        &format!("?project={project_path}&subject_agent=rust-developer&outcome=shipped"),
    )
    .await?;
    let ids = turn_ids(&body);
    assert_eq!(
        ids,
        vec![turn1],
        "subject_agent=rust-developer&outcome=shipped must return only turn1; got {ids:?}"
    );
    let body = fetch_turns(
        &client,
        &inspect_url,
        &format!("?project={project_path}&subject_agent=rust-developer&outcome=unknown"),
    )
    .await?;
    assert!(
        body.is_empty(),
        "subject_agent=rust-developer&outcome=unknown must be empty; got {body:?}"
    );

    let body = fetch_turns(&client, &inspect_url, &format!("?project={project_path}")).await?;
    let mut ids = turn_ids(&body);
    ids.sort_unstable();
    let mut expected = vec![turn1, turn2, turn3];
    expected.sort_unstable();
    assert_eq!(
        ids, expected,
        "omitting subject_command/subject_agent/skill must reproduce the full unfiltered set; got {ids:?}"
    );

    test_db.cleanup().await?;
    Ok(())
}
