use crate::seed;
use crate::test_db;

use chrono::{DateTime, TimeZone, Utc};
use seed::seed_project;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

fn sess_uuid(label: &str) -> String {
    match label {
        "sess_alpha" => "a1000000-0000-4000-8000-000000000001",
        "sess_beta" => "a1000000-0000-4000-8000-000000000002",
        "sess_gamma" => "a1000000-0000-4000-8000-000000000003",
        "sess_delta" => "a1000000-0000-4000-8000-000000000004",
        "sess_d2" => "d2000000-0000-4000-8000-000000000001",
        other => panic!("unknown session label in fixture: {other}"),
    }
    .to_string()
}

async fn seed_request(
    pool: &PgPool,
    project_id: i64,
    id: i64,
    session_id: &str,
    turn_origin: &str,
    created_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)
         ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(created_at)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO requests.requests (
            id, session_id, project_id, method_id, path, is_streaming,
            message_count, is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id,
            turn_origin_id, created_at
         )
         OVERRIDING SYSTEM VALUE
         SELECT $1,
                (SELECT id FROM sessions.sessions WHERE session_uuid = $2::text::uuid),
                $3, rm.id, '/v1/messages', false,
                0, false, rs.id, '{}'::jsonb,
                '', '', rtt.id,
                rto.id, $4
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = $5",
    )
    .bind(id)
    .bind(session_id)
    .bind(project_id)
    .bind(created_at)
    .bind(turn_origin)
    .execute(pool)
    .await?;
    Ok(())
}

fn fixture_rows() -> Vec<(i64, String, &'static str, DateTime<Utc>)> {
    vec![
        (
            1001,
            sess_uuid("sess_alpha"),
            "primary",
            Utc.with_ymd_and_hms(2026, 5, 10, 10, 0, 0).unwrap(),
        ),
        (
            1002,
            sess_uuid("sess_alpha"),
            "primary",
            Utc.with_ymd_and_hms(2026, 5, 10, 10, 0, 1).unwrap(),
        ),
        (
            1003,
            sess_uuid("sess_alpha"),
            "primary",
            Utc.with_ymd_and_hms(2026, 5, 10, 10, 0, 2).unwrap(),
        ),
        (
            1004,
            sess_uuid("sess_alpha"),
            "title_sidecar",
            Utc.with_ymd_and_hms(2026, 5, 10, 10, 0, 0).unwrap()
                + chrono::Duration::milliseconds(500),
        ),
        (
            2001,
            sess_uuid("sess_beta"),
            "primary",
            Utc.with_ymd_and_hms(2026, 5, 10, 11, 0, 0).unwrap(),
        ),
        (
            3001,
            sess_uuid("sess_gamma"),
            "subagent",
            Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap(),
        ),
        (
            4001,
            sess_uuid("sess_delta"),
            "primary",
            Utc.with_ymd_and_hms(2026, 5, 10, 13, 0, 0).unwrap(),
        ),
        (
            4002,
            sess_uuid("sess_delta"),
            "primary",
            Utc.with_ymd_and_hms(2026, 5, 10, 13, 0, 1).unwrap(),
        ),
    ]
}

fn expected_predecessor(row_id: i64) -> Option<i64> {
    match row_id {
        1001 => None,
        1002 => Some(1001),
        1003 => Some(1002),
        1004 => None,
        2001 => None,
        3001 => None,
        4001 => None,
        4002 => Some(4001),
        other => panic!("unexpected fixture row id {other}"),
    }
}

async fn predecessor_via_sql(
    pool: &PgPool,
    session_id: &str,
    created_at: DateTime<Utc>,
) -> anyhow::Result<Option<i64>> {
    let row: Option<i64> = sqlx::query_scalar(
        "SELECT r.id
         FROM   requests.requests r
         JOIN   requests.turn_origins tor ON tor.id = r.turn_origin_id AND tor.code = 'primary'
         WHERE  r.session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid)
           AND  r.created_at < $2
         ORDER  BY r.created_at DESC, r.id DESC
         LIMIT  1",
    )
    .bind(session_id)
    .bind(created_at)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

#[tokio::test]
async fn predecessor_request_id_resolves_per_db_fixture() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let project_id = seed_project(&pool, "test-project-w2.4-fixture").await?;
    for (id, session_id, turn_origin, created_at) in fixture_rows() {
        seed_request(&pool, project_id, id, &session_id, turn_origin, created_at).await?;
    }

    for (row_id, session_id, turn_origin, created_at) in fixture_rows() {
        let actual = if turn_origin == "primary" {
            predecessor_via_sql(&pool, &session_id, created_at).await?
        } else {
            None
        };
        let expected = expected_predecessor(row_id);
        assert_eq!(
            actual, expected,
            "row {row_id} ({turn_origin} on {session_id}): expected {expected:?}, got {actual:?}",
        );
    }

    test_db.cleanup().await?;
    Ok(())
}

async fn predecessor_via_corrected_sql(
    pool: &PgPool,
    session_id: &str,
    created_at: DateTime<Utc>,
) -> anyhow::Result<Option<i64>> {
    let row: Option<i64> = sqlx::query_scalar(
        "SELECT r.id
         FROM   requests.requests r
         JOIN   requests.turn_origins tor ON tor.id = r.turn_origin_id AND tor.code IN ('primary', 'primary_first')
         WHERE  r.session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid)
           AND  r.created_at < $2
         ORDER  BY r.created_at DESC, r.id DESC
         LIMIT  1",
    )
    .bind(session_id)
    .bind(created_at)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

#[tokio::test]
async fn predecessor_resolves_across_primary_first_boundary() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let project_id = seed_project(&pool, "test-project-d2-primary-first").await?;

    let t0 = Utc.with_ymd_and_hms(2026, 5, 18, 9, 0, 0).unwrap();
    let t1 = t0 + chrono::Duration::seconds(1);
    let t2 = t0 + chrono::Duration::seconds(2);
    let sess_d2 = sess_uuid("sess_d2");
    seed_request(&pool, project_id, 24443, &sess_d2, "primary_first", t0).await?;
    seed_request(&pool, project_id, 24448, &sess_d2, "subagent", t1).await?;
    seed_request(&pool, project_id, 24449, &sess_d2, "primary_first", t2).await?;

    let pred_for_24449 = predecessor_via_corrected_sql(&pool, &sess_d2, t2).await?;
    assert_eq!(
        pred_for_24449,
        Some(24443),
        "24449 (primary_first) must resolve 24443 (primary_first) as predecessor — D2"
    );

    let pred_for_24448 = predecessor_via_corrected_sql(&pool, &sess_d2, t1).await?;
    assert_eq!(
        pred_for_24448,
        Some(24443),
        "24448 (mislabelled-subagent compaction) must resolve 24443 — D1+D2"
    );

    test_db.cleanup().await?;
    Ok(())
}
