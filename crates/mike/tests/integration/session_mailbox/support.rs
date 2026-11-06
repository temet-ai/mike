use sqlx::PgPool;

pub(super) const TEST_BACKOFF_BASE_SECS: f64 = 0.3;
pub(super) const TEST_BACKOFF_CAP_SECS: f64 = 5.0;

pub(super) async fn insert_message(
    pool: &PgPool,
    message_uuid: uuid::Uuid,
    to_session_uuid: uuid::Uuid,
    from_session_id: Option<i64>,
    body: &str,
) -> anyhow::Result<Option<uuid::Uuid>> {
    let row = sqlx::query!(
        r#"INSERT INTO mailbox.messages
               (message_uuid, to_session_id, from_session_id, body)
           SELECT $1, s.id, $3, $4
             FROM sessions.sessions s
            WHERE s.session_uuid = $2
           RETURNING message_uuid"#,
        message_uuid,
        to_session_uuid,
        from_session_id,
        body,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.message_uuid))
}

pub(super) async fn claim_message(pool: &PgPool, message_uuid: uuid::Uuid) -> anyhow::Result<bool> {
    let rows = sqlx::query!(
        "UPDATE mailbox.messages
            SET claimed_at = now()
          WHERE message_uuid = $1
            AND claimed_at IS NULL",
        message_uuid,
    )
    .execute(pool)
    .await?;
    Ok(rows.rows_affected() > 0)
}

pub(super) async fn backfill_sent_request_id(
    pool: &PgPool,
    message_uuid: uuid::Uuid,
    request_id: i64,
) -> anyhow::Result<bool> {
    let rows = sqlx::query!(
        "UPDATE mailbox.messages
            SET sent_request_id   = $1,
                sent_at           = now(),
                delivery_attempts = delivery_attempts + 1
          WHERE message_uuid = $2
            AND sent_request_id IS NULL",
        request_id,
        message_uuid,
    )
    .execute(pool)
    .await?;
    Ok(rows.rows_affected() > 0)
}

pub(super) async fn bump_redelivery_attempt(
    pool: &PgPool,
    message_uuid: uuid::Uuid,
    max_attempts: i32,
    backoff_base_secs: f64,
    backoff_cap_secs: f64,
) -> anyhow::Result<Option<(i32, bool, bool)>> {
    let row = sqlx::query!(
        r#"WITH snap AS (
               SELECT delivery_attempts, next_retry_at
                 FROM mailbox.messages
                WHERE message_uuid = $1
                  AND sent_at IS NOT NULL
                  AND consumed_at IS NULL
                  AND dead_lettered_at IS NULL
                FOR UPDATE
           ),
           decided AS (
               SELECT delivery_attempts,
                      (next_retry_at IS NULL OR next_retry_at <= now()) AS eligible
                 FROM snap
           )
           UPDATE mailbox.messages m
              SET delivery_attempts = CASE
                      WHEN d.eligible THEN d.delivery_attempts + 1
                      ELSE d.delivery_attempts
                  END,
                  next_retry_at = CASE
                      WHEN d.eligible THEN now() + (
                          LEAST(
                              $3::double precision * power(2::double precision, (d.delivery_attempts - 1)::double precision),
                              $4::double precision
                          ) * interval '1 second'
                      )
                      ELSE m.next_retry_at
                  END,
                  dead_lettered_at = CASE
                      WHEN d.eligible AND d.delivery_attempts + 1 >= $2 THEN now()
                      ELSE m.dead_lettered_at
                  END
             FROM decided d
            WHERE m.message_uuid = $1
        RETURNING m.delivery_attempts AS "delivery_attempts!",
                  (m.dead_lettered_at IS NOT NULL) AS "dead_lettered!",
                  d.eligible AS "attempted!""#,
        message_uuid,
        max_attempts,
        backoff_base_secs,
        backoff_cap_secs,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.delivery_attempts, r.dead_lettered, r.attempted)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AckOutcome {
    Consumed(chrono::DateTime<chrono::Utc>),
    NeverDelivered,
    NotFound,
}

pub(super) async fn ack_message(
    pool: &PgPool,
    to_session_uuid: uuid::Uuid,
    message_uuid: uuid::Uuid,
) -> anyhow::Result<AckOutcome> {
    let updated = sqlx::query_scalar!(
        r#"UPDATE mailbox.messages m
              SET consumed_at = now()
             FROM sessions.sessions recip
            WHERE m.message_uuid = $2
              AND recip.id = m.to_session_id
              AND recip.session_uuid = $1
              AND m.consumed_at IS NULL
              AND m.sent_request_id IS NOT NULL
        RETURNING m.consumed_at AS "consumed_at!""#,
        to_session_uuid,
        message_uuid,
    )
    .fetch_optional(pool)
    .await?;
    if let Some(consumed_at) = updated {
        return Ok(AckOutcome::Consumed(consumed_at));
    }
    let existing = sqlx::query_scalar!(
        r#"SELECT m.consumed_at AS "consumed_at?"
             FROM mailbox.messages m
             JOIN sessions.sessions recip ON recip.id = m.to_session_id
            WHERE m.message_uuid = $2 AND recip.session_uuid = $1"#,
        to_session_uuid,
        message_uuid,
    )
    .fetch_optional(pool)
    .await?;
    Ok(match existing {
        None => AckOutcome::NotFound,
        Some(Some(consumed_at)) => AckOutcome::Consumed(consumed_at),
        Some(None) => AckOutcome::NeverDelivered,
    })
}

pub(super) async fn redelivery_state(
    pool: &PgPool,
    message_uuid: uuid::Uuid,
) -> anyhow::Result<(i32, bool, bool)> {
    let row = sqlx::query!(
        r#"SELECT delivery_attempts AS "delivery_attempts!",
                  consumed_at AS "consumed_at?",
                  dead_lettered_at AS "dead_lettered_at?"
             FROM mailbox.messages WHERE message_uuid = $1"#,
        message_uuid,
    )
    .fetch_one(pool)
    .await?;
    Ok((
        row.delivery_attempts,
        row.consumed_at.is_some(),
        row.dead_lettered_at.is_some(),
    ))
}

pub(super) async fn fetch_pending_unclaimed(
    pool: &PgPool,
    to_session_uuid: uuid::Uuid,
) -> anyhow::Result<Vec<uuid::Uuid>> {
    let rows = sqlx::query!(
        r#"SELECT m.message_uuid AS "message_uuid!"
             FROM mailbox.messages m
             JOIN sessions.sessions recip ON recip.id = m.to_session_id
            WHERE recip.session_uuid = $1
              AND m.sent_request_id IS NULL
              AND m.claimed_at IS NULL
              AND m.consumed_at IS NULL
              AND m.dead_lettered_at IS NULL
            ORDER BY m.id ASC
            LIMIT 256"#,
        to_session_uuid,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.message_uuid).collect())
}

pub(super) async fn row_state(
    pool: &PgPool,
    message_uuid: uuid::Uuid,
) -> anyhow::Result<(Option<chrono::DateTime<chrono::Utc>>, Option<i64>)> {
    let row = sqlx::query!(
        r#"SELECT claimed_at AS "claimed_at?", sent_request_id AS "sent_request_id?"
             FROM mailbox.messages WHERE message_uuid = $1"#,
        message_uuid,
    )
    .fetch_one(pool)
    .await?;
    Ok((row.claimed_at, row.sent_request_id))
}

pub(super) async fn seed_request(
    pool: &PgPool,
    project_id: i64,
    session_uuid: uuid::Uuid,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming,
            message_count, is_tool_loop, status_id, params,
            last_user_message, human_prompt,
            turn_type_id, turn_origin_id, created_at
         )
         SELECT
                (SELECT id FROM sessions.sessions WHERE session_uuid = $1),
                $2, rm.id, '/v1/messages', false,
                0, false, rs.id, '{}'::jsonb,
                '', '',
                rtt.id, rto.id, now()
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(session_uuid)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(super) async fn seed_sent_message(
    pool: &PgPool,
    project_path: &str,
    to_uuid: uuid::Uuid,
) -> anyhow::Result<uuid::Uuid> {
    let project_id = crate::seed::seed_project(pool, project_path).await?;
    crate::seed::seed_session(pool, project_id, &to_uuid.to_string()).await?;
    let request_id = seed_request(pool, project_id, to_uuid).await?;

    let msg = uuid::Uuid::new_v4();
    insert_message(pool, msg, to_uuid, None, "redelivery-pin: please ack this").await?;
    claim_message(pool, msg).await?;
    backfill_sent_request_id(pool, msg, request_id).await?;
    Ok(msg)
}
