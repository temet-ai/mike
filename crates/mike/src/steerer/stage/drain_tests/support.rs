use bytes::Bytes;

use crate::config::{AppState, ProxyConfig};
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::PersistHandle;

pub(super) fn user_turn_body() -> Bytes {
    Bytes::from_static(
        br#"{"model":"claude-sonnet-4","messages":[{"role":"user","content":[{"type":"text","text":"q"}]}]}"#,
    )
}

pub(super) fn steerer_reminder(intervention_uuid: uuid::Uuid) -> SystemReminder {
    SystemReminder {
        source: ReminderSource::Steerer {
            kind: "refocus".to_string(),
            intervention_uuid: intervention_uuid.to_string(),
            advisory: false,
        },
        payload: "<system-reminder>steer</system-reminder>".to_string(),
    }
}

pub(super) async fn seed_request_row(
    pool: &sqlx::PgPool,
    project_id: i64,
    session_id: i64,
    human_prompt: &str,
) -> anyhow::Result<i64> {
    seed_request_row_with_origin(pool, project_id, session_id, human_prompt, "primary").await
}

pub(super) async fn seed_request_row_with_origin(
    pool: &sqlx::PgPool,
    project_id: i64,
    session_id: i64,
    human_prompt: &str,
    turn_origin_code: &str,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar!(
        r#"INSERT INTO requests.requests
               (session_id, project_id, method_id, path, is_streaming,
                message_count, is_tool_loop, status_id, params,
                last_user_message, human_prompt, turn_type_id, turn_origin_id)
           SELECT $1, $2, rm.id, '/v1/messages', false,
                  0, false, rs.id, '{}'::jsonb,
                  '', $3, rtt.id, rto.id
             FROM requests.methods rm, requests.statuses rs,
                  requests.turn_types rtt, requests.turn_origins rto
            WHERE rm.code = 'POST' AND rs.code = 'complete'
              AND rtt.code = 'user_prompt' AND rto.code = $4
           RETURNING id"#,
        session_id,
        project_id,
        human_prompt,
        turn_origin_code,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn drain_fixture() -> anyhow::Result<
    Option<(
        crate::store::persistence::test_support::TestDbHandle,
        PersistHandle,
        sqlx::PgPool,
        AppState,
        i64,
        i64,
        String,
        i64,
    )>,
> {
    let Some(db) = crate::store::persistence::test_support::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
        format!("proj-drain-staleness-{}", uuid::Uuid::new_v4()),
    )
    .fetch_one(&pool)
    .await?;
    let session_uuid = uuid::Uuid::new_v4();
    let session_id: i64 = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW()) RETURNING id",
        session_uuid,
        project_id,
    )
    .fetch_one(&pool)
    .await?;
    let trigger_request_id = seed_request_row(&pool, project_id, session_id, "").await?;

    let mut state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    state.persist = Some(handle.clone());

    Ok(Some((
        db,
        handle,
        pool,
        state,
        project_id,
        session_id,
        session_uuid.to_string(),
        trigger_request_id,
    )))
}

pub(super) async fn seed_intervention(
    handle: &PersistHandle,
    pool: &sqlx::PgPool,
    session_id: i64,
    trigger_request_id: i64,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<uuid::Uuid> {
    let steerer_id: i64 =
        sqlx::query_scalar!("SELECT id FROM steering.steerers WHERE name = 'verifier'")
            .fetch_one(pool)
            .await?;
    let intervention_uuid = uuid::Uuid::new_v4();
    handle
        .insert_intervention(crate::store::persistence::steering::NewIntervention {
            steerer_id,
            enrolment_id: None,
            session_id,
            verdict_code: "fired",
            mode_code: "augment",
            authority_code: "redirect",
            trigger_request_id: Some(trigger_request_id),
            confidence: Some(0.9),
            off_thread_topic: None,
            payload_text: Some("nudge text"),
            missing_evidence: None,
            model: Some("test-model"),
            anchor_text: None,
            intervention_uuid,
            expires_at,
            verdict_json: None,
            operator_turn_id: None,
            bundle_id: None,
            delivery_channel: None,
            addressed_to: None,
            deliver_after: None,
            wake_state: None,
        })
        .await?;
    Ok(intervention_uuid)
}

pub(super) async fn reverted_row(
    pool: &sqlx::PgPool,
    intervention_uuid: uuid::Uuid,
) -> anyhow::Result<(bool, Option<String>)> {
    let row = sqlx::query!(
        r#"SELECT (reverted_at IS NOT NULL) AS "reverted!", revert_reason
             FROM steering.interventions WHERE uuid = $1"#,
        intervention_uuid,
    )
    .fetch_one(pool)
    .await?;
    Ok((row.reverted, row.revert_reason))
}

pub(super) async fn stamp_pushed_at(
    pool: &sqlx::PgPool,
    intervention_uuid: uuid::Uuid,
    pushed_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<()> {
    sqlx::query!(
        "UPDATE steering.interventions SET pushed_at = $2 WHERE uuid = $1",
        intervention_uuid,
        pushed_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}
