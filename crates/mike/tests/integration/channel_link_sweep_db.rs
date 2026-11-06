use uuid::Uuid;

use crate::test_db::TestDb;

use mike::channel_link_sweep::sweep::run_sweep_tick;
use mike::store::persistence::PersistHandle;

async fn seed_project(pool: &sqlx::PgPool, path: &str) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
        path,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn seed_session_with_request(
    pool: &sqlx::PgPool,
    project_id: i64,
    request_body: Option<&[u8]>,
) -> anyhow::Result<Uuid> {
    let session_uuid = Uuid::new_v4();
    let session_id: i64 = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW()) RETURNING id",
        session_uuid,
        project_id,
    )
    .fetch_one(pool)
    .await?;
    sqlx::query!(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id,
             params, last_user_message, human_prompt, turn_type_id,
             turn_origin_id, request_body, created_at, completed_at
         )
         SELECT $1, $2, rm.id, '/v1/messages', false,
                1, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id,
                $3, NOW(), NOW()
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'",
        session_id,
        project_id,
        request_body,
    )
    .execute(pool)
    .await?;
    Ok(session_uuid)
}

#[tokio::test]
async fn sweep_tick_links_a_candidate_with_a_marker() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    sqlx::query!(
        "INSERT INTO remote_control.channels (cse_id) VALUES ($1)",
        "cse_sweepTickAbc123",
    )
    .execute(handle.pool())
    .await?;

    let project_id = seed_project(handle.pool(), "/tmp/sweep-tick-links").await?;
    let session_uuid = seed_session_with_request(
        handle.pool(),
        project_id,
        Some(
            br#"{"messages":[{"role":"user","content":[{"type":"text","text":"<system-reminder>\nAttribution for git commits and pull requests\nClaude-Session: https://claude.ai/code/session_01SAMPLE\n</system-reminder>"}]}]}"#,
        ),
    )
    .await?;

    run_sweep_tick(&handle, 120, 200).await;

    let linked: Option<Uuid> = sqlx::query_scalar!(
        "SELECT local_session_uuid FROM remote_control.channels
          WHERE cse_id = 'cse_sweepTickAbc123'"
    )
    .fetch_optional(handle.pool())
    .await?
    .flatten();
    assert_eq!(
        linked,
        Some(session_uuid),
        "the channel row must exist and be linked to the candidate session"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn sweep_tick_leaves_a_candidate_with_no_marker_alone() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let project_id = seed_project(handle.pool(), "/tmp/sweep-tick-no-marker").await?;
    seed_session_with_request(handle.pool(), project_id, Some(b"nothing to see here")).await?;

    run_sweep_tick(&handle, 120, 200).await;

    let count: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) AS \"count!\" FROM remote_control.channels")
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(
        count, 0,
        "no channel row may be fabricated for a markerless body"
    );

    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
