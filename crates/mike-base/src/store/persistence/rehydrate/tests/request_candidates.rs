use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::messages::{canonicalise_body_messages, write_request_messages};
use crate::store::persistence::reconstruct::strip_bulk_arrays;
use crate::store::persistence::rehydrate::fetch_request_bodies_batch;
use crate::store::persistence::rehydrate::text_search::{
    BodySource, FindTextQuery, find_text_candidates,
};
use crate::store::persistence::test_support::{TestDb, seed::seed_project};

async fn seed_session(pool: &PgPool, project_id: i64, at: DateTime<Utc>) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES (gen_random_uuid(), $1, $2, $2)
         RETURNING id",
    )
    .bind(project_id)
    .bind(at)
    .fetch_one(pool)
    .await?)
}

async fn seed_request(
    pool: &PgPool,
    session_id: i64,
    project_id: i64,
    body: &serde_json::Value,
    created_at: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let wire = serde_json::to_vec(body)?;
    let envelope = strip_bulk_arrays(&wire);
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id,
             params, last_user_message, human_prompt, turn_type_id,
             turn_origin_id, created_at, request_envelope
         )
         SELECT $1, $2, rm.id, '/v1/messages', true,
                1, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id, $3, $4
           FROM requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'
          RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(created_at)
    .bind(envelope)
    .fetch_one(pool)
    .await?;
    let canonical = canonicalise_body_messages(&wire)?;
    let mut tx = pool.begin().await?;
    write_request_messages(&mut tx, request_id, &canonical, &HashMap::new()).await?;
    tx.commit().await?;
    Ok(request_id)
}

fn user_says(text: &str) -> serde_json::Value {
    serde_json::json!({
        "model": "m",
        "system": [{"type": "text", "text": "plain system"}],
        "messages": [{"role": "user", "content": text}],
    })
}

async fn candidates(
    pool: &PgPool,
    needle: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> anyhow::Result<Vec<i64>> {
    let rows = find_text_candidates(
        pool,
        needle.as_bytes(),
        FindTextQuery {
            source: BodySource::Request,
            from,
            to,
            candidate_cap: 200,
        },
    )
    .await?;
    Ok(rows.into_iter().map(|row| row.request_id).collect())
}

#[tokio::test]
async fn request_candidates_message_holds_needle_found_without_request_body() -> anyhow::Result<()>
{
    let Some(db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = db.acquire().await?;
    let persist = PersistHandle::connect(&handle.database_url).await?;
    let pool = persist.pool();
    let needle = format!("needle-{}", uuid::Uuid::new_v4());
    let project_id = seed_project(pool, &format!("/tmp/{needle}")).await?;
    let base = Utc::now() - Duration::days(30);

    let hit_session = seed_session(pool, project_id, base).await?;
    let hit = seed_request(
        pool,
        hit_session,
        project_id,
        &user_says(&format!("say \"{needle}\" please")),
        base,
    )
    .await?;
    let miss_session = seed_session(pool, project_id, base).await?;
    let miss = seed_request(
        pool,
        miss_session,
        project_id,
        &user_says("nothing here"),
        base,
    )
    .await?;
    let superseded_session = seed_session(pool, project_id, base).await?;
    let old = seed_request(
        pool,
        superseded_session,
        project_id,
        &user_says(&needle),
        base,
    )
    .await?;
    seed_request(
        pool,
        superseded_session,
        project_id,
        &user_says("later turn without it"),
        base + Duration::minutes(5),
    )
    .await?;

    let raw_bodies: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM requests.requests WHERE project_id = $1 AND request_body IS NOT NULL",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(raw_bodies, 0, "only the message tables hold the content");

    let found = candidates(
        pool,
        &needle,
        base - Duration::hours(1),
        base + Duration::hours(1),
    )
    .await?;
    assert_eq!(found, vec![hit]);
    assert!(!found.contains(&miss) && !found.contains(&old));
    handle.cleanup().await
}

#[tokio::test]
async fn request_candidates_system_prompt_holds_needle_found_from_envelope() -> anyhow::Result<()> {
    let Some(db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = db.acquire().await?;
    let persist = PersistHandle::connect(&handle.database_url).await?;
    let pool = persist.pool();
    let needle = format!("sysneedle-{}", uuid::Uuid::new_v4());
    let project_id = seed_project(pool, &format!("/tmp/{needle}")).await?;
    let base = Utc::now() - Duration::days(30);
    let session = seed_session(pool, project_id, base).await?;
    let body = serde_json::json!({
        "model": "m",
        "system": [{"type": "text", "text": needle}],
        "messages": [{"role": "user", "content": "hi"}],
    });
    let id = seed_request(pool, session, project_id, &body, base).await?;
    let found = candidates(
        pool,
        &needle,
        base - Duration::hours(1),
        base + Duration::hours(1),
    )
    .await?;
    assert_eq!(found, vec![id]);
    handle.cleanup().await
}

#[tokio::test]
async fn request_bodies_batch_rebuilds_bodies_from_envelope_and_messages() -> anyhow::Result<()> {
    let Some(db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = db.acquire().await?;
    let persist = PersistHandle::connect(&handle.database_url).await?;
    let pool = persist.pool();
    let project_id = seed_project(pool, &format!("/tmp/batch-{}", uuid::Uuid::new_v4())).await?;
    let base = Utc::now();
    let session = seed_session(pool, project_id, base).await?;
    let body = user_says("rebuild me");
    let id = seed_request(pool, session, project_id, &body, base).await?;

    let bodies = fetch_request_bodies_batch(pool, &[id, i64::MAX]).await?;
    assert_eq!(bodies.len(), 1);
    let rebuilt: serde_json::Value = serde_json::from_slice(&bodies[&id])?;
    assert_eq!(rebuilt, body);
    handle.cleanup().await
}
