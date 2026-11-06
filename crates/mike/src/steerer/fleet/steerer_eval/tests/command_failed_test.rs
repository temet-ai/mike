use std::sync::Arc;

use tokio::sync::RwLock;

use super::super::*;
use super::support::{
    evaluate_turn, make_fleet_ctx, make_verifier_steerer, mock_openrouter_returning,
    seed_project_and_session,
};
use crate::store::RequestStore;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;
use crate::store::retention::RetentionPolicy;

#[tokio::test]
async fn failed_command_does_not_overrule_a_declining_judge() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let (project_id, session_uuid, session_db_id) =
        seed_project_and_session(&pool, "proj-verifier-failed-command").await?;
    let (request_id, turn_id) = seed_closed_turn(&pool, session_db_id, project_id).await?;
    seed_failed_bash_tool_call(&pool, request_id, session_db_id, session_uuid).await?;

    let verdict_json = r#"{"fires":false,"confidence":0.9,"reason":"looks complete","nudge":""}"#;
    let (_openrouter_mock, llm) = mock_openrouter_returning(verdict_json).await?;

    let steerer = make_verifier_steerer();
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let ctx = make_fleet_ctx(store.clone(), Some(handle), llm);

    let eval_inputs = EvalInputs {
        eval_text: "All tests pass, this is complete.".to_string(),
        operator_ask: "please finish this up".to_string(),
        last_user_message: "please finish this up".to_string(),
        tool_calls_summary: String::new(),
        diagnostics: Vec::new(),
        anchor: None,
        trigger_point: "response",
        turn_origin: String::new(),
        subagent_persona: None,
        judged_turn_id: Some(turn_id),
        evaluation_turn_id: None,
        question: None,
    };
    evaluate_turn(
        &ctx,
        &steerer,
        session_uuid.to_string(),
        Some(request_id),
        eval_inputs,
    )
    .await;

    let reminders = store
        .write()
        .await
        .claim_reminders(&session_uuid.to_string());
    assert!(
        reminders.is_empty(),
        "a failed Bash command must not force a fire when the judge declined: {:?}",
        reminders.iter().map(|r| &r.payload).collect::<Vec<_>>()
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

pub(super) async fn seed_closed_turn(
    pool: &sqlx::PgPool,
    session_db_id: i64,
    project_id: i64,
) -> anyhow::Result<(i64, i64)> {
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
                session_id, project_id, method_id, path, is_streaming,
                message_count, is_tool_loop, status_id,
                params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
                created_at, completed_at
             )
             SELECT $1, $2, rm.id, '/v1/messages', false,
                    0, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id, NOW(), NOW()
               FROM requests.methods rm, requests.statuses rs,
                    requests.turn_types rtt, requests.turn_origins rto
              WHERE rm.code = 'POST' AND rs.code = 'complete'
                AND rtt.code = 'user_prompt' AND rto.code = 'primary'
             RETURNING id",
    )
    .bind(session_db_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
                (session_id, project_id, start_request_id, end_request_id,
                 outcome_id, classified_at, closed_at, created_at)
             SELECT $1, $2, $3, $3, tot.id, NOW(), NOW(), NOW()
               FROM memory.turn_outcomes tot
              WHERE tot.code = 'unknown'
             RETURNING id",
    )
    .bind(session_db_id)
    .bind(project_id)
    .bind(request_id)
    .fetch_one(pool)
    .await?;

    sqlx::query("UPDATE requests.requests SET operator_turn_id = $1 WHERE id = $2")
        .bind(turn_id)
        .bind(request_id)
        .execute(pool)
        .await?;

    Ok((request_id, turn_id))
}

async fn seed_failed_bash_tool_call(
    pool: &sqlx::PgPool,
    request_id: i64,
    session_db_id: i64,
    session_uuid: uuid::Uuid,
) -> anyhow::Result<()> {
    let tool_use_id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses
                (wire_tool_use_id, request_id, session_id, ord, tool_name, input)
             VALUES ($1, $2, $3, 0, 'Bash', $4)
             RETURNING id",
    )
    .bind(format!("toolu_test_{session_uuid}"))
    .bind(request_id)
    .bind(session_db_id)
    .bind(serde_json::json!({"command": "cargo test -p mike"}))
    .fetch_one(pool)
    .await?;

    sqlx::query(
            "INSERT INTO tools.results
                (request_id, tool_use_id, session_id, ord, is_error, content, content_sha256, content_bytes)
             VALUES ($1, $2, $3, 0, true, $4::jsonb, 'deadbeef', 20)",
        )
        .bind(request_id)
        .bind(tool_use_id)
        .bind(session_db_id)
        .bind(serde_json::json!("2 failed, 0 passed"))
        .execute(pool)
        .await?;

    Ok(())
}
