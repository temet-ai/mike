use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::rehydrate::text_search::{
    BodySource, FindTextQuery, find_text_candidates,
};
use crate::store::persistence::test_support::{TestDb, seed::seed_project};

struct Captured {
    created_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
}

async fn seed_response(
    pool: &PgPool,
    project_id: i64,
    body: &[u8],
    at: Captured,
) -> anyhow::Result<i64> {
    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES (gen_random_uuid(), $1, $2, $2)
         RETURNING id",
    )
    .bind(project_id)
    .bind(at.created_at)
    .fetch_one(pool)
    .await?;
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id,
             params, last_user_message, human_prompt, turn_type_id,
             turn_origin_id, created_at, completed_at
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
    .bind(at.created_at)
    .bind(at.completed_at)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO responses.responses (request_id, response_status, response_body, completed_at)
         VALUES ($1, 200, $2, $3)",
    )
    .bind(request_id)
    .bind(body)
    .bind(at.completed_at)
    .execute(pool)
    .await?;
    Ok(request_id)
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
            source: BodySource::Response,
            from,
            to,
            candidate_cap: 200,
        },
    )
    .await?;
    Ok(rows.into_iter().map(|row| row.request_id).collect())
}

#[tokio::test]
async fn response_candidates_request_in_window_found_by_its_created_at() -> anyhow::Result<()> {
    let Some(db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = db.acquire().await?;
    let persist = PersistHandle::connect(&handle.database_url).await?;
    let pool = persist.pool();
    let needle = format!("needle-{}", uuid::Uuid::new_v4());
    let project_id = seed_project(pool, &format!("/tmp/{needle}")).await?;
    let base = Utc::now() - Duration::days(30);
    let inside = seed_response(
        pool,
        project_id,
        format!("data: {needle}").as_bytes(),
        Captured {
            created_at: base,
            completed_at: base + Duration::minutes(12),
        },
    )
    .await?;
    let before = seed_response(
        pool,
        project_id,
        format!("data: {needle}").as_bytes(),
        Captured {
            created_at: base - Duration::hours(2),
            completed_at: base + Duration::minutes(1),
        },
    )
    .await?;
    let other_text = seed_response(
        pool,
        project_id,
        b"data: unrelated",
        Captured {
            created_at: base,
            completed_at: base + Duration::minutes(1),
        },
    )
    .await?;
    let found = candidates(
        pool,
        &needle,
        base - Duration::hours(1),
        base + Duration::minutes(1),
    )
    .await?;
    assert_eq!(found, vec![inside]);
    assert!(!found.contains(&before) && !found.contains(&other_text));
    handle.cleanup().await
}

#[tokio::test]
async fn response_candidates_window_after_request_excludes_it() -> anyhow::Result<()> {
    let Some(db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = db.acquire().await?;
    let persist = PersistHandle::connect(&handle.database_url).await?;
    let pool = persist.pool();
    let needle = format!("needle-{}", uuid::Uuid::new_v4());
    let project_id = seed_project(pool, &format!("/tmp/{needle}")).await?;
    let base = Utc::now() - Duration::days(20);
    seed_response(
        pool,
        project_id,
        format!("data: {needle}").as_bytes(),
        Captured {
            created_at: base,
            completed_at: base + Duration::minutes(5),
        },
    )
    .await?;
    let found = candidates(
        pool,
        &needle,
        base + Duration::minutes(1),
        base + Duration::hours(1),
    )
    .await?;
    assert!(found.is_empty(), "got {found:?}");
    handle.cleanup().await
}
