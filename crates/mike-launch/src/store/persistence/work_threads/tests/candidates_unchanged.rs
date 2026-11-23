use crate::store::persistence::launch_repo::LaunchPersist as _;

use super::{fixture, titled};
use crate::store::persistence::work_threads::NewThreadCandidate;

fn candidate(reference: &str, score: f32) -> NewThreadCandidate {
    NewThreadCandidate {
        reference: reference.to_owned(),
        kind: "lesson".to_owned(),
        score,
    }
}

async fn generation(pool: &sqlx::PgPool) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(max(generation), 0) FROM memory.change_generations WHERE name = 'thread_card_sources'",
    )
    .fetch_one(pool)
    .await?)
}

#[tokio::test]
async fn the_same_candidates_again_write_nothing_and_bump_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let thread_id = f
        .handle
        .launch()
        .checkpoint_work_thread(f.project, &titled("same-candidates", "same candidates"))
        .await?
        .thread
        .id;
    let pool = f.handle.pool();
    let first = vec![candidate("lesson:a", 0.9), candidate("lesson:b", 0.5)];
    assert_eq!(
        f.handle
            .launch()
            .replace_thread_card_candidates(thread_id, &first)
            .await?,
        2
    );
    let bumped = generation(pool).await?;

    let rescored = vec![candidate("lesson:a", 0.8), candidate("lesson:b", 0.4)];
    assert_eq!(
        f.handle
            .launch()
            .replace_thread_card_candidates(thread_id, &rescored)
            .await?,
        0,
        "the same references in the same order are left as they are"
    );
    assert_eq!(
        generation(pool).await?,
        bumped,
        "nothing tells the card worker to look again"
    );

    let rows_before: Vec<(i32, String)> = sqlx::query_as(
        "SELECT rank, ref FROM work.thread_card_candidates WHERE thread_id = $1 ORDER BY rank",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    sqlx::query(
        "UPDATE work.thread_card_candidates SET built_at = now() - interval '2 days' WHERE thread_id = $1",
    )
    .bind(thread_id)
    .execute(pool)
    .await?;
    assert_eq!(
        f.handle
            .launch()
            .replace_thread_card_candidates(thread_id, &first)
            .await?,
        0
    );
    let checked_again: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM work.thread_card_candidates
          WHERE thread_id = $1 AND built_at > now() - interval '1 minute'",
    )
    .bind(thread_id)
    .fetch_one(pool)
    .await?;
    let rows_after: Vec<(i32, String)> = sqlx::query_as(
        "SELECT rank, ref FROM work.thread_card_candidates WHERE thread_id = $1 ORDER BY rank",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    assert_eq!(checked_again, 2, "an equal result still records the check");
    assert_eq!(rows_after, rows_before, "and leaves the rows alone");
    assert_eq!(
        generation(pool).await?,
        bumped,
        "without waking the card worker"
    );

    let reordered = vec![candidate("lesson:b", 0.9), candidate("lesson:a", 0.5)];
    assert_eq!(
        f.handle
            .launch()
            .replace_thread_card_candidates(thread_id, &reordered)
            .await?,
        2
    );
    assert!(generation(pool).await? > bumped);
    Ok(())
}
