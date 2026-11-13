use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::PersistError;
use crate::work_items::ledger::{GitOutputRow, WorkItemRow};
use crate::work_items::ledger_cache::{ActivityRow, GitDeltaRow, ScopeRow, SessionReads};

struct ScopeRecord {
    session_id: i64,
    session_uuid: String,
    kind: String,
    task_id: String,
    description: Option<String>,
    owner_agent_id: Option<String>,
    state: String,
    launched_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    exit_code: Option<i32>,
    summary: Option<String>,
}

struct ActivityRecord {
    session_id: i64,
    agent_id: Option<String>,
    last_request_at: DateTime<Utc>,
    main_last_request_at: Option<DateTime<Utc>>,
    last_edit_at: Option<DateTime<Utc>>,
}

struct GitRecord {
    result_id: i64,
    session_id: i64,
    agent_id: Option<String>,
    at: DateTime<Utc>,
    text: String,
}

fn scope_row(r: ScopeRecord) -> ScopeRow {
    ScopeRow {
        session_db_id: r.session_id,
        session_uuid: r.session_uuid,
        item: WorkItemRow {
            kind: r.kind,
            task_id: r.task_id,
            description: r.description,
            owner_agent_id: r.owner_agent_id,
            state: r.state,
            launched_at: r.launched_at,
            finished_at: r.finished_at,
            exit_code: r.exit_code,
            summary: r.summary,
            last_request_at: None,
            request_count: 0,
            last_edit_at: None,
        },
    }
}

async fn scope(
    pool: &PgPool,
    window_hours: i32,
    named: Option<&[Uuid]>,
) -> Result<Vec<ScopeRow>, PersistError> {
    let rows = match named {
        None => {
            sqlx::query_file_as!(
                ScopeRecord,
                "src/store/persistence/work_items_scope_all.sql",
                window_hours,
            )
            .fetch_all(pool)
            .await?
        }
        Some(sessions) => {
            sqlx::query_file_as!(
                ScopeRecord,
                "src/store/persistence/work_items_scope_named.sql",
                window_hours,
                sessions,
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows.into_iter().map(scope_row).collect())
}

async fn reads(
    pool: &PgPool,
    after: &[(i64, DateTime<Utc>)],
) -> Result<SessionReads, PersistError> {
    let ids: Vec<i64> = after.iter().map(|(id, _)| *id).collect();
    let since: Vec<DateTime<Utc>> = after.iter().map(|(_, at)| *at).collect();
    let activity = sqlx::query_file_as!(
        ActivityRecord,
        "src/store/persistence/work_items_activity_since.sql",
        &ids,
        &since,
    )
    .fetch_all(pool)
    .await?;
    let git = sqlx::query_file_as!(
        GitRecord,
        "src/store/persistence/work_items_git_since.sql",
        &ids,
        &since,
    )
    .fetch_all(pool)
    .await?;
    Ok(SessionReads {
        activity: activity
            .into_iter()
            .map(|a| ActivityRow {
                session_db_id: a.session_id,
                agent_id: a.agent_id,
                last_request_at: a.last_request_at,
                main_last_request_at: a.main_last_request_at,
                last_edit_at: a.last_edit_at,
            })
            .collect(),
        git: git
            .into_iter()
            .map(|g| GitDeltaRow {
                result_id: g.result_id,
                session_db_id: g.session_id,
                row: GitOutputRow {
                    agent_id: g.agent_id,
                    at: g.at,
                    text: g.text,
                },
            })
            .collect(),
    })
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn work_item_scope(
        self,
        window_hours: i32,
        named: Option<&[Uuid]>,
    ) -> Result<Vec<ScopeRow>, PersistError> {
        crate::metrics::time_db("work_item_scope", scope(&self.pool, window_hours, named)).await
    }

    pub async fn work_item_reads(
        self,
        after: &[(i64, DateTime<Utc>)],
    ) -> Result<SessionReads, PersistError> {
        if after.is_empty() {
            return Ok(SessionReads::default());
        }
        crate::metrics::time_db("work_item_reads", reads(&self.pool, after)).await
    }
}
