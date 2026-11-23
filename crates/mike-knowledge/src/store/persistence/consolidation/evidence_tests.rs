use super::test_fixture::{fixture, vector};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;

#[tokio::test]
async fn nearest_evidence_chunks_returns_evidence_kinds_of_the_project_nearest_first() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let (mine, theirs) = (fx.project().await?, fx.project().await?);
    let close = fx.evidence_item("correction", mine, "correction:900001").await?;
    let wide = fx.evidence_item("mail", mine, "mail:wide").await?;
    let other_project = fx.evidence_item("note", theirs, "note:foreign").await?;
    let not_evidence = fx.lesson(mine, "A lesson is not evidence.").await?;
    fx.chunk(&close, "Dawid said: use jj.", &vector(&[(40, 1.0)])).await?;
    fx.chunk(&wide, "Mail about something else.", &vector(&[(40, 0.2), (41, 1.0)])).await?;
    fx.chunk(&other_project, "Foreign note.", &vector(&[(40, 1.0)])).await?;
    fx.chunk(&not_evidence, "Lesson chunk.", &vector(&[(40, 1.0)])).await?;

    let found = fx
        .handle
        .knowledge()
        .nearest_evidence_chunks(mine, &vector(&[(40, 1.0)]), 20)
        .await?;
    let refs: Vec<&str> = found.iter().map(|c| c.reference.as_str()).collect();
    assert_eq!(refs, vec!["correction:900001", "mail:wide"]);
    assert_eq!(found[0].kind, "correction");
    assert_eq!(found[0].body, "Dawid said: use jj.");
    assert!(found[0].similarity > 0.99 && found[0].similarity > found[1].similarity);
    let changed_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT changed_at FROM memory.knowledge_items WHERE id = $1")
            .bind(close.item_id)
            .fetch_one(fx.handle.pool())
            .await?;
    assert_eq!(found[0].dated_at, changed_at);

    let one = fx
        .handle
        .knowledge()
        .nearest_evidence_chunks(mine, &vector(&[(40, 1.0)]), 1)
        .await?;
    assert_eq!(one.len(), 1);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn nearest_evidence_chunks_dates_a_thread_entry_by_its_log_time() -> anyhow::Result<()> {
    let Some(fx) = fixture(false).await? else {
        return Ok(());
    };
    let project = fx.project().await?;
    let thread_id: i64 = sqlx::query_scalar(
        "INSERT INTO work.threads (project_id, slug, title) VALUES ($1, $2, 'Keep jj') RETURNING id",
    )
    .bind(project)
    .bind(format!("thread-{}", uuid::Uuid::new_v4()))
    .fetch_one(fx.handle.pool())
    .await?;
    let (log_id, logged_at): (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO work.thread_log (thread_id, ord, summary, created_at)
         VALUES ($1, 1, 'Decided to keep jj.', now() - interval '3 days')
         RETURNING id, created_at",
    )
    .bind(thread_id)
    .fetch_one(fx.handle.pool())
    .await?;
    let entry = fx.carrier("thread_entry", log_id, &format!("thread_entry:{log_id}")).await?;
    fx.chunk(&entry, "Decided to keep jj.", &vector(&[(42, 1.0)])).await?;

    let found = fx
        .handle
        .knowledge()
        .nearest_evidence_chunks(project, &vector(&[(42, 1.0)]), 5)
        .await?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].dated_at, logged_at);
    fx.db.cleanup().await?;
    Ok(())
}
