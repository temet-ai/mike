use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::{LaunchSurfaceRefs, NewLaunch, NewRun};

pub(super) fn micros_now() -> anyhow::Result<DateTime<Utc>> {
    DateTime::from_timestamp_micros(Utc::now().timestamp_micros())
        .ok_or_else(|| anyhow::anyhow!("clock out of range"))
}

pub(super) async fn seed_run(handle: &PersistHandle, run_id: &str) -> anyhow::Result<()> {
    handle
        .upsert_run(&NewRun {
            run_id: run_id.to_owned(),
            project_path: Some("/tmp/fleet-a".to_owned()),
            backend: "herdr".to_owned(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    Ok(())
}

pub(super) async fn seed_session(
    pool: &PgPool,
    project_id: i64,
    session_uuid: Uuid,
) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW())
         RETURNING id",
        session_uuid,
        project_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn seed_launch(
    handle: &PersistHandle,
    run_id: &str,
    seat: &str,
    session_uuid: Option<Uuid>,
    opened: bool,
) -> anyhow::Result<i64> {
    let mut new = NewLaunch::new("herdr", "/tmp/fleet-a");
    new.run_id = Some(run_id.to_owned());
    new.seat = Some(seat.to_owned());
    new.agent = Some(format!("{seat}-agent"));
    new.model = Some("opus".to_owned());
    new.session_uuid = session_uuid;
    let id = handle.insert_launch(&new).await?;
    if opened {
        let surface = LaunchSurfaceRefs {
            surface_ref: Some(format!("pane-{seat}")),
            ..LaunchSurfaceRefs::default()
        };
        handle.record_launch_opened(id, &surface).await?;
    }
    Ok(id)
}

pub(super) async fn seed_request(
    pool: &PgPool,
    session_id: i64,
    project_id: i64,
    persona: Option<&str>,
    created_at: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar!(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id,
             params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
             subagent_persona, created_at, completed_at
         )
         SELECT $1, $2, rm.id, '/v1/messages', false,
                0, false, rs.id,
                '{}'::jsonb, '', '', rtt.id, rto.id,
                $3, $4, $4
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
        session_id,
        project_id,
        persona,
        created_at,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn seed_response(
    pool: &PgPool,
    request_id: i64,
    usage: &serde_json::Value,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO responses.responses (request_id, response_status, token_usage, completed_at)
         VALUES ($1, 200, $2, NOW())",
        request_id,
        usage,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) fn usage(input: i64, cache_read: i64, cache_creation: i64) -> serde_json::Value {
    json!({
        "input_tokens": input,
        "cache_read_input_tokens": cache_read,
        "cache_creation_input_tokens": cache_creation,
        "output_tokens": 7,
    })
}

pub(super) async fn seed_coordinator_traffic(
    pool: &PgPool,
    session_id: i64,
    project_id: i64,
    base: DateTime<Utc>,
) -> anyhow::Result<DateTime<Utc>> {
    let too_old = seed_request(
        pool,
        session_id,
        project_id,
        None,
        base - Duration::hours(2),
    )
    .await?;
    seed_response(pool, too_old, &usage(9_000, 0, 0)).await?;
    let first = seed_request(
        pool,
        session_id,
        project_id,
        None,
        base - Duration::minutes(5),
    )
    .await?;
    seed_response(pool, first, &usage(100, 50, 25)).await?;
    let subagent = seed_request(
        pool,
        session_id,
        project_id,
        Some("Explore"),
        base - Duration::minutes(3),
    )
    .await?;
    seed_response(pool, subagent, &usage(999, 0, 0)).await?;
    let last_at = base - Duration::minutes(1);
    let unmetered = seed_request(pool, session_id, project_id, None, last_at).await?;
    seed_response(pool, unmetered, &json!({})).await?;
    Ok(last_at)
}
