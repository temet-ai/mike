use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::{PgConnection, PgPool};

use super::PersistError;
use super::work_items::{
    BackfillCounts, FinishContext, LaunchRecord, apply_notification, insert_launch, reopen_agent,
};
use crate::work_items::parse;

struct LaunchRow {
    wire_tool_use_id: String,
    tool_name: String,
    description: Option<String>,
    content: JsonValue,
    request_id: i64,
    created_at: DateTime<Utc>,
    agent_id: Option<String>,
}

struct NoteRow {
    body: JsonValue,
    request_id: i64,
    created_at: DateTime<Utc>,
    agent_id: Option<String>,
}

struct ResumeRow {
    tool_name: String,
    content: JsonValue,
    request_id: i64,
}

enum Event {
    Resume(String),
    Notes(JsonValue, DateTime<Utc>, Option<String>),
}

pub async fn backfill_session(
    pool: &PgPool,
    session_db_id: i64,
) -> Result<BackfillCounts, PersistError> {
    let launches = sqlx::query_file_as!(
        LaunchRow,
        "src/store/persistence/work_items_backfill_launches.sql",
        session_db_id,
    )
    .fetch_all(pool)
    .await?;
    let events = ordered_events(pool, session_db_id).await?;
    let mut tx = pool.begin().await?;
    let mut counts = BackfillCounts {
        launches: insert_launches(&mut tx, session_db_id, &launches).await?,
        ..BackfillCounts::default()
    };
    for (request_db_id, event) in events {
        match event {
            Event::Resume(agent_id) => {
                counts.resumes += reopen_agent(&mut tx, session_db_id, &agent_id).await?;
            }
            Event::Notes(body, at, agent) => {
                let finish = FinishContext {
                    session_db_id,
                    notified_agent_id: agent.as_deref(),
                    request_db_id,
                    at,
                };
                for note in parse::notifications_in_value(&body) {
                    counts.notifications += apply_notification(&mut tx, &note, &finish).await?;
                }
            }
        }
    }
    tx.commit().await?;
    Ok(counts)
}

async fn insert_launches(
    conn: &mut PgConnection,
    session_db_id: i64,
    rows: &[LaunchRow],
) -> Result<u64, PersistError> {
    let mut inserted = 0;
    for row in rows {
        let Some(launch) = parse::parse_launch(&row.tool_name, &parse::result_text(&row.content))
        else {
            continue;
        };
        let record = LaunchRecord {
            session_db_id,
            wire_tool_use_id: &row.wire_tool_use_id,
            launch,
            description: row.description.as_deref(),
            owner_agent_id: row.agent_id.as_deref(),
            request_db_id: row.request_id,
            at: row.created_at,
        };
        inserted += insert_launch(conn, &record).await?;
    }
    Ok(inserted)
}

async fn ordered_events(
    pool: &PgPool,
    session_db_id: i64,
) -> Result<Vec<(i64, Event)>, PersistError> {
    let resumes = sqlx::query_file_as!(
        ResumeRow,
        "src/store/persistence/work_items_backfill_resumes.sql",
        session_db_id,
    )
    .fetch_all(pool)
    .await?;
    let notes = sqlx::query_file_as!(
        NoteRow,
        "src/store/persistence/work_items_backfill_notes.sql",
        session_db_id,
    )
    .fetch_all(pool)
    .await?;
    let mut events: Vec<(i64, u8, Event)> = resumes
        .into_iter()
        .filter_map(|row| {
            parse::parse_resume(&row.tool_name, &parse::result_text(&row.content))
                .map(|agent| (row.request_id, 0, Event::Resume(agent)))
        })
        .collect();
    events.extend(notes.into_iter().map(|row| {
        (
            row.request_id,
            1,
            Event::Notes(row.body, row.created_at, row.agent_id),
        )
    }));
    events.sort_by_key(|(request_db_id, order, _)| (*request_db_id, *order));
    Ok(events
        .into_iter()
        .map(|(id, _, event)| (id, event))
        .collect())
}
