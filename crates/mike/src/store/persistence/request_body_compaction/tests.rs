use super::*;
use crate::store::persistence::test_support;
use chrono::{Duration, Utc};

struct SeededSession {
    session_id: i64,
    request_ids: Vec<i64>,
}

async fn seed_session(
    pool: &sqlx::PgPool,
    idle_days: i64,
    with_envelope: &[bool],
) -> anyhow::Result<SeededSession> {
    let now = Utc::now();
    let last_seen = now - Duration::days(idle_days);
    let project_path = format!("/tmp/compaction-{}", uuid::Uuid::new_v4());

    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects
            (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         RETURNING id",
    )
    .bind(&project_path)
    .bind(last_seen)
    .fetch_one(pool)
    .await?;

    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)
         RETURNING id",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(project_id)
    .bind(last_seen)
    .fetch_one(pool)
    .await?;

    let mut request_ids = Vec::with_capacity(with_envelope.len());
    for (ord, envelope) in with_envelope.iter().enumerate() {
        let body = format!(r#"{{"model":"claude-opus-5","messages":[],"turn":{ord}}}"#);
        let envelope_bytes = envelope
            .then(|| br#"{"model":"claude-opus-5","messages":[]}"#.to_vec())
            .map(|v| v.to_vec());
        let request_id: i64 = sqlx::query_scalar(
            "INSERT INTO requests.requests (
                session_id, project_id, method_id, path, is_streaming,
                message_count, is_tool_loop, status_id, params,
                last_user_message, human_prompt, turn_type_id, turn_origin_id,
                request_body, request_envelope, created_at
             )
             SELECT $1, $2, rm.id, '/v1/messages', false, 1, false, rs.id,
                    '{}'::jsonb, '', '', rtt.id, rto.id, $3, $4, $5
               FROM requests.methods rm, requests.statuses rs,
                    requests.turn_types rtt, requests.turn_origins rto
              WHERE rm.code = 'POST' AND rs.code = 'complete'
                AND rtt.code = 'user_prompt' AND rto.code = 'primary'
             RETURNING id",
        )
        .bind(session_id)
        .bind(project_id)
        .bind(body.as_bytes())
        .bind(envelope_bytes.as_deref())
        .bind(last_seen)
        .fetch_one(pool)
        .await?;
        request_ids.push(request_id);
    }

    Ok(SeededSession {
        session_id,
        request_ids,
    })
}

async fn body_of(pool: &sqlx::PgPool, request_id: i64) -> anyhow::Result<Option<Vec<u8>>> {
    let body: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT request_body FROM requests.requests WHERE id = $1")
            .bind(request_id)
            .fetch_one(pool)
            .await?;
    Ok(body)
}

#[tokio::test]
async fn compaction_keeps_the_final_request_of_a_session() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let seeded = seed_session(&handle.pool, 30, &[true, true, true]).await?;

    let survey = handle.survey_request_body_compaction(14).await?;
    assert_eq!(survey.candidate_rows, 2);
    assert_eq!(survey.sessions_affected, 1);
    assert_eq!(survey.refused_no_envelope_rows, 0);

    let outcome = handle.compact_request_bodies(14, 100).await?;
    assert_eq!(outcome.cleared_rows, 2);
    assert!(outcome.reclaimed_bytes > 0);

    assert!(
        body_of(&handle.pool, seeded.request_ids[0])
            .await?
            .is_none()
    );
    assert!(
        body_of(&handle.pool, seeded.request_ids[1])
            .await?
            .is_none()
    );
    assert!(
        body_of(&handle.pool, seeded.request_ids[2])
            .await?
            .is_some(),
        "a session's final request must keep its body"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn compaction_keeps_bodies_of_recently_active_sessions() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let young = seed_session(&handle.pool, 2, &[true, true, true]).await?;

    let survey = handle.survey_request_body_compaction(14).await?;
    assert_eq!(survey.candidate_rows, 0);
    assert_eq!(survey.reclaimable_bytes, 0);

    let outcome = handle.compact_request_bodies(14, 100).await?;
    assert_eq!(outcome.cleared_rows, 0);
    assert_eq!(outcome.batches, 0, "an empty first batch must not loop");

    for request_id in &young.request_ids {
        assert!(body_of(&handle.pool, *request_id).await?.is_some());
    }

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn compaction_refuses_a_row_without_an_envelope() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let seeded = seed_session(&handle.pool, 30, &[true, false, true]).await?;

    let survey = handle.survey_request_body_compaction(14).await?;
    assert_eq!(survey.candidate_rows, 1);
    assert_eq!(survey.refused_no_envelope_rows, 1);
    assert!(
        survey.refused_no_envelope_bytes > 0,
        "the refusal must report the bytes it is leaving on disk"
    );

    let outcome = handle.compact_request_bodies(14, 100).await?;
    assert_eq!(outcome.cleared_rows, 1);

    assert!(
        body_of(&handle.pool, seeded.request_ids[0])
            .await?
            .is_none()
    );
    assert!(
        body_of(&handle.pool, seeded.request_ids[1])
            .await?
            .is_some(),
        "an envelope-less row must never be cleared"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn compaction_rerun_is_a_noop() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    seed_session(&handle.pool, 30, &[true, true, true]).await?;

    let first = handle.compact_request_bodies(14, 1).await?;
    assert_eq!(first.cleared_rows, 2);
    assert_eq!(first.batches, 2, "a batch size of 1 must commit per row");

    let second = handle.compact_request_bodies(14, 1).await?;
    assert_eq!(second.cleared_rows, 0);
    assert_eq!(second.reclaimed_bytes, 0);
    assert_eq!(second.batches, 0);

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn compaction_leaves_delta_rows_untouched() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let seeded = seed_session(&handle.pool, 30, &[true, true]).await?;
    let compacted = seeded.request_ids[0];

    let message_id: i64 = sqlx::query_scalar(
        "INSERT INTO messages.messages
            (message_sha256, body, byte_size, role_id, first_seen, last_seen)
         SELECT $1, '{\"role\":\"user\"}'::jsonb, 16, r.id, now(), now()
           FROM messages.roles r WHERE r.code = 'user'
         RETURNING id",
    )
    .bind(format!("{:064x}", 1))
    .fetch_one(&handle.pool)
    .await?;

    #[rustfmt::skip]
    sqlx::query(
        "INSERT INTO requests.deltas
            (request_id, session_id, message_id, change_kind_id, ord,
             computed_at, sent_patch_start, sent_patch_end, sent_patch_bytes)
         SELECT $1, $2, $3, ck.id, 0, now(), 0, 4, $4
           FROM requests.delta_change_kinds ck WHERE ck.code = 'added'",
    )
    .bind(compacted)
    .bind(seeded.session_id)
    .bind(message_id)
    .bind(b"patc".as_slice())
    .execute(&handle.pool)
    .await?;

    let outcome = handle.compact_request_bodies(14, 100).await?;
    assert_eq!(outcome.cleared_rows, 1);
    assert!(body_of(&handle.pool, compacted).await?.is_none());

    let surviving: Vec<(i32, Option<Vec<u8>>)> =
        sqlx::query_as("SELECT ord, sent_patch_bytes FROM requests.deltas WHERE request_id = $1")
            .bind(compacted)
            .fetch_all(&handle.pool)
            .await?;
    assert_eq!(surviving.len(), 1, "the delta row must survive compaction");
    let patch = surviving[0].1.clone();
    assert_eq!(
        patch,
        Some(b"patc".to_vec()),
        "the sent-patch bytes must survive compaction"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn survey_writes_nothing() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let seeded = seed_session(&handle.pool, 30, &[true, true, true]).await?;

    let first = handle.survey_request_body_compaction(14).await?;
    let second = handle.survey_request_body_compaction(14).await?;
    assert_eq!(first, second, "a survey must be repeatable");
    assert_eq!(first.candidate_rows, 2);

    for request_id in &seeded.request_ids {
        assert!(
            body_of(&handle.pool, *request_id).await?.is_some(),
            "surveying must not clear anything"
        );
    }

    db.cleanup().await?;
    Ok(())
}
