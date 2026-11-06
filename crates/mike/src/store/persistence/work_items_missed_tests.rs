use bytes::Bytes;
use chrono::Utc;
use serde_json::{Value, json};

use super::{LaunchRecord, earlier_notifications, insert_launch};
use crate::store::models::RequestId;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::request_write::IdMode;
use crate::store::persistence::test_support as test_util;
use crate::work_items::parse::{Launch, WorkKind};

const BASH_TOOL_USE: &str = "toolu_01TUo1VKZVFPLSkz1GvnHgDW";

fn notification_text(task: &str, tool_use: &str, status: &str) -> String {
    format!(
        "<system-reminder>\n[SYSTEM NOTIFICATION - NOT USER INPUT]\nThis is an automated background-task event.\n\n<task-notification>\n<task-id>{task}</task-id>\n<tool-use-id>{tool_use}</tool-use-id>\n<output-file>/tmp/{task}.output</output-file>\n<status>{status}</status>\n<summary>Background command \"wait\" completed (exit code 0)</summary>\n</task-notification>\n</system-reminder>"
    )
}

fn text_message(role: &str, text: &str) -> Value {
    json!({"role": role, "content": [{"type": "text", "text": text}]})
}

fn tool_result_message(text: &str) -> Value {
    json!({"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": "toolu_x", "content": text}
    ]})
}

fn completion_answered_before_capture(task: &str, tool_use: &str) -> Value {
    json!({"messages": [
        text_message("user", "start the watch"),
        text_message("assistant", "watching"),
        {"role": "user", "content": notification_text(task, tool_use, "completed")},
        {"role": "assistant", "content": [{"type": "tool_use", "id": "toolu_y", "name": "Bash", "input": {}}]},
        tool_result_message("ok"),
        {"role": "system", "content": [{"type": "text", "text": "<total_tokens>1</total_tokens>"}]}
    ]})
}

fn ids(body: &Value) -> anyhow::Result<Vec<String>> {
    Ok(earlier_notifications(&serde_json::to_vec(body)?)
        .into_iter()
        .map(|n| n.task_id)
        .collect())
}

#[test]
fn completion_followed_by_more_messages_is_found() -> anyhow::Result<()> {
    let body = completion_answered_before_capture("b0qllsiw7", BASH_TOOL_USE);
    assert_eq!(ids(&body)?, vec!["b0qllsiw7"]);
    Ok(())
}

#[test]
fn notification_in_the_last_message_is_left_to_the_last_message_path() -> anyhow::Result<()> {
    let body = json!({"messages": [
        text_message("user", "go"),
        {"role": "user", "content": [{"type": "text", "text": notification_text("b1", "toolu_1", "completed")}]}
    ]});
    assert!(ids(&body)?.is_empty());
    Ok(())
}

#[test]
fn a_resume_after_a_notification_drops_that_notification() -> anyhow::Result<()> {
    let resumed = r#"{"success":true,"message":"Resuming agent a1","resumedAgentId":"a1"}"#;
    let body = json!({"messages": [
        {"role": "user", "content": notification_text("a1", "toolu_a", "completed")},
        tool_result_message(resumed),
        {"role": "user", "content": notification_text("b2", "toolu_b", "completed")},
        text_message("user", "next")
    ]});
    assert_eq!(ids(&body)?, vec!["b2"]);
    Ok(())
}

#[test]
fn a_notification_quoted_by_the_assistant_is_ignored() -> anyhow::Result<()> {
    let quoted = notification_text("b3", "toolu_c", "completed");
    let body = json!({"messages": [
        text_message("assistant", &quoted),
        text_message("user", "next")
    ]});
    assert!(ids(&body)?.is_empty());
    Ok(())
}

async fn session_db_id(handle: &PersistHandle, session: &str) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(session)
            .fetch_one(&handle.pool)
            .await?,
    )
}

async fn launch(
    handle: &PersistHandle,
    session_db_id: i64,
    kind: WorkKind,
    task: &str,
    wire: &str,
) -> anyhow::Result<()> {
    let mut conn = handle.pool.acquire().await?;
    let record = LaunchRecord {
        session_db_id,
        wire_tool_use_id: wire,
        launch: Launch {
            kind,
            task_id: task.to_string(),
            output_path: None,
        },
        description: None,
        owner_agent_id: None,
        request_db_id: 97_001,
        at: Utc::now() - chrono::Duration::minutes(10),
    };
    insert_launch(&mut conn, &record).await?;
    Ok(())
}

async fn send(handle: &PersistHandle, session: &str, id: u64, body: &Value) -> anyhow::Result<()> {
    let mut req = crate::store::tests::make_test_request(Some(RequestId::from_raw(id)), session);
    req.request_body = Some(Bytes::from(serde_json::to_vec(body)?));
    handle
        .insert_request_initial(&req, id == 97_001, IdMode::Ensure)
        .await?;
    Ok(())
}

async fn state_of(handle: &PersistHandle, wire: &str) -> anyhow::Result<(String, Option<i64>)> {
    Ok(sqlx::query_as(
        "SELECT state, finished_request_id FROM agents.work_items WHERE wire_tool_use_id = $1",
    )
    .bind(wire)
    .fetch_one(&handle.pool)
    .await?)
}

#[tokio::test]
async fn completion_seen_only_mid_conversation_finishes_the_running_item() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = uuid::Uuid::new_v4().to_string();
    send(
        &handle,
        &session,
        97_001,
        &json!({"messages": [text_message("user", "hi")]}),
    )
    .await?;
    let session_db = session_db_id(&handle, &session).await?;
    launch(
        &handle,
        session_db,
        WorkKind::Bash,
        "b0qllsiw7",
        BASH_TOOL_USE,
    )
    .await?;
    assert_eq!(state_of(&handle, BASH_TOOL_USE).await?.0, "running");

    let body = completion_answered_before_capture("b0qllsiw7", BASH_TOOL_USE);
    send(&handle, &session, 97_002, &body).await?;

    let (state, finished_request) = state_of(&handle, BASH_TOOL_USE).await?;
    assert_eq!(state, "completed");
    assert!(finished_request.is_some());
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn resumed_agent_is_not_closed_by_its_earlier_completion() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = uuid::Uuid::new_v4().to_string();
    send(
        &handle,
        &session,
        97_001,
        &json!({"messages": [text_message("user", "hi")]}),
    )
    .await?;
    let session_db = session_db_id(&handle, &session).await?;
    launch(&handle, session_db, WorkKind::Agent, "a1", "toolu_agent").await?;
    let resumed = r#"{"success":true,"message":"Resuming agent a1","resumedAgentId":"a1"}"#;
    let body = json!({"messages": [
        {"role": "user", "content": notification_text("a1", "toolu_agent", "completed")},
        text_message("assistant", "resuming"),
        tool_result_message(resumed),
        text_message("assistant", "working"),
        text_message("user", "carry on")
    ]});
    send(&handle, &session, 97_002, &body).await?;

    assert_eq!(state_of(&handle, "toolu_agent").await?.0, "running");
    db.cleanup().await?;
    Ok(())
}
