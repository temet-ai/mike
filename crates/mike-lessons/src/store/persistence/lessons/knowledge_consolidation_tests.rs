use crate::store::persistence::lessons_repo::LessonsPersist as _;
use crate::store::persistence::test_support::{TestDb, seed_project};
use crate::store::persistence::PersistHandle;
use uuid::Uuid;

async fn lesson(handle: &PersistHandle, project_id: i64, text: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text)
         SELECT $1, k.id, $2 FROM memory.lesson_kinds k ORDER BY k.id LIMIT 1
         RETURNING id",
    )
    .bind(project_id)
    .bind(text)
    .fetch_one(handle.pool())
    .await?)
}

type Retirement = (Option<String>, Option<String>, Option<i64>);

async fn retirement(handle: &PersistHandle, id: i64) -> anyhow::Result<Retirement> {
    Ok(sqlx::query_as(
        "SELECT retired_kind, retired_reason, retired_by_lesson_id FROM memory.lessons WHERE id = $1",
    )
    .bind(id)
    .fetch_one(handle.pool())
    .await?)
}

async fn handle() -> anyhow::Result<Option<(crate::store::persistence::test_support::TestDbHandle, PersistHandle, i64)>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = seed_project(handle.pool(), &format!("/test/lesson-consolidation-{}", Uuid::new_v4())).await?;
    Ok(Some((db, handle, project)))
}

#[tokio::test]
async fn retire_lesson_consolidated_records_reason_and_replacement_once() -> anyhow::Result<()> {
    let Some((db, handle, project)) = handle().await? else {
        return Ok(());
    };
    let old = lesson(&handle, project, "Use git.").await?;
    let new = lesson(&handle, project, "Use jj, not git.").await?;

    assert!(handle.lessons().retire_lesson_consolidated(old, "Covered by lesson.", Some(new)).await?);
    assert_eq!(
        retirement(&handle, old).await?,
        (Some("consolidated".into()), Some("Covered by lesson.".into()), Some(new))
    );
    assert!(!handle.lessons().retire_lesson_consolidated(old, "Again.", None).await?);
    assert!(!handle.lessons().retire_lesson_consolidated(new, "Self.", Some(new)).await?);
    assert_eq!(retirement(&handle, new).await?, (None, None, None));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn retire_lesson_consolidated_without_replacement_or_for_unknown_lesson() -> anyhow::Result<()> {
    let Some((db, handle, project)) = handle().await? else {
        return Ok(());
    };
    let id = lesson(&handle, project, "Duplicate of a memory file.").await?;
    assert!(handle.lessons().retire_lesson_consolidated(id, "Covered by memory file.", None).await?);
    assert_eq!(retirement(&handle, id).await?.2, None);
    assert!(!handle.lessons().retire_lesson_consolidated(i64::MAX, "None.", None).await?);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn restore_consolidated_lesson_undoes_only_consolidated_retirements() -> anyhow::Result<()> {
    let Some((db, handle, project)) = handle().await? else {
        return Ok(());
    };
    let consolidated = lesson(&handle, project, "A.").await?;
    let by_outcome = lesson(&handle, project, "B.").await?;
    handle.lessons().retire_lesson_consolidated(consolidated, "Covered.", None).await?;
    sqlx::query(
        "UPDATE memory.lessons SET retired_at = now(), retired_kind = 'outcome', retired_reason = 'x' WHERE id = $1",
    )
    .bind(by_outcome)
    .execute(handle.pool())
    .await?;

    assert!(handle.lessons().restore_consolidated_lesson(consolidated).await?);
    assert_eq!(retirement(&handle, consolidated).await?, (None, None, None));
    assert!(!handle.lessons().restore_consolidated_lesson(by_outcome).await?);
    assert_eq!(retirement(&handle, by_outcome).await?.0.as_deref(), Some("outcome"));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn update_lesson_text_bumps_revision_and_refuses_blank_text() -> anyhow::Result<()> {
    let Some((db, handle, project)) = handle().await? else {
        return Ok(());
    };
    let id = lesson(&handle, project, "Run tests.").await?;
    let before: i32 = sqlx::query_scalar("SELECT revision FROM memory.lessons WHERE id = $1")
        .bind(id)
        .fetch_one(handle.pool())
        .await?;

    let revision = handle.lessons().update_lesson_text(id, "Run tests before every commit.").await?;
    assert_eq!(revision, Some(before + 1));
    let text: String = sqlx::query_scalar("SELECT text FROM memory.lessons WHERE id = $1")
        .bind(id)
        .fetch_one(handle.pool())
        .await?;
    assert_eq!(text, "Run tests before every commit.");

    assert_eq!(handle.lessons().update_lesson_text(id, "   ").await?, None);
    assert_eq!(handle.lessons().update_lesson_text(i64::MAX, "Anything.").await?, None);
    db.cleanup().await?;
    Ok(())
}
