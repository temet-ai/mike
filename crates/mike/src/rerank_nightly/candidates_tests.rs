use std::sync::atomic::AtomicUsize;

use sqlx::PgPool;
use uuid::Uuid;

use super::worker_tests::{AlwaysIdle, IdleOnce, at};
use super::{Stop, night_pass, quiet_budget};
use crate::api::rerank_search::harness::{Harness, failing_answer};
use crate::store::persistence::work_threads::{
    CheckpointInput, MetaUpdate, NewThreadCandidate, StateFields,
};

async fn project(harness: &Harness) -> anyhow::Result<i64> {
    Ok(harness
        .handle
        .define_knowledge_project("candidates", Uuid::new_v4())
        .await?
        .id)
}

async fn thread(harness: &Harness, project: i64, slug: &str, tldr: &str) -> anyhow::Result<i64> {
    let outcome = harness
        .handle
        .checkpoint_work_thread(
            project,
            &CheckpointInput {
                slug,
                tldr,
                state: StateFields {
                    where_we_are: "reranker cache tuning in progress",
                    ..StateFields::default()
                },
                session_uuid: None,
                commit_sha: None,
                contribution: None,
                meta: MetaUpdate {
                    title: Some("Reranker cache tuning"),
                    ..MetaUpdate::default()
                },
            },
        )
        .await?;
    Ok(outcome.thread.id)
}

async fn lessons(pool: &PgPool, project: i64, count: usize) -> anyhow::Result<()> {
    for n in 0..count {
        sqlx::query(
            "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, topic)
             SELECT $1, lk.id, $2, ls.id, lsrc.id, 'cache'
               FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
              WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'operator'",
        )
        .bind(project)
        .bind(format!("Reranker cache tuning lesson number {n}"))
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn rows(pool: &PgPool, thread_id: i64) -> anyhow::Result<Vec<(i32, String, String, f32)>> {
    Ok(sqlx::query_as(
        "SELECT rank, ref, kind, score FROM work.thread_card_candidates
          WHERE thread_id = $1 ORDER BY rank",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?)
}

async fn sources_generation(pool: &PgPool) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT generation FROM memory.change_generations WHERE name = 'thread_card_sources'",
    )
    .fetch_one(pool)
    .await?)
}

#[tokio::test]
async fn night_pass_with_many_lessons_ranks_ten_for_the_thread_and_replaces_them_next_night()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness.reranker_prefers_later_documents(None).await;
    let project = project(&harness).await?;
    let thread_id = thread(&harness, project, "cache-tuning", "tune the reranker cache").await?;
    lessons(harness.handle.pool(), project, 12).await?;
    let budget = quiet_budget(&harness.state);
    let pool = harness.handle.pool();
    let before = sources_generation(pool).await?;

    let first = night_pass(
        &harness.state,
        &harness.handle,
        &budget,
        at(23),
        &AlwaysIdle,
    )
    .await;
    let first_rows = rows(pool, thread_id).await?;
    sqlx::query(
        "INSERT INTO work.thread_card_candidates (thread_id, rank, ref, kind, score)
         VALUES ($1, 11, 'lesson:stale', 'lesson', 0.1)",
    )
    .bind(thread_id)
    .execute(pool)
    .await?;
    let second = night_pass(
        &harness.state,
        &harness.handle,
        &budget,
        at(23),
        &AlwaysIdle,
    )
    .await;
    let second_rows = rows(pool, thread_id).await?;

    assert_eq!(first.stopped, None);
    assert_eq!(second.stopped, None);
    assert_eq!(first_rows.len(), 10);
    let ranks: Vec<i32> = first_rows.iter().map(|r| r.0).collect();
    assert_eq!(ranks, (1..=10).collect::<Vec<_>>());
    assert!(
        first_rows
            .iter()
            .all(|r| r.2 == "lesson" && r.1.starts_with("lesson:"))
    );
    assert!(first_rows.windows(2).all(|w| w[0].3 >= w[1].3));
    assert_eq!(second_rows.len(), 10);
    assert!(second_rows.iter().all(|r| r.1 != "lesson:stale"));
    assert!(sources_generation(pool).await? > before);
    harness.cleanup().await
}

#[tokio::test]
async fn night_pass_keeps_other_threads_entries_and_drops_the_threads_own() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness.reranker_prefers_later_documents(None).await;
    let project = project(&harness).await?;
    let own = thread(&harness, project, "cache-tuning", "tune the reranker cache").await?;
    let other = thread(
        &harness,
        project,
        "cache-notes",
        "reranker cache tuning notes",
    )
    .await?;
    let budget = quiet_budget(&harness.state);
    let pool = harness.handle.pool();

    night_pass(
        &harness.state,
        &harness.handle,
        &budget,
        at(23),
        &AlwaysIdle,
    )
    .await;

    let refs: Vec<String> = rows(pool, own).await?.into_iter().map(|r| r.1).collect();
    let own_entries: Vec<String> =
        sqlx::query_scalar("SELECT 'entry:' || id FROM work.thread_log WHERE thread_id = $1")
            .bind(own)
            .fetch_all(pool)
            .await?;
    let other_entries: Vec<String> =
        sqlx::query_scalar("SELECT 'entry:' || id FROM work.thread_log WHERE thread_id = $1")
            .bind(other)
            .fetch_all(pool)
            .await?;
    assert!(refs.iter().any(|r| other_entries.contains(r)), "{refs:?}");
    assert!(refs.iter().all(|r| !own_entries.contains(r)), "{refs:?}");
    harness.cleanup().await
}

#[tokio::test]
async fn night_pass_when_the_reranker_fails_leaves_the_threads_old_candidates() -> anyhow::Result<()>
{
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness.reranker_answers(failing_answer()).await;
    let project = project(&harness).await?;
    let thread_id = thread(&harness, project, "cache-tuning", "tune the reranker cache").await?;
    lessons(harness.handle.pool(), project, 3).await?;
    harness
        .handle
        .replace_thread_card_candidates(
            thread_id,
            &[NewThreadCandidate {
                reference: "lesson:old".to_owned(),
                kind: "lesson".to_owned(),
                score: 0.9,
            }],
        )
        .await?;
    let budget = quiet_budget(&harness.state);

    let pass = night_pass(
        &harness.state,
        &harness.handle,
        &budget,
        at(23),
        &AlwaysIdle,
    )
    .await;

    assert_eq!(pass.stopped, Some(Stop::RerankerFailed));
    let kept = rows(harness.handle.pool(), thread_id).await?;
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].1, "lesson:old");
    harness.cleanup().await
}

#[tokio::test]
async fn night_pass_when_a_session_becomes_active_stops_before_the_next_thread()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness.reranker_prefers_later_documents(None).await;
    let project = project(&harness).await?;
    let first = thread(&harness, project, "cache-a", "tune the reranker cache").await?;
    let second = thread(&harness, project, "cache-b", "tune the reranker cache").await?;
    lessons(harness.handle.pool(), project, 3).await?;
    let budget = quiet_budget(&harness.state);
    let idle = IdleOnce(AtomicUsize::new(0));

    let pass = night_pass(&harness.state, &harness.handle, &budget, at(23), &idle).await;

    let pool = harness.handle.pool();
    assert_eq!(pass.stopped, Some(Stop::SessionActive));
    assert_eq!(pass.thread_candidates, 1);
    assert!(!rows(pool, first).await?.is_empty());
    assert!(rows(pool, second).await?.is_empty());
    harness.cleanup().await
}
