use crate::test_db;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

struct MultiOpenTurnFixture {
    session_pk: i64,
    turn_ids: Vec<i64>,
}

async fn seed_multi_open_turn_fixture(pool: &PgPool) -> anyhow::Result<MultiOpenTurnFixture> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let project_path = format!("/tmp/ord-{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now();

    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects
            (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         RETURNING id",
    )
    .bind(&project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;

    let session_pk: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)
         RETURNING id",
    )
    .bind(&session_id)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;

    let request_id_base: i64 = 8_800_001;
    for i in 0..3_i64 {
        sqlx::query(
            "INSERT INTO requests.requests
                (id, session_id, project_id, method_id, path, is_streaming,
                 message_count, is_tool_loop, status_id, created_at,
                 params,
                 last_user_message, human_prompt, turn_type_id, turn_origin_id)
             OVERRIDING SYSTEM VALUE
             SELECT $1, $2, $3, rm.id, '/v1/messages', FALSE,
                    1, FALSE, rs.id, $4,
                    '{}'::jsonb,
                    'hi', 'hi', rtt.id, rto.id
             FROM requests.methods rm,
                  requests.statuses rs,
                  requests.turn_types rtt,
                  requests.turn_origins rto
             WHERE rm.code = 'POST'
               AND rs.code = 'complete'
               AND rtt.code = 'user_prompt'
               AND rto.code = 'primary'",
        )
        .bind(request_id_base + i)
        .bind(session_pk)
        .bind(project_id)
        .bind(now + chrono::Duration::seconds(i))
        .execute(pool)
        .await?;
    }

    let mut turn_ids: Vec<i64> = Vec::with_capacity(3);
    for i in 0..3_i64 {
        let req_id = request_id_base + i;
        let turn_id: i64 = sqlx::query_scalar(
            "INSERT INTO memory.operator_turns
                (session_id, project_id, start_request_id, end_request_id)
             VALUES ($1, $2, $3, $3)
             RETURNING id",
        )
        .bind(session_pk)
        .bind(project_id)
        .bind(req_id)
        .fetch_one(pool)
        .await?;
        turn_ids.push(turn_id);
    }

    Ok(MultiOpenTurnFixture {
        session_pk,
        turn_ids,
    })
}

#[tokio::test]
async fn pre_index_close_handles_multiple_open_turns_per_session() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping migration ordering smoke");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let fixture = seed_multi_open_turn_fixture(&pool).await?;

    let open_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM memory.operator_turns
          WHERE session_id = $1 AND closed_at IS NULL",
    )
    .bind(fixture.session_pk)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        open_count, 3,
        "fixture must seed 3 OPEN turns for one session (pathology shape)"
    );

    let pre_close_index_attempt = sqlx::query(
        "CREATE UNIQUE INDEX operator_turns_one_open_per_session
            ON memory.operator_turns (session_id) WHERE closed_at IS NULL",
    )
    .execute(&pool)
    .await;
    assert!(
        pre_close_index_attempt.is_err(),
        "pre-fix: creating the partial unique index against multi-OPEN \
         pathology data MUST fail; otherwise the regression test is not \
         exercising the bug"
    );

    sqlx::query(
        "WITH trailing_turns AS (
            SELECT DISTINCT ON (session_id) session_id, id
            FROM memory.operator_turns
            ORDER BY session_id, end_request_id DESC
        )
        UPDATE memory.operator_turns ot
        SET closed_at = COALESCE(ot.classified_at, ot.created_at)
        WHERE ot.id NOT IN (SELECT id FROM trailing_turns)",
    )
    .execute(&pool)
    .await?;

    let post_close_open_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM memory.operator_turns
          WHERE session_id = $1 AND closed_at IS NULL",
    )
    .bind(fixture.session_pk)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        post_close_open_count, 1,
        "post-close: exactly one OPEN turn must remain per session \
         (the trailing one)"
    );

    let trailing_open_turn_id: i64 = sqlx::query_scalar(
        "SELECT id
           FROM memory.operator_turns
          WHERE session_id = $1 AND closed_at IS NULL",
    )
    .bind(fixture.session_pk)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        trailing_open_turn_id, fixture.turn_ids[2],
        "the trailing OPEN turn must be the one with the largest \
         end_request_id"
    );

    sqlx::query(
        "CREATE UNIQUE INDEX operator_turns_one_open_per_session
            ON memory.operator_turns (session_id) WHERE closed_at IS NULL",
    )
    .execute(&pool)
    .await?;

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
