use chrono::{Duration, Utc};
use sqlx::PgPool;

use super::{Fixture, fixture, titled};
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::work_threads::NewThreadCandidate;

fn candidate(reference: &str) -> NewThreadCandidate {
    NewThreadCandidate {
        reference: reference.to_owned(),
        kind: "lesson".to_owned(),
        score: 0.5,
    }
}

async fn thread(f: &Fixture, slug: &str, tldr: &str) -> anyhow::Result<i64> {
    Ok(f.handle
        .launch()
        .checkpoint_work_thread(f.project, &titled(slug, tldr))
        .await?
        .thread
        .id)
}

async fn lesson(pool: &PgPool, project: i64, text: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, topic)
         SELECT $1, lk.id, $2, ls.id, lsrc.id, 'cache'
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
          WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'operator'",
    )
    .bind(project)
    .bind(text)
    .execute(pool)
    .await?;
    Ok(())
}

async fn built(f: &Fixture, thread_id: i64) -> anyhow::Result<()> {
    f.handle
        .launch()
        .replace_thread_card_candidates(thread_id, &[candidate("lesson:a")])
        .await?;
    Ok(())
}

async fn make_everything_older(pool: &PgPool, project: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.lessons SET created_at = now() - interval '5 days' WHERE project_id = $1",
    )
    .bind(project)
    .execute(pool)
    .await?;
    sqlx::query("UPDATE work.thread_log SET created_at = now() - interval '5 days' WHERE thread_id IN (SELECT id FROM work.threads WHERE project_id = $1)")
        .bind(project)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE work.thread_state SET updated_at = now() - interval '5 days' WHERE thread_id IN (SELECT id FROM work.threads WHERE project_id = $1)")
        .bind(project)
        .execute(pool)
        .await?;
    sqlx::query(
        "UPDATE work.threads SET updated_at = now() - interval '5 days' WHERE project_id = $1",
    )
    .bind(project)
    .execute(pool)
    .await?;
    sqlx::query("UPDATE work.thread_card_candidates SET built_at = now() - interval '2 days' WHERE thread_id IN (SELECT id FROM work.threads WHERE project_id = $1)")
        .bind(project)
        .execute(pool)
        .await?;
    Ok(())
}

async fn due(f: &Fixture) -> anyhow::Result<Vec<i64>> {
    let window_start = Utc::now() - Duration::hours(1);
    Ok(f.handle
        .launch()
        .threads_due_for_card_candidates(window_start)
        .await?
        .into_iter()
        .filter(|t| t.project_id == f.project)
        .map(|t| t.thread_id)
        .collect())
}

#[tokio::test]
async fn due_threads_second_listing_in_the_same_window_is_empty() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let first = thread(&f, "cache-a", "tune the reranker cache").await?;
    let second = thread(&f, "cache-b", "reranker cache notes").await?;
    lesson(f.handle.pool(), f.project, "reranker cache lesson").await?;

    assert_eq!(due(&f).await?, vec![first, second]);
    built(&f, first).await?;
    built(&f, second).await?;

    assert!(due(&f).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn due_threads_without_query_text_are_never_listed() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let blank = thread(&f, "blank", "placeholder").await?;
    let pool = f.handle.pool();
    sqlx::query("UPDATE work.threads SET title = ' ', goal = NULL WHERE id = $1")
        .bind(blank)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE work.thread_state SET tldr = '', where_we_are = '', next_up = '' WHERE thread_id = $1")
        .bind(blank)
        .execute(pool)
        .await?;
    lesson(pool, f.project, "reranker cache lesson").await?;

    assert!(due(&f).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn due_threads_with_no_other_knowledge_in_the_project_are_not_listed() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    thread(&f, "lonely", "tune the reranker cache").await?;

    assert!(
        due(&f).await?.is_empty(),
        "the thread's own checkpoint entry is not knowledge to search"
    );
    Ok(())
}

#[tokio::test]
async fn due_threads_unchanged_thread_without_new_knowledge_is_not_due_the_next_night()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let thread_id = thread(&f, "cache-a", "tune the reranker cache").await?;
    let pool = f.handle.pool();
    lesson(pool, f.project, "reranker cache lesson").await?;
    built(&f, thread_id).await?;
    make_everything_older(pool, f.project).await?;

    assert!(due(&f).await?.is_empty());

    lesson(pool, f.project, "a lesson written after the last check").await?;
    assert_eq!(due(&f).await?, vec![thread_id]);
    Ok(())
}

#[tokio::test]
async fn due_threads_new_knowledge_inside_the_window_waits_for_the_next_window()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let thread_id = thread(&f, "cache-a", "tune the reranker cache").await?;
    let pool = f.handle.pool();
    lesson(pool, f.project, "reranker cache lesson").await?;
    built(&f, thread_id).await?;
    lesson(pool, f.project, "a lesson written after the check").await?;

    assert!(due(&f).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn due_threads_checkpoint_after_the_check_makes_the_thread_due_again() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let thread_id = thread(&f, "cache-a", "tune the reranker cache").await?;
    lesson(f.handle.pool(), f.project, "reranker cache lesson").await?;
    built(&f, thread_id).await?;
    assert!(due(&f).await?.is_empty());

    thread(&f, "cache-a", "now tuning the embedding cache").await?;

    assert_eq!(due(&f).await?, vec![thread_id]);
    Ok(())
}

#[tokio::test]
async fn due_threads_a_check_with_the_same_result_takes_the_thread_off_the_list()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let thread_id = thread(&f, "cache-a", "tune the reranker cache").await?;
    let pool = f.handle.pool();
    lesson(pool, f.project, "reranker cache lesson").await?;
    built(&f, thread_id).await?;
    make_everything_older(pool, f.project).await?;
    lesson(pool, f.project, "a lesson written after the last check").await?;
    assert_eq!(due(&f).await?, vec![thread_id]);

    let written = f
        .handle
        .launch()
        .replace_thread_card_candidates(thread_id, &[candidate("lesson:a")])
        .await?;

    assert_eq!(written, 0);
    assert!(due(&f).await?.is_empty());
    Ok(())
}
