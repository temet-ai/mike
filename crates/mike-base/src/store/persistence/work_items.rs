use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value as JsonValue;
use sqlx::PgConnection;

use super::PersistError;
use crate::store::models::InterceptedRequest;
use crate::work_items::parse::{
    self, AGENT_LAUNCH_PREFIX, BASH_LAUNCH_PREFIX, Launch, Notification, RESUME_MARK, STOP_MARK,
};

#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct BackfillCounts {
    pub launches: u64,
    pub resumes: u64,
    pub notifications: u64,
}

pub struct LaunchRecord<'a> {
    pub session_db_id: i64,
    pub wire_tool_use_id: &'a str,
    pub launch: Launch,
    pub description: Option<&'a str>,
    pub owner_agent_id: Option<&'a str>,
    pub request_db_id: i64,
    pub at: DateTime<Utc>,
}

pub struct FinishContext<'a> {
    pub session_db_id: i64,
    pub notified_agent_id: Option<&'a str>,
    pub request_db_id: i64,
    pub at: DateTime<Utc>,
}

pub fn is_work_item_result(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with(BASH_LAUNCH_PREFIX)
        || trimmed.starts_with(AGENT_LAUNCH_PREFIX)
        || text.contains(RESUME_MARK)
        || text.contains(STOP_MARK)
}

pub async fn record_from_request_guarded(
    tx: &mut PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    request_db_id: i64,
) -> Result<(), PersistError> {
    let mut savepoint = sqlx::Acquire::begin(&mut *tx).await?;
    match record_from_request(&mut savepoint, req, session_db_id, request_db_id).await {
        Ok(()) => savepoint.commit().await?,
        Err(e) => {
            savepoint.rollback().await?;
            tracing::warn!(request_id = request_db_id, error = %e, "work item capture skipped");
        }
    }
    Ok(())
}

async fn record_from_request(
    tx: &mut PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    request_db_id: i64,
) -> Result<(), PersistError> {
    record_live_results(tx, req, session_db_id, request_db_id).await?;
    let Some(body) = req.request_body.as_deref() else {
        return Ok(());
    };
    let finish = FinishContext {
        session_db_id,
        notified_agent_id: req.claude_code_agent_id.as_deref(),
        request_db_id,
        at: req.created_at,
    };
    reconcile_missed_finishes(tx, body, &finish).await?;
    for note in parse::notifications_in_request_body(body) {
        apply_notification(tx, &note, &finish).await?;
    }
    Ok(())
}

struct RunningItem {
    kind: String,
    task_id: String,
    wire_tool_use_id: String,
}

impl RunningItem {
    fn is_finished_by(&self, note: &Notification) -> bool {
        match note.tool_use_id.as_deref() {
            Some(wire) => {
                self.wire_tool_use_id == wire
                    || (self.task_id == note.task_id && self.kind == "agent")
            }
            None => self.task_id == note.task_id,
        }
    }
}

async fn reconcile_missed_finishes(
    tx: &mut PgConnection,
    body: &[u8],
    finish: &FinishContext<'_>,
) -> Result<(), PersistError> {
    if !std::str::from_utf8(body).is_ok_and(|raw| raw.contains(parse::NOTIFICATION_OPEN)) {
        return Ok(());
    }
    let running = running_items(tx, finish.session_db_id, finish.notified_agent_id).await?;
    if running.is_empty() {
        return Ok(());
    }
    for note in earlier_notifications(body)
        .into_iter()
        .filter(|note| running.iter().any(|item| item.is_finished_by(note)))
    {
        apply_notification(tx, &note, finish).await?;
    }
    Ok(())
}

async fn running_items(
    tx: &mut PgConnection,
    session_db_id: i64,
    owner_agent_id: Option<&str>,
) -> Result<Vec<RunningItem>, PersistError> {
    let rows = sqlx::query!(
        "SELECT kind, task_id, wire_tool_use_id
           FROM agents.work_items
          WHERE session_id = $1
            AND state = 'running'
            AND owner_agent_id IS NOT DISTINCT FROM $2",
        session_db_id,
        owner_agent_id,
    )
    .fetch_all(&mut *tx)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| RunningItem {
            kind: row.kind,
            task_id: row.task_id,
            wire_tool_use_id: row.wire_tool_use_id,
        })
        .collect())
}

fn earlier_notifications(body: &[u8]) -> Vec<Notification> {
    let Ok(parsed) = serde_json::from_slice::<JsonValue>(body) else {
        return Vec::new();
    };
    let Some(messages) = parsed.get("messages").and_then(JsonValue::as_array) else {
        return Vec::new();
    };
    let last = messages.len().saturating_sub(1);
    let mut pending: Vec<Notification> = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if index != last && message.get("role").and_then(JsonValue::as_str) == Some("user") {
            pending.extend(parse::notifications_in_value(message));
        }
        for agent_id in resumed_agents(message) {
            pending.retain(|note| note.task_id != agent_id);
        }
    }
    pending
}

fn resumed_agents(message: &JsonValue) -> Vec<String> {
    let Some(blocks) = message.get("content").and_then(JsonValue::as_array) else {
        return Vec::new();
    };
    blocks
        .iter()
        .filter(|block| block.get("type").and_then(JsonValue::as_str) == Some("tool_result"))
        .filter_map(|block| block.get("content"))
        .filter_map(|content| parse::parse_resume("SendMessage", &parse::result_text(content)))
        .collect()
}

async fn record_live_results(
    tx: &mut PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    request_db_id: i64,
) -> Result<(), PersistError> {
    if !req
        .tool_results_in_body
        .iter()
        .any(|r| is_work_item_result(&parse::result_text(&r.content)))
    {
        return Ok(());
    }
    let rows = sqlx::query!(
        "SELECT u.wire_tool_use_id, u.tool_name, u.input->>'description' AS description, res.content, u.created_at
           FROM tools.results res
           JOIN tools.uses u ON u.id = res.tool_use_id
          WHERE res.request_id = $1
            AND u.tool_name IN ('Bash', 'Agent', 'Task', 'SendMessage', 'TaskStop')",
        request_db_id,
    )
    .fetch_all(&mut *tx)
    .await?;
    for row in &rows {
        let (id, name, description, content, called_at) = (
            &row.wire_tool_use_id,
            &row.tool_name,
            &row.description,
            &row.content,
            &row.created_at,
        );
        let text = parse::result_text(content);
        if let Some(agent_id) = parse::parse_resume(name, &text) {
            reopen_agent(tx, session_db_id, &agent_id).await?;
            continue;
        }
        if let Some(stopped) = parse::parse_stop(name, &text) {
            let finish = FinishContext {
                session_db_id,
                notified_agent_id: req.claude_code_agent_id.as_deref(),
                request_db_id,
                at: *called_at,
            };
            apply_notification(tx, &stopped, &finish).await?;
            continue;
        }
        let Some(launch) = parse::parse_launch(name, &text) else {
            continue;
        };
        let record = LaunchRecord {
            session_db_id,
            wire_tool_use_id: id,
            launch,
            description: description.as_deref(),
            owner_agent_id: req.claude_code_agent_id.as_deref(),
            request_db_id,
            at: *called_at,
        };
        insert_launch(tx, &record).await?;
    }
    Ok(())
}

pub async fn insert_launch(
    tx: &mut PgConnection,
    record: &LaunchRecord<'_>,
) -> Result<u64, PersistError> {
    let done = sqlx::query!(
        "INSERT INTO agents.work_items
            (session_id, wire_tool_use_id, kind, task_id, output_path, owner_agent_id,
             description, launched_request_id, launched_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (wire_tool_use_id) DO UPDATE
            SET launched_at = LEAST(agents.work_items.launched_at, EXCLUDED.launched_at)
          WHERE agents.work_items.launched_at > EXCLUDED.launched_at",
        record.session_db_id,
        record.wire_tool_use_id,
        record.launch.kind.code(),
        record.launch.task_id,
        record.launch.output_path.as_deref(),
        record.owner_agent_id,
        record.description,
        record.request_db_id,
        record.at,
    )
    .execute(&mut *tx)
    .await?;
    Ok(done.rows_affected())
}

pub async fn reopen_agent(
    tx: &mut PgConnection,
    session_db_id: i64,
    agent_id: &str,
) -> Result<u64, PersistError> {
    let done = sqlx::query!(
        "UPDATE agents.work_items
            SET state = 'running', finished_at = NULL, finished_request_id = NULL,
                exit_code = NULL, summary = NULL
          WHERE session_id = $1 AND task_id = $2 AND kind = 'agent' AND state <> 'running'",
        session_db_id,
        agent_id,
    )
    .execute(&mut *tx)
    .await?;
    Ok(done.rows_affected())
}

pub async fn apply_notification(
    tx: &mut PgConnection,
    note: &Notification,
    finish: &FinishContext<'_>,
) -> Result<u64, PersistError> {
    match note.tool_use_id.as_deref() {
        Some(wire_tool_use_id) => finish_by_wire(tx, note, finish, wire_tool_use_id).await,
        None => finish_by_task(tx, note, finish).await,
    }
}

async fn finish_by_wire(
    tx: &mut PgConnection,
    note: &Notification,
    finish: &FinishContext<'_>,
    wire_tool_use_id: &str,
) -> Result<u64, PersistError> {
    let done = sqlx::query_file!(
        "src/store/persistence/work_items_finish_by_wire.sql",
        note.state(),
        finish.at,
        finish.request_db_id,
        finish.notified_agent_id,
        note.exit_code,
        note.summary.as_deref(),
        finish.session_db_id,
        wire_tool_use_id,
        note.task_id,
    )
    .execute(&mut *tx)
    .await?;
    Ok(done.rows_affected())
}

async fn finish_by_task(
    tx: &mut PgConnection,
    note: &Notification,
    finish: &FinishContext<'_>,
) -> Result<u64, PersistError> {
    let done = sqlx::query_file!(
        "src/store/persistence/work_items_finish_by_task.sql",
        note.state(),
        finish.at,
        finish.request_db_id,
        finish.notified_agent_id,
        note.exit_code,
        note.summary.as_deref(),
        finish.session_db_id,
        note.task_id,
    )
    .execute(&mut *tx)
    .await?;
    Ok(done.rows_affected())
}

#[cfg(test)]
#[path = "work_items_missed_tests.rs"]
pub mod missed_tests;
