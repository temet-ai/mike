use crate::test_db;
use mike::store::persistence::knowledge_repo::KnowledgePersist as _;

use super::fixtures::{make_pool_and_persist, seed_project, seed_session};
use chrono::{Duration as ChronoDuration, SubsecRound, Utc};
use test_db::TestDb;

#[tokio::test]
async fn lessons_from_sessions_returns_full_text_active_only() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping lessons_from_sessions_returns_full_text_active_only"
        );
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let (pool, persist) = make_pool_and_persist(&test_db.database_url).await?;

    let project_id = seed_project(
        &pool,
        &format!("lessons-from-sessions-test-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let now = Utc::now().trunc_subsecs(6);
    let (session_pk, session_uuid) = seed_session(&pool, project_id, None, now).await?;
    let (other_pk, other_uuid) = seed_session(&pool, project_id, None, now).await?;

    let long_text = "A".repeat(5000) + " — the full, untruncated lesson body.";

    let active_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lessons
            (project_id, session_id, kind_id, text, stage_id)
         VALUES ($1, $2,
                 (SELECT id FROM memory.lesson_kinds WHERE code = 'correction'),
                 $3,
                 (SELECT id FROM memory.lesson_stages WHERE code = 'long_term'))
         RETURNING id",
    )
    .bind(project_id)
    .bind(session_pk)
    .bind(&long_text)
    .fetch_one(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO memory.lessons
            (project_id, session_id, kind_id, text, stage_id, archived_at, archived_reason_id)
         VALUES ($1, $2,
                 (SELECT id FROM memory.lesson_kinds WHERE code = 'warning'),
                 'archived lesson',
                 (SELECT id FROM memory.lesson_stages WHERE code = 'archived'),
                 now(),
                 (SELECT id FROM memory.lesson_archived_reasons WHERE code = 'operator'))",
    )
    .bind(project_id)
    .bind(session_pk)
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO memory.lessons
            (project_id, session_id, kind_id, text, stage_id, expires_at)
         VALUES ($1, $2,
                 (SELECT id FROM memory.lesson_kinds WHERE code = 'preference'),
                 'expired lesson',
                 (SELECT id FROM memory.lesson_stages WHERE code = 'daily'),
                 $3)",
    )
    .bind(project_id)
    .bind(session_pk)
    .bind(now - ChronoDuration::days(1))
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO memory.lessons
            (project_id, session_id, kind_id, text, stage_id)
         VALUES ($1, $2,
                 (SELECT id FROM memory.lesson_kinds WHERE code = 'convention'),
                 'other session lesson',
                 (SELECT id FROM memory.lesson_stages WHERE code = 'long_term'))",
    )
    .bind(project_id)
    .bind(other_pk)
    .execute(&pool)
    .await?;
    let knowledge_db = persist.knowledge();

    let rows = knowledge_db.lessons_from_sessions(&[session_uuid]).await?;
    assert_eq!(
        rows.len(),
        1,
        "only the one active, non-expired lesson on session_uuid must return"
    );
    let row = &rows[0];
    assert_eq!(row.id, active_id);
    assert_eq!(row.session_id, session_uuid);
    assert_eq!(row.kind, "correction");
    assert_eq!(row.stage, "long_term");
    assert_eq!(
        row.text, long_text,
        "text must be returned in full, never truncated"
    );

    let other_only = knowledge_db.lessons_from_sessions(&[other_uuid]).await?;
    assert_eq!(other_only.len(), 1);
    assert_eq!(other_only[0].text, "other session lesson");

    let (_none_pk, none_uuid) = seed_session(&pool, project_id, None, now).await?;
    let none = knowledge_db.lessons_from_sessions(&[none_uuid]).await?;
    assert!(
        none.is_empty(),
        "a session with no lessons must return an empty result, not an error"
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
