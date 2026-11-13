use crate::store::persistence::knowledge_repo::KnowledgePersist as _;

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::ResponseTemplate;

use crate::api::cross_crate_tests::rerank_nightly_worker_tests::{AlwaysIdle, at};
use crate::api::rerank_search::harness::{Harness, failing_answer};
use crate::rerank_nightly::{Stop, night_pass, quiet_budget};

const SIMILAR: &str = "Always run the narrow check before committing the change";
const SAME_TOPIC: &str = "Run one narrow check of the change, then commit it";

async fn embedded_pair(harness: &Harness) -> anyhow::Result<(i64, i64)> {
    let project = harness
        .handle
        .knowledge()
        .define_knowledge_project("lesson pairs", Uuid::new_v4())
        .await?
        .id;
    let ids: Vec<i64> = sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, topic)
         SELECT $1, lk.id, t.text, ls.id, lsrc.id, 'checks'
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc,
                (VALUES ($2::text, 1), ($3::text, 2)) AS t(text, ord)
          WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'operator'
          ORDER BY t.ord
      RETURNING id",
    )
    .bind(project)
    .bind(SIMILAR)
    .bind(SAME_TOPIC)
    .fetch_all(harness.handle.pool())
    .await?;
    sqlx::query(
        "UPDATE memory.lessons
            SET embedding = array_fill(0.1::real, ARRAY[(
                    SELECT atttypmod FROM pg_attribute
                     WHERE attrelid = 'memory.lessons'::regclass AND attname = 'embedding')])::vector,
                embedded_at = now(), embedding_model = 'test'
          WHERE id = ANY($1)",
    )
    .bind(&ids)
    .execute(harness.handle.pool())
    .await?;
    let [a, b] = ids.as_slice() else {
        anyhow::bail!("two lessons expected, got {}", ids.len());
    };
    Ok((*a.min(b), *a.max(b)))
}

async fn answering(harness: &Harness, score: f64) {
    harness
        .reranker_answers(
            ResponseTemplate::new(200)
                .set_body_json(json!({"results": [{"index": 0, "relevance_score": score}]})),
        )
        .await;
}

async fn pair_rows(pool: &PgPool, a: i64) -> anyhow::Result<Vec<(i64, f32, f64, bool)>> {
    Ok(sqlx::query_as(
        "SELECT lesson_b_id, score, distance, judged_at IS NOT NULL
           FROM memory.lesson_pair_candidates WHERE lesson_a_id = $1",
    )
    .bind(a)
    .fetch_all(pool)
    .await?)
}

async fn relation_count(pool: &PgPool, ids: &[i64]) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM memory.lesson_relations
          WHERE from_lesson_id = ANY($1) OR to_lesson_id = ANY($1)",
    )
    .bind(ids)
    .fetch_one(pool)
    .await?)
}

#[tokio::test]
async fn night_pass_with_a_pair_the_reranker_rates_high_queues_it_and_writes_no_relation()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    answering(&harness, 0.9).await;
    let (a, b) = embedded_pair(&harness).await?;
    let budget = quiet_budget(&harness.state);

    let pass = night_pass(
        &harness.state,
        &harness.handle,
        &budget,
        at(23),
        &AlwaysIdle,
    )
    .await;

    assert_eq!(pass.stopped, None);
    assert_eq!(pass.lesson_pairs, 1);
    let rows = pair_rows(harness.handle.pool(), a).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, b);
    assert!(rows[0].1 >= 0.5);
    assert!(rows[0].2 < 0.01);
    assert!(!rows[0].3);
    assert_eq!(relation_count(harness.handle.pool(), &[a, b]).await?, 0);
    assert_eq!(harness.rerank_requests().await?.len(), 2);
    harness.cleanup().await
}

#[tokio::test]
async fn night_pass_with_a_pair_the_reranker_rates_low_queues_nothing() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    answering(&harness, 0.2).await;
    let (a, _) = embedded_pair(&harness).await?;
    let budget = quiet_budget(&harness.state);

    let pass = night_pass(
        &harness.state,
        &harness.handle,
        &budget,
        at(23),
        &AlwaysIdle,
    )
    .await;

    assert_eq!(pass.lesson_pairs, 0);
    assert!(pair_rows(harness.handle.pool(), a).await?.is_empty());
    harness.cleanup().await
}

#[tokio::test]
async fn night_pass_with_a_queued_pair_found_again_keeps_its_judged_mark() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    answering(&harness, 0.9).await;
    let (a, _) = embedded_pair(&harness).await?;
    let budget = quiet_budget(&harness.state);
    night_pass(
        &harness.state,
        &harness.handle,
        &budget,
        at(23),
        &AlwaysIdle,
    )
    .await;
    sqlx::query(
        "UPDATE memory.lesson_pair_candidates SET judged_at = now() WHERE lesson_a_id = $1",
    )
    .bind(a)
    .execute(harness.handle.pool())
    .await?;

    night_pass(
        &harness.state,
        &harness.handle,
        &budget,
        at(23),
        &AlwaysIdle,
    )
    .await;

    let rows = pair_rows(harness.handle.pool(), a).await?;
    assert_eq!(rows.len(), 1);
    assert!(rows[0].3);
    harness.cleanup().await
}

#[tokio::test]
async fn night_pass_when_the_reranker_fails_queues_no_pair() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness.reranker_answers(failing_answer()).await;
    let (a, _) = embedded_pair(&harness).await?;
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
    assert!(pair_rows(harness.handle.pool(), a).await?.is_empty());
    harness.cleanup().await
}
