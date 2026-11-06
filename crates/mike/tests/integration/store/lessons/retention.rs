use sqlx::postgres::PgPoolOptions;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::test_support;

use crate::store::support::{seed_daily_lesson_with_embedding, unit_vec};
use crate::test_db::TestDb;

async fn seed_expiring_lesson_with_turn(
    pool: &sqlx::PgPool,
) -> anyhow::Result<(i64, i64, i64, &'static str)> {
    let project_id =
        test_support::seed_project(pool, &format!("gc-audit-survives-{}", uuid::Uuid::new_v4()))
            .await?;
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let other_uuid = uuid::Uuid::new_v4().to_string();
    test_support::seed_two_sessions(pool, &session_uuid, &other_uuid, project_id).await?;
    let session_db_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::uuid")
            .bind(&session_uuid)
            .fetch_one(pool)
            .await?;

    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
                session_id, project_id, method_id, path, is_streaming,
                message_count, is_tool_loop, status_id,
                params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
                created_at, completed_at
             )
             SELECT $1, $2, rm.id, '/v1/messages', false, 0, false, rs.id,
                    '{}'::jsonb, '', '', rtt.id, rto.id, NOW(), NOW()
               FROM requests.methods rm, requests.statuses rs,
                    requests.turn_types rtt, requests.turn_origins rto
              WHERE rm.code = 'POST' AND rs.code = 'complete'
                AND rtt.code = 'user_prompt' AND rto.code = 'primary'
             RETURNING id",
    )
    .bind(session_db_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns
                (session_id, project_id, start_request_id, end_request_id, outcome_id,
                 files_touched, closed_at)
             SELECT $1, $2, $3, $3, tot.id, ARRAY['src/gc_audit.rs'], NOW()
               FROM memory.turn_outcomes tot
              WHERE tot.code = 'unknown'
             RETURNING id",
    )
    .bind(session_db_id)
    .bind(project_id)
    .bind(request_id)
    .fetch_one(pool)
    .await?;

    let lesson_text = "GC-audit survival probe lesson text";
    let lesson_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lessons
            (project_id, kind_id, text, stage_id, source_id, session_id, source_turn_id, expires_at)
         SELECT $1, lk.id, $2, ls.id, lsrc.id, $3, $4, NOW() - INTERVAL '1 hour'
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
          WHERE lk.code = 'convention' AND ls.code = 'daily' AND lsrc.code = 'operator'
         RETURNING id",
    )
    .bind(project_id)
    .bind(lesson_text)
    .bind(session_db_id)
    .bind(turn_id)
    .fetch_one(pool)
    .await?;

    Ok((project_id, session_db_id, lesson_id, lesson_text))
}

#[tokio::test]
async fn gc_expired_lessons_archives_and_never_deletes_the_row() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let (_project_id, session_db_id, lesson_id, lesson_text) =
        seed_expiring_lesson_with_turn(&pool).await?;

    let run_id = handle.start_consolidation_run("gc", "schedule").await?;
    let (archived, deleted) = handle.gc_expired_lessons(run_id).await?;
    assert_eq!(archived, 1, "the expired daily lesson must be archived");
    assert_eq!(deleted, 0, "GC never hard-deletes — the count stays 0");

    let row_count: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.lessons WHERE id = $1")
        .bind(lesson_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(row_count, 1, "the expired lesson row must survive GC");

    let (stage_code, reason_code): (String, Option<String>) = sqlx::query_as(
        "SELECT ls.code, lar.code
           FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
           LEFT JOIN memory.lesson_archived_reasons lar ON lar.id = l.archived_reason_id
          WHERE l.id = $1",
    )
    .bind(lesson_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(stage_code, "archived", "GC must archive, not delete");
    assert_eq!(
        reason_code.as_deref(),
        Some("expired"),
        "the archive reason must record expiry"
    );

    let row: (String, Option<i64>, Option<Vec<String>>) = sqlx::query_as(
        "SELECT a.lesson_text, a.session_id, a.files_touched
           FROM memory.consolidation_actions a
           JOIN memory.consolidation_action_kinds ak ON ak.id = a.action_kind_id
          WHERE a.run_id = $1 AND ak.code = 'archived_expired'",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        row.0, lesson_text,
        "audit row must carry the full lesson text"
    );
    assert_eq!(
        row.1,
        Some(session_db_id),
        "audit row must carry the session id"
    );
    assert_eq!(
        row.2,
        Some(vec!["src/gc_audit.rs".to_string()]),
        "audit row must carry files_touched"
    );

    handle_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn trim_lessons_to_max_records_rank_detail() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let project_id =
        test_support::seed_project(&pool, &format!("trim-rank-detail-{}", uuid::Uuid::new_v4()))
            .await?;

    let older = seed_daily_lesson_with_embedding(
        &pool,
        project_id,
        None,
        "older, gets trimmed",
        &unit_vec(0),
    )
    .await?;
    let newer =
        seed_daily_lesson_with_embedding(&pool, project_id, None, "newer, survives", &unit_vec(1))
            .await?;
    sqlx::query("UPDATE memory.lessons SET created_at = NOW() - INTERVAL '1 hour' WHERE id = $1")
        .bind(older)
        .execute(&pool)
        .await?;

    let run_id = handle.start_consolidation_run("nightly", "manual").await?;
    let trimmed = handle.trim_lessons_to_max(project_id, 1, run_id).await?;
    assert_eq!(trimmed, 1, "exactly one excess lesson must be trimmed");

    let rank_detail: serde_json::Value = sqlx::query_scalar(
        "SELECT rank_detail FROM memory.consolidation_actions
          WHERE run_id = $1 AND lesson_id = $2",
    )
    .bind(run_id)
    .bind(older)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        rank_detail["rank"], 1,
        "the trimmed lesson must record its own rank: {rank_detail}"
    );
    assert_eq!(
        rank_detail["excess_n"], 1,
        "must record the excess count that made it a target: {rank_detail}"
    );
    assert!(rank_detail.get("consolidation_count").is_some());
    assert!(rank_detail.get("created_at").is_some());

    let (newer_stage,): (String,) = sqlx::query_as(
        "SELECT ls.code FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
          WHERE l.id = $1",
    )
    .bind(newer)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        newer_stage, "daily",
        "the surviving lesson must not be trimmed"
    );

    Ok(())
}
