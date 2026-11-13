use super::PersistError;
use super::SENTINEL_PROJECT_PATH;
use super::flow_capture_admission::{
    FLOW_HOOK_EVENT_NAME, FLOW_PRODUCER, FLOW_START_SOURCE_LIVE, capture_admits,
};

pub async fn resolve_session_id<'e, E>(
    executor: E,
    session_id: &str,
    project_id: Option<i64>,
    message_count: Option<i32>,
) -> Result<i64, PersistError>
where
    E: sqlx::PgExecutor<'e>,
{
    resolve_session_and_project_id(executor, session_id, project_id, message_count)
        .await
        .map(|(id, _)| id)
}

pub async fn resolve_session_and_project_id<'e, E>(
    executor: E,
    session_id: &str,
    project_id: Option<i64>,
    message_count: Option<i32>,
) -> Result<(i64, i64), PersistError>
where
    E: sqlx::PgExecutor<'e>,
{
    let admits = capture_admits(message_count);
    match project_id {
        Some(pid) => insert_session_in_project(executor, session_id, pid, admits).await,
        None => insert_session_in_sentinel_project(executor, session_id, admits).await,
    }
}

async fn insert_session_in_project<'e, E>(
    executor: E,
    session_id: &str,
    pid: i64,
    admits: bool,
) -> Result<(i64, i64), PersistError>
where
    E: sqlx::PgExecutor<'e>,
{
    let row = sqlx::query!(
        r#"INSERT INTO sessions.sessions
                         (session_uuid, project_id, first_seen, last_seen,
                          flow_capture_version, flow_admitted_at, flow_producer,
                          flow_hook_event_name, flow_start_source)
                     VALUES ($1::text::uuid, $2, NOW(), NOW(),
                         CASE WHEN $3 THEN 1 END,
                         CASE WHEN $3 THEN clock_timestamp() END,
                         CASE WHEN $3 THEN $4 END,
                         CASE WHEN $3 THEN $5 END,
                         CASE WHEN $3 THEN $6 END)
                     ON CONFLICT (session_uuid) DO SELECT
                     RETURNING id AS "id!", project_id AS "project_id!""#,
        session_id,
        pid,
        admits,
        FLOW_PRODUCER,
        FLOW_HOOK_EVENT_NAME,
        FLOW_START_SOURCE_LIVE,
    )
    .fetch_one(executor)
    .await?;
    Ok((row.id, row.project_id))
}

async fn insert_session_in_sentinel_project<'e, E>(
    executor: E,
    session_id: &str,
    admits: bool,
) -> Result<(i64, i64), PersistError>
where
    E: sqlx::PgExecutor<'e>,
{
    let row = sqlx::query!(
        r#"WITH p AS (
                     INSERT INTO projects.projects
                         (project_path, display_name, first_seen, last_seen)
                     VALUES ($2, $2, NOW(), NOW())
                     ON CONFLICT (project_path) DO SELECT
                     RETURNING id
                 )
                     INSERT INTO sessions.sessions
                         (session_uuid, project_id, first_seen, last_seen,
                          flow_capture_version, flow_admitted_at, flow_producer,
                          flow_hook_event_name, flow_start_source)
                     SELECT $1::text::uuid, pid.id, NOW(), NOW(),
                            CASE WHEN $3 THEN 1 END,
                            CASE WHEN $3 THEN clock_timestamp() END,
                            CASE WHEN $3 THEN $4 END,
                            CASE WHEN $3 THEN $5 END,
                            CASE WHEN $3 THEN $6 END
                       FROM p AS pid
                     ON CONFLICT (session_uuid) DO SELECT
                     RETURNING id AS "id!", project_id AS "project_id!""#,
        session_id,
        SENTINEL_PROJECT_PATH,
        admits,
        FLOW_PRODUCER,
        FLOW_HOOK_EVENT_NAME,
        FLOW_START_SOURCE_LIVE,
    )
    .fetch_one(executor)
    .await?;
    Ok((row.id, row.project_id))
}

pub async fn resolve_tool_use_id<'e, E>(
    executor: E,
    tool_id: &str,
) -> Result<Option<i64>, PersistError>
where
    E: sqlx::PgExecutor<'e>,
{
    let id = sqlx::query_scalar!(
        "SELECT id FROM tools.uses WHERE wire_tool_use_id = $1",
        tool_id
    )
    .fetch_optional(executor)
    .await?;
    Ok(id)
}

pub async fn resolve_or_create_agent_id<'e, E>(executor: E, name: &str) -> Result<i64, PersistError>
where
    E: sqlx::PgExecutor<'e>,
{
    let id = sqlx::query_scalar!(
        r#"INSERT INTO agents.agents (name)
             VALUES ($1)
             ON CONFLICT (LOWER(name)) DO SELECT
             RETURNING id AS "id!""#,
        name,
    )
    .fetch_one(executor)
    .await?;
    Ok(id)
}

pub async fn resolve_agent_id_by_uuid<'e, E>(
    executor: E,
    uuid: uuid::Uuid,
) -> Result<Option<i64>, PersistError>
where
    E: sqlx::PgExecutor<'e>,
{
    let id = sqlx::query_scalar!("SELECT id FROM agents.agents WHERE uuid = $1", uuid)
        .fetch_optional(executor)
        .await?;
    Ok(id)
}

pub async fn resolve_lesson_id_by_uuid<'e, E>(
    executor: E,
    uuid: uuid::Uuid,
) -> Result<Option<i64>, PersistError>
where
    E: sqlx::PgExecutor<'e>,
{
    let id = sqlx::query_scalar!("SELECT id FROM memory.lessons WHERE uuid = $1", uuid)
        .fetch_optional(executor)
        .await?;
    Ok(id)
}

#[cfg(test)]
pub mod tests {
    use super::super::PersistHandle;
    use super::*;
    use crate::store::models::RequestStatus;
    use crate::store::persistence::request_write::IdMode;
    use crate::store::persistence::test_support as test_util;

    #[tokio::test]
    async fn resolve_tool_use_id_some_for_existing_none_for_missing() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let missing = resolve_tool_use_id(&handle.pool, "toolu_does_not_exist").await?;
        assert!(missing.is_none(), "missing tool_id must resolve to None");

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(90_001)),
            &session_id,
        );
        req.status = RequestStatus::Complete;
        req.completed_at = Some(req.created_at);
        req.tool_calls = vec![crate::types::ToolCall {
            tool_id: "toolu_resolve_me_001".to_string(),
            name: "Bash".to_string(),
            input_json: String::new(),
            complete: true,
        }];
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        let found = resolve_tool_use_id(&handle.pool, "toolu_resolve_me_001").await?;
        let found_id =
            found.ok_or_else(|| anyhow::anyhow!("seeded tool_use must resolve to Some(id)"))?;
        assert!(found_id > 0, "resolved tool_use id must be positive bigint");

        let db_id = sqlx::query_scalar!(
            "SELECT id FROM tools.uses WHERE wire_tool_use_id = $1",
            "toolu_resolve_me_001"
        )
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(db_id, found_id, "resolved id must match tool_uses PK");

        db.cleanup().await?;
        Ok(())
    }
}
